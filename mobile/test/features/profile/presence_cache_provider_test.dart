import 'dart:convert';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart' as http_testing;
import 'package:sprout_mobile/features/profile/presence_cache_provider.dart';
import 'package:sprout_mobile/shared/relay/relay.dart';

void main() {
  test('WS presence event updates cache for tracked pubkey', () async {
    final relaySession = _RecordingRelaySessionNotifier();
    final container = _buildContainer(
      relaySession: relaySession,
      presenceJson: {'alice': 'online'},
    );
    addTearDown(container.dispose);

    // Initialize the notifier (triggers build → subscribes to WS).
    container.read(presenceCacheProvider);
    await _pumpEventQueue();

    // Start tracking alice — triggers REST fetch.
    container.read(presenceCacheProvider.notifier).track(['alice']);
    await _pumpEventQueue();
    // Wait for the batch timer (50ms) to flush.
    await Future<void>.delayed(const Duration(milliseconds: 100));

    expect(container.read(presenceCacheProvider)['alice'], 'online');

    // Simulate a WS presence event: alice goes away.
    relaySession.emit(
      NostrEvent(
        id: 'evt-1',
        pubkey: 'alice',
        createdAt: 1000,
        kind: EventKind.presenceUpdate,
        tags: const [],
        content: 'away',
        sig: 'sig',
      ),
    );

    // Cache should update immediately via the WS handler.
    expect(container.read(presenceCacheProvider)['alice'], 'away');
  });

  test('WS presence event ignores untracked pubkeys', () async {
    final relaySession = _RecordingRelaySessionNotifier();
    final container = _buildContainer(
      relaySession: relaySession,
      presenceJson: {'alice': 'online'},
    );
    addTearDown(container.dispose);

    container.read(presenceCacheProvider);
    await _pumpEventQueue();

    // Track only alice.
    container.read(presenceCacheProvider.notifier).track(['alice']);
    await _pumpEventQueue();
    await Future<void>.delayed(const Duration(milliseconds: 100));

    // Emit event for bob (untracked).
    relaySession.emit(
      NostrEvent(
        id: 'evt-2',
        pubkey: 'bob',
        createdAt: 1000,
        kind: EventKind.presenceUpdate,
        tags: const [],
        content: 'online',
        sig: 'sig',
      ),
    );

    // Bob should NOT appear in the cache.
    expect(container.read(presenceCacheProvider).containsKey('bob'), isFalse);
  });

  test('WS presence event ignores invalid status values', () async {
    final relaySession = _RecordingRelaySessionNotifier();
    final container = _buildContainer(
      relaySession: relaySession,
      presenceJson: {'alice': 'online'},
    );
    addTearDown(container.dispose);

    container.read(presenceCacheProvider);
    await _pumpEventQueue();

    container.read(presenceCacheProvider.notifier).track(['alice']);
    await _pumpEventQueue();
    await Future<void>.delayed(const Duration(milliseconds: 100));

    expect(container.read(presenceCacheProvider)['alice'], 'online');

    // Emit event with garbage status.
    relaySession.emit(
      NostrEvent(
        id: 'evt-3',
        pubkey: 'alice',
        createdAt: 1000,
        kind: EventKind.presenceUpdate,
        tags: const [],
        content: 'garbage-status',
        sig: 'sig',
      ),
    );

    // Status should remain 'online' — the invalid value is rejected.
    expect(container.read(presenceCacheProvider)['alice'], 'online');
  });

  test('WS presence event skips no-op updates', () async {
    final relaySession = _RecordingRelaySessionNotifier();
    var stateChangeCount = 0;
    final container = _buildContainer(
      relaySession: relaySession,
      presenceJson: {'alice': 'online'},
    );
    addTearDown(container.dispose);

    container.read(presenceCacheProvider);
    await _pumpEventQueue();

    container.read(presenceCacheProvider.notifier).track(['alice']);
    await _pumpEventQueue();
    await Future<void>.delayed(const Duration(milliseconds: 100));

    // Listen for state changes after initial setup.
    container.listen(presenceCacheProvider, (prev, next) => stateChangeCount++);

    // Emit event with same status as current.
    relaySession.emit(
      NostrEvent(
        id: 'evt-4',
        pubkey: 'alice',
        createdAt: 1000,
        kind: EventKind.presenceUpdate,
        tags: const [],
        content: 'online',
        sig: 'sig',
      ),
    );

    // No state change should occur — it's a no-op.
    expect(stateChangeCount, 0);
  });

  test('subscribes to kind:20001 with limit 0', () async {
    final relaySession = _RecordingRelaySessionNotifier();
    final container = _buildContainer(
      relaySession: relaySession,
      presenceJson: {},
    );
    addTearDown(container.dispose);

    container.read(presenceCacheProvider);
    await _pumpEventQueue();

    // Should have subscribed with the correct filter.
    expect(relaySession.filters, hasLength(1));
    expect(relaySession.filters.single.kinds, [EventKind.presenceUpdate]);
    expect(relaySession.filters.single.limit, 0);
  });

  test('WS event uses pubkey variable, not literal string', () async {
    // Regression test for the map key bug where `{...state, pubkey: status}`
    // used the literal string "pubkey" instead of the variable's value.
    final relaySession = _RecordingRelaySessionNotifier();
    final container = _buildContainer(
      relaySession: relaySession,
      presenceJson: {'deadbeef': 'offline', 'cafebabe': 'offline'},
    );
    addTearDown(container.dispose);

    container.read(presenceCacheProvider);
    await _pumpEventQueue();

    container.read(presenceCacheProvider.notifier).track([
      'deadbeef',
      'cafebabe',
    ]);
    await _pumpEventQueue();
    await Future<void>.delayed(const Duration(milliseconds: 100));

    // Set deadbeef online via WS.
    relaySession.emit(
      NostrEvent(
        id: 'evt-5',
        pubkey: 'deadbeef',
        createdAt: 1000,
        kind: EventKind.presenceUpdate,
        tags: const [],
        content: 'online',
        sig: 'sig',
      ),
    );

    final cache = container.read(presenceCacheProvider);
    // deadbeef should be online (the actual pubkey, not a literal "pubkey" key).
    expect(cache['deadbeef'], 'online');
    // cafebabe should still be offline (not clobbered).
    expect(cache['cafebabe'], 'offline');
    // There should be no literal "pubkey" key in the map.
    expect(cache.containsKey('pubkey'), isFalse);
  });
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

Future<void> _pumpEventQueue() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}

ProviderContainer _buildContainer({
  required _RecordingRelaySessionNotifier relaySession,
  required Map<String, String> presenceJson,
}) {
  final client = RelayClient(
    baseUrl: 'http://localhost:3000',
    httpClient: http_testing.MockClient((request) async {
      expect(request.url.path, '/api/presence');
      return http.Response(jsonEncode(presenceJson), 200);
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

class _RecordingRelaySessionNotifier extends RelaySessionNotifier {
  final List<NostrFilter> filters = [];
  final List<void Function(NostrEvent)> _listeners = [];

  @override
  SessionState build() => const SessionState(status: SessionStatus.connected);

  @override
  Future<void Function()> subscribe(
    NostrFilter filter,
    void Function(NostrEvent) onEvent, {
    void Function(String message)? onClosed,
  }) async {
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
