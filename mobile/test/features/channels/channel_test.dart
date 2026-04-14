import 'package:flutter_test/flutter_test.dart';
import 'package:sprout_mobile/features/channels/channel.dart';

void main() {
  group('Channel.fromJson', () {
    test('parses a full channel response', () {
      final json = {
        'id': 'abc-123',
        'name': 'general',
        'channel_type': 'stream',
        'visibility': 'open',
        'description': 'General discussion',
        'topic': 'Welcome!',
        'purpose': 'Team chat',
        'created_by': 'deadbeef',
        'created_at': '2025-01-01T00:00:00+00:00',
        'member_count': 42,
        'last_message_at': '2025-06-01T12:00:00+00:00',
        'is_member': true,
      };

      final channel = Channel.fromJson(json);

      expect(channel.id, 'abc-123');
      expect(channel.name, 'general');
      expect(channel.channelType, 'stream');
      expect(channel.visibility, 'open');
      expect(channel.description, 'General discussion');
      expect(channel.topic, 'Welcome!');
      expect(channel.purpose, 'Team chat');
      expect(channel.memberCount, 42);
      expect(channel.isMember, isTrue);
      expect(channel.isStream, isTrue);
      expect(channel.isForum, isFalse);
      expect(channel.isDm, isFalse);
      expect(channel.isPrivate, isFalse);
    });

    test('handles null optional fields', () {
      final json = {
        'id': 'abc-123',
        'name': 'private-chat',
        'channel_type': 'stream',
        'visibility': 'private',
        'description': null,
        'topic': null,
        'purpose': null,
        'created_by': 'deadbeef',
        'created_at': '2025-01-01T00:00:00+00:00',
        'member_count': 2,
        'last_message_at': null,
        'is_member': false,
      };

      final channel = Channel.fromJson(json);

      expect(channel.description, '');
      expect(channel.topic, isNull);
      expect(channel.lastMessageAt, isNull);
      expect(channel.isMember, isFalse);
      expect(channel.isPrivate, isTrue);
    });

    test('defaults is_member to false when missing', () {
      final json = {
        'id': 'abc-123',
        'name': 'test',
        'channel_type': 'forum',
        'visibility': 'open',
        'created_by': 'deadbeef',
        'created_at': '2025-01-01T00:00:00+00:00',
        'member_count': 0,
      };

      final channel = Channel.fromJson(json);

      expect(channel.isMember, isFalse);
      expect(channel.isForum, isTrue);
    });
  });
}
