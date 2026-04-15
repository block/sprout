import 'package:flutter/material.dart';
import 'package:gpt_markdown/gpt_markdown.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../shared/theme/theme.dart';

/// Renders message content with markdown formatting, @mentions, and
/// #channel links using [GptMarkdown] for standard markdown and a
/// pre-processing step for Sprout-specific tokens.
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
        context.textTheme.bodyMedium?.copyWith(color: context.colors.onSurface);

    // If the content has mentions or channel refs, we need a custom component
    // builder to render them. Otherwise, just use plain GptMarkdown.
    final hasMentions = _mentionPattern.hasMatch(content);
    final hasChannels = _channelPattern.hasMatch(content);

    if (!hasMentions && !hasChannels) {
      return _buildMarkdown(context, content, style);
    }

    // Pre-process: split content into segments of plain text, @mentions,
    // and #channels. Render each appropriately.
    return _buildWithTokens(context, content, style);
  }

  Widget _buildMarkdown(BuildContext context, String text, TextStyle? style) {
    return GptMarkdown(
      text,
      style: style,
      followLinkColor: false,
      linkBuilder: (context, linkText, url, linkStyle) =>
          _buildLink(context, linkText, url, linkStyle, style),
    );
  }

  /// Build content that contains @mentions and/or #channel tokens.
  /// We split on these tokens and render a Column of GptMarkdown widgets
  /// interspersed with mention/channel pills.
  Widget _buildWithTokens(BuildContext context, String text, TextStyle? style) {
    final segments = _tokenize(text);

    // If all segments fit on one line (no block-level markdown), use a Wrap.
    // Otherwise fall back to Column.
    final hasBlockContent = segments.any(
      (s) => s.type == _TokenType.text && _hasBlockMarkdown(s.value),
    );

    if (hasBlockContent) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final segment in segments)
            switch (segment.type) {
              _TokenType.text => _buildMarkdown(context, segment.value, style),
              _TokenType.mention => _buildMentionPill(context, segment.value),
              _TokenType.channel => _buildChannelPill(context, segment.value),
            },
        ],
      );
    }

    // Inline-only: use Wrap so mentions/channels flow with text.
    return Wrap(
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        for (final segment in segments)
          switch (segment.type) {
            _TokenType.text => _buildMarkdown(context, segment.value, style),
            _TokenType.mention => _buildMentionPill(context, segment.value),
            _TokenType.channel => _buildChannelPill(context, segment.value),
          },
      ],
    );
  }

  // ---------------------------------------------------------------------------
  // Link builder (URL scheme validation)
  // ---------------------------------------------------------------------------

  Widget _buildLink(
    BuildContext context,
    InlineSpan linkText,
    String url,
    TextStyle linkStyle,
    TextStyle? fallbackStyle,
  ) {
    String text = '';
    linkText.visitChildren((span) {
      if (span is TextSpan && span.text != null) {
        text += span.text!;
      }
      return true;
    });

    final baseStyle = fallbackStyle ?? linkStyle;

    return GestureDetector(
      onTap: () {
        final uri = Uri.tryParse(url);
        if (uri != null && (uri.scheme == 'http' || uri.scheme == 'https')) {
          launchUrl(uri, mode: LaunchMode.externalApplication);
        }
      },
      child: Text(
        text,
        style: baseStyle.copyWith(
          color: context.colors.primary,
          decoration: TextDecoration.underline,
          decorationColor: context.colors.primary,
        ),
      ),
    );
  }

  // ---------------------------------------------------------------------------
  // Mention / channel pills
  // ---------------------------------------------------------------------------

  Widget _buildMentionPill(BuildContext context, String raw) {
    final name = raw.substring(1).toLowerCase();

    String? displayName;
    for (final entry in mentionNames.entries) {
      final entryName = entry.value.toLowerCase();
      final firstName = entryName.split(RegExp(r'\s+')).first;
      if (entryName == name || firstName == name) {
        displayName = entry.value;
        break;
      }
    }

    return Container(
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
    );
  }

  Widget _buildChannelPill(BuildContext context, String raw) {
    final name = raw.substring(1).toLowerCase();

    String? channelId;
    String? displayChannelName;
    for (final entry in channelNames.entries) {
      if (entry.key == name) {
        channelId = entry.value;
        displayChannelName = entry.key;
        break;
      }
    }

    return GestureDetector(
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
    );
  }

  // ---------------------------------------------------------------------------
  // Tokenizer
  // ---------------------------------------------------------------------------

  static final _mentionPattern = RegExp(r'@\S+');
  static final _channelPattern = RegExp(r'#\S+');
  static final _tokenPattern = RegExp(r'(@\S+|#\S+)');

  static bool _hasBlockMarkdown(String text) {
    return text.contains('```') ||
        text.contains('\n> ') ||
        text.startsWith('> ') ||
        text.contains('\n- ') ||
        text.startsWith('- ') ||
        text.contains('\n1. ') ||
        text.startsWith('1. ') ||
        RegExp(r'^#{1,3}\s', multiLine: true).hasMatch(text);
  }

  List<_Token> _tokenize(String text) {
    final tokens = <_Token>[];
    var lastEnd = 0;

    for (final match in _tokenPattern.allMatches(text)) {
      if (match.start > lastEnd) {
        tokens.add(
          _Token(_TokenType.text, text.substring(lastEnd, match.start)),
        );
      }

      final value = match.group(0)!;
      if (value.startsWith('@')) {
        tokens.add(_Token(_TokenType.mention, value));
      } else {
        tokens.add(_Token(_TokenType.channel, value));
      }

      lastEnd = match.end;
    }

    if (lastEnd < text.length) {
      tokens.add(_Token(_TokenType.text, text.substring(lastEnd)));
    }

    return tokens;
  }
}

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

enum _TokenType { text, mention, channel }

class _Token {
  final _TokenType type;
  final String value;
  const _Token(this.type, this.value);
}
