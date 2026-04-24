import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'channel_management_provider.dart';

/// Provides the message list for a specific channel. Fetches history on init,
/// then subscribes to live events via the websocket session.
class ChannelMessagesNotifier extends Notifier<AsyncValue<List<NostrEvent>>> {
  final String channelId;
  void Function()? _unsubscribe;
  bool _reachedOldest = false;
  bool _initInFlight = false;

  ChannelMessagesNotifier(this.channelId);

  /// Last successfully loaded messages, preserved across reconnections so the
  /// UI can show stale data instead of a blank loading spinner.
  List<NostrEvent>? _lastKnownMessages;

  @override
  AsyncValue<List<NostrEvent>> build() {
    final sessionState = ref.watch(relaySessionProvider);
    ref.onDispose(() {
      _unsubscribe?.call();
      _unsubscribe = null;
    });

    if (sessionState.status != SessionStatus.connected) {
      // Return cached messages if available so the UI remains usable while
      // disconnected/reconnecting, instead of showing an empty screen.
      return AsyncData(_lastKnownMessages ?? const []);
    }

    // Reset pagination state on rebuild (e.g. after reconnect).
    _reachedOldest = false;
    _init();
    // Show previous messages while fetching fresh ones, instead of a spinner.
    if (_lastKnownMessages case final cached? when cached.isNotEmpty) {
      return AsyncData(cached);
    }
    return const AsyncLoading();
  }

  Future<void> _init() async {
    _initInFlight = true;
    try {
      final session = ref.read(relaySessionProvider.notifier);

      // 1. Fetch recent history via REQ/EOSE.
      final history = await session.fetchHistory(
        NostrFilter(
          kinds: EventKind.channelEventKinds,
          tags: {
            '#h': [channelId],
          },
          limit: 50,
        ),
      );

      // 2. Subscribe to live events for this channel.
      _unsubscribe = await session.subscribe(
        NostrFilter(
          kinds: EventKind.channelEventKinds,
          tags: {
            '#h': [channelId],
          },
          limit: 0,
        ),
        _handleLiveEvent,
      );

      // Merge fresh history with any events already in state (e.g. from
      // fetchOlder() or live events that arrived while _init was in flight)
      // to avoid discarding data the user has already scrolled through.
      final existing = state.value ?? const [];
      final existingIds = existing.map((e) => e.id).toSet();
      final newEvents = history
          .where((e) => !existingIds.contains(e.id))
          .toList();
      final merged = [...existing, ...newEvents];
      merged.sort((a, b) => a.createdAt.compareTo(b.createdAt));
      _lastKnownMessages = merged;
      state = AsyncData(merged);
    } catch (e, st) {
      state = AsyncError(e, st);
    } finally {
      _initInFlight = false;
    }
  }

  void _handleLiveEvent(NostrEvent event) {
    state = state.whenData((events) {
      final merged = _mergeEvent(events, event);
      _lastKnownMessages = merged;
      return merged;
    });

    // When a membership system event arrives, refresh the channel member list
    // so the @mention autocomplete picks up new members without a restart.
    if (event.kind == EventKind.systemMessage &&
        _isMembershipEvent(event.content)) {
      ref.invalidate(channelMembersProvider(channelId));
    }
  }

  static bool _isMembershipEvent(String content) {
    return content.contains('member_joined') ||
        content.contains('member_left') ||
        content.contains('member_removed');
  }

  /// Merge a new event into the sorted list, deduplicating by ID.
  static List<NostrEvent> _mergeEvent(
    List<NostrEvent> current,
    NostrEvent incoming,
  ) {
    if (current.any((e) => e.id == incoming.id)) return current;
    final updated = [...current, incoming];
    updated.sort((a, b) => a.createdAt.compareTo(b.createdAt));
    return updated;
  }

  /// Whether all history has been loaded (no more older messages).
  bool get reachedOldest => _reachedOldest;

  /// Fetch older messages (pagination). Call this when the user scrolls up.
  /// Returns `true` if new messages were loaded.
  Future<bool> fetchOlder() async {
    if (_reachedOldest || _initInFlight) return false;

    final currentEvents = state.value;
    if (currentEvents == null || currentEvents.isEmpty) return false;

    final oldest = currentEvents.first.createdAt;
    final session = ref.read(relaySessionProvider.notifier);

    final older = await session.fetchHistory(
      NostrFilter(
        kinds: EventKind.channelEventKinds,
        tags: {
          '#h': [channelId],
        },
        limit: 50,
        until: oldest,
      ),
    );

    if (older.isEmpty) {
      _reachedOldest = true;
      return false;
    }

    // Dedup against existing events. If nothing new remains after dedup
    // (e.g. all returned events share the boundary timestamp), mark as
    // exhausted to avoid an infinite fetch loop.
    final currentIds = state.value?.map((e) => e.id).toSet() ?? {};
    final deduped = older.where((e) => !currentIds.contains(e.id)).toList();

    if (deduped.isEmpty) {
      _reachedOldest = true;
      return false;
    }

    state = state.whenData((events) {
      final merged = [...deduped, ...events];
      merged.sort((a, b) => a.createdAt.compareTo(b.createdAt));
      _lastKnownMessages = merged;
      return merged;
    });
    return true;
  }
}

final channelMessagesProvider =
    NotifierProvider.family<
      ChannelMessagesNotifier,
      AsyncValue<List<NostrEvent>>,
      String
    >(ChannelMessagesNotifier.new);
