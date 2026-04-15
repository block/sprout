import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart' as http_testing;
import 'package:sprout_mobile/features/pairing/pairing_provider.dart';
import 'package:sprout_mobile/shared/auth/auth.dart';

/// Encode a credentials payload the same way the desktop app would.
String _encodePairingCode({
  String relayUrl = 'http://test:3000',
  String token = 'sprout_test_token',
  String? pubkey,
}) {
  final json = <String, dynamic>{
    'relayUrl': relayUrl,
    'token': token,
    // ignore: use_null_aware_elements
    if (pubkey != null) 'pubkey': pubkey,
  };
  return base64Url.encode(utf8.encode(jsonEncode(json)));
}

/// A fake [AuthNotifier] that records calls instead of touching secure storage.
class FakeAuthNotifier extends AsyncNotifier<AuthState>
    implements AuthNotifier {
  StoredCredentials? lastCredentials;
  bool signedOut = false;

  @override
  Future<AuthState> build() async =>
      const AuthState(status: AuthStatus.unauthenticated);

  @override
  Future<void> authenticate(StoredCredentials creds) async {
    lastCredentials = creds;
    state = AsyncData(
      AuthState(status: AuthStatus.authenticated, credentials: creds),
    );
  }

  @override
  Future<void> signOut() async {
    signedOut = true;
    state = const AsyncData(AuthState(status: AuthStatus.unauthenticated));
  }

  @override
  Future<void> retry() async {}
}

