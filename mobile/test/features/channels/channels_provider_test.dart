import 'dart:convert';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart' as http_testing;
import 'package:sprout_mobile/features/channels/channels_provider.dart';
import 'package:sprout_mobile/shared/relay/relay.dart';

void main() {
  test('subscribes to live channel events per loaded member channel', () async {
    final relaySession = _RecordingRelaySessionNotifier();
    final container = _buildContainer(
      relaySession: relaySession,
      channelsJson: [
        _channelJson(id: _channelA, name: 'general'),
        _channelJson(id: _channelB, name: 'random'),
        _channelJson(id: _channelC, name: 'archived', archived: true),
        _channelJson(id: _channelD, name: 'unjoined', member: false),
      ],
    );
    addTearDown(container.dispose);

    await container.read(channelsProvider.future);

    expect(relaySession.filters, hasLength(2));
    expect(
      relaySession.filters.map((filter) => filter.tags['#h']?.single).toSet(),
      {_channelA, _channelB},
    );
    expect(
      relaySession.filters.every(
        (filter) => filter.kinds == EventKind.channelEventKinds,
      ),
      isTrue,
    );
  });

  test('live channel events update channel lastMessageAt', () async {
    final relaySession = _RecordingRelaySessionNotifier();
    final container = _buildContainer(
      relaySession: relaySession,
      channelsJson: [
        _channelJson(
          id: _channelA,
          name: 'general',
          lastMessageAt: DateTime.fromMillisecondsSinceEpoch(
            10 * 1000,
            isUtc: true,
          ),
        ),
      ],
    );
    addTearDown(container.dispose);

    await container.read(channelsProvider.future);

    relaySession.emit(
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
}

const _channelA = '11111111-1111-4111-8111-111111111111';
const _channelB = '22222222-2222-4222-8222-222222222222';
const _channelC = '33333333-3333-4333-8333-333333333333';
const _channelD = '44444444-4444-4444-8444-444444444444';

ProviderContainer _buildContainer({
  required _RecordingRelaySessionNotifier relaySession,
  required List<Map<String, dynamic>> channelsJson,
}) {
  final client = RelayClient(
    baseUrl: 'http://localhost:3000',
    httpClient: http_testing.MockClient((request) async {
      expect(request.url.path, '/api/channels');
      return http.Response(jsonEncode(channelsJson), 200);
    }),
  );

  return ProviderContainer(
    overrides: [
      appLifecycleProvider.overrideWith(() => _FakeAppLifecycleNotifier()),
      relayClientProvider.overrideWithValue(client),
      relaySessionProvider.overrideWith(() => relaySession),
    ],
  );
}

Map<String, dynamic> _channelJson({
  required String id,
  required String name,
  bool member = true,
  bool archived = false,
  DateTime? lastMessageAt,
}) {
  return {
    'id': id,
    'name': name,
    'channel_type': 'stream',
    'visibility': 'open',
    'description': '',
    'created_by': 'creator',
    'created_at': DateTime(2025).toIso8601String(),
    'member_count': 1,
    'is_member': member,
    'last_message_at': lastMessageAt?.toIso8601String(),
    'archived_at': archived ? DateTime(2025, 1, 2).toIso8601String() : null,
  };
}

class _RecordingRelaySessionNotifier extends RelaySessionNotifier {
  final List<NostrFilter> filters = [];
  final List<void Function(NostrEvent)> _listeners = [];

  @override
  SessionState build() => const SessionState(status: SessionStatus.connected);

  @override
  Future<void Function()> subscribe(
    NostrFilter filter,
    void Function(NostrEvent) onEvent,
  ) async {
    filters.add(filter);
    _listeners.add(onEvent);
    return () {
      filters.remove(filter);
      _listeners.remove(onEvent);
    };
  }

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
