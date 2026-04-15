import 'dart:convert';

import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:nostr/nostr.dart' as nostr;

import '../../shared/relay/relay.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';

/// Sends messages via the relay HTTP API. The event is signed on the client
/// with the user's nsec, then POSTed as a full signed Nostr event — matching
/// what the desktop does via `submit_event`.
class SendMessage {
  final RelayClient _client;
  final String? _nsec;
  final Map<String, UserProfile> Function() _readUserCache;

  SendMessage({
    required RelayClient client,
    required String? nsec,
    required Map<String, UserProfile> Function() readUserCache,
  }) : _client = client,
       _nsec = nsec,
       _readUserCache = readUserCache;

  /// Send a text message to a channel.
  Future<void> call({
    required String channelId,
    required String content,
    String? parentEventId,
    List<String>? mentionPubkeys,
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

    // Resolve @mentions in the message content to pubkeys.
    final resolvedMentions = mentionPubkeys ?? _resolveMentions(content);

    final tags = <List<String>>[
      ['h', channelId],
      if (parentEventId != null) ['e', parentEventId],
      for (final pk in resolvedMentions) ['p', pk],
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

  /// Parse @mentions from message content and resolve to pubkeys using
  /// the user cache. Reads the cache at call time (not construction time)
  /// to ensure freshly loaded profiles are available.
  List<String> _resolveMentions(String content) {
    final mentionPattern = RegExp(r'@(\w+)');
    final matches = mentionPattern.allMatches(content);
    final pubkeys = <String>{};

    // Read the current cache state at send time.
    final cache = _readUserCache();

    for (final match in matches) {
      final name = match.group(1)?.toLowerCase();
      if (name == null || name.isEmpty) continue;

      for (final profile in cache.values) {
        final displayName = profile.displayName?.toLowerCase();
        if (displayName == null) continue;

        // Match against full display name or first word of it.
        final firstName = displayName.split(RegExp(r'\s+')).first;
        if (displayName == name || firstName == name) {
          pubkeys.add(profile.pubkey);
          break;
        }
      }
    }

    return pubkeys.toList();
  }
}

final sendMessageProvider = Provider<SendMessage>((ref) {
  final client = ref.watch(relayClientProvider);
  final config = ref.watch(relayConfigProvider);
  return SendMessage(
    client: client,
    nsec: config.nsec,
    readUserCache: () => ref.read(userCacheProvider),
  );
});
