import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'channel.dart';

const _channelTypeOrder = {'stream': 0, 'forum': 1, 'dm': 2};

/// Loads the user's channel list from the relay over WebSocket.
///
/// Two-step query:
///   1. Fetch kind:39002 membership events tagged `#p:<my-pubkey>` to find
///      the channel ids I'm a member of.
///   2. Fetch the corresponding kind:39000 channel metadata events.
///
/// Live updates are layered on top via per-channel subscriptions on the
/// `#h` tag for any of the visible channel event kinds — incoming events
/// bump `lastMessageAt` for that channel.
class ChannelsNotifier extends AsyncNotifier<List<Channel>> {
  static const _backstopInterval = Duration(seconds: 60);

  final List<void Function()> _unsubscribers = [];
  int _subscriptionVersion = 0;
  Timer? _backstopTimer;

  @override
  Future<List<Channel>> build() {
    final sessionState = ref.watch(relaySessionProvider);
    ref.watch(relayConfigProvider);

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

    return _fetch(
      subscribeLive: sessionState.status == SessionStatus.connected,
    );
  }

  Future<List<Channel>> _fetch({bool subscribeLive = false}) async {
    final myPk = ref.read(myPubkeyProvider);
    if (myPk == null) return const [];

    final session = ref.read(relaySessionProvider.notifier);

    // Step 1: find the channels I'm a member of via kind:39002.
    final memberships = await session.fetchHistory(
      NostrFilters.myChannels(myPk),
    );
    final channelIds = memberships
        .map((e) => e.getTagValue('d'))
        .whereType<String>()
        .toSet()
        .toList();
    if (channelIds.isEmpty) return const [];

    // Step 2: pull channel metadata in one batched filter.
    final metas = await session.fetchHistory(
      NostrFilters.channelMetadata(channelIds),
    );

    final channels = <Channel>[];
    for (final event in metas) {
      if (event.kind != 39000) continue;
      channels.add(_channelFromMeta(event, isMember: true));
    }

    channels.sort((left, right) {
      final typeOrder =
          (_channelTypeOrder[left.channelType] ?? 99) -
          (_channelTypeOrder[right.channelType] ?? 99);
      if (typeOrder != 0) return typeOrder;
      return left.name.compareTo(right.name);
    });

    if (subscribeLive) {
      await _subscribeLive(channels);
    }
    return channels;
  }

  /// Build a [Channel] from a kind:39000 metadata event.
  Channel _channelFromMeta(NostrEvent event, {required bool isMember}) {
    final data = ChannelData.fromEvent(event);
    return Channel(
      id: data.id,
      name: data.name,
      channelType: data.channelType,
      visibility: data.visibility,
      description: data.description,
      topic: data.topic,
      createdBy: event.pubkey,
      createdAt: DateTime.fromMillisecondsSinceEpoch(
        event.createdAt * 1000,
        isUtc: true,
      ),
      memberCount: 0,
      lastMessageAt: null,
      participants: const [],
      participantPubkeys: data.participantPubkeys,
      isMember: isMember,
    );
  }

  /// Subscribe per-channel to live events (requires `#h` tag for relay
  /// channel-scoped fan-out). Also starts a 60s WS backstop poll to detect
  /// newly created channels we don't yet have subscriptions for.
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

  /// Backstop refresh that preserves existing state on transient failure.
  Future<void> _backstopRefresh() async {
    try {
      final sessionState = ref.read(relaySessionProvider);
      final channels = await _fetch(
        subscribeLive: sessionState.status == SessionStatus.connected,
      );
      state = AsyncData(channels);
    } catch (error) {
      debugPrint('[ChannelsNotifier] backstop refresh failed: $error');
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
