import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import '../channels/channel_management_provider.dart';
import 'forum_models.dart';

/// Fetches forum posts for a channel from the REST API.
///
/// Posts are top-level kind-45001 events with thread summaries.
/// Invalidate to refresh (e.g. after creating a new post).
final forumPostsProvider = FutureProvider.family<ForumPostsResponse, String>((
  ref,
  channelId,
) async {
  final client = ref.watch(relayClientProvider);
  final json =
      await client.get(
            '/api/channels/$channelId/messages',
            queryParams: {'kinds': '${EventKind.forumPost}', 'limit': '50'},
          )
          as Map<String, dynamic>;
  return ForumPostsResponse.fromJson(json);
});

/// Fetches a forum thread (root post + replies) from the REST API.
final forumThreadProvider =
    FutureProvider.family<
      ForumThreadResponse,
      ({String channelId, String eventId})
    >((ref, args) async {
      final client = ref.watch(relayClientProvider);
      final json =
          await client.get(
                '/api/channels/${args.channelId}/threads/${args.eventId}',
                queryParams: {'limit': '100'},
              )
              as Map<String, dynamic>;
      return ForumThreadResponse.fromJson(json);
    });

/// Creates a new forum post (kind 45001).
Future<void> createForumPost(
  WidgetRef ref, {
  required String channelId,
  required String content,
  List<String> mentionPubkeys = const [],
  List<List<String>> mediaTags = const [],
}) async {
  final config = ref.read(relayConfigProvider);
  final client = ref.read(relayClientProvider);
  final relay = SignedEventRelay(client: client, nsec: config.nsec);

  final selfPubkey = relay.pubkey?.toLowerCase();
  final seen = <String>{?selfPubkey};
  final normalizedMentions = [
    for (final pk in mentionPubkeys)
      if (seen.add(pk.toLowerCase())) pk,
  ];

  await relay.submit(
    kind: EventKind.forumPost,
    content: content,
    tags: [
      ['h', channelId],
      for (final pk in normalizedMentions) ['p', pk],
      ...mediaTags,
    ],
  );
  ref.invalidate(forumPostsProvider(channelId));
}

/// Creates a reply to a forum post (kind 45003).
Future<void> createForumReply(
  WidgetRef ref, {
  required String channelId,
  required String parentEventId,
  required String content,
  List<String> mentionPubkeys = const [],
  List<List<String>> mediaTags = const [],
}) async {
  final config = ref.read(relayConfigProvider);
  final client = ref.read(relayClientProvider);
  final relay = SignedEventRelay(client: client, nsec: config.nsec);

  final selfPubkey = relay.pubkey?.toLowerCase();
  final seen = <String>{?selfPubkey};
  final normalizedMentions = [
    for (final pk in mentionPubkeys)
      if (seen.add(pk.toLowerCase())) pk,
  ];

  await relay.submit(
    kind: EventKind.forumComment,
    content: content,
    tags: [
      ['h', channelId],
      ['e', parentEventId, '', 'reply'],
      for (final pk in normalizedMentions) ['p', pk],
      ...mediaTags,
    ],
  );
  ref.invalidate(forumPostsProvider(channelId));
  ref.invalidate(
    forumThreadProvider((channelId: channelId, eventId: parentEventId)),
  );
}

/// Deletes a forum post or reply and invalidates relevant caches.
Future<void> deleteForumEvent(
  WidgetRef ref, {
  required String channelId,
  required String eventId,
  String? rootEventId,
}) async {
  final actions = ref.read(channelActionsProvider);
  await actions.deleteMessage(eventId);
  ref.invalidate(forumPostsProvider(channelId));
  if (rootEventId != null) {
    ref.invalidate(
      forumThreadProvider((channelId: channelId, eventId: rootEventId)),
    );
  }
}
