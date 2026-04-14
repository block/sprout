import 'package:flutter/foundation.dart';

@immutable
class Channel {
  final String id;
  final String name;
  final String channelType; // "stream", "forum", "dm"
  final String visibility; // "open", "private"
  final String description;
  final String? topic;
  final String? purpose;
  final String createdBy;
  final DateTime createdAt;
  final int memberCount;
  final DateTime? lastMessageAt;
  final bool isMember;

  const Channel({
    required this.id,
    required this.name,
    required this.channelType,
    required this.visibility,
    required this.description,
    required this.createdBy,
    required this.createdAt,
    required this.memberCount,
    this.topic,
    this.purpose,
    this.lastMessageAt,
    this.isMember = false,
  });

  factory Channel.fromJson(Map<String, dynamic> json) => Channel(
    id: json['id'] as String,
    name: json['name'] as String,
    channelType: json['channel_type'] as String,
    visibility: json['visibility'] as String,
    description: (json['description'] as String?) ?? '',
    topic: json['topic'] as String?,
    purpose: json['purpose'] as String?,
    createdBy: json['created_by'] as String,
    createdAt: DateTime.parse(json['created_at'] as String),
    memberCount: json['member_count'] as int,
    lastMessageAt: json['last_message_at'] != null
        ? DateTime.parse(json['last_message_at'] as String)
        : null,
    isMember: json['is_member'] as bool? ?? false,
  );

  bool get isStream => channelType == 'stream';
  bool get isForum => channelType == 'forum';
  bool get isDm => channelType == 'dm';
  bool get isPrivate => visibility == 'private';
}
