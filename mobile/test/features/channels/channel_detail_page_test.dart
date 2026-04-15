import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:sprout_mobile/features/channels/channel.dart';
import 'package:sprout_mobile/features/channels/channel_detail_page.dart';
import 'package:sprout_mobile/features/channels/channel_messages_provider.dart';
import 'package:sprout_mobile/features/channels/channel_typing_provider.dart';
import 'package:sprout_mobile/features/channels/channels_provider.dart';
import 'package:sprout_mobile/features/profile/user_cache_provider.dart';
import 'package:sprout_mobile/features/profile/user_profile.dart';
import 'package:sprout_mobile/shared/relay/relay.dart';
import 'package:sprout_mobile/shared/theme/theme.dart';

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

const _channelId = 'test-channel';

final _testChannel = Channel(
  id: _channelId,
  name: 'general',
  channelType: 'stream',
  visibility: 'open',
  description: 'General discussion',
  createdBy: 'abc123',
  createdAt: DateTime(2025),
  memberCount: 5,
  isMember: true,
);

NostrEvent _textMsg({
  required String id,
  required String pubkey,
  required String content,
  int createdAt = 1000,
}) => NostrEvent(
  id: id,
  pubkey: pubkey,
  createdAt: createdAt,
  kind: EventKind.streamMessage,
  tags: [
    ['h', _channelId],
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
    ['h', _channelId],
  ],
  content: jsonEncode(payload),
  sig: '',
);

