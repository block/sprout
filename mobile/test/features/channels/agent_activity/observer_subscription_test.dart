import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:nostr/nostr.dart' as nostr;
import 'package:sprout_mobile/features/channels/agent_activity/observer_models.dart';
import 'package:sprout_mobile/features/channels/agent_activity/observer_subscription.dart';
import 'package:sprout_mobile/shared/relay/relay.dart';

void main() {
  test('provider initializes without circular dependency error', () {
    // Regression test: reading the provider should NOT throw
    // "Bad state: Tried to read the state of an uninitialized provider".
    final container = ProviderContainer(
      overrides: [
        relaySessionProvider.overrideWith(() => _RecordingRelaySession()),
        relayConfigProvider.overrideWith(
          () => _FakeRelayConfigNotifier(nsec: null),
        ),
      ],
    );
    addTearDown(container.dispose);

    const key = (channelId: 'test-channel', agentPubkey: 'deadbeef');
    // This line threw before the fix.
    final state = container.read(observerSubscriptionProvider(key));
    expect(state.connection, ObserverConnectionState.idle);
    expect(state.transcript, isEmpty);
  });

  test('transitions to error when nsec is invalid', () async {
    final container = ProviderContainer(
      overrides: [
        relaySessionProvider.overrideWith(() => _RecordingRelaySession()),
        relayConfigProvider.overrideWith(
          () => _FakeRelayConfigNotifier(nsec: 'nsec1invalid'),
        ),
      ],
    );
    addTearDown(container.dispose);

    const key = (channelId: 'test-channel', agentPubkey: 'deadbeef');
    container.read(observerSubscriptionProvider(key));

    // Let the microtask (_subscribeLive) run.
    await Future<void>.delayed(Duration.zero);

    final state = container.read(observerSubscriptionProvider(key));
    // With invalid nsec, it should be in error state, NOT throw.
    expect(state.connection, ObserverConnectionState.error);
    expect(state.errorMessage, isNotNull);
  });

  test('stays idle when nsec is null', () async {
    final container = ProviderContainer(
      overrides: [
        relaySessionProvider.overrideWith(() => _RecordingRelaySession()),
        relayConfigProvider.overrideWith(
          () => _FakeRelayConfigNotifier(nsec: null),
        ),
      ],
    );
    addTearDown(container.dispose);

    const key = (channelId: 'test-channel', agentPubkey: 'deadbeef');
    container.read(observerSubscriptionProvider(key));

    // Let microtask run — _subscribeLive should early-return (no nsec).
    await Future<void>.delayed(Duration.zero);

    final state = container.read(observerSubscriptionProvider(key));
    // No nsec means _subscribeLive returns early, state stays idle.
    expect(state.connection, ObserverConnectionState.idle);
    expect(state.transcript, isEmpty);
  });

  test(
    'subscribes with correct filter shape and transitions to open',
    () async {
      final userKeychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(userKeychain.private);
      final myPubkey = userKeychain.public;
      // Agent needs a valid 64-char hex pubkey for getConversationKey.
      final agentKeychain = nostr.Keychain.generate();
      final agentPubkey = agentKeychain.public;

      final relaySession = _RecordingRelaySession();
      final container = ProviderContainer(
        overrides: [
          relaySessionProvider.overrideWith(() => relaySession),
          relayConfigProvider.overrideWith(
            () => _FakeRelayConfigNotifier(nsec: nsec),
          ),
        ],
      );
      addTearDown(container.dispose);

      final key = (channelId: 'test-channel', agentPubkey: agentPubkey);
      container.read(observerSubscriptionProvider(key));

      // Let the microtask (_subscribeLive) run.
      await Future<void>.delayed(Duration.zero);

      final state = container.read(observerSubscriptionProvider(key));
      expect(state.connection, ObserverConnectionState.open);
      expect(state.transcript, isEmpty);

      // Verify the subscription filter shape.
      expect(relaySession.filters, hasLength(1));
      final filter = relaySession.filters.first;
      expect(filter.kinds, [24200]);
      expect(filter.limit, 0);
      expect(filter.tags['#p'], contains(myPubkey));
      expect(filter.since, isNotNull);
    },
  );
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

class _RecordingRelaySession extends RelaySessionNotifier {
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

class _FakeRelayConfigNotifier extends RelayConfigNotifier {
  final String? _nsec;

  _FakeRelayConfigNotifier({required String? nsec}) : _nsec = nsec;

  @override
  RelayConfig build() =>
      RelayConfig(baseUrl: 'http://localhost:3000', nsec: _nsec);
}
