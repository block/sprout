import 'package:flutter/foundation.dart';

/// A top-level forum post with an optional thread summary.
@immutable
class ForumPost {
  final String eventId;
  final String pubkey;
  final String content;
  final int kind;
  final int createdAt;
  final String channelId;
  final List<List<String>> tags;
  final ForumThreadSummary? threadSummary;

  const ForumPost({
    required this.eventId,
    required this.pubkey,
    required this.content,
    required this.kind,
    required this.createdAt,
    required this.channelId,
    required this.tags,
    this.threadSummary,
  });

  factory ForumPost.fromJson(Map<String, dynamic> json) {
    final rawSummary = json['thread_summary'] as Map<String, dynamic>?;
    return ForumPost(
      eventId: json['event_id'] as String,
      pubkey: json['pubkey'] as String,
      content: json['content'] as String,
      kind: json['kind'] as int,
      createdAt: json['created_at'] as int,
      channelId: json['channel_id'] as String,
      tags: (json['tags'] as List<dynamic>)
          .map((t) => (t as List<dynamic>).map((e) => e as String).toList())
          .toList(),
      threadSummary: rawSummary != null
          ? ForumThreadSummary.fromJson(rawSummary)
          : null,
    );
  }

  /// Extract mention pubkeys from p-tags.
  List<String> get mentionPubkeys => [
    for (final tag in tags)
      if (tag.length >= 2 && tag[0] == 'p') tag[1],
  ];
}

/// Summary of replies on a forum post.
@immutable
class ForumThreadSummary {
  final int replyCount;
  final int descendantCount;
  final int? lastReplyAt;
  final List<String> participants;

  const ForumThreadSummary({
    required this.replyCount,
    required this.descendantCount,
    this.lastReplyAt,
    required this.participants,
  });

  factory ForumThreadSummary.fromJson(Map<String, dynamic> json) {
    return ForumThreadSummary(
      replyCount: json['reply_count'] as int? ?? 0,
      descendantCount: json['descendant_count'] as int? ?? 0,
      lastReplyAt: json['last_reply_at'] as int?,
      participants:
          (json['participants'] as List<dynamic>?)
              ?.map((e) => e as String)
              .toList() ??
          const [],
    );
  }
}

/// A reply within a forum thread.
@immutable
class ThreadReply {
  final String eventId;
  final String pubkey;
  final String content;
  final int kind;
  final int createdAt;
  final String channelId;
  final List<List<String>> tags;
  final String? parentEventId;
  final String? rootEventId;
  final int depth;

  const ThreadReply({
    required this.eventId,
    required this.pubkey,
    required this.content,
    required this.kind,
    required this.createdAt,
    required this.channelId,
    required this.tags,
    this.parentEventId,
    this.rootEventId,
    required this.depth,
  });

  factory ThreadReply.fromJson(Map<String, dynamic> json) {
    return ThreadReply(
      eventId: json['event_id'] as String,
      pubkey: json['pubkey'] as String,
      content: json['content'] as String,
      kind: json['kind'] as int,
      createdAt: json['created_at'] as int,
      channelId: json['channel_id'] as String,
      tags: (json['tags'] as List<dynamic>)
          .map((t) => (t as List<dynamic>).map((e) => e as String).toList())
          .toList(),
      parentEventId: json['parent_event_id'] as String?,
      rootEventId: json['root_event_id'] as String?,
      depth: json['depth'] as int? ?? 0,
    );
  }

  /// Extract mention pubkeys from p-tags.
  List<String> get mentionPubkeys => [
    for (final tag in tags)
      if (tag.length >= 2 && tag[0] == 'p') tag[1],
  ];
}

/// Paginated response for forum posts.
@immutable
class ForumPostsResponse {
  final List<ForumPost> posts;
  final int? nextCursor;

  const ForumPostsResponse({required this.posts, this.nextCursor});

  factory ForumPostsResponse.fromJson(Map<String, dynamic> json) {
    final messages = json['messages'] as List<dynamic>? ?? const [];
    return ForumPostsResponse(
      posts: messages
          .cast<Map<String, dynamic>>()
          .map(ForumPost.fromJson)
          .toList(),
      nextCursor: json['next_cursor'] as int?,
    );
  }
}

/// Response for a single forum thread with replies.
@immutable
class ForumThreadResponse {
  final ForumPost post;
  final List<ThreadReply> replies;
  final int totalReplies;
  final String? nextCursor;

  const ForumThreadResponse({
    required this.post,
    required this.replies,
    required this.totalReplies,
    this.nextCursor,
  });

  factory ForumThreadResponse.fromJson(Map<String, dynamic> json) {
    final repliesJson = json['replies'] as List<dynamic>? ?? const [];
    return ForumThreadResponse(
      post: ForumPost.fromJson(json['root'] as Map<String, dynamic>),
      replies: repliesJson
          .cast<Map<String, dynamic>>()
          .map(ThreadReply.fromJson)
          .toList(),
      totalReplies: json['total_replies'] as int? ?? 0,
      nextCursor: json['next_cursor'] as String?,
    );
  }
}

/// Format a unix timestamp as a relative time string (e.g. "2h ago").
String formatRelativeTime(int timestamp) {
  final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
  final diff = now - timestamp;

  if (diff < 60) return 'just now';
  if (diff < 3600) return '${diff ~/ 60}m ago';
  if (diff < 86400) return '${diff ~/ 3600}h ago';
  if (diff < 604800) return '${diff ~/ 86400}d ago';

  final dt = DateTime.fromMillisecondsSinceEpoch(
    timestamp * 1000,
    isUtc: true,
  ).toLocal();
  return '${dt.month}/${dt.day}/${dt.year}';
}
