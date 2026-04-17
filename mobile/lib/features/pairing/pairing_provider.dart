import 'dart:async';
import 'dart:convert';
import 'dart:io' show InternetAddress, InternetAddressType;
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:nostr/nostr.dart' as nostr;

import '../../shared/auth/auth.dart';
import '../../shared/crypto/ecdh.dart';
import '../../shared/crypto/nip44.dart';
import '../../shared/relay/relay.dart';
import 'pairing_crypto.dart';
import 'pairing_socket.dart';

/// HTTP client used by [PairingNotifier] for the validation request.
final pairingHttpClientProvider = Provider<http.Client>((ref) {
  final client = http.Client();
  ref.onDispose(client.close);
  return client;
});

enum PairingStatus {
  idle,
  connecting,
  confirmingSas,
  transferring,
  storing,
  success,
  error,
}

class PairingState {
  final PairingStatus status;
  final String? errorMessage;
  final String? sasCode;
  final bool userConfirmedSas;

  const PairingState({
    this.status = PairingStatus.idle,
    this.errorMessage,
    this.sasCode,
    this.userConfirmedSas = false,
  });

  PairingState copyWith({
    PairingStatus? status,
    String? errorMessage,
    String? sasCode,
    bool? userConfirmedSas,
  }) => PairingState(
    status: status ?? this.status,
    errorMessage: errorMessage ?? this.errorMessage,
    sasCode: sasCode ?? this.sasCode,
    userConfirmedSas: userConfirmedSas ?? this.userConfirmedSas,
  );
}

class PairingNotifier extends Notifier<PairingState> {
  PairingSocket? _socket;
  Timer? _sessionTimeout;

  @override
  PairingState build() => const PairingState();

  Future<void> pair(String rawInput) async {
    if (state.status == PairingStatus.connecting ||
        state.status == PairingStatus.confirmingSas ||
        state.status == PairingStatus.transferring) {
      return;
    }

    final trimmed = rawInput.trim();
    if (trimmed.startsWith('nostrpair://')) {
      return _pairNipAb(trimmed);
    }
    // Legacy sprout:// flow.
    return _pairLegacy(trimmed);
  }

  /// Confirm that the SAS code matches. Called by the UI after user approval.
  void confirmSas() {
    if (state.status != PairingStatus.confirmingSas) return;

    // If the desktop's sas-confirm has already arrived and been verified,
    // transition immediately and process any buffered payload.
    if (_sasConfirmReceived) {
      state = state.copyWith(status: PairingStatus.transferring);
      final pending = _pendingPayload;
      if (pending != null) {
        _pendingPayload = null;
        _handlePayload(pending);
      }
      return;
    }

    // Desktop hasn't confirmed yet — record intent and wait. The transition
    // will happen in _handleSasConfirm() once the transcript hash is verified.
    _userConfirmedSas = true;
    state = state.copyWith(userConfirmedSas: true);
  }

  /// Deny the SAS code. Send abort and terminate.
  void denySas() {
    _sendAbort('sas_mismatch');
    _cleanup();
    state = PairingState(
      status: PairingStatus.error,
      errorMessage: 'SAS code mismatch — pairing cancelled for security.',
    );
  }

  void reset() {
    _cleanup();
    state = const PairingState();
  }

  void _cleanup() {
    _sessionTimeout?.cancel();
    _sessionTimeout = null;
    _socket?.dispose();
    _socket = null;
    _processedEventIds.clear();
    _sasConfirmReceived = false;
    _userConfirmedSas = false;
    _pendingPayload = null;
  }

  // ── NIP-AB pairing flow ─────────────────────────────────────────────────

  // Session state kept between steps.
  String? _ephemeralPrivkey;
  String? _ephemeralPubkey;
  Uint8List? _sessionSecret;
  String? _sourcePubkey;
  Uint8List? _sessionId;
  Uint8List? _sasInput;
  Uint8List? _conversationKey;
  bool _sasConfirmReceived = false;
  bool _userConfirmedSas = false;
  Map<String, dynamic>? _pendingPayload; // buffered until user confirms SAS
  final Set<String> _processedEventIds = {}; // NIP-AB §Duplicate Event Handling

