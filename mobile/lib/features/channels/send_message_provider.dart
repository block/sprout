import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';

/// Sends messages via the relay HTTP API. The event is signed on the client
/// with the user's nsec, then POSTed as a full signed Nostr event — matching
/// what the desktop does via `submit_event`.
class SendMessage {
  final SignedEventRelay _signedEventRelay;
  final Map<String, UserProfile> Function() _readUserCache;

  SendMessage({
    required SignedEventRelay signedEventRelay,
    required Map<String, UserProfile> Function() readUserCache,
  }) : _signedEventRelay = signedEventRelay,
       _readUserCache = readUserCache;

  /// Send a text message to a channel.
  ///
  /// For thread replies, pass [parentEventId] and optionally [rootEventId].
  /// If [rootEventId] is null it defaults to [parentEventId] (direct reply to
  /// thread head). Tags are built to match the desktop's `buildReplyTags`
  /// convention with `root` / `reply` markers.
  Future<void> call({
    required String channelId,
    required String content,
    String? parentEventId,
    String? rootEventId,
    List<String>? mentionPubkeys,
  }) async {
    // Resolve @mentions in the message content to pubkeys.
    final resolvedMentions = mentionPubkeys ?? _resolveMentions(content);
    final authorPubkey = _signedEventRelay.pubkey;

    // Normalize mentions: lowercase, deduplicate, exclude self (matching
    // the desktop's normalizeMentionPubkeys).
    final selfLower = authorPubkey?.toLowerCase();
    final seenMentions = <String>{?selfLower};
    final normalizedMentions = <String>[
      for (final pk in resolvedMentions)
        if (seenMentions.add(pk.toLowerCase())) pk,
    ];

    final tags = <List<String>>[
      ['h', channelId],
      if (parentEventId != null) ..._buildReplyTags(parentEventId, rootEventId),
      for (final pk in normalizedMentions) ['p', pk],
    ];

    await _signedEventRelay.submit(
      kind: EventKind.streamMessage,
      content: content,
      tags: tags,
    );
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

  /// Build `e`-tags for a thread reply, matching the desktop convention:
  /// - Direct reply to thread head: `["e", id, "", "reply"]`
  /// - Nested reply: `["e", rootId, "", "root"]` + `["e", parentId, "", "reply"]`
  static List<List<String>> _buildReplyTags(
    String parentEventId,
    String? rootEventId,
  ) {
    final root = rootEventId ?? parentEventId;
    if (parentEventId == root) {
      return [
        ['e', root, '', 'reply'],
      ];
    }
    return [
      ['e', root, '', 'root'],
      ['e', parentEventId, '', 'reply'],
    ];
  }
}

final sendMessageProvider = Provider<SendMessage>((ref) {
  final config = ref.watch(relayConfigProvider);
  return SendMessage(
    signedEventRelay: SignedEventRelay(
      client: ref.watch(relayClientProvider),
      nsec: config.nsec,
    ),
    readUserCache: () => ref.read(userCacheProvider),
  );
});
