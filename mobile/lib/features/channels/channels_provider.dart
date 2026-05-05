import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'channel.dart';

const _channelTypeOrder = {'stream': 0, 'forum': 1, 'dm': 2};

class ChannelsNotifier extends AsyncNotifier<List<Channel>> {
  static const _backstopInterval = Duration(seconds: 60);

  final List<void Function()> _unsubscribers = [];
  int _subscriptionVersion = 0;
  Timer? _backstopTimer;

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
      _clearLiveSubscriptions();
      _backstopTimer?.cancel();
      _backstopTimer = null;
    });

    if (sessionState.status != SessionStatus.connected) {
      _clearLiveSubscriptions();
    }

    // Initial fetch via HTTP (reliable, paginated).
    return _fetch(
      subscribeLive: sessionState.status == SessionStatus.connected,
    );
  }

  Future<List<Channel>> _fetch({bool subscribeLive = false}) async {
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
    if (subscribeLive) {
      await _subscribeLive(channels);
    }
    return channels;
  }

  /// Subscribe per-channel to live events (requires `#h` tag for relay
  /// channel-scoped fan-out). Also starts a 60s REST backstop timer to
  /// detect newly created channels that we don't yet have subscriptions for.
  Future<void> _subscribeLive(List<Channel> channels) async {
    _clearLiveSubscriptions();
    final subscriptionVersion = _subscriptionVersion;
    if (ref.read(relaySessionProvider).status != SessionStatus.connected) {
      return;
    }

    final session = ref.read(relaySessionProvider.notifier);
    final channelIds = {
      for (final channel in channels)
        if (channel.isMember && !channel.isArchived) channel.id,
    };

    final subscriptions = await Future.wait(
      channelIds.map((channelId) async {
        try {
          return await session.subscribe(
            NostrFilter(
              kinds: EventKind.channelEventKinds,
              tags: {
                '#h': [channelId],
              },
              limit: 0,
            ),
            _handleLiveEvent,
          );
        } catch (error) {
          debugPrint(
            '[ChannelsNotifier] live subscription failed for $channelId: $error',
          );
          return null;
        }
      }),
    );

    if (subscriptionVersion != _subscriptionVersion ||
        ref.read(relaySessionProvider).status != SessionStatus.connected) {
      for (final unsubscribe in subscriptions.whereType<void Function()>()) {
        unsubscribe();
      }
      return;
    }

    _unsubscribers.addAll(subscriptions.whereType<void Function()>());

    // Start a lightweight REST backstop so newly created channels (which we
    // don't have a WS subscription for) get picked up within 60s.
    // Uses _backstopRefresh instead of refresh() to preserve existing state
    // on transient REST failures (avoids AsyncError overwriting good data).
    _backstopTimer?.cancel();
    _backstopTimer = Timer.periodic(
      _backstopInterval,
      (_) => _backstopRefresh(),
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

  /// Backstop refresh that preserves existing state on transient REST failure.
  ///
  /// Unlike [refresh], this won't overwrite state with [AsyncError] if the
  /// network request fails — keeping WS live-event handling functional.
  Future<void> _backstopRefresh() async {
    try {
      final sessionState = ref.read(relaySessionProvider);
      final channels = await _fetch(
        subscribeLive: sessionState.status == SessionStatus.connected,
      );
      state = AsyncData(channels);
    } catch (error) {
      debugPrint('[ChannelsNotifier] backstop refresh failed: $error');
      // Keep current state — WS events continue working.
    }
  }

  Future<void> refresh() async {
    final sessionState = ref.read(relaySessionProvider);
    state = await AsyncValue.guard(
      () =>
          _fetch(subscribeLive: sessionState.status == SessionStatus.connected),
    );
  }

  void _clearLiveSubscriptions() {
    _subscriptionVersion++;
    for (final unsubscribe in _unsubscribers) {
      unsubscribe();
    }
    _unsubscribers.clear();
    _backstopTimer?.cancel();
    _backstopTimer = null;
  }
}

final channelsProvider = AsyncNotifierProvider<ChannelsNotifier, List<Channel>>(
  ChannelsNotifier.new,
);