  Future<void> _pairNipAb(String uri) async {
    state = const PairingState(status: PairingStatus.connecting);

    try {
      // 1. Parse the nostrpair:// URI.
      final qr = parseNostrpairUri(uri);
      _sourcePubkey = qr.sourcePubkey;
      _sessionSecret = qr.sessionSecret;

      final relayWsUrl = qr.relays.first;

      // 2. Generate ephemeral keypair.
      final keychain = nostr.Keychain.generate();
      _ephemeralPrivkey = keychain.private;
      _ephemeralPubkey = keychain.public;

      // 3. Derive session ID and SAS immediately (we know source pubkey from QR).
      _sessionId = deriveSessionId(qr.sessionSecret);
      final ecdhShared = ecdhSharedSecret(_ephemeralPrivkey!, qr.sourcePubkey);
      final (sasCode, sasInput) = deriveSas(ecdhShared, qr.sessionSecret);
      _sasInput = sasInput;

      // Pre-compute NIP-44 conversation key for encrypting events.
      _conversationKey = getConversationKey(
        _ephemeralPrivkey!,
        qr.sourcePubkey,
      );

      // 4. Connect to relay with ephemeral keys.
      _socket = PairingSocket(
        wsUrl: relayWsUrl,
        ephemeralPrivkey: _ephemeralPrivkey!,
        onMessage: _handleRelayMessage,
        onDisconnected: _handleDisconnected,
      );
      await _socket!.connect();

      if (!_socket!.isConnected) {
        throw Exception('Failed to connect to pairing relay');
      }

      // 5. Subscribe for kind:24134 events tagged to our ephemeral pubkey.
      _socket!.subscribe('pair', 24134, _ephemeralPubkey!);

      // 6. Wait briefly for EOSE, then send offer.
      // (In practice, we send the offer immediately — the relay will buffer it.)
      await Future.delayed(const Duration(milliseconds: 500));

      // 7. Build and send the offer event.
      final offerContent = _encryptMessage({
        'type': 'offer',
        'version': 1,
        'session_id': bytesToHex(_sessionId!),
      });

      _publishEvent(
        kind: 24134,
        content: offerContent,
        tags: [
          ['p', qr.sourcePubkey],
        ],
      );

      // 8. Display SAS code and wait for sas-confirm from source.
      state = PairingState(
        status: PairingStatus.confirmingSas,
        sasCode: formatSas(sasCode),
      );

      // 9. Start 120s session timeout.
      _sessionTimeout = Timer(const Duration(seconds: 120), () {
        if (state.status != PairingStatus.success &&
            state.status != PairingStatus.error) {
          _cleanup();
          state = const PairingState(
            status: PairingStatus.error,
            errorMessage: 'Pairing session timed out.',
          );
        }
      });
    } on FormatException catch (e) {
      _cleanup();
      state = PairingState(
        status: PairingStatus.error,
        errorMessage: 'Invalid pairing code: ${e.message}',
      );
    } catch (e) {
      _cleanup();
      state = PairingState(
        status: PairingStatus.error,
        errorMessage: 'Connection failed: $e',
      );
    }
  }

  void _handleRelayMessage(List<dynamic> data) {
    if (data.isEmpty) return;
    final type = data[0] as String;

    if (type == 'EVENT' && data.length >= 3) {
      final eventJson = data[2] as Map<String, dynamic>;
      _handlePairingEvent(eventJson);
    }
    // Ignore EOSE, NOTICE, etc.
  }

