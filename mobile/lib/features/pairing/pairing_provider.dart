import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:http/http.dart' as http;

import '../../shared/auth/auth.dart';
import '../../shared/relay/relay.dart';

/// HTTP client used by [PairingNotifier] for the validation request.
/// Override in tests to inject a mock client.
final pairingHttpClientProvider = Provider<http.Client>((ref) {
  final client = http.Client();
  ref.onDispose(client.close);
  return client;
});

enum PairingStatus { idle, connecting, success, error }

class PairingState {
  final PairingStatus status;
  final String? errorMessage;

  const PairingState({this.status = PairingStatus.idle, this.errorMessage});

  PairingState copyWith({PairingStatus? status, String? errorMessage}) =>
      PairingState(
        status: status ?? this.status,
        errorMessage: errorMessage ?? this.errorMessage,
      );
}

class PairingNotifier extends Notifier<PairingState> {
  @override
  PairingState build() => const PairingState();

  Future<void> pair(String rawInput) async {
    if (state.status == PairingStatus.connecting) return;

    state = const PairingState(status: PairingStatus.connecting);

    try {
      final creds = _parseInput(rawInput);

      // Test the connection before storing.
      final client = RelayClient(
        baseUrl: creds.relayUrl,
        apiToken: creds.token,
        httpClient: ref.read(pairingHttpClientProvider),
      );
      try {
        await client.get('/api/users/me/profile');
      } finally {
        client.dispose();
      }

      await ref.read(authProvider.notifier).authenticate(creds);
      state = const PairingState(status: PairingStatus.success);
    } on FormatException catch (e) {
      state = PairingState(
        status: PairingStatus.error,
        errorMessage: 'Invalid pairing code: ${e.message}',
      );
    } on RelayException catch (e) {
      state = PairingState(
        status: PairingStatus.error,
        errorMessage:
            'Could not connect to relay (${e.statusCode}). '
            'Check that the pairing code is valid.',
      );
    } catch (e) {
      state = PairingState(
        status: PairingStatus.error,
        errorMessage:
            'Connection failed. Make sure your device can reach the '
            'relay server.',
      );
    }
  }

  void reset() {
    state = const PairingState();
  }

  StoredCredentials _parseInput(String raw) {
    var payload = raw.trim();

    if (payload.startsWith('sprout://')) {
      payload = payload.substring('sprout://'.length);
    }

    // base64url decode.
    final normalized = base64Url.normalize(payload);
    final jsonStr = utf8.decode(base64Url.decode(normalized));
    final decoded = jsonDecode(jsonStr);
    if (decoded is! Map<String, dynamic>) {
      throw const FormatException('Pairing payload is not a JSON object');
    }

    final relayUrl = decoded['relayUrl'] as String?;
    final token = decoded['token'] as String?;
    if (relayUrl == null || token == null) {
      throw const FormatException('Missing relayUrl or token');
    }

    _validateRelayUrl(relayUrl);

    return StoredCredentials(
      relayUrl: relayUrl,
      token: token,
      pubkey: decoded['pubkey'] as String?,
      nsec: decoded['nsec'] as String?,
    );
  }

  /// Reject relay URLs that aren't HTTPS (unless debug mode) or target
  /// private/link-local addresses.
  void _validateRelayUrl(String url) {
    final uri = Uri.parse(url);

    if (!kDebugMode && uri.scheme != 'https') {
      throw const FormatException('Relay URL must use HTTPS');
    }
    if (uri.scheme != 'http' && uri.scheme != 'https') {
      throw FormatException('Invalid URL scheme: ${uri.scheme}');
    }

    final host = uri.host.toLowerCase();
    if (host == 'localhost' || host == '127.0.0.1' || host == '::1') {
      if (!kDebugMode) {
        throw const FormatException('Relay URL cannot target localhost');
      }
      return;
    }

    // Block private and link-local IPs.
    final ip = Uri.tryParse('http://$host')?.host ?? host;
    if (_isPrivateHost(ip)) {
      throw const FormatException(
        'Relay URL cannot target private network addresses',
      );
    }
  }

  static bool _isPrivateHost(String host) {
    final parts = host.split('.');
    if (parts.length != 4) return false;
    final octets = parts.map(int.tryParse).toList();
    if (octets.any((o) => o == null)) return false;

    final a = octets[0]!;
    final b = octets[1]!;

    // 10.0.0.0/8
    if (a == 10) return true;
    // 172.16.0.0/12
    if (a == 172 && b >= 16 && b <= 31) return true;
    // 192.168.0.0/16
    if (a == 192 && b == 168) return true;
    // 169.254.0.0/16 (link-local / cloud metadata)
    if (a == 169 && b == 254) return true;
    return false;
  }
}

final pairingProvider = NotifierProvider<PairingNotifier, PairingState>(
  PairingNotifier.new,
);
