import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:nostr/nostr.dart' as nostr;
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sprout_mobile/features/channels/read_state/read_state_manager.dart';
import 'package:sprout_mobile/shared/relay/relay.dart';

void main() {
  test('dispose flushes a pending publish after marking disposed', () async {
    SharedPreferences.setMockInitialValues({});
    final prefs = await SharedPreferences.getInstance();
    final keychain = nostr.Keychain.generate();
    final nsec = nostr.Nip19.encodePrivkey(keychain.private);
    final crypto = ReadStateCrypto.tryCreate(
      nsec: nsec,
      pubkey: keychain.public,
    );
    final relay = _FakeSignedEventRelay();
    final manager = ReadStateManager(
      pubkey: keychain.public,
      prefs: prefs,
      crypto: crypto!,
      relaySession: null,
      signedEventRelay: relay,
      remoteEnabled: true,
      onChanged: () {},
    );

    manager.markContextRead('channel-1', 42);
    manager.dispose();

    final submitted = await relay.submitted.future.timeout(
      const Duration(seconds: 1),
    );
    expect(submitted.kind, EventKind.readState);
    expect(
      submitted.tags.any(
        (tag) => tag.length == 2 && tag[0] == 't' && tag[1] == 'read-state',
      ),
      isTrue,
    );
  });

  test('disables remote sync after relay rejects read-state kind', () async {
    SharedPreferences.setMockInitialValues({});
    final prefs = await SharedPreferences.getInstance();
    final keychain = nostr.Keychain.generate();
    final nsec = nostr.Nip19.encodePrivkey(keychain.private);
    final crypto = ReadStateCrypto.tryCreate(
      nsec: nsec,
      pubkey: keychain.public,
    );
    final relay = _UnsupportedKindSignedEventRelay();
    final manager = ReadStateManager(
      pubkey: keychain.public,
      prefs: prefs,
      crypto: crypto!,
      relaySession: null,
      signedEventRelay: relay,
      remoteEnabled: true,
      onChanged: () {},
    );

    manager.markContextRead('channel-1', 42);
    await manager.flush();

    manager.markContextRead('channel-2', 43);
    await manager.flush();

    expect(relay.submitCount, 1);
    expect(manager.getEffectiveTimestamp('channel-2'), 43);
  });

  test(
    'disables remote sync after token permanently lacks write scope',
    () async {
      SharedPreferences.setMockInitialValues({});
      final prefs = await SharedPreferences.getInstance();
      final keychain = nostr.Keychain.generate();
      final nsec = nostr.Nip19.encodePrivkey(keychain.private);
      final crypto = ReadStateCrypto.tryCreate(
        nsec: nsec,
        pubkey: keychain.public,
      );
      final relay = _MissingScopeSignedEventRelay();
      final manager = ReadStateManager(
        pubkey: keychain.public,
        prefs: prefs,
        crypto: crypto!,
        relaySession: null,
        signedEventRelay: relay,
        remoteEnabled: true,
        onChanged: () {},
      );

      manager.markContextRead('channel-1', 42);
      await manager.flush();

      manager.markContextRead('channel-2', 43);
      await manager.flush();

      expect(relay.submitCount, 1);
      expect(manager.getEffectiveTimestamp('channel-2'), 43);
    },
  );
}

class _SubmittedEvent {
  final int kind;
  final List<List<String>> tags;

  const _SubmittedEvent({required this.kind, required this.tags});
}

class _FakeSignedEventRelay extends SignedEventRelay {
  final Completer<_SubmittedEvent> submitted = Completer<_SubmittedEvent>();

  _FakeSignedEventRelay()
    : super(client: RelayClient(baseUrl: 'http://localhost:3000'), nsec: null);

  @override
  Future<void> submit({
    required int kind,
    required String content,
    required List<List<String>> tags,
    int? createdAt,
  }) async {
    submitted.complete(_SubmittedEvent(kind: kind, tags: tags));
  }
}

class _UnsupportedKindSignedEventRelay extends SignedEventRelay {
  int submitCount = 0;

  _UnsupportedKindSignedEventRelay()
    : super(client: RelayClient(baseUrl: 'http://localhost:3000'), nsec: null);

  @override
  Future<void> submit({
    required int kind,
    required String content,
    required List<List<String>> tags,
    int? createdAt,
  }) async {
    submitCount++;
    throw RelayException(400, '{"error":"restricted: unknown event kind"}');
  }
}

class _MissingScopeSignedEventRelay extends SignedEventRelay {
  int submitCount = 0;

  _MissingScopeSignedEventRelay()
    : super(client: RelayClient(baseUrl: 'http://localhost:3000'), nsec: null);

  @override
  Future<void> submit({
    required int kind,
    required String content,
    required List<List<String>> tags,
    int? createdAt,
  }) async {
    submitCount++;
    throw RelayException(403, '{"message":"missing users:write"}');
  }
}
