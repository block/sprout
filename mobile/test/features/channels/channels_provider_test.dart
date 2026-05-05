import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:sprout_mobile/features/channels/channels_provider.dart';
import 'package:sprout_mobile/shared/relay/relay.dart';

/// Tests for [ChannelsNotifier] in the pure-Nostr world.
///
/// The provider performs a two-step WS query:
///   1. kind:39002 memberships tagged `#p:<my-pubkey>`
///   2. kind:39000 metadata for those channel ids
/// then layers per-channel live subscriptions on the `#h` tag.
///
/// Tests stub out the relay session by overriding [relaySessionProvider] with
/// a [_FakeRelaySession] that returns canned events from [fetchHistory] and
/// records [subscribe] calls so we can assert filter shapes and emit live
/// events on demand.
void main() {
  const myPk = 'me';

  test(
    'subscribes per-channel with #h tags (only joined, non-archived)',
    () async {
      final session = _FakeRelaySession(
        memberships: [
          _membership(_channelA, myPk),
          _membership(_channelB, myPk),
          _membership(_channelD, myPk),
        ],
        metadata: [
          _meta(id: _channelA, name: 'general'),
          _meta(id: _channelB, name: 'random'),
          // channelD metadata missing -> won't appear in channel list
        ],
      );
      final container = _buildContainer(session: session);
      addTearDown(container.dispose);

      await container.read(channelsProvider.future);

      // One subscription per joined, non-archived channel.
      expect(session.subscribeFilters, hasLength(2));
      expect(
        session.subscribeFilters.map((f) => f.tags['#h']?.single).toSet(),
        {_channelA, _channelB},
      );
      for (final filter in session.subscribeFilters) {
        expect(filter.kinds, EventKind.channelEventKinds);
        expect(filter.limit, 0);
      }
    },
  );

  test('live channel events update channel lastMessageAt', () async {
    final session = _FakeRelaySession(
      memberships: [_membership(_channelA, myPk)],
      metadata: [_meta(id: _channelA, name: 'general', createdAt: 10)],
    );
    final container = _buildContainer(session: session);
    addTearDown(container.dispose);

    await container.read(channelsProvider.future);

    // Emit a live message event on channelA.
    session.emit(
      NostrEvent(
        id: 'event-1',
        pubkey: 'alice',
        createdAt: 20,
        kind: EventKind.streamMessageV2,
        tags: const [
          ['h', _channelA],
        ],
        content: 'new message',
        sig: 'sig',
      ),
    );

    final channels = container.read(channelsProvider).value!;
    expect(channels.single.lastMessageAt?.millisecondsSinceEpoch, 20 * 1000);
  });

  test('initial fetch issues membership + metadata queries', () async {
    final session = _FakeRelaySession(
      memberships: [_membership(_channelA, myPk)],
      metadata: [_meta(id: _channelA, name: 'general')],
    );
    final container = _buildContainer(session: session);
    addTearDown(container.dispose);

    await container.read(channelsProvider.future);

    // Two history fetches: memberships (kind:39002) then metadata (kind:39000).
    expect(session.historyFilters, hasLength(2));
    expect(session.historyFilters[0].kinds, [39002]);
    expect(session.historyFilters[0].tags['#p'], [myPk]);
    expect(session.historyFilters[1].kinds, [39000]);
    expect(session.historyFilters[1].tags['#d'], [_channelA]);

    // And one live subscription on the resulting channel.
    expect(session.subscribeFilters, hasLength(1));
  });
}

const _channelA = '11111111-1111-4111-8111-111111111111';
const _channelB = '22222222-2222-4222-8222-222222222222';
const _channelD = '44444444-4444-4444-8444-444444444444';

/// Build a kind:39002 membership event tagged with the channel id and member.
NostrEvent _membership(String channelId, String pubkey) => NostrEvent(
  id: 'mem-$channelId',
  pubkey: 'creator',
  createdAt: 1,
  kind: 39002,
  tags: [
    ['d', channelId],
    ['p', pubkey],
  ],
  content: '',
  sig: 'sig',
);

/// Build a kind:39000 channel metadata event.
NostrEvent _meta({
  required String id,
  required String name,
  String channelType = 'stream',
  int createdAt = 1,
}) => NostrEvent(
  id: 'meta-$id',
  pubkey: 'creator',
  createdAt: createdAt,
  kind: 39000,
  tags: [
    ['d', id],
    ['name', name],
    ['t', channelType],
    ['public'],
  ],
  content: '',
  sig: 'sig',
);

ProviderContainer _buildContainer({required _FakeRelaySession session}) {
  return ProviderContainer(
    overrides: [
      appLifecycleProvider.overrideWith(() => _FakeAppLifecycleNotifier()),
      relaySessionProvider.overrideWith(() => session),
      myPubkeyProvider.overrideWithValue('me'),
    ],
  );
}

/// Fake [RelaySessionNotifier] that returns canned events from [fetchHistory]
/// and records subscribe calls.
class _FakeRelaySession extends RelaySessionNotifier {
  _FakeRelaySession({required this.memberships, required this.metadata});

  final List<NostrEvent> memberships;
  final List<NostrEvent> metadata;

  final List<NostrFilter> historyFilters = [];
  final List<NostrFilter> subscribeFilters = [];
  final List<void Function(NostrEvent)> _listeners = [];

  @override
  SessionState build() => const SessionState(status: SessionStatus.connected);

  @override
  Future<List<NostrEvent>> fetchHistory(
    NostrFilter filter, {
    Duration timeout = const Duration(seconds: 8),
  }) async {
    historyFilters.add(filter);
    if (filter.kinds.contains(39002)) {
      // Membership query — return all memberships we have for this pubkey.
      final myPk = filter.tags['#p']?.single;
      return memberships
          .where(
            (e) =>
                e.tags.any((t) => t.length >= 2 && t[0] == 'p' && t[1] == myPk),
          )
          .toList();
    }
    if (filter.kinds.contains(39000)) {
      // Metadata query — return all metadata events whose `d` tag matches.
      final ids = (filter.tags['#d'] ?? const <String>[]).toSet();
      return metadata.where((e) => ids.contains(e.getTagValue('d'))).toList();
    }
    return const [];
  }

  @override
  Future<void Function()> subscribe(
    NostrFilter filter,
    void Function(NostrEvent) onEvent, {
    void Function(String message)? onClosed,
  }) async {
    subscribeFilters.add(filter);
    _listeners.add(onEvent);
    return () {
      subscribeFilters.remove(filter);
      _listeners.remove(onEvent);
    };
  }

  /// Emit a live event to all subscribers.
  void emit(NostrEvent event) {
    for (final listener in List.of(_listeners)) {
      listener(event);
    }
  }
}

class _FakeAppLifecycleNotifier extends AppLifecycleNotifier {
  @override
  AppLifecycleState build() => AppLifecycleState.resumed;
}
