import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'channel_management_provider.dart';

/// Provides the message list for a specific channel. Fetches history on init,
/// then subscribes to live events via the websocket session.
class ChannelMessagesNotifier extends Notifier<AsyncValue<List<NostrEvent>>> {
  final String channelId;
  void Function()? _unsubscribe;
  bool _reachedOldest = false;

  ChannelMessagesNotifier(this.channelId);

  @override
  AsyncValue<List<NostrEvent>> build() {
    final sessionState = ref.watch(relaySessionProvider);
    ref.onDispose(() {
      _unsubscribe?.call();
      _unsubscribe = null;
    });

    if (sessionState.status != SessionStatus.connected) {
      return const AsyncData([]);
    }

    // Reset pagination state on rebuild (e.g. after reconnect).
    _reachedOldest = false;
    _init();
    return const AsyncLoading();
  }

  Future<void> _init() async {
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

      history.sort((a, b) => a.createdAt.compareTo(b.createdAt));
      state = AsyncData(history);
    } catch (e, st) {
      state = AsyncError(e, st);
    }
  }

  void _handleLiveEvent(NostrEvent event) {
    state = state.whenData((events) => _mergeEvent(events, event));

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
    if (_reachedOldest) return false;

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