NostrEvent _deletion({
  required String id,
  required List<String> targetIds,
  int createdAt = 2000,
}) => NostrEvent(
  id: id,
  pubkey: 'abc123',
  createdAt: createdAt,
  kind: EventKind.deletion,
  tags: [
    ['h', _channelId],
    for (final t in targetIds) ['e', t],
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
  pubkey: 'abc123',
  createdAt: createdAt,
  kind: EventKind.streamMessageEdit,
  tags: [
    ['h', _channelId],
    ['e', targetId],
  ],
  content: content,
  sig: '',
);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

Widget _buildTestable({
  required List<NostrEvent> messages,
  List<TypingEntry> typing = const [],
  Map<String, UserProfile> users = const {},
  Channel? channel,
  List<Channel>? channels,
  List<NavigatorObserver> navigatorObservers = const [],
}) {
  return ProviderScope(
    overrides: [
      channelMessagesProvider(
        _channelId,
      ).overrideWith(() => _FakeMessagesNotifier(messages)),
      channelTypingProvider(
        _channelId,
      ).overrideWith(() => _FakeTypingNotifier(typing)),
      userCacheProvider.overrideWith(() => _FakeUserCacheNotifier(users)),
      channelsProvider.overrideWith(
        () => _FakeChannelsNotifier(channels ?? [channel ?? _testChannel]),
      ),
      // Stub the relay client provider so preloadMembers doesn't crash.
      relayClientProvider.overrideWithValue(
        RelayClient(baseUrl: 'http://localhost:3000'),
      ),
    ],
    child: MaterialApp(
      theme: AppTheme.lightTheme,
      navigatorObservers: navigatorObservers,
      home: ChannelDetailPage(channel: channel ?? _testChannel),
    ),
  );
}

/// Finder that searches for text within RichText spans. [find.text] only
/// matches the top-level text property; this also searches nested TextSpans.
Finder findRichText(String text) {
  return find.byWidgetPredicate((widget) {
    if (widget is RichText) {
      return widget.text.toPlainText().contains(text);
    }
    return false;
  }, description: 'RichText containing "$text"');
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

void main() {
  group('ChannelDetailPage', () {
    testWidgets('shows empty state when no messages', (tester) async {
      await tester.pumpWidget(_buildTestable(messages: []));
      await tester.pumpAndSettle();

      expect(find.text('No messages yet'), findsOneWidget);
      expect(find.text('Be the first to say something!'), findsOneWidget);
    });

    testWidgets('renders text messages with author and content', (
      tester,
    ) async {
      final messages = [
        _textMsg(
          id: 'msg1',
          pubkey: 'alice',
          content: 'Hello world!',
          createdAt: 1000,
        ),
        _textMsg(
          id: 'msg2',
          pubkey: 'bob',
          content: 'Hey Alice!',
          createdAt: 1100,
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
            'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(findRichText('Hello world!'), findsOneWidget);
      expect(findRichText('Hey Alice!'), findsOneWidget);
      expect(find.text('Alice'), findsOneWidget);
      expect(find.text('Bob'), findsOneWidget);
    });

    testWidgets('groups consecutive messages from same author', (tester) async {
      final messages = [
        _textMsg(
          id: 'msg1',
          pubkey: 'alice',
          content: 'First message',
          createdAt: 1000,
        ),
        _textMsg(
          id: 'msg2',
          pubkey: 'alice',
          content: 'Second message',
          createdAt: 1060, // within 5 min
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      // Author name should appear only once (grouped).
      expect(find.text('Alice'), findsOneWidget);
      expect(findRichText('First message'), findsOneWidget);
      expect(findRichText('Second message'), findsOneWidget);
    });

    testWidgets('shows author again after 5min gap', (tester) async {
      final messages = [
        _textMsg(
          id: 'msg1',
          pubkey: 'alice',
          content: 'First',
          createdAt: 1000,
        ),
        _textMsg(
          id: 'msg2',
          pubkey: 'alice',
          content: 'Second',
          createdAt: 1400, // 6+ min later
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      // Author name appears twice since messages are >5min apart.
      expect(find.text('Alice'), findsNWidgets(2));
    });

    testWidgets('shows pubkey fallback when no profile', (tester) async {
      final messages = [
        _textMsg(
          id: 'msg1',
          pubkey: 'abcdef1234567890',
          content: 'Hi',
          createdAt: 1000,
        ),
      ];

      await tester.pumpWidget(_buildTestable(messages: messages));
      await tester.pumpAndSettle();

      expect(findRichText('Hi'), findsOneWidget);
      // Should show first 8 chars of pubkey + ellipsis
      expect(find.text('abcdef12…'), findsOneWidget);
    });
  });

  group('System messages', () {
    testWidgets('renders channel_created system event', (tester) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {'type': 'channel_created', 'actor': 'alice'},
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Alice created this channel'), findsOneWidget);
    });

    testWidgets('renders member_joined (self-join) system event', (
      tester,
    ) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {'type': 'member_joined', 'actor': 'bob', 'target': 'bob'},
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob')},
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Bob joined the channel'), findsOneWidget);
    });

    testWidgets('renders member_joined (added by other) system event', (
      tester,
    ) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {'type': 'member_joined', 'actor': 'alice', 'target': 'bob'},
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
            'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Alice added Bob to the channel'), findsOneWidget);
    });

    testWidgets('renders member_left system event', (tester) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {'type': 'member_left', 'actor': 'bob'},
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob')},
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Bob left the channel'), findsOneWidget);
    });

    testWidgets('renders member_removed system event', (tester) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {
            'type': 'member_removed',
            'actor': 'alice',
            'target': 'bob',
          },
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
            'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Alice removed Bob from the channel'), findsOneWidget);
    });

    testWidgets('renders topic_changed system event', (tester) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {
            'type': 'topic_changed',
            'actor': 'alice',
            'topic': 'Release planning',
          },
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.text('Alice changed the topic to "Release planning"'),
        findsOneWidget,
      );
    });

    testWidgets('renders purpose_changed system event', (tester) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {
            'type': 'purpose_changed',
            'actor': 'alice',
            'purpose': 'Team standup notes',
          },
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.text('Alice changed the purpose to "Team standup notes"'),
        findsOneWidget,
      );
    });

    testWidgets('system message breaks author grouping', (tester) async {
      final messages = [
        _textMsg(
          id: 'msg1',
          pubkey: 'alice',
          content: 'Before',
          createdAt: 1000,
        ),
        _systemMsg(
          id: 'sys1',
          payload: {'type': 'member_joined', 'actor': 'bob', 'target': 'bob'},
          createdAt: 1010,
        ),
        _textMsg(
          id: 'msg2',
          pubkey: 'alice',
          content: 'After',
          createdAt: 1020,
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
            'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob'),
          },
        ),
      );
      await tester.pumpAndSettle();

      // Alice should appear twice — system message breaks grouping.
      expect(find.text('Alice'), findsNWidgets(2));
    });

    testWidgets('skips unknown system event types', (tester) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {'type': 'unknown_future_type', 'actor': 'alice'},
        ),
        _textMsg(
          id: 'msg1',
          pubkey: 'alice',
          content: 'Hello',
          createdAt: 1100,
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      // Only the text message should render, unknown system event is skipped.
      expect(findRichText('Hello'), findsOneWidget);
      // No system message row rendered for unknown type.
      expect(find.byIcon(LucideIcons.arrowLeftRight), findsNothing);
    });
  });

  group('Deletions', () {
    testWidgets('deleted messages are not shown', (tester) async {
      final messages = [
        _textMsg(
          id: 'msg1',
          pubkey: 'alice',
          content: 'Keep this',
          createdAt: 1000,
        ),
        _textMsg(
          id: 'msg2',
          pubkey: 'bob',
          content: 'Delete this',
          createdAt: 1100,
        ),
        _deletion(id: 'del1', targetIds: ['msg2'], createdAt: 1200),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
            'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(findRichText('Keep this'), findsOneWidget);
      expect(findRichText('Delete this'), findsNothing);
    });

    testWidgets('deletion of multiple messages', (tester) async {
      final messages = [
        _textMsg(id: 'msg1', pubkey: 'a', content: 'One', createdAt: 1000),
        _textMsg(id: 'msg2', pubkey: 'a', content: 'Two', createdAt: 1100),
        _textMsg(id: 'msg3', pubkey: 'a', content: 'Three', createdAt: 1200),
        _deletion(id: 'del1', targetIds: ['msg1', 'msg3'], createdAt: 1300),
      ];

      await tester.pumpWidget(_buildTestable(messages: messages));
      await tester.pumpAndSettle();

      expect(findRichText('One'), findsNothing);
      expect(findRichText('Two'), findsOneWidget);
      expect(findRichText('Three'), findsNothing);
    });
  });

  group('Edits', () {
    testWidgets('edited message shows updated content and (edited) label', (
      tester,
    ) async {
      final messages = [
        _textMsg(
          id: 'msg1',
          pubkey: 'alice',
          content: 'Original text',
          createdAt: 1000,
        ),
        _edit(
          id: 'edit1',
          targetId: 'msg1',
          content: 'Edited text',
          createdAt: 1100,
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(findRichText('Edited text'), findsOneWidget);
      expect(findRichText('Original text'), findsNothing);
      expect(find.text('(edited)'), findsOneWidget);
    });

    testWidgets('latest edit wins when multiple edits exist', (tester) async {
      final messages = [
        _textMsg(id: 'msg1', pubkey: 'alice', content: 'V1', createdAt: 1000),
        _edit(id: 'e1', targetId: 'msg1', content: 'V2', createdAt: 1100),
        _edit(id: 'e2', targetId: 'msg1', content: 'V3', createdAt: 1200),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(findRichText('V3'), findsOneWidget);
      expect(findRichText('V1'), findsNothing);
      expect(findRichText('V2'), findsNothing);
    });
  });

  group('Typing indicator', () {
    testWidgets('shows single typer', (tester) async {
      await tester.pumpWidget(
        _buildTestable(
          messages: [],
          typing: [
            TypingEntry(
              pubkey: 'alice',
              expiresAtMs: DateTime.now().millisecondsSinceEpoch + 8000,
            ),
          ],
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Alice is typing…'), findsOneWidget);
    });

    testWidgets('shows two typers', (tester) async {
      await tester.pumpWidget(
        _buildTestable(
          messages: [],
          typing: [
            TypingEntry(
              pubkey: 'alice',
              expiresAtMs: DateTime.now().millisecondsSinceEpoch + 8000,
            ),
            TypingEntry(
              pubkey: 'bob',
              expiresAtMs: DateTime.now().millisecondsSinceEpoch + 8000,
            ),
          ],
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
            'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Alice and Bob are typing…'), findsOneWidget);
    });

    testWidgets('shows N others for 3+ typers', (tester) async {
      await tester.pumpWidget(
        _buildTestable(
          messages: [],
          typing: [
            TypingEntry(
              pubkey: 'alice',
              expiresAtMs: DateTime.now().millisecondsSinceEpoch + 8000,
            ),
            TypingEntry(
              pubkey: 'bob',
              expiresAtMs: DateTime.now().millisecondsSinceEpoch + 8000,
            ),
            TypingEntry(
              pubkey: 'carol',
              expiresAtMs: DateTime.now().millisecondsSinceEpoch + 8000,
            ),
          ],
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
            'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob'),
            'carol': const UserProfile(pubkey: 'carol', displayName: 'Carol'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Alice and 2 others are typing…'), findsOneWidget);
    });
  });

  group('Compose bar', () {
    testWidgets('shows text field and send button', (tester) async {
      await tester.pumpWidget(_buildTestable(messages: []));
      await tester.pumpAndSettle();

      expect(find.byType(TextField), findsOneWidget);
      expect(find.byIcon(LucideIcons.sendHorizontal), findsOneWidget);
    });

    testWidgets('shows hint text', (tester) async {
      await tester.pumpWidget(_buildTestable(messages: []));
      await tester.pumpAndSettle();

      expect(find.text('Message…'), findsOneWidget);
    });
  });

  group('App bar', () {
    testWidgets('shows channel name with hash icon', (tester) async {
      await tester.pumpWidget(_buildTestable(messages: []));
      await tester.pumpAndSettle();

      expect(find.text('general'), findsOneWidget);
      expect(find.byIcon(LucideIcons.hash), findsOneWidget);
    });

    testWidgets('shows lock icon for private channel', (tester) async {
      final privateChannel = Channel(
        id: _channelId,
        name: 'secret',
        channelType: 'stream',
        visibility: 'private',
        description: 'Private channel',
        createdBy: 'abc',
        createdAt: DateTime(2025),
        memberCount: 3,
        isMember: true,
      );

      await tester.pumpWidget(
        _buildTestable(messages: [], channel: privateChannel),
      );
      await tester.pumpAndSettle();

      expect(find.text('secret'), findsOneWidget);
      expect(find.byIcon(LucideIcons.lock), findsOneWidget);
    });
  });

  group('Error and loading states', () {
    testWidgets('shows error message on failure', (tester) async {
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            channelMessagesProvider(
              _channelId,
            ).overrideWith(() => _ErrorMessagesNotifier()),
            channelTypingProvider(
              _channelId,
            ).overrideWith(() => _FakeTypingNotifier([])),
            userCacheProvider.overrideWith(() => _FakeUserCacheNotifier({})),
            channelsProvider.overrideWith(
              () => _FakeChannelsNotifier([_testChannel]),
            ),
            relayClientProvider.overrideWithValue(
              RelayClient(baseUrl: 'http://localhost:3000'),
            ),
          ],
          child: MaterialApp(
            theme: AppTheme.lightTheme,
            home: ChannelDetailPage(channel: _testChannel),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Failed to load messages'), findsOneWidget);
    });
  });

  group('Mixed message timeline', () {
    testWidgets('interleaves text and system messages correctly', (
      tester,
    ) async {
      final messages = [
        _systemMsg(
          id: 'sys1',
          payload: {'type': 'channel_created', 'actor': 'alice'},
          createdAt: 900,
        ),
        _textMsg(
          id: 'msg1',
          pubkey: 'alice',
          content: 'Welcome everyone!',
          createdAt: 1000,
        ),
        _systemMsg(
          id: 'sys2',
          payload: {'type': 'member_joined', 'actor': 'bob', 'target': 'bob'},
          createdAt: 1100,
        ),
        _textMsg(
          id: 'msg2',
          pubkey: 'bob',
          content: 'Thanks for the invite!',
          createdAt: 1200,
        ),
      ];

      await tester.pumpWidget(
        _buildTestable(
          messages: messages,
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
            'bob': const UserProfile(pubkey: 'bob', displayName: 'Bob'),
          },
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Alice created this channel'), findsOneWidget);
      expect(findRichText('Welcome everyone!'), findsOneWidget);
      expect(find.text('Bob joined the channel'), findsOneWidget);
      expect(findRichText('Thanks for the invite!'), findsOneWidget);
    });
  });

  group('Channel links', () {
    testWidgets('tapping a channel link opens that channel', (tester) async {
      final randomChannel = Channel(
        id: 'random-channel',
        name: 'random',
        channelType: 'stream',
        visibility: 'open',
        description: 'Random discussion',
        createdBy: 'abc123',
        createdAt: DateTime(2025),
        memberCount: 3,
        isMember: true,
      );
      final observer = _TestNavigatorObserver();

      await tester.pumpWidget(
        _buildTestable(
          messages: [
            _textMsg(
              id: 'msg1',
              pubkey: 'alice',
              content: 'Take this to #random',
              createdAt: 1000,
            ),
          ],
          users: {
            'alice': const UserProfile(pubkey: 'alice', displayName: 'Alice'),
          },
          channels: [_testChannel, randomChannel],
          navigatorObservers: [observer],
        ),
      );
      await tester.pumpAndSettle();
      final initialPushCount = observer.pushCount;

      await tester.tap(find.text('#random'));
      await tester.pumpAndSettle();

      expect(observer.pushCount, initialPushCount + 1);
    });
  });
}

// ---------------------------------------------------------------------------
// Fake providers
// ---------------------------------------------------------------------------

class _FakeMessagesNotifier extends ChannelMessagesNotifier {
  final List<NostrEvent> _messages;
  _FakeMessagesNotifier(this._messages) : super(_channelId);

  @override
  AsyncValue<List<NostrEvent>> build() => AsyncData(_messages);
}

class _ErrorMessagesNotifier extends ChannelMessagesNotifier {
  _ErrorMessagesNotifier() : super(_channelId);

  @override
  AsyncValue<List<NostrEvent>> build() =>
      AsyncError('Connection failed', StackTrace.current);
}

class _FakeTypingNotifier extends ChannelTypingNotifier {
  final List<TypingEntry> _entries;
  _FakeTypingNotifier(this._entries) : super(_channelId);

  @override
  List<TypingEntry> build() => _entries;
}

class _FakeUserCacheNotifier extends UserCacheNotifier {
  final Map<String, UserProfile> _users;
  _FakeUserCacheNotifier(this._users);

  @override
  Map<String, UserProfile> build() => _users;

  @override
  UserProfile? get(String pubkey) => _users[pubkey.toLowerCase()];
}

class _FakeChannelsNotifier extends ChannelsNotifier {
  final List<Channel> _channels;
  _FakeChannelsNotifier(this._channels);

  @override
  Future<List<Channel>> build() => SynchronousFuture(_channels);
}

class _TestNavigatorObserver extends NavigatorObserver {
  int pushCount = 0;

  @override
  void didPush(Route<dynamic> route, Route<dynamic>? previousRoute) {
    pushCount += 1;
    super.didPush(route, previousRoute);
  }
}
