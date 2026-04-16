import 'package:flutter/material.dart';
import 'package:gpt_markdown/gpt_markdown.dart';
import 'package:gpt_markdown/custom_widgets/markdown_config.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../shared/theme/theme.dart';

/// Renders message content with markdown formatting, @mentions, and
/// #channel links using [GptMarkdown] plus custom inline components for
/// Sprout-specific tokens.
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

    return GptMarkdown(
      content,
      style: style,
      followLinkColor: false,
      linkBuilder: (context, linkText, url, linkStyle) =>
          _buildLink(context, linkText, url, linkStyle, style),
      inlineComponents: [
        _MentionMd(mentionNames: mentionNames),
        _ChannelLinkMd(channelNames: channelNames, onChannelTap: onChannelTap),
        ...MarkdownComponent.inlineComponents,
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
}

class _MentionMd extends InlineMd {
  final Map<String, String> mentionNames;
  late final RegExp _exp = _buildPrefixPattern(
    prefix: '@',
    knownNames: _mentionAliases(mentionNames.values),
    genericTokenPattern: r'[A-Za-z0-9_][A-Za-z0-9_-]*',
  );

  _MentionMd({required this.mentionNames});

  @override
  RegExp get exp => _exp;

  @override
  InlineSpan span(
    BuildContext context,
    String text,
    final GptMarkdownConfig config,
  ) {
    final raw = exp.firstMatch(text.trim())?.group(0);
    if (raw == null) {
      return TextSpan(text: text, style: config.style);
    }

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

    return WidgetSpan(
      alignment: PlaceholderAlignment.baseline,
      baseline: TextBaseline.alphabetic,
      child: _TokenPill(
        text: '@${displayName ?? raw.substring(1)}',
        textStyle: config.style,
      ),
    );
  }
}

class _ChannelLinkMd extends InlineMd {
  final Map<String, String> channelNames;
  final void Function(String channelId)? onChannelTap;
  late final RegExp _exp = _buildPrefixPattern(
    prefix: '#',
    knownNames: channelNames.keys,
    genericTokenPattern: r'[A-Za-z0-9_][A-Za-z0-9_-]*',
  );

  _ChannelLinkMd({required this.channelNames, this.onChannelTap});

  @override
  RegExp get exp => _exp;

  @override
  InlineSpan span(
    BuildContext context,
    String text,
    final GptMarkdownConfig config,
  ) {
    final raw = exp.firstMatch(text.trim())?.group(0);
    if (raw == null) {
      return TextSpan(text: text, style: config.style);
    }

    final channelId = channelNames[raw.substring(1).toLowerCase()];
    final child = _TokenPill(
      text: raw,
      textStyle: config.style?.copyWith(fontWeight: FontWeight.w500),
    );

    return WidgetSpan(
      alignment: PlaceholderAlignment.baseline,
      baseline: TextBaseline.alphabetic,
      child: channelId != null && onChannelTap != null
          ? GestureDetector(onTap: () => onChannelTap!(channelId), child: child)
          : child,
    );
  }
}

class _TokenPill extends StatelessWidget {
  final String text;
  final TextStyle? textStyle;

  const _TokenPill({required this.text, this.textStyle});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
      decoration: BoxDecoration(
        color: context.colors.primary.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(Radii.sm),
      ),
      child: Text(
        text,
        style:
            textStyle?.copyWith(color: context.colors.primary) ??
            context.textTheme.bodyMedium?.copyWith(
              color: context.colors.primary,
            ),
      ),
    );
  }
}

RegExp _buildPrefixPattern({
  required String prefix,
  required Iterable<String> knownNames,
  required String genericTokenPattern,
}) {
  final names =
      knownNames
          .map((name) => name.trim())
          .where((name) => name.isNotEmpty)
          .toSet()
          .toList()
        ..sort((a, b) => b.length.compareTo(a.length));

  final escapedPrefix = RegExp.escape(prefix);
  const leadingBoundary = r'(?<![\w./:-])';
  const trailingBoundary = r'(?=$|[\s,;.!?:)\]}])';

  if (names.isEmpty) {
    return RegExp(
      '$leadingBoundary$escapedPrefix(?:$genericTokenPattern)$trailingBoundary',
      caseSensitive: false,
      multiLine: true,
    );
  }

  final knownAlternatives = names.map(RegExp.escape).join('|');
  return RegExp(
    '$leadingBoundary$escapedPrefix(?:(?:$knownAlternatives)$trailingBoundary|(?:$genericTokenPattern)$trailingBoundary)',
    caseSensitive: false,
    multiLine: true,
  );
}

Iterable<String> _mentionAliases(Iterable<String> mentionNames) sync* {
  for (final name in mentionNames) {
    final trimmed = name.trim();
    if (trimmed.isEmpty) continue;
    yield trimmed;
    final firstName = trimmed.split(RegExp(r'\s+')).first;
    if (firstName.isNotEmpty) {
      yield firstName;
    }
  }
}
