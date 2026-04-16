import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';

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

    // Reset pagination state on rebuild (e.g. after reconnect).
    _reachedOldest = false;

    if (sessionState.status == SessionStatus.connected) {
      _init();
    } else {
      // WebSocket not connected yet (e.g. first-time pairing).
      // Fetch messages via HTTP so the user sees content immediately.
      _fetchViaHttp();
    }
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

  /// Fetch messages via HTTP REST API as a fallback when the WebSocket
  /// session isn't connected yet. Once the WebSocket connects,
  /// build() will re-run and switch to the full WebSocket flow.
  Future<void> _fetchViaHttp() async {
    try {
      final client = ref.read(relayClientProvider);
      final json =
          await client.get('/api/channels/$channelId/messages')
              as Map<String, dynamic>;
      final messagesJson = json['messages'] as List<dynamic>? ?? [];
      final messages = messagesJson
          .cast<Map<String, dynamic>>()
          .map(_httpMessageToNostrEvent)
          .whereType<NostrEvent>()
          .toList();
      messages.sort((a, b) => a.createdAt.compareTo(b.createdAt));
      state = AsyncData(messages);
    } catch (e, st) {
      state = AsyncError(e, st);
    }
  }

  /// Convert an HTTP API message object to a NostrEvent for display.
  static NostrEvent? _httpMessageToNostrEvent(Map<String, dynamic> msg) {
    try {
      return NostrEvent(
        id: msg['event_id'] as String? ?? msg['id'] as String? ?? '',
        pubkey: msg['pubkey'] as String? ?? '',
        createdAt: msg['created_at'] is int
            ? msg['created_at'] as int
            : (DateTime.tryParse(
                        msg['created_at']?.toString() ?? '',
                      )?.millisecondsSinceEpoch ??
                      0) ~/
                  1000,
        kind: msg['kind'] as int? ?? EventKind.streamMessage,
        tags:
            (msg['tags'] as List<dynamic>?)
                ?.map((t) => (t as List<dynamic>).cast<String>().toList())
                .toList() ??
            [],
        content: msg['content'] as String? ?? '',
        sig: msg['sig'] as String? ?? '',
      );
    } catch (_) {
      return null;
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