  void _handlePairingEvent(Map<String, dynamic> eventJson) {
    try {
      // NIP-AB §Event Validation: validate kind.
      final kind = eventJson['kind'] as int?;
      if (kind != 24134) return;

      // NIP-AB §Event Validation: validate pubkey is from expected source.
      final eventPubkey = eventJson['pubkey'] as String?;
      if (eventPubkey == null) return;
      if (_sourcePubkey != null && eventPubkey != _sourcePubkey) return;

      // NIP-AB §Duplicate Event Handling: discard already-processed events.
      final eventId = eventJson['id'] as String?;
      if (eventId == null) return;
      if (_processedEventIds.contains(eventId)) return;

      // NIP-AB §Event Validation: check p-tag points to us.
      final tags = (eventJson['tags'] as List<dynamic>?) ?? [];
      final hasOurPTag = tags.any((t) {
        if (t is List && t.length >= 2) {
          return t[0] == 'p' && t[1] == _ephemeralPubkey;
        }
        return false;
      });
      if (!hasOurPTag) return;

      // NIP-AB §Event Validation: verify event signature (NIP-01).
      // The nostr package's Event.fromJson verifies id + sig on construction.
      try {
        final event = nostr.Event.fromJson(eventJson);
        if (event.id != eventId) return; // id mismatch
      } catch (_) {
        return; // invalid signature or malformed event
      }

      // Decrypt NIP-44 content.
      final content = eventJson['content'] as String?;
      if (content == null || content.isEmpty) return;

      final decryptKey = getConversationKey(_ephemeralPrivkey!, eventPubkey);
      final decrypted = nip44Decrypt(decryptKey, content);
      final msg = jsonDecode(decrypted) as Map<String, dynamic>;
      final msgType = msg['type'] as String?;

      switch (msgType) {
        case 'sas-confirm':
          _handleSasConfirm(msg);
          _processedEventIds.add(eventId); // record after successful processing
        case 'payload':
          _handlePayload(msg);
          _processedEventIds.add(eventId);
        case 'abort':
          _handleAbort(msg);
          _processedEventIds.add(eventId);
      }
    } catch (e) {
      // Silently discard invalid events per NIP-AB §Event Validation.
    }
  }

  void _handleSasConfirm(Map<String, dynamic> msg) {
    if (state.status != PairingStatus.confirmingSas) return;

    final receivedHash = msg['transcript_hash'] as String?;
    if (receivedHash == null) return;

    // Verify transcript hash.
    final expectedHash = deriveTranscriptHash(
      _sessionId!,
      hexToBytes(_sourcePubkey!),
      hexToBytes(_ephemeralPubkey!),
      _sasInput!,
      _sessionSecret!,
    );

    final receivedBytes = hexToBytes(receivedHash);
    if (!constantTimeEquals(receivedBytes, expectedHash)) {
      // NIP-AB §Step 3: target MUST send abort with reason "sas_mismatch".
      _sendAbort('sas_mismatch');
      _cleanup();
      state = const PairingState(
        status: PairingStatus.error,
        errorMessage:
            'Security verification failed — possible attack. Pairing aborted.',
      );
      return;
    }

    _sasConfirmReceived = true;

    // If the user already tapped "Codes Match", complete the transition now
    // that the transcript hash is verified.
    if (_userConfirmedSas) {
      _userConfirmedSas = false;
      state = state.copyWith(status: PairingStatus.transferring);
      final pending = _pendingPayload;
      if (pending != null) {
        _pendingPayload = null;
        _handlePayload(pending);
      }
    }
    // Otherwise stay in confirmingSas — user must still confirm via confirmSas().
  }

  void _handlePayload(Map<String, dynamic> msg) {
    // Only accept payload after the transcript hash was verified.
    if (!_sasConfirmReceived) return;

    // If the user hasn't confirmed SAS yet, buffer the payload.
    // It will be processed when confirmSas() is called.
    if (state.status == PairingStatus.confirmingSas) {
      _pendingPayload = msg;
      return;
    }
    if (state.status != PairingStatus.transferring) return;

    state = state.copyWith(status: PairingStatus.storing);

    final payloadType = msg['payload_type'] as String?;
    final payload = msg['payload'] as String?;
    if (payload == null) {
      state = const PairingState(
        status: PairingStatus.error,
        errorMessage: 'Received empty payload from source.',
      );
      return;
    }

    _processPayload(payloadType, payload);
  }

