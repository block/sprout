import 'package:hooks_riverpod/hooks_riverpod.dart';

import 'relay_client.dart';

/// Relay connection configuration.
class RelayConfig {
  final String baseUrl;
  final String? apiToken;

  /// Hex pubkey for dev-mode auth via X-Pubkey header.
  /// Used when the relay has `SPROUT_REQUIRE_AUTH_TOKEN=false`.
  final String? devPubkey;

  const RelayConfig({required this.baseUrl, this.apiToken, this.devPubkey});
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
  RelayConfig build() => RelayConfig(
    baseUrl: Env.relayUrl,
    apiToken: Env.apiToken.isEmpty ? null : Env.apiToken,
    devPubkey: Env.devPubkey.isEmpty ? null : Env.devPubkey,
  );

  void update({
    required String baseUrl,
    required String? apiToken,
    required String? devPubkey,
  }) {
    state = RelayConfig(
      baseUrl: baseUrl,
      apiToken: apiToken,
      devPubkey: devPubkey,
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
