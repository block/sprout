import 'package:flutter/foundation.dart';

/// Nostr event kind constants.
///
/// Keep in sync with `desktop/src/shared/constants/kinds.ts`.
abstract final class EventKind {
  static const deletion = 5;
  static const reaction = 7;
  static const streamMessage = 9;
  static const presenceUpdate = 20001;
  static const typingIndicator = 20002;
  static const auth = 22242;
  static const agentObserverFrame = 24200;
  static const readState = 30078;
  static const userStatus = 30315;
  static const streamMessageV2 = 40002;
  static const streamMessageEdit = 40003;
  static const streamMessageDiff = 40008;
  static const systemMessage = 40099;
  static const forumPost = 45001;
  static const forumComment = 45003;

  /// Event kinds that represent user-visible channel messages.
  static const channelMessageEventKinds = [
    streamMessage, // 9
    streamMessageV2, // 40002
    forumPost, // 45001
    forumComment, // 45003
  ];

  /// Event kinds that represent channel activity (messages, edits, reactions,
  /// deletions, system events). Matches the desktop's `CHANNEL_EVENT_KINDS`.
  static const channelEventKinds = [
    deletion, // 5
    reaction, // 7
    ...channelMessageEventKinds,
    40001, // legacy pre-migration stream messages
    streamMessageEdit, // 40003
    streamMessageDiff, // 40008
    systemMessage, // 40099
  ];
}

/// A Nostr event as defined by NIP-01.
@immutable
class NostrEvent {
  final String id;
  final String pubkey;
  final int createdAt;
  final int kind;
  final List<List<String>> tags;
  final String content;
  final String sig;

  const NostrEvent({
    required this.id,
    required this.pubkey,
    required this.createdAt,
    required this.kind,
    required this.tags,
    required this.content,
    required this.sig,
  });

  factory NostrEvent.fromJson(Map<String, dynamic> json) {
    return NostrEvent(
      id: json['id'] as String,
      pubkey: json['pubkey'] as String,
      createdAt: json['created_at'] as int,
      kind: json['kind'] as int,
      tags: (json['tags'] as List<dynamic>)
          .map((t) => (t as List<dynamic>).map((e) => e as String).toList())
          .toList(),
      content: json['content'] as String,
      sig: json['sig'] as String,
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'pubkey': pubkey,
    'created_at': createdAt,
    'kind': kind,
    'tags': tags,
    'content': content,
    'sig': sig,
  };

  /// Get the first value for a given tag key.
  String? getTagValue(String key) {
    for (final tag in tags) {
      if (tag.isNotEmpty && tag[0] == key && tag.length > 1) {
        return tag[1];
      }
    }
    return null;
  }

  /// The channel/group ID from the `h` tag (NIP-29).
  String? get channelId => getTagValue('h');

  /// Extract thread parent and root IDs from `e` tags.
  ///
  /// Matches the desktop's `getThreadReference` logic:
  /// - Tags with marker `"reply"` identify the direct parent.
  /// - Tags with marker `"root"` identify the thread root.
  /// - If no markers are present, falls back to null (top-level message).
  ({String? parentId, String? rootId}) get threadReference {
    final eTags = [
      for (final tag in tags)
        if (tag.length >= 2 && tag[0] == 'e') tag,
    ];

    if (eTags.isEmpty) return (parentId: null, rootId: null);

    // Find tagged root and reply markers (desktop convention).
    List<String>? rootTag;
    List<String>? replyTag;
    for (final tag in eTags) {
      if (tag.length >= 4) {
        if (tag[3] == 'root') rootTag = tag;
        if (tag[3] == 'reply') replyTag = tag;
      }
    }

    if (replyTag == null) return (parentId: null, rootId: null);

    final parentId = replyTag[1];
    final rootId = rootTag?[1] ?? parentId;
    return (parentId: parentId, rootId: rootId);
  }

  /// The parent event ID from the `e` tag.
  String? get parentEventId => threadReference.parentId;

  @override
  bool operator ==(Object other) =>
      identical(this, other) || other is NostrEvent && id == other.id;

  @override
  int get hashCode => id.hashCode;
}

/// A NIP-01 subscription filter.
@immutable
class NostrFilter {
  final List<int> kinds;
  final List<String>? authors;
  final int limit;
  final int? since;
  final int? until;

  /// Tag filters, e.g. `{'#h': ['channel-id']}`.
  final Map<String, List<String>> tags;

  const NostrFilter({
    required this.kinds,
    this.authors,
    this.limit = 100,
    this.since,
    this.until,
    this.tags = const {},
  });

  /// Return a copy with an updated `since` value.
  NostrFilter copyWithSince(int since) => NostrFilter(
    kinds: kinds,
    authors: authors,
    limit: limit,
    since: since,
    until: until,
    tags: tags,
  );

  Map<String, dynamic> toJson() {
    final json = <String, dynamic>{'kinds': kinds, 'limit': limit};
    if (authors != null) json['authors'] = authors;
    if (since != null) json['since'] = since;
    if (until != null) json['until'] = until;
    for (final entry in tags.entries) {
      json[entry.key] = entry.value;
    }
    return json;
  }
}