  void _handleAbort(Map<String, dynamic> msg) {
    final reason = msg['reason'] as String? ?? 'unknown';
    _cleanup();
    state = PairingState(
      status: PairingStatus.error,
      errorMessage: 'Source device aborted pairing: $reason',
    );
  }

  Future<void> _processPayload(String? payloadType, String payload) async {
    try {
      // Parse the custom payload.
      final data = jsonDecode(payload) as Map<String, dynamic>;
      final relayUrl = data['relayUrl'] as String?;
      final token = data['token'] as String?;
      final pubkey = data['pubkey'] as String?;
      final nsec = data['nsec'] as String?;

      if (relayUrl == null || token == null) {
        throw const FormatException('Missing relayUrl or token in payload');
      }

      // Validate relay URL to prevent SSRF via private network addresses.
      _validateRelayUrl(relayUrl);

      // Validate credentials against the relay.
      final client = RelayClient(
        baseUrl: relayUrl,
        apiToken: token,
        httpClient: ref.read(pairingHttpClientProvider),
      );
      try {
        await client.get('/api/users/me/profile');
      } finally {
        client.dispose();
      }

      // Send complete only after credentials are validated.
      _sendComplete(true);

      // Store credentials.
      await ref
          .read(authProvider.notifier)
          .authenticate(
            StoredCredentials(
              relayUrl: relayUrl,
              token: token,
              pubkey: pubkey,
              nsec: nsec,
            ),
          );

      _cleanup();
      state = const PairingState(status: PairingStatus.success);
    } catch (e) {
      _sendComplete(false);
      _cleanup();
      state = PairingState(
        status: PairingStatus.error,
        errorMessage: 'Failed to import credentials: $e',
      );
    }
  }

  void _sendAbort(String reason) {
    try {
      final content = _encryptMessage({'type': 'abort', 'reason': reason});
      _publishEvent(
        kind: 24134,
        content: content,
        tags: [
          ['p', _sourcePubkey!],
        ],
      );
    } catch (_) {
      // Best-effort.
    }
  }

  void _sendComplete(bool success) {
    try {
      final content = _encryptMessage({'type': 'complete', 'success': success});
      _publishEvent(
        kind: 24134,
        content: content,
        tags: [
          ['p', _sourcePubkey!],
        ],
      );
    } catch (_) {
      // Best-effort — complete is advisory per NIP-AB.
    }
  }

  /// Encrypt a message using NIP-44 with the ephemeral conversation key.
  String _encryptMessage(Map<String, dynamic> message) {
    final plaintext = jsonEncode(message);
    return nip44Encrypt(_conversationKey!, plaintext);
  }

  /// Build and publish a kind:24134 event signed with ephemeral keys.
  void _publishEvent({
    required int kind,
    required String content,
    required List<List<String>> tags,
  }) {
    // Add timestamp jitter (0-30s) for metadata privacy.
    final jitter = math.Random.secure().nextInt(31);
    final createdAt = (DateTime.now().millisecondsSinceEpoch ~/ 1000) - jitter;

    final event = nostr.Event.from(
      kind: kind,
      content: content,
      tags: tags,
      privkey: _ephemeralPrivkey!,
      createdAt: createdAt,
    );

    _socket?.publishEvent(event.toJson());
  }

  void _handleDisconnected(Object? error) {
    if (state.status == PairingStatus.success ||
        state.status == PairingStatus.error) {
      return;
    }
    _cleanup();
    state = PairingState(
      status: PairingStatus.error,
      errorMessage: 'Lost connection to pairing relay.',
    );
  }

  // ── Legacy sprout:// flow ───────────────────────────────────────────────

