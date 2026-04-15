import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sprout_mobile/features/channels/message_content.dart';
import 'package:sprout_mobile/shared/theme/theme.dart';

Widget _testable(Widget child) {
  return MaterialApp(
    theme: AppTheme.lightTheme,
    home: Scaffold(body: child),
  );
}

/// Extracts all plain text from all RichText widgets in the tree.
String _allRichText(WidgetTester tester) {
  final richTexts = tester.widgetList<RichText>(find.byType(RichText));
  return richTexts.map((rt) => rt.text.toPlainText()).join('\n');
}

/// Finds a RichText widget whose plain text contains [text].
Finder _findRich(String text) {
  return find.byWidgetPredicate(
    (widget) => widget is RichText && widget.text.toPlainText().contains(text),
    description: 'RichText containing "$text"',
  );
}

/// Checks that the given text appears as bold (fontWeight >= w600) in some
/// TextSpan within any RichText widget.
bool _hasBoldSpan(WidgetTester tester, String text) {
  for (final rt in tester.widgetList<RichText>(find.byType(RichText))) {
    if (_spanHasStyle(
      rt.text,
      text,
      (s) =>
          s.fontWeight != null && s.fontWeight!.value >= FontWeight.w600.value,
    )) {
      return true;
    }
  }
  return false;
}

bool _hasItalicSpan(WidgetTester tester, String text) {
  for (final rt in tester.widgetList<RichText>(find.byType(RichText))) {
    if (_spanHasStyle(rt.text, text, (s) => s.fontStyle == FontStyle.italic)) {
      return true;
    }
  }
  return false;
}

bool _hasStrikethroughSpan(WidgetTester tester, String text) {
  for (final rt in tester.widgetList<RichText>(find.byType(RichText))) {
    if (_spanHasStyle(
      rt.text,
      text,
      (s) => s.decoration == TextDecoration.lineThrough,
    )) {
      return true;
    }
  }
  return false;
}

bool _spanHasStyle(
  InlineSpan root,
  String text,
  bool Function(TextStyle) check,
) {
  var found = false;
  root.visitChildren((span) {
    if (span is TextSpan &&
        span.text != null &&
        span.text!.contains(text) &&
        span.style != null &&
        check(span.style!)) {
      found = true;
      return false; // stop visiting
    }
    return true;
  });
  return found;
}

