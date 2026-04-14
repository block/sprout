import 'package:flutter/foundation.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../relay/relay.dart';
import 'credential_storage.dart';

enum AuthStatus { unknown, unauthenticated, authenticated, offline }

class AuthState {
  final AuthStatus status;
  final StoredCredentials? credentials;

  const AuthState({required this.status, this.credentials});
}

class AuthNotifier extends AsyncNotifier<AuthState> {
  @override
  Future<AuthState> build() async {
    // If a token is provided via dart-define in debug mode, treat as
    // pre-authenticated.
    if (kDebugMode && Env.apiToken.isNotEmpty) {
      return const AuthState(status: AuthStatus.authenticated);
    }

    final storage = CredentialStorage();
    final creds = await storage.load();
    if (creds == null) {
      return const AuthState(status: AuthStatus.unauthenticated);
    }

    // Validate stored credentials against the relay.
    final client = RelayClient(baseUrl: creds.relayUrl, apiToken: creds.token);
    try {
      await client.get('/api/users/me/profile');
      // Credentials are valid — propagate to relay config.
      ref
          .read(relayConfigProvider.notifier)
          .update(
            baseUrl: creds.relayUrl,
            apiToken: creds.token,
            devPubkey: null,
          );
      return AuthState(status: AuthStatus.authenticated, credentials: creds);
    } on RelayException {
      // Token is invalid or expired — clear and require re-pairing.
      await storage.clear();
      return const AuthState(status: AuthStatus.unauthenticated);
    } catch (_) {
      // Network error — keep credentials and let the user retry.
      return AuthState(status: AuthStatus.offline, credentials: creds);
    } finally {
      client.dispose();
    }
  }

  /// Retry credential validation (e.g. after a network error).
  Future<void> retry() async {
    ref.invalidateSelf();
    await future;
  }

  Future<void> authenticate(StoredCredentials creds) async {
    final storage = CredentialStorage();
    await storage.save(creds);

    ref
        .read(relayConfigProvider.notifier)
        .update(
          baseUrl: creds.relayUrl,
          apiToken: creds.token,
          devPubkey: null,
        );

    state = AsyncData(
      AuthState(status: AuthStatus.authenticated, credentials: creds),
    );
  }

  Future<void> signOut() async {
    final storage = CredentialStorage();
    await storage.clear();
    state = const AsyncData(AuthState(status: AuthStatus.unauthenticated));
  }
}

final authProvider = AsyncNotifierProvider<AuthNotifier, AuthState>(
  AuthNotifier.new,
);