  Future<void> _pairLegacy(String rawInput) async {
    state = const PairingState(status: PairingStatus.connecting);

    try {
      final creds = _parseLegacyInput(rawInput);

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

  StoredCredentials _parseLegacyInput(String raw) {
    var payload = raw.trim();

    if (payload.startsWith('sprout://')) {
      payload = payload.substring('sprout://'.length);
    }

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

  void _validateRelayUrl(String url) {
    final uri = Uri.parse(url);

    if (!kDebugMode && uri.scheme != 'https') {
      throw const FormatException('Relay URL must use HTTPS');
    }
    if (uri.scheme != 'http' && uri.scheme != 'https') {
      throw FormatException('Invalid URL scheme: ${uri.scheme}');
    }

    final host = uri.host.toLowerCase();

    // Localhost check: allow in debug, block in production.
    // DNS rebinding is mitigated by the HTTPS requirement in production —
    // the TLS certificate pins the hostname to the server, so a rebind to a
    // private address will fail the certificate check before any data is sent.
    if (host == 'localhost') {
      if (!kDebugMode) {
        throw const FormatException('Relay URL cannot target localhost');
      }
      return;
    }

    if (_isPrivateHost(host)) {
      throw const FormatException(
        'Relay URL cannot target private network addresses',
      );
    }
  }

  /// Returns true if [host] resolves to a private, loopback, link-local,
  /// or unspecified address that must not be reachable from a relay URL.
  ///
  /// Uses [InternetAddress.tryParse] to normalise all IP representations
  /// (dotted-quad, decimal, octal, hex, IPv6, IPv4-mapped IPv6) before
  /// checking ranges, so alternative encodings cannot bypass the filter.
  static bool _isPrivateHost(String host) {
    // Strip IPv6 brackets if present (e.g. "[::1]" → "::1").
    final bare = host.startsWith('[') && host.endsWith(']')
        ? host.substring(1, host.length - 1)
        : host;

    final addr = InternetAddress.tryParse(bare);
    if (addr == null) {
      // Not a bare IP literal — it's a hostname; let DNS + TLS handle it.
      return false;
    }

    if (addr.type == InternetAddressType.IPv4) {
      return _isPrivateIPv4(addr.rawAddress);
    }

    // IPv6
    final raw = addr.rawAddress; // 16 bytes, big-endian
    // Unspecified: ::
    if (raw.every((b) => b == 0)) return true;
    // Loopback: ::1
    if (addr.isLoopback) return true;
    // Link-local: fe80::/10 — first 10 bits are 1111 1110 10
    if (raw[0] == 0xfe && (raw[1] & 0xc0) == 0x80) return true;
    // Unique local: fc00::/7 — first 7 bits are 1111 110
    if ((raw[0] & 0xfe) == 0xfc) return true;
    // IPv4-mapped: ::ffff:x.x.x.x — bytes 0-9 are 0, bytes 10-11 are 0xff
    if (raw[10] == 0xff &&
        raw[11] == 0xff &&
        raw.sublist(0, 10).every((b) => b == 0)) {
      return _isPrivateIPv4(raw.sublist(12));
    }

    return false;
  }

  /// Checks whether a 4-byte big-endian IPv4 address is private/reserved.
  static bool _isPrivateIPv4(List<int> raw) {
    final a = raw[0];
    final b = raw[1];
    if (a == 0) return true; // 0.0.0.0/8 unspecified
    if (a == 10) return true; // 10.0.0.0/8
    if (a == 127) return true; // 127.0.0.0/8 loopback
    if (a == 169 && b == 254) return true; // 169.254.0.0/16 link-local
    if (a == 172 && b >= 16 && b <= 31) return true; // 172.16.0.0/12
    if (a == 192 && b == 168) return true; // 192.168.0.0/16
    return false;
  }
}

final pairingProvider = NotifierProvider<PairingNotifier, PairingState>(
  PairingNotifier.new,
);
