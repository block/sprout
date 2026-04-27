import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../workspace/workspace_provider.dart';
import 'relay_client.dart';

/// Relay connection configuration.
class RelayConfig {
  final String baseUrl;
  final String? apiToken;

  /// Hex pubkey for dev-mode auth via X-Pubkey header.
  /// Used when the relay has `SPROUT_REQUIRE_AUTH_TOKEN=false`.
  final String? devPubkey;

  /// Nostr secret key (bech32 nsec) for signing NIP-42 AUTH events.
  final String? nsec;

  const RelayConfig({
    required this.baseUrl,
    this.apiToken,
    this.devPubkey,
    this.nsec,
  });

  /// Derive the websocket URL from the HTTP base URL.
  String get wsUrl {
    final uri = Uri.parse(baseUrl);
    final scheme = uri.scheme == 'https' ? 'wss' : 'ws';
    return uri.replace(scheme: scheme).toString();
  }
}

/// Compile-time environment config via --dart-define.
///
/// Run with:
///   flutter run \
///     --dart-define=SPROUT_RELAY_URL=http://localhost:3000 \
///     --dart-define=SPROUT_DEV_PUBKEY=5e58f620... \
///     --dart-define=SPROUT_API_TOKEN=sprout_...
///
/// Or create a `.env.json` and use --dart-define-from-file=.env.json
class Env {
  static const relayUrl = String.fromEnvironment(
    'SPROUT_RELAY_URL',
    defaultValue: 'http://localhost:3000',
  );
  static const devPubkey = String.fromEnvironment('SPROUT_DEV_PUBKEY');
  static const apiToken = String.fromEnvironment('SPROUT_API_TOKEN');
}

class RelayConfigNotifier extends Notifier<RelayConfig> {
  @override
  RelayConfig build() {
    // Watch the active workspace so that when it changes (workspace switch),
    // the config rebuilds, triggering the full provider cascade.
    final activeAsync = ref.watch(activeWorkspaceProvider);
    final active = activeAsync.value;
    if (active != null) {
      return RelayConfig(
        baseUrl: active.relayUrl,
        apiToken: active.token,
        nsec: active.nsec,
      );
    }

    // Fallback to compile-time env config (dev mode).
    return RelayConfig(
      baseUrl: Env.relayUrl,
      apiToken: Env.apiToken.isEmpty ? null : Env.apiToken,
      devPubkey: Env.devPubkey.isEmpty ? null : Env.devPubkey,
    );
  }

  void update({
    required String baseUrl,
    required String? apiToken,
    required String? devPubkey,
    String? nsec,
  }) {
    state = RelayConfig(
      baseUrl: baseUrl,
      apiToken: apiToken,
      devPubkey: devPubkey,
      nsec: nsec,
    );
  }
}

final relayConfigProvider = NotifierProvider<RelayConfigNotifier, RelayConfig>(
  RelayConfigNotifier.new,
);

/// Provides a [RelayClient] that reacts to config changes.
final relayClientProvider = Provider<RelayClient>((ref) {
  final config = ref.watch(relayConfigProvider);
  final client = RelayClient(
    baseUrl: config.baseUrl,
    apiToken: config.apiToken,
    devPubkey: config.devPubkey,
  );
  ref.onDispose(client.dispose);
  return client;
});
