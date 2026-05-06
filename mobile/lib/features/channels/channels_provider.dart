import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import '../../shared/utils/string_utils.dart';
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
    if (myPk == null) throw StateError('No signing identity available');

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

    // Dedupe by `d` tag (channel id) — kind:39000 is parameterized-replaceable,
    // so logically there's exactly one current event per id, but stale revisions
    // from before the relay's d_tag backfill can linger. Keep the highest
    // `created_at` per id so the latest channel_type / name wins.
    final latestMetaPerId = <String, NostrEvent>{};
    for (final event in metas) {
      if (event.kind != 39000) continue;
      final id = event.getTagValue('d');
      if (id == null) continue;
      final existing = latestMetaPerId[id];
      if (existing == null || event.createdAt > existing.createdAt) {
        latestMetaPerId[id] = event;
      }
    }
    final dedupedMetas = latestMetaPerId.values;

    // Resolve DM participant display names. Relay stores DM channels with
    // literal name="DM"; pure-Nostr architecture pushes name resolution to
    // the client, so collect non-self participant pubkeys across all DM
    // metas and batch-fetch their kind:0 profiles in one round-trip.
    final dmParticipants = <String>{};
    final myPkLower = myPk.toLowerCase();
    for (final event in dedupedMetas) {
      final data = ChannelData.fromEvent(event);
      if (data.channelType != 'dm') continue;
      for (final pk in data.participantPubkeys) {
        final lower = pk.toLowerCase();
        if (lower != myPkLower) dmParticipants.add(lower);
      }
    }

    final displayNames = <String, String>{};
    if (dmParticipants.isNotEmpty) {
      final profileEvents = await session.fetchHistory(
        NostrFilters.profilesBatch(dmParticipants.toList()),
      );
      for (final event in profileEvents) {
        if (event.kind != 0) continue;
        final profile = ProfileData.fromEvent(event);
        final label = profile.displayName?.trim().isNotEmpty == true
            ? profile.displayName!.trim()
            : profile.nip05?.trim().isNotEmpty == true
            ? profile.nip05!.trim()
            : shortPubkey(profile.pubkey);
        displayNames[profile.pubkey.toLowerCase()] = label;
      }
    }

    final channels = <Channel>[];
    for (final event in dedupedMetas) {
      final channel = _channelFromMeta(
        event,
        isMember: true,
        displayNames: displayNames,
      );
      if (!channel.isEphemeral) {
        channels.add(channel);
      }
    }

    channels.sort((left, right) {
      final typeOrder =
          (_channelTypeOrder[left.channelType] ?? 99) -
          (_channelTypeOrder[right.channelType] ?? 99);
      if (typeOrder != 0) return typeOrder;
      // Case-insensitive to match desktop's `localeCompare` ordering.
      return left.name.toLowerCase().compareTo(right.name.toLowerCase());
    });

    if (subscribeLive) {
      await _subscribeLive(channels);
    }
    return channels;
  }

  /// Build a [Channel] from a kind:39000 metadata event.
  ///
  /// [displayNames] maps lowercase participant pubkey → resolved label and is
  /// used to populate [Channel.participants] for DMs so [Channel.displayLabel]
  /// can render real names instead of the relay-canonical "DM" name.
  Channel _channelFromMeta(
    NostrEvent event, {
    required bool isMember,
    Map<String, String> displayNames = const {},
  }) {
    final data = ChannelData.fromEvent(event);
    final participants = data.channelType == 'dm'
        ? [
            for (final pk in data.participantPubkeys)
              displayNames[pk.toLowerCase()] ?? shortPubkey(pk),
          ]
        : const <String>[];
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
      participants: participants,
      participantPubkeys: data.participantPubkeys,
      isMember: isMember,
      ttlSeconds: data.ttlSeconds,
      ttlDeadline: data.ttlDeadline,
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
