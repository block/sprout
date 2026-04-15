import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../shared/theme/theme.dart';

/// Renders message content with inline markdown formatting, @mentions, and
/// #channel links. Keeps things lightweight — no external markdown package.
///
/// Supported inline syntax:
///   **bold**, *italic*, ~~strikethrough~~, `inline code`,
///   [link text](url), bare URLs, @mentions, #channel-links
///
/// Block-level: fenced code blocks (```), blockquotes (>)
class MessageContent extends StatelessWidget {
  final String content;

  /// Display names for mentioned pubkeys, extracted from event p-tags.
  /// Keys are lowercase pubkeys, values are display names.
  final Map<String, String> mentionNames;

  /// Known channel names for #channel links. Keys are lowercase channel
  /// names, values are channel IDs.
  final Map<String, String> channelNames;

  /// Called when a #channel link is tapped.
  final void Function(String channelId)? onChannelTap;

  final TextStyle? baseStyle;

  const MessageContent({
    super.key,
    required this.content,
    this.mentionNames = const {},
    this.channelNames = const {},
    this.onChannelTap,
    this.baseStyle,
  });

  @override
  Widget build(BuildContext context) {
    final style =
        baseStyle ??
        context.textTheme.bodyMedium?.copyWith(
          color: context.colors.onSurface,
        ) ??
        const TextStyle();

    final blocks = _parseBlocks(content);
    if (blocks.length == 1 && blocks[0] is _InlineBlock) {
      // Fast path: single paragraph — no Column overhead.
      return RichText(
        text: TextSpan(
          children: _buildInlineSpans(
            (blocks[0] as _InlineBlock).text,
            style,
            context,
          ),
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final block in blocks)
          switch (block) {
            _CodeBlock b => _buildCodeBlock(b, context),
            _BlockquoteBlock b => _buildBlockquote(b, style, context),
            _InlineBlock b => Padding(
              padding: blocks.length > 1
                  ? const EdgeInsets.only(bottom: Grid.half)
                  : EdgeInsets.zero,
              child: RichText(
                text: TextSpan(
                  children: _buildInlineSpans(b.text, style, context),
                ),
              ),
            ),
          },
      ],
    );
  }

  // ---------------------------------------------------------------------------
  // Block parsing
  // ---------------------------------------------------------------------------

  static final _codeBlockPattern = RegExp(r'```(\w*)\n?([\s\S]*?)```');

  List<_Block> _parseBlocks(String text) {
    final blocks = <_Block>[];
    var remaining = text;

    while (remaining.isNotEmpty) {
      final codeMatch = _codeBlockPattern.firstMatch(remaining);
      if (codeMatch == null) {
        // No more code blocks — handle blockquotes in remaining text.
        _addInlineOrBlockquote(blocks, remaining);
        break;
      }

      // Text before the code block.
      final before = remaining.substring(0, codeMatch.start).trimRight();
      if (before.isNotEmpty) {
        _addInlineOrBlockquote(blocks, before);
      }

      blocks.add(
        _CodeBlock(
          language: codeMatch.group(1) ?? '',
          code: codeMatch.group(2)?.trimRight() ?? '',
        ),
      );

      remaining = remaining.substring(codeMatch.end).trimLeft();
    }

    return blocks;
  }

  void _addInlineOrBlockquote(List<_Block> blocks, String text) {
    // Split into lines, group consecutive blockquote lines.
    final lines = text.split('\n');
    final buffer = StringBuffer();
    var inBlockquote = false;

    void flushBuffer() {
      final content = buffer.toString().trim();
      if (content.isNotEmpty) {
        blocks.add(
          inBlockquote ? _BlockquoteBlock(content) : _InlineBlock(content),
        );
      }
      buffer.clear();
    }

    for (final line in lines) {
      final isQuote = line.startsWith('>');
      if (isQuote != inBlockquote) {
        flushBuffer();
        inBlockquote = isQuote;
      }
      final stripped = isQuote ? line.substring(1).trimLeft() : line;
      if (buffer.isNotEmpty) buffer.write('\n');
      buffer.write(stripped);
    }
    flushBuffer();
  }

