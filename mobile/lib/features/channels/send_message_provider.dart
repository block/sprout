import 'dart:convert';

import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:nostr/nostr.dart' as nostr;

import '../../shared/relay/relay.dart';

/// Sends messages via the relay HTTP API. The event is signed on the client
/// with the user's nsec, then POSTed as a full signed Nostr event — matching
/// what the desktop does via `submit_event`.
class SendMessage {
  final RelayClient _client;
  final String? _nsec;

  SendMessage({required RelayClient client, required String? nsec})
    : _client = client,
      _nsec = nsec;

  /// Send a text message to a channel.
  Future<void> call({
    required String channelId,
    required String content,
    String? parentEventId,
  }) async {
    final nsec = _nsec;
    if (nsec == null || nsec.isEmpty) {
      throw Exception('Cannot send messages: no signing key available');
    }

    // Decode bech32 nsec to hex private key.
    final privkeyHex = nostr.Nip19.decodePrivkey(nsec);
    if (privkeyHex.isEmpty) {
      throw Exception('Invalid nsec');
    }

    final tags = <List<String>>[
      ['h', channelId],
      if (parentEventId != null) ['e', parentEventId],
    ];

    // Create and sign the event using the nostr package.
    final event = nostr.Event.from(
      kind: EventKind.streamMessage,
      content: content,
      tags: tags,
      privkey: privkeyHex,
      verify: false,
    );

    // POST the full signed event JSON to the relay.
    final response = await _client.postRaw(
      '/api/events',
      body: jsonEncode(event.toJson()),
    );

    final result = jsonDecode(response) as Map<String, dynamic>;
    if (result['accepted'] != true) {
      throw Exception(result['message'] ?? 'Event rejected by relay');
    }
  }
}

final sendMessageProvider = Provider<SendMessage>((ref) {
  final client = ref.watch(relayClientProvider);
  final config = ref.watch(relayConfigProvider);
  return SendMessage(client: client, nsec: config.nsec);
});
