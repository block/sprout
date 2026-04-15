import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:sprout_mobile/features/channels/timeline_message.dart';
import 'package:sprout_mobile/shared/relay/relay.dart';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

NostrEvent _textMsg({
  required String id,
  String pubkey = 'alice',
  String content = 'hello',
  int createdAt = 1000,
}) => NostrEvent(
  id: id,
  pubkey: pubkey,
  createdAt: createdAt,
  kind: EventKind.streamMessage,
  tags: [
    ['h', 'ch1'],
  ],
  content: content,
  sig: '',
);

NostrEvent _systemMsg({
  required String id,
  required Map<String, dynamic> payload,
  int createdAt = 1000,
}) => NostrEvent(
  id: id,
  pubkey: 'relay',
  createdAt: createdAt,
  kind: EventKind.systemMessage,
  tags: [
    ['h', 'ch1'],
  ],
  content: jsonEncode(payload),
  sig: '',
);

NostrEvent _deletion({
  required String id,
  required List<String> targets,
  int createdAt = 2000,
}) => NostrEvent(
  id: id,
  pubkey: 'alice',
  createdAt: createdAt,
  kind: EventKind.deletion,
  tags: [
    ['h', 'ch1'],
    for (final t in targets) ['e', t],
  ],
  content: '',
  sig: '',
);

NostrEvent _edit({
  required String id,
  required String targetId,
  required String content,
  int createdAt = 2000,
}) => NostrEvent(
  id: id,
  pubkey: 'alice',
  createdAt: createdAt,
  kind: EventKind.streamMessageEdit,
  tags: [
    ['h', 'ch1'],
    ['e', targetId],
  ],
  content: content,
  sig: '',
);

NostrEvent _reaction({
  required String id,
  required String targetId,
  int createdAt = 2000,
}) => NostrEvent(
  id: id,
  pubkey: 'bob',
  createdAt: createdAt,
  kind: EventKind.reaction,
  tags: [
    ['h', 'ch1'],
    ['e', targetId],
  ],
  content: '👍',
  sig: '',
);

// ---------------------------------------------------------------------------
// SystemEvent.fromContent
// ---------------------------------------------------------------------------

