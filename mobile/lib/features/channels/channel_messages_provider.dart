import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';

/// Provides the message list for a specific channel. Fetches history on init,
/// then subscribes to live events via the websocket session.
class ChannelMessagesNotifier extends Notifier<AsyncValue<List<NostrEvent>>> {
  final String channelId;
  void Function()? _unsubscribe;

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

  /// Fetch older messages (pagination). Call this when the user scrolls up.
  Future<void> fetchOlder() async {
    final currentEvents = state.value;
    if (currentEvents == null || currentEvents.isEmpty) return;

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

    if (older.isEmpty) return;

    state = state.whenData((events) {
      final ids = events.map((e) => e.id).toSet();
      final deduped = older.where((e) => !ids.contains(e.id)).toList();
      final merged = [...deduped, ...events];
      merged.sort((a, b) => a.createdAt.compareTo(b.createdAt));
      return merged;
    });
  }
}

final channelMessagesProvider =
    NotifierProvider.family<
      ChannelMessagesNotifier,
      AsyncValue<List<NostrEvent>>,
      String
    >(ChannelMessagesNotifier.new);