  // ---------------------------------------------------------------------------
  // Block widgets
  // ---------------------------------------------------------------------------

  Widget _buildCodeBlock(_CodeBlock block, BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Grid.half),
      child: Container(
        width: double.infinity,
        padding: const EdgeInsets.all(Grid.xxs),
        decoration: BoxDecoration(
          color: context.colors.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(Radii.sm),
        ),
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Text(
            block.code,
            style: context.textTheme.bodySmall?.copyWith(
              fontFamily: 'GeistMono',
              color: context.colors.onSurface,
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildBlockquote(
    _BlockquoteBlock block,
    TextStyle style,
    BuildContext context,
  ) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Grid.half),
      child: Container(
        decoration: BoxDecoration(
          border: Border(
            left: BorderSide(color: context.colors.outline, width: 3),
          ),
        ),
        padding: const EdgeInsets.only(left: Grid.xxs),
        child: RichText(
          text: TextSpan(
            children: _buildInlineSpans(
              block.text,
              style.copyWith(fontStyle: FontStyle.italic),
              context,
            ),
          ),
        ),
      ),
    );
  }

  // ---------------------------------------------------------------------------
  // Inline span parsing
  // ---------------------------------------------------------------------------

  /// Master regex for inline elements. Order matters — earlier alternatives
  /// take priority.
  static final _inlinePattern = RegExp(
    r'(`[^`]+`)' // 1: inline code
    r'|(\*\*(?:[^*]|\*(?!\*))+\*\*)' // 2: bold
    r'|(\*(?:[^*])+\*)' // 3: italic
    r'|(~~(?:[^~])+~~)' // 4: strikethrough
    r'|(\[([^\]]+)\]\(([^)]+)\))' // 5,6,7: [text](url) link
    r'|(https?://[^\s<>\)]+)' // 8: bare URL
    r'|(@\S+)' // 9: @mention
    r'|(#\S+)', // 10: #channel
  );

  List<InlineSpan> _buildInlineSpans(
    String text,
    TextStyle style,
    BuildContext context,
  ) {
    final spans = <InlineSpan>[];
    var lastEnd = 0;

    for (final match in _inlinePattern.allMatches(text)) {
      // Plain text before this match.
      if (match.start > lastEnd) {
        spans.add(
          TextSpan(text: text.substring(lastEnd, match.start), style: style),
        );
      }

      if (match.group(1) != null) {
        // Inline code
        final code = match.group(1)!;
        spans.add(_codeSpan(code.substring(1, code.length - 1), context));
      } else if (match.group(2) != null) {
        // Bold
        final bold = match.group(2)!;
        final inner = bold.substring(2, bold.length - 2);
        spans.addAll(
          _buildInlineSpans(
            inner,
            style.copyWith(fontWeight: FontWeight.w600),
            context,
          ),
        );
      } else if (match.group(3) != null) {
        // Italic
        final italic = match.group(3)!;
        final inner = italic.substring(1, italic.length - 1);
        spans.addAll(
          _buildInlineSpans(
            inner,
            style.copyWith(fontStyle: FontStyle.italic),
            context,
          ),
        );
      } else if (match.group(4) != null) {
        // Strikethrough
        final strike = match.group(4)!;
        final inner = strike.substring(2, strike.length - 2);
        spans.addAll(
          _buildInlineSpans(
            inner,
            style.copyWith(decoration: TextDecoration.lineThrough),
            context,
          ),
        );
      } else if (match.group(5) != null) {
        // Markdown link [text](url)
        final linkText = match.group(6)!;
        final url = match.group(7)!;
        spans.add(_linkSpan(linkText, url, style, context));
      } else if (match.group(8) != null) {
        // Bare URL
        final url = match.group(8)!;
        spans.add(_linkSpan(url, url, style, context));
      } else if (match.group(9) != null) {
        // @mention
        spans.add(_mentionSpan(match.group(9)!, context));
      } else if (match.group(10) != null) {
        // #channel
        spans.add(_channelSpan(match.group(10)!, context));
      }

      lastEnd = match.end;
    }

    // Trailing plain text.
    if (lastEnd < text.length) {
      spans.add(TextSpan(text: text.substring(lastEnd), style: style));
    }

    // If no spans at all (empty string), return a single empty span.
    if (spans.isEmpty) {
      spans.add(TextSpan(text: text, style: style));
    }

    return spans;
  }