void main() {
  group('PairingNotifier', () {
    late ProviderContainer container;
    late FakeAuthNotifier fakeAuth;

    /// Creates a container with the HTTP client wired to [mockClient].
    ProviderContainer createContainer(http_testing.MockClient mockClient) {
      fakeAuth = FakeAuthNotifier();
      return ProviderContainer(
        overrides: [
          authProvider.overrideWith(() => fakeAuth),
          pairingHttpClientProvider.overrideWithValue(mockClient),
        ],
      );
    }

    tearDown(() => container.dispose());

    test('starts in idle state', () {
      final mock = http_testing.MockClient(
        (_) async => http.Response('{}', 200),
      );
      container = createContainer(mock);
      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.idle);
      expect(state.errorMessage, isNull);
    });

    test('successful pairing with raw base64 code', () async {
      final mock = http_testing.MockClient((request) async {
        expect(request.url.path, '/api/users/me/profile');
        return http.Response(jsonEncode({'id': '1', 'name': 'Wes'}), 200);
      });
      container = createContainer(mock);

      final code = _encodePairingCode();
      await container.read(pairingProvider.notifier).pair(code);

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.success);
      expect(fakeAuth.lastCredentials?.relayUrl, 'http://test:3000');
      expect(fakeAuth.lastCredentials?.token, 'sprout_test_token');
    });

    test('successful pairing with sprout:// prefix', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response(jsonEncode({'id': '1'}), 200),
      );
      container = createContainer(mock);

      final code = 'sprout://${_encodePairingCode()}';
      await container.read(pairingProvider.notifier).pair(code);

      expect(container.read(pairingProvider).status, PairingStatus.success);
    });

    test('successful pairing with pubkey', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response(jsonEncode({'id': '1'}), 200),
      );
      container = createContainer(mock);

      final code = _encodePairingCode(pubkey: 'abc123');
      await container.read(pairingProvider.notifier).pair(code);

      expect(container.read(pairingProvider).status, PairingStatus.success);
      expect(fakeAuth.lastCredentials?.pubkey, 'abc123');
    });

    test('relay 401 sets error state', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response('{"error": "unauthorized"}', 401),
      );
      container = createContainer(mock);

      final code = _encodePairingCode();
      await container.read(pairingProvider.notifier).pair(code);

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.error);
      expect(state.errorMessage, contains('Could not connect to relay'));
      expect(state.errorMessage, contains('401'));
    });

    test('relay 500 sets error state', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response('internal error', 500),
      );
      container = createContainer(mock);

      final code = _encodePairingCode();
      await container.read(pairingProvider.notifier).pair(code);

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.error);
      expect(state.errorMessage, contains('500'));
    });

    test('network error sets generic error', () async {
      final mock = http_testing.MockClient(
        (_) => throw Exception('no internet'),
      );
      container = createContainer(mock);

      final code = _encodePairingCode();
      await container.read(pairingProvider.notifier).pair(code);

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.error);
      expect(state.errorMessage, contains('Connection failed'));
    });

    test('invalid base64 sets format error', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response('{}', 200),
      );
      container = createContainer(mock);

      await container.read(pairingProvider.notifier).pair('not-valid!!!');

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.error);
      expect(state.errorMessage, contains('Invalid pairing code'));
    });

    test('base64 with valid JSON but missing fields errors', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response('{}', 200),
      );
      container = createContainer(mock);

      // Valid base64 JSON, but no relayUrl/token keys.
      final code = base64Url.encode(utf8.encode(jsonEncode({'foo': 'bar'})));
      await container.read(pairingProvider.notifier).pair(code);

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.error);
      expect(state.errorMessage, contains('Missing relayUrl or token'));
    });

    test('empty input errors', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response('{}', 200),
      );
      container = createContainer(mock);

      await container.read(pairingProvider.notifier).pair('');

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.error);
    });

    test('whitespace-padded input is trimmed', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response(jsonEncode({'id': '1'}), 200),
      );
      container = createContainer(mock);

      final code = '  ${_encodePairingCode()}  \n';
      await container.read(pairingProvider.notifier).pair(code);

      expect(container.read(pairingProvider).status, PairingStatus.success);
    });

    test('rejects private IP relay URLs (SSRF)', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response('{}', 200),
      );
      container = createContainer(mock);

      for (final ip in [
        '10.0.0.1',
        '172.16.0.1',
        '192.168.1.1',
        '169.254.169.254',
      ]) {
        final code = _encodePairingCode(relayUrl: 'http://$ip:3000');
        await container.read(pairingProvider.notifier).pair(code);
        final state = container.read(pairingProvider);
        expect(state.status, PairingStatus.error, reason: 'should reject $ip');
        expect(state.errorMessage, contains('private network'));
        container.read(pairingProvider.notifier).reset();
      }
    });

    test('rejects non-http/https schemes', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response('{}', 200),
      );
      container = createContainer(mock);

      final code = _encodePairingCode(relayUrl: 'file:///etc/passwd');
      await container.read(pairingProvider.notifier).pair(code);

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.error);
      expect(state.errorMessage, contains('Invalid pairing code'));
    });

    test('rejects JSON array payload', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response('{}', 200),
      );
      container = createContainer(mock);

      final code = base64Url.encode(utf8.encode(jsonEncode([1, 2, 3])));
      await container.read(pairingProvider.notifier).pair(code);

      final state = container.read(pairingProvider);
      expect(state.status, PairingStatus.error);
      expect(state.errorMessage, contains('not a JSON object'));
    });

    test('ignores duplicate pair() calls while connecting', () async {
      var requestCount = 0;
      final mock = http_testing.MockClient((_) async {
        requestCount++;
        // Simulate slow network.
        await Future<void>.delayed(const Duration(milliseconds: 50));
        return http.Response(jsonEncode({'id': '1'}), 200);
      });
      container = createContainer(mock);

      final code = _encodePairingCode();
      // Fire two calls without awaiting the first.
      final f1 = container.read(pairingProvider.notifier).pair(code);
      final f2 = container.read(pairingProvider.notifier).pair(code);
      await Future.wait([f1, f2]);

      // Only one network request should have been made.
      expect(requestCount, 1);
    });

    test('reset returns to idle', () async {
      final mock = http_testing.MockClient(
        (_) async => http.Response(jsonEncode({'id': '1'}), 200),
      );
      container = createContainer(mock);

      await container.read(pairingProvider.notifier).pair(_encodePairingCode());
      expect(container.read(pairingProvider).status, PairingStatus.success);

      container.read(pairingProvider.notifier).reset();
      expect(container.read(pairingProvider).status, PairingStatus.idle);
    });
  });
}