void main() {
  group('MessageContent', () {
    group('plain text', () {
      testWidgets('renders simple text', (tester) async {
        await tester.pumpWidget(
          _testable(const MessageContent(content: 'Hello world')),
        );

        expect(_findRich('Hello world'), findsOneWidget);
      });

      testWidgets('renders empty content', (tester) async {
        await tester.pumpWidget(_testable(const MessageContent(content: '')));

        // Should not crash.
        expect(find.byType(MessageContent), findsOneWidget);
      });
    });

    group('inline formatting', () {
      testWidgets('renders bold text', (tester) async {
        await tester.pumpWidget(
          _testable(const MessageContent(content: 'This is **bold** text')),
        );

        final allText = _allRichText(tester);
        expect(allText, contains('bold'));
        expect(allText, isNot(contains('**')));
        expect(_hasBoldSpan(tester, 'bold'), isTrue);
      });

      testWidgets('renders italic text', (tester) async {
        await tester.pumpWidget(
          _testable(const MessageContent(content: 'This is *italic* text')),
        );

        final allText = _allRichText(tester);
        expect(allText, contains('italic'));
        expect(_hasItalicSpan(tester, 'italic'), isTrue);
      });

      testWidgets('renders strikethrough text', (tester) async {
        await tester.pumpWidget(
          _testable(const MessageContent(content: 'This is ~~struck~~ text')),
        );

        final allText = _allRichText(tester);
        expect(allText, contains('struck'));
        expect(allText, isNot(contains('~~')));
        expect(_hasStrikethroughSpan(tester, 'struck'), isTrue);
      });

      testWidgets('renders inline code', (tester) async {
        await tester.pumpWidget(
          _testable(const MessageContent(content: 'Use `flutter test` to run')),
        );

        // Inline code is rendered inside a styled span.
        expect(_findRich('flutter test'), findsWidgets);
      });

      testWidgets('renders markdown link', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: 'Check [Sprout](https://example.com)',
            ),
          ),
        );

        final allText = _allRichText(tester);
        expect(allText, contains('Sprout'));
        // Should not show raw markdown syntax.
        expect(allText, isNot(contains('[Sprout]')));
        expect(allText, isNot(contains('(https://example.com)')));
      });

      testWidgets('renders bare URL as link', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(content: 'Visit https://example.com today'),
          ),
        );

        final allText = _allRichText(tester);
        expect(allText, contains('https://example.com'));
      });
    });

    group('code blocks', () {
      testWidgets('renders fenced code block', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(content: 'Before\n```\ncode here\n```\nAfter'),
          ),
        );

        expect(_findRich('code here'), findsWidgets);
        expect(_findRich('Before'), findsWidgets);
        expect(_findRich('After'), findsWidgets);
      });

      testWidgets('renders code block with language tag', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(content: '```dart\nvoid main() {}\n```'),
          ),
        );

        expect(_findRich('void main() {}'), findsWidgets);
      });
    });

    group('blockquotes', () {
      testWidgets('renders blockquote with left border', (tester) async {
        await tester.pumpWidget(
          _testable(const MessageContent(content: '> This is a quote')),
        );

        final allText = _allRichText(tester);
        expect(allText, contains('This is a quote'));
        // Should strip the > prefix.
        expect(allText, isNot(contains('> This')));
      });
    });

    group('@mentions', () {
      testWidgets('renders @mention with highlight', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: 'Hey @Alice check this out',
              mentionNames: {'pk1': 'Alice'},
            ),
          ),
        );

        // Mention should be rendered as @Alice in a highlighted container.
        expect(find.text('@Alice'), findsOneWidget);
      });

      testWidgets('renders unknown @mention as-is', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: 'Hey @unknown check this',
              mentionNames: {},
            ),
          ),
        );

        expect(find.text('@unknown'), findsOneWidget);
      });

      testWidgets('does not treat email addresses as mentions', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: 'Email alice@example.com for access',
              mentionNames: {'pk1': 'Alice'},
            ),
          ),
        );

        expect(_allRichText(tester), contains('alice@example.com'));
        expect(find.text('@example.com'), findsNothing);
      });
    });

    group('#channel links', () {
      testWidgets('renders #channel with highlight', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: 'Check out #general',
              channelNames: {'general': 'ch-id-1'},
            ),
          ),
        );

        expect(find.text('#general'), findsOneWidget);
      });

      testWidgets('channel tap callback fires', (tester) async {
        String? tappedId;
        await tester.pumpWidget(
          _testable(
            MessageContent(
              content: 'See #general',
              channelNames: const {'general': 'ch-id-1'},
              onChannelTap: (id) => tappedId = id,
            ),
          ),
        );

        await tester.tap(find.text('#general'));
        expect(tappedId, 'ch-id-1');
      });

      testWidgets('unknown channel renders without tap', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(content: 'Check #unknown', channelNames: {}),
          ),
        );

        expect(find.text('#unknown'), findsOneWidget);
      });

      testWidgets('does not treat URL fragments as channel links', (
        tester,
      ) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: 'See https://example.com/docs#frag',
              channelNames: {'frag': 'ch-id-1'},
            ),
          ),
        );

        expect(_allRichText(tester), contains('https://example.com/docs#frag'));
        expect(find.text('#frag'), findsNothing);
      });
    });

    group('mixed content', () {
      testWidgets('renders bold with mentions', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: '**Important** @Alice please review',
              mentionNames: {'pk1': 'Alice'},
            ),
          ),
        );

        expect(_hasBoldSpan(tester, 'Important'), isTrue);
        expect(find.text('@Alice'), findsOneWidget);
      });

      testWidgets('preserves markdown around mentions', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: '**@Alice** please review',
              mentionNames: {'pk1': 'Alice'},
            ),
          ),
        );

        expect(find.text('@Alice'), findsOneWidget);
        expect(_allRichText(tester), isNot(contains('**')));
      });

      testWidgets('renders code block between paragraphs', (tester) async {
        await tester.pumpWidget(
          _testable(
            const MessageContent(
              content: 'Try this:\n```\nflutter test\n```\nDid it work?',
            ),
          ),
        );

        expect(_findRich('flutter test'), findsWidgets);
        expect(_findRich('Try this:'), findsWidgets);
        expect(_findRich('Did it work?'), findsWidgets);
      });
    });
  });
}
