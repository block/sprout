import 'dart:convert';

import 'package:nostr/nostr.dart' as nostr;

import 'relay_client.dart';

/// Signs and submits Nostr events through the relay HTTP API.
class SignedEventRelay {
  final RelayClient _client;
  final String? _nsec;

  SignedEventRelay({required RelayClient client, required String? nsec})
    : _client = client,
      _nsec = nsec;

  /// The hex pubkey derived from the signing key, or null if no key.
  String? get pubkey {
    final nsec = _nsec;
    if (nsec == null || nsec.isEmpty) return null;
    final privkeyHex = nostr.Nip19.decodePrivkey(nsec);
    if (privkeyHex.isEmpty) return null;
    return nostr.Keychain(privkeyHex).public;
  }

  Future<void> submit({
    required int kind,
    required String content,
    required List<List<String>> tags,
  }) async {
    final nsec = _nsec;
    if (nsec == null || nsec.isEmpty) {
      throw Exception('Cannot submit event: no signing key available');
    }

    final privkeyHex = nostr.Nip19.decodePrivkey(nsec);
    if (privkeyHex.isEmpty) {
      throw Exception('Invalid nsec');
    }

    final event = nostr.Event.from(
      kind: kind,
      content: content,
      tags: tags,
      privkey: privkeyHex,
      verify: false,
    );

    final response = await _client.postRaw(
      '/api/events',
      body: jsonEncode(event.toJson()),
    );
    final payload = jsonDecode(response) as Map<String, dynamic>;
    if (payload['accepted'] != true) {
      throw Exception(payload['message'] ?? 'Event rejected by relay');
    }
  }
}