void main() {
  group('SystemEvent.fromContent', () {
    test('parses all known event types', () {
      final types = {
        'member_joined': SystemEventType.memberJoined,
        'member_left': SystemEventType.memberLeft,
        'member_removed': SystemEventType.memberRemoved,
        'topic_changed': SystemEventType.topicChanged,
        'purpose_changed': SystemEventType.purposeChanged,
        'channel_created': SystemEventType.channelCreated,
        'channel_archived': SystemEventType.channelArchived,
        'channel_unarchived': SystemEventType.channelUnarchived,
      };

      for (final entry in types.entries) {
        final event = SystemEvent.fromContent(
          jsonEncode({'type': entry.key, 'actor': 'pk1'}),
        );
        expect(event, isNotNull, reason: 'Failed for ${entry.key}');
        expect(event!.type, entry.value);
        expect(event.actorPubkey, 'pk1');
      }
    });

    test('returns null for unknown type', () {
      final event = SystemEvent.fromContent(
        jsonEncode({'type': 'unknown_type'}),
      );
      expect(event, isNull);
    });

    test('returns null for invalid JSON', () {
      expect(SystemEvent.fromContent('not json'), isNull);
    });

    test('parses target, topic, and purpose fields', () {
      final event = SystemEvent.fromContent(
        jsonEncode({
          'type': 'topic_changed',
          'actor': 'pk1',
          'target': 'pk2',
          'topic': 'New topic',
          'purpose': 'New purpose',
        }),
      );

      expect(event, isNotNull);
      expect(event!.targetPubkey, 'pk2');
      expect(event.topic, 'New topic');
      expect(event.purpose, 'New purpose');
    });
  });

  group('SystemEvent.describe', () {
    String resolve(String? pk) => pk == 'pk1' ? 'Alice' : 'Bob';

    test('member_joined self', () {
      final event = SystemEvent(
        type: SystemEventType.memberJoined,
        actorPubkey: 'pk1',
        targetPubkey: 'pk1',
      );
      expect(event.describe(resolve), 'Alice joined the channel');
    });

    test('member_joined by other', () {
      final event = SystemEvent(
        type: SystemEventType.memberJoined,
        actorPubkey: 'pk1',
        targetPubkey: 'pk2',
      );
      expect(event.describe(resolve), 'Alice added Bob to the channel');
    });

    test('member_left', () {
      final event = SystemEvent(
        type: SystemEventType.memberLeft,
        actorPubkey: 'pk1',
      );
      expect(event.describe(resolve), 'Alice left the channel');
    });

    test('member_removed', () {
      final event = SystemEvent(
        type: SystemEventType.memberRemoved,
        actorPubkey: 'pk1',
        targetPubkey: 'pk2',
      );
      expect(event.describe(resolve), 'Alice removed Bob from the channel');
    });

    test('topic_changed', () {
      final event = SystemEvent(
        type: SystemEventType.topicChanged,
        actorPubkey: 'pk1',
        topic: 'Release v2',
      );
      expect(
        event.describe(resolve),
        'Alice changed the topic to "Release v2"',
      );
    });

    test('purpose_changed', () {
      final event = SystemEvent(
        type: SystemEventType.purposeChanged,
        actorPubkey: 'pk1',
        purpose: 'Daily standups',
      );
      expect(
        event.describe(resolve),
        'Alice changed the purpose to "Daily standups"',
      );
    });

    test('channel_created', () {
      final event = SystemEvent(
        type: SystemEventType.channelCreated,
        actorPubkey: 'pk1',
      );
      expect(event.describe(resolve), 'Alice created this channel');
    });

    test('channel_archived', () {
      final event = SystemEvent(
        type: SystemEventType.channelArchived,
        actorPubkey: 'pk1',
      );
      expect(event.describe(resolve), 'Alice archived this channel');
    });

    test('channel_unarchived', () {
      final event = SystemEvent(
        type: SystemEventType.channelUnarchived,
        actorPubkey: 'pk1',
      );
      expect(event.describe(resolve), 'Alice unarchived this channel');
    });
  });

  group('formatTimeline', () {
    test('passes through text messages', () {
      final events = [
        _textMsg(id: 'a', content: 'hello'),
        _textMsg(id: 'b', content: 'world', createdAt: 1100),
      ];

      final result = formatTimeline(events);
      expect(result, hasLength(2));
      expect(result[0].content, 'hello');
      expect(result[1].content, 'world');
      expect(result[0].isSystem, false);
      expect(result[0].edited, false);
    });

    test('filters deleted messages', () {
      final events = [
        _textMsg(id: 'a', content: 'keep'),
        _textMsg(id: 'b', content: 'delete', createdAt: 1100),
        _deletion(id: 'd1', targets: ['b']),
      ];

      final result = formatTimeline(events);
      expect(result, hasLength(1));
      expect(result[0].content, 'keep');
    });

    test('applies edits', () {
      final events = [
        _textMsg(id: 'a', content: 'original'),
        _edit(id: 'e1', targetId: 'a', content: 'edited'),
      ];

      final result = formatTimeline(events);
      expect(result, hasLength(1));
      expect(result[0].content, 'edited');
      expect(result[0].edited, true);
    });

    test('latest edit wins', () {
      final events = [
        _textMsg(id: 'a', content: 'v1'),
        _edit(id: 'e1', targetId: 'a', content: 'v2', createdAt: 2000),
        _edit(id: 'e2', targetId: 'a', content: 'v3', createdAt: 3000),
      ];

      final result = formatTimeline(events);
      expect(result[0].content, 'v3');
    });

    test('deleted edit is ignored', () {
      final events = [
        _textMsg(id: 'a', content: 'original'),
        _edit(id: 'e1', targetId: 'a', content: 'edited'),
        _deletion(id: 'd1', targets: ['e1']),
      ];

      final result = formatTimeline(events);
      expect(result[0].content, 'original');
      expect(result[0].edited, false);
    });

    test('edit of deleted message is ignored', () {
      final events = [
        _textMsg(id: 'a', content: 'original'),
        _edit(id: 'e1', targetId: 'a', content: 'edited'),
        _deletion(id: 'd1', targets: ['a']),
      ];

      final result = formatTimeline(events);
      expect(result, isEmpty);
    });

    test('system messages are parsed', () {
      final events = [
        _systemMsg(
          id: 's1',
          payload: {'type': 'channel_created', 'actor': 'pk1'},
        ),
      ];

      final result = formatTimeline(events);
      expect(result, hasLength(1));
      expect(result[0].isSystem, true);
      expect(result[0].systemEvent, isNotNull);
      expect(result[0].systemEvent!.type, SystemEventType.channelCreated);
    });

    test('unknown system messages are dropped', () {
      final events = [
        _systemMsg(id: 's1', payload: {'type': 'unknown'}),
      ];

      final result = formatTimeline(events);
      expect(result, isEmpty);
    });

    test('reactions and typing indicators are filtered out', () {
      final events = [
        _textMsg(id: 'a', content: 'hello'),
        _reaction(id: 'r1', targetId: 'a'),
        NostrEvent(
          id: 'typing1',
          pubkey: 'bob',
          createdAt: 1000,
          kind: EventKind.typingIndicator,
          tags: [
            ['h', 'ch1'],
          ],
          content: '',
          sig: '',
        ),
      ];

      final result = formatTimeline(events);
      expect(result, hasLength(1));
      expect(result[0].content, 'hello');
    });

    test('preserves chronological order', () {
      final events = [
        _textMsg(id: 'a', content: 'first', createdAt: 1000),
        _systemMsg(
          id: 's1',
          payload: {'type': 'member_joined', 'actor': 'pk1', 'target': 'pk1'},
          createdAt: 1100,
        ),
        _textMsg(id: 'b', content: 'second', createdAt: 1200),
      ];

      final result = formatTimeline(events);
      expect(result, hasLength(3));
      expect(result[0].content, 'first');
      expect(result[1].isSystem, true);
      expect(result[2].content, 'second');
    });

    test('streamMessageV2 events are included', () {
      final events = [
        NostrEvent(
          id: 'v2msg',
          pubkey: 'alice',
          createdAt: 1000,
          kind: EventKind.streamMessageV2,
          tags: [
            ['h', 'ch1'],
          ],
          content: 'legacy message',
          sig: '',
        ),
      ];

      final result = formatTimeline(events);
      expect(result, hasLength(1));
      expect(result[0].content, 'legacy message');
    });

    test('streamMessageDiff events are included', () {
      final events = [
        NostrEvent(
          id: 'diff1',
          pubkey: 'alice',
          createdAt: 1000,
          kind: EventKind.streamMessageDiff,
          tags: [
            ['h', 'ch1'],
          ],
          content: '```diff\n-old\n+new\n```',
          sig: '',
        ),
      ];

      final result = formatTimeline(events);
      expect(result, hasLength(1));
      expect(result[0].content, contains('diff'));
    });
  });
}
