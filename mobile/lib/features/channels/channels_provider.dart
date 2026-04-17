import 'package:flutter/widgets.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'channel.dart';

const _channelTypeOrder = {'stream': 0, 'forum': 1, 'dm': 2};

class ChannelsNotifier extends AsyncNotifier<List<Channel>> {
  void Function()? _unsubscribe;

  @override
  Future<List<Channel>> build() {
    ref.watch(relayClientProvider);
    final sessionState = ref.watch(relaySessionProvider);

    // Re-fetch when the app returns to foreground so channels created on
    // another device while mobile was backgrounded appear immediately.
    ref.listen(appLifecycleProvider, (prev, next) {
      if (next == AppLifecycleState.resumed) {
        refresh();
      }
    });

    ref.onDispose(() {
      _unsubscribe?.call();
      _unsubscribe = null;
    });

    // Initial fetch via HTTP (reliable, paginated).
    final fetchFuture = _fetch();

    // If websocket is connected, subscribe to live channel events to keep
    // the list up to date without polling.
    if (sessionState.status == SessionStatus.connected) {
      _subscribeLive();
    }

    return fetchFuture;
  }

  Future<List<Channel>> _fetch() async {
    final client = ref.read(relayClientProvider);
    final json = await client.get('/api/channels') as List<dynamic>;
    final channels = json
        .cast<Map<String, dynamic>>()
        .map(Channel.fromJson)
        .toList();
    channels.sort((left, right) {
      final typeOrder =
          (_channelTypeOrder[left.channelType] ?? 99) -
          (_channelTypeOrder[right.channelType] ?? 99);
      if (typeOrder != 0) {
        return typeOrder;
      }
      return left.name.compareTo(right.name);
    });
    return channels;
  }

  void _subscribeLive() async {
    final session = ref.read(relaySessionProvider.notifier);
    _unsubscribe = await session.subscribe(
      NostrFilter(kinds: EventKind.channelEventKinds, limit: 0),
      _handleLiveEvent,
    );
  }

  void _handleLiveEvent(NostrEvent event) {
    final channelId = event.channelId;
    if (channelId == null) return;

    state = state.whenData((channels) {
      final idx = channels.indexWhere((c) => c.id == channelId);
      if (idx == -1) {
        // Unknown channel — queue a full refresh to pick it up.
        refresh();
        return channels;
      }
      // Update lastMessageAt for the affected channel.
      final updated = List<Channel>.of(channels);
      final channel = updated[idx];
      final eventTime = DateTime.fromMillisecondsSinceEpoch(
        event.createdAt * 1000,
        isUtc: true,
      );
      if (channel.lastMessageAt == null ||
          eventTime.isAfter(channel.lastMessageAt!)) {
        updated[idx] = channel.copyWith(lastMessageAt: eventTime);
      }
      return updated;
    });
  }

  Future<void> refresh() async {
    state = await AsyncValue.guard(_fetch);
  }
}

final channelsProvider = AsyncNotifierProvider<ChannelsNotifier, List<Channel>>(
  ChannelsNotifier.new,
);