  // ---------------------------------------------------------------------------
  // Span builders
  // ---------------------------------------------------------------------------

  WidgetSpan _codeSpan(String code, BuildContext context) {
    return WidgetSpan(
      alignment: PlaceholderAlignment.middle,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
        decoration: BoxDecoration(
          color: context.colors.surfaceContainerHighest,
          borderRadius: BorderRadius.circular(Radii.sm),
        ),
        child: Text(
          code,
          style: context.textTheme.bodySmall?.copyWith(
            fontFamily: 'GeistMono',
            color: context.colors.onSurface,
          ),
        ),
      ),
    );
  }

  TextSpan _linkSpan(
    String text,
    String url,
    TextStyle style,
    BuildContext context,
  ) {
    return TextSpan(
      text: text,
      style: style.copyWith(
        color: context.colors.primary,
        decoration: TextDecoration.underline,
        decorationColor: context.colors.primary,
      ),
      recognizer: TapGestureRecognizer()
        ..onTap = () {
          final uri = Uri.tryParse(url);
          if (uri != null) launchUrl(uri, mode: LaunchMode.externalApplication);
        },
    );
  }

  /// Render @mention as a highlighted span. Matches the mention text against
  /// known display names from the event's p-tags.
  InlineSpan _mentionSpan(String raw, BuildContext context) {
    // Strip the @ prefix for matching.
    final name = raw.substring(1).toLowerCase();

    // Try to find the mentioned user in our resolved names map.
    String? displayName;
    for (final entry in mentionNames.entries) {
      final entryName = entry.value.toLowerCase();
      final firstName = entryName.split(RegExp(r'\s+')).first;
      if (entryName == name || firstName == name) {
        displayName = entry.value;
        break;
      }
    }

    return WidgetSpan(
      alignment: PlaceholderAlignment.middle,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
        decoration: BoxDecoration(
          color: context.colors.primary.withValues(alpha: 0.15),
          borderRadius: BorderRadius.circular(Radii.sm),
        ),
        child: Text(
          '@${displayName ?? raw.substring(1)}',
          style: context.textTheme.bodyMedium?.copyWith(
            color: context.colors.primary,
          ),
        ),
      ),
    );
  }

  /// Render #channel as a tappable highlighted span.
  InlineSpan _channelSpan(String raw, BuildContext context) {
    final name = raw.substring(1).toLowerCase();

    // Look up channel ID.
    String? channelId;
    String? displayChannelName;
    for (final entry in channelNames.entries) {
      if (entry.key == name) {
        channelId = entry.value;
        displayChannelName = entry.key;
        break;
      }
    }

    return WidgetSpan(
      alignment: PlaceholderAlignment.middle,
      child: GestureDetector(
        onTap: channelId != null && onChannelTap != null
            ? () => onChannelTap!(channelId!)
            : null,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
          decoration: BoxDecoration(
            color: context.colors.primary.withValues(alpha: 0.15),
            borderRadius: BorderRadius.circular(Radii.sm),
          ),
          child: Text(
            '#${displayChannelName ?? raw.substring(1)}',
            style: context.textTheme.bodyMedium?.copyWith(
              color: context.colors.primary,
              fontWeight: FontWeight.w500,
            ),
          ),
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Block types
// ---------------------------------------------------------------------------

sealed class _Block {}

class _InlineBlock extends _Block {
  final String text;
  _InlineBlock(this.text);
}

class _CodeBlock extends _Block {
  final String language;
  final String code;
  _CodeBlock({required this.language, required this.code});
}

class _BlockquoteBlock extends _Block {
  final String text;
  _BlockquoteBlock(this.text);
}
