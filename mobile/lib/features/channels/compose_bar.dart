import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/theme/theme.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';
import 'channel_management_provider.dart';

/// Rich compose bar with @mention autocomplete, emoji picker, and a markdown
/// formatting toolbar. Used in both channel and thread views — the caller
/// provides an [onSend] callback that handles actual message submission.
class ComposeBar extends HookConsumerWidget {
  final String channelId;
  final String channelName;
  final String? hintText;
  final Future<void> Function(String content) onSend;

  const ComposeBar({
    super.key,
    required this.channelId,
    this.channelName = '',
    this.hintText,
    required this.onSend,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = useTextEditingController();
    final focusNode = useFocusNode();
    final isSending = useState(false);
    final showFormatting = useState(false);

    final resolvedHint =
        hintText ??
        (channelName.isNotEmpty ? 'Message #$channelName' : 'Message\u2026');

    // Mention state --------------------------------------------------------
    final mentionQuery = useState<String?>(null);
    final mentionStartIdx = useState(-1);

    final membersAsync = ref.watch(channelMembersProvider(channelId));
    final currentPubkey = ref.watch(currentPubkeyProvider);
    final userCache = ref.watch(userCacheProvider);

    // Detect @mention query on every text / selection change.
    useEffect(() {
      void listener() {
        final text = controller.text;
        final sel = controller.selection;
        if (!sel.isValid || !sel.isCollapsed) {
          mentionQuery.value = null;
          return;
        }
        final cursor = sel.baseOffset;
        if (cursor < 1) {
          mentionQuery.value = null;
          return;
        }

        // Walk backward from cursor looking for a bare `@` at a word boundary.
        int? atPos;
        for (var i = cursor - 1; i >= 0; i--) {
          final ch = text[i];
          if (ch == '\n') break;
          if (ch == '@') {
            if (i == 0 || text[i - 1] == ' ' || text[i - 1] == '\n') {
              atPos = i;
            }
            break;
          }
        }

        if (atPos != null) {
          mentionQuery.value = text.substring(atPos + 1, cursor).toLowerCase();
          mentionStartIdx.value = atPos;
        } else {
          mentionQuery.value = null;
        }
      }

      controller.addListener(listener);
      return () => controller.removeListener(listener);
    }, [controller]);

    // Filter channel members against the query.
    final members = membersAsync.asData?.value ?? <ChannelMember>[];
    final suggestions = _filterMembers(
      members,
      mentionQuery.value,
      currentPubkey,
    );

    // Insert a selected mention into the text field.
    void insertMention(ChannelMember member) {
      final name = member.displayName?.trim().isNotEmpty == true
          ? member.displayName!.trim()
          : member.pubkey.substring(0, 8);
      final text = controller.text;
      final start = mentionStartIdx.value;
      final cursor = controller.selection.baseOffset;

      final before = text.substring(0, start);
      final after = text.substring(cursor);
      final mention = '@$name ';

      controller.text = '$before$mention$after';
      controller.selection = TextSelection.collapsed(
        offset: start + mention.length,
      );
      mentionQuery.value = null;
      focusNode.requestFocus();
    }

    // Insert `@` at the cursor to manually trigger mention mode.
    void triggerMention() {
      final text = controller.text;
      final cursor = controller.selection.isValid
          ? controller.selection.baseOffset
          : text.length;
      final needsSpace =
          cursor > 0 && text[cursor - 1] != ' ' && text[cursor - 1] != '\n';
      final insert = needsSpace ? ' @' : '@';
      final before = text.substring(0, cursor);
      final after = text.substring(cursor);
      controller.text = '$before$insert$after';
      controller.selection = TextSelection.collapsed(
        offset: cursor + insert.length,
      );
      focusNode.requestFocus();
    }

    // Send the message.
    Future<void> send() async {
      final text = controller.text.trim();
      if (text.isEmpty || isSending.value) return;

      isSending.value = true;
      try {
        await onSend(text);
        if (context.mounted) {
          controller.clear();
          mentionQuery.value = null;
          showFormatting.value = false;
        }
      } finally {
        if (context.mounted) isSending.value = false;
      }
    }

    // Insert an emoji at the cursor.
    void insertEmoji(String emoji) {
      final text = controller.text;
      final cursor = controller.selection.isValid
          ? controller.selection.baseOffset
          : text.length;
      final before = text.substring(0, cursor);
      final after = text.substring(cursor);
      controller.text = '$before$emoji$after';
      controller.selection = TextSelection.collapsed(
        offset: cursor + emoji.length,
      );
      focusNode.requestFocus();
    }

    // Wrap (or insert) markdown formatting around the current selection.
    void applyFormat(String prefix, [String? suffix]) {
      suffix ??= prefix;
      final text = controller.text;
      final sel = controller.selection;
      if (!sel.isValid) return;

      if (sel.isCollapsed) {
        final offset = sel.baseOffset;
        final updated =
            '${text.substring(0, offset)}$prefix$suffix${text.substring(offset)}';
        controller.text = updated;
        controller.selection = TextSelection.collapsed(
          offset: offset + prefix.length,
        );
      } else {
        final selected = text.substring(sel.start, sel.end);
        final updated =
            '${text.substring(0, sel.start)}$prefix$selected$suffix${text.substring(sel.end)}';
        controller.text = updated;
        controller.selection = TextSelection.collapsed(
          offset: sel.start + prefix.length + selected.length + suffix.length,
        );
      }
      focusNode.requestFocus();
    }

    // ----- Widget tree ----------------------------------------------------

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        // Mention suggestions (above the compose chrome).
        if (suggestions.isNotEmpty)
          _MentionSuggestions(
            suggestions: suggestions,
            userCache: userCache,
            currentPubkey: currentPubkey,
            onSelect: insertMention,
          ),

        // Compose chrome — bottom-sheet style container.
        Container(
          decoration: BoxDecoration(
            color: context.colors.surfaceContainerHighest,
            borderRadius: suggestions.isEmpty
                ? const BorderRadius.vertical(
                    top: Radius.circular(Radii.dialog),
                  )
                : BorderRadius.zero,
            boxShadow: suggestions.isEmpty
                ? [
                    BoxShadow(
                      color: context.colors.shadow.withValues(alpha: 0.08),
                      blurRadius: 8,
                      offset: const Offset(0, -2),
                    ),
                  ]
                : null,
          ),
          padding: EdgeInsets.only(
            left: Grid.xs,
            right: Grid.xs,
            top: Grid.xs,
            bottom: MediaQuery.viewPaddingOf(context).bottom + Grid.twelve,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              // Formatting toolbar (toggled via Aa button).
              if (showFormatting.value)
                _FormattingToolbar(onFormat: applyFormat),

              // Row 1 — text input (full width, grows).
              TextField(
                controller: controller,
                focusNode: focusNode,
                textInputAction: TextInputAction.send,
                onSubmitted: (_) => send(),
                minLines: 1,
                maxLines: 5,
                style: context.textTheme.bodyMedium,
                decoration: InputDecoration(
                  hintText: resolvedHint,
                  hintStyle: context.textTheme.bodyMedium?.copyWith(
                    color: context.colors.onSurfaceVariant,
                  ),
                  border: InputBorder.none,
                  enabledBorder: InputBorder.none,
                  focusedBorder: InputBorder.none,
                  contentPadding: const EdgeInsets.symmetric(
                    horizontal: Grid.half,
                    vertical: Grid.half,
                  ),
                  isDense: true,
                ),
              ),

              const SizedBox(height: Grid.xxs),

              // Row 2 — action buttons [paperclip, emoji, @, Aa] ... [send].
              Row(
                children: [
                  _ComposeAction(
                    icon: LucideIcons.paperclip,
                    onTap: () {}, // attachment placeholder
                  ),
                  _ComposeAction(
                    icon: LucideIcons.smilePlus,
                    onTap: () => _showEmojiPicker(
                      context: context,
                      onSelect: insertEmoji,
                    ),
                  ),
                  _ComposeAction(
                    icon: LucideIcons.atSign,
                    onTap: triggerMention,
                  ),
                  _ComposeAction(
                    icon: LucideIcons.aLargeSmall,
                    active: showFormatting.value,
                    onTap: () => showFormatting.value = !showFormatting.value,
                  ),
                  const Spacer(),
                  _SendButton(isSending: isSending.value, onTap: send),
                ],
              ),
            ],
          ),
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Emoji picker
// ---------------------------------------------------------------------------

void _showEmojiPicker({
  required BuildContext context,
  required void Function(String emoji) onSelect,
}) {
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    backgroundColor: Theme.of(context).colorScheme.surfaceContainerHighest,
    builder: (sheetContext) => _EmojiPickerSheet(
      onSelect: (emoji) {
        Navigator.of(sheetContext).pop();
        onSelect(emoji);
      },
    ),
  );
}

/// Emoji categories for the picker. System Unicode emoji — no packages needed.
const _emojiCategories = <({String label, IconData icon, List<String> emoji})>[
  (
    label: 'Recent',
    icon: LucideIcons.clock,
    emoji: [
      '\u{1F44D}',
      '\u{2764}\u{FE0F}',
      '\u{1F602}',
      '\u{1F389}',
      '\u{1F440}',
      '\u{1F64F}',
      '\u{1F525}',
      '\u{2705}',
    ],
  ),
  (
    label: 'Smileys',
    icon: LucideIcons.smile,
    emoji: [
      '\u{1F600}',
      '\u{1F603}',
      '\u{1F604}',
      '\u{1F601}',
      '\u{1F605}',
      '\u{1F602}',
      '\u{1F923}',
      '\u{1F607}',
      '\u{1F60A}',
      '\u{1F60D}',
      '\u{1F618}',
      '\u{1F617}',
      '\u{1F61A}',
      '\u{1F619}',
      '\u{1F60B}',
      '\u{1F61B}',
      '\u{1F61D}',
      '\u{1F61C}',
      '\u{1F911}',
      '\u{1F917}',
      '\u{1F914}',
      '\u{1F910}',
      '\u{1F928}',
      '\u{1F610}',
      '\u{1F611}',
      '\u{1F636}',
      '\u{1F60F}',
      '\u{1F612}',
      '\u{1F644}',
      '\u{1F62C}',
      '\u{1F925}',
      '\u{1F60C}',
      '\u{1F614}',
      '\u{1F62A}',
      '\u{1F924}',
      '\u{1F634}',
      '\u{1F637}',
      '\u{1F912}',
      '\u{1F915}',
      '\u{1F922}',
      '\u{1F92E}',
      '\u{1F927}',
      '\u{1F975}',
      '\u{1F976}',
      '\u{1F974}',
      '\u{1F635}',
      '\u{1F92F}',
      '\u{1F920}',
      '\u{1F973}',
      '\u{1F978}',
    ],
  ),
  (
    label: 'Gestures',
    icon: LucideIcons.hand,
    emoji: [
      '\u{1F44D}',
      '\u{1F44E}',
      '\u{1F44A}',
      '\u{270A}',
      '\u{1F91B}',
      '\u{1F91C}',
      '\u{1F44F}',
      '\u{1F64C}',
      '\u{1F450}',
      '\u{1F64F}',
      '\u{1F91D}',
      '\u{270C}\u{FE0F}',
      '\u{1F91E}',
      '\u{1F91F}',
      '\u{1F918}',
      '\u{1F448}',
      '\u{1F449}',
      '\u{1F446}',
      '\u{1F447}',
      '\u{261D}\u{FE0F}',
      '\u{1F4AA}',
      '\u{1F44B}',
      '\u{1F590}\u{FE0F}',
    ],
  ),
  (
    label: 'Objects',
    icon: LucideIcons.lightbulb,
    emoji: [
      '\u{2764}\u{FE0F}',
      '\u{1F525}',
      '\u{2B50}',
      '\u{1F31F}',
      '\u{1F4A5}',
      '\u{1F389}',
      '\u{1F38A}',
      '\u{1F3C6}',
      '\u{1F947}',
      '\u{1F4A1}',
      '\u{1F4AF}',
      '\u{2705}',
      '\u{274C}',
      '\u{26A0}\u{FE0F}',
      '\u{1F6A8}',
      '\u{1F4DD}',
      '\u{1F4CB}',
      '\u{1F4CC}',
      '\u{1F517}',
      '\u{1F4E3}',
      '\u{1F514}',
      '\u{1F3B5}',
      '\u{1F3B6}',
      '\u{1F680}',
    ],
  ),
  (
    label: 'Nature',
    icon: LucideIcons.sprout,
    emoji: [
      '\u{1F331}',
      '\u{1F332}',
      '\u{1F333}',
      '\u{1F334}',
      '\u{1F335}',
      '\u{1F33B}',
      '\u{1F33A}',
      '\u{1F337}',
      '\u{1F339}',
      '\u{1F340}',
      '\u{1F341}',
      '\u{1F343}',
      '\u{1F31E}',
      '\u{1F308}',
      '\u{2600}\u{FE0F}',
      '\u{1F327}\u{FE0F}',
      '\u{26A1}',
      '\u{2744}\u{FE0F}',
      '\u{1F30A}',
      '\u{1F436}',
      '\u{1F431}',
      '\u{1F98A}',
      '\u{1F42C}',
      '\u{1F985}',
    ],
  ),
];

class _EmojiPickerSheet extends StatefulWidget {
  final void Function(String emoji) onSelect;

  const _EmojiPickerSheet({required this.onSelect});

  @override
  State<_EmojiPickerSheet> createState() => _EmojiPickerSheetState();
}

class _EmojiPickerSheetState extends State<_EmojiPickerSheet> {
  /// -1 = "All", 0..N = specific category.
  int _selectedCategory = -1;

  static final _allEmoji = () {
    final seen = <String>{};
    return [
      for (final cat in _emojiCategories)
        for (final e in cat.emoji)
          if (seen.add(e)) e,
    ];
  }();

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final emoji = _selectedCategory < 0
        ? _allEmoji
        : _emojiCategories[_selectedCategory].emoji;

    return SizedBox(
      height: 340,
      child: Column(
        children: [
          // Category icon bar.
          SizedBox(
            height: 40,
            child: Row(
              children: [
                const SizedBox(width: Grid.twelve),
                _CategoryIcon(
                  icon: LucideIcons.layoutGrid,
                  selected: _selectedCategory < 0,
                  onTap: () => setState(() => _selectedCategory = -1),
                ),
                for (var i = 0; i < _emojiCategories.length; i++)
                  _CategoryIcon(
                    icon: _emojiCategories[i].icon,
                    selected: _selectedCategory == i,
                    onTap: () => setState(() => _selectedCategory = i),
                  ),
              ],
            ),
          ),
          Divider(height: 1, color: colors.outlineVariant),
          const SizedBox(height: Grid.xxs),
          // Emoji grid.
          Expanded(
            child: GridView.builder(
              padding: const EdgeInsets.symmetric(horizontal: Grid.xs),
              gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: 8,
                mainAxisSpacing: Grid.half,
                crossAxisSpacing: Grid.half,
              ),
              itemCount: emoji.length,
              itemBuilder: (context, index) {
                final e = emoji[index];
                return GestureDetector(
                  onTap: () => widget.onSelect(e),
                  child: Center(
                    child: Text(e, style: const TextStyle(fontSize: 28)),
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

class _CategoryIcon extends StatelessWidget {
  final IconData icon;
  final bool selected;
  final VoidCallback onTap;

  const _CategoryIcon({
    required this.icon,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return SizedBox(
      width: 40,
      height: 40,
      child: IconButton(
        onPressed: onTap,
        icon: Icon(
          icon,
          size: 18,
          color: selected ? colors.primary : colors.onSurfaceVariant,
        ),
        padding: EdgeInsets.zero,
        visualDensity: VisualDensity.compact,
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Mention suggestions
// ---------------------------------------------------------------------------

List<ChannelMember> _filterMembers(
  List<ChannelMember> members,
  String? query,
  String? currentPubkey,
) {
  if (query == null) return const [];
  final q = query.toLowerCase();
  return members
      .where(
        (m) =>
            currentPubkey == null ||
            m.pubkey.toLowerCase() != currentPubkey.toLowerCase(),
      )
      .where((m) {
        if (q.isEmpty) return true;
        final name = (m.displayName ?? '').toLowerCase();
        final firstName = name.split(RegExp(r'\s+')).first;
        return name.startsWith(q) ||
            firstName.startsWith(q) ||
            name.contains(q);
      })
      .take(6)
      .toList();
}

class _MentionSuggestions extends StatelessWidget {
  final List<ChannelMember> suggestions;
  final Map<String, UserProfile> userCache;
  final String? currentPubkey;
  final void Function(ChannelMember) onSelect;

  const _MentionSuggestions({
    required this.suggestions,
    required this.userCache,
    required this.currentPubkey,
    required this.onSelect,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      constraints: const BoxConstraints(maxHeight: 240),
      clipBehavior: Clip.hardEdge,
      decoration: BoxDecoration(
        color: context.colors.surfaceContainerHighest,
        borderRadius: const BorderRadius.vertical(
          top: Radius.circular(Radii.dialog),
        ),
        boxShadow: [
          BoxShadow(
            color: context.colors.shadow.withValues(alpha: 0.08),
            blurRadius: 8,
            offset: const Offset(0, -2),
          ),
        ],
      ),
      child: ListView.separated(
        shrinkWrap: true,
        padding: const EdgeInsets.symmetric(vertical: Grid.xxs),
        itemCount: suggestions.length,
        separatorBuilder: (_, _) => const SizedBox.shrink(),
        itemBuilder: (context, index) {
          final member = suggestions[index];
          final name = member.labelFor(currentPubkey);
          final profile = userCache[member.pubkey.toLowerCase()];
          final avatarUrl = profile?.avatarUrl;
          final initial = (member.displayName ?? member.pubkey)[0]
              .toUpperCase();

          return ListTile(
            dense: true,
            visualDensity: VisualDensity.compact,
            leading: CircleAvatar(
              radius: 14,
              backgroundColor: context.colors.primaryContainer,
              backgroundImage: avatarUrl != null
                  ? NetworkImage(avatarUrl)
                  : null,
              child: avatarUrl == null
                  ? Text(
                      initial,
                      style: context.textTheme.labelSmall?.copyWith(
                        color: context.colors.onPrimaryContainer,
                      ),
                    )
                  : null,
            ),
            title: Text(name, style: context.textTheme.bodyMedium),
            trailing: member.isBot
                ? Icon(LucideIcons.bot, size: 14, color: context.colors.outline)
                : null,
            onTap: () => onSelect(member),
          );
        },
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Formatting toolbar
// ---------------------------------------------------------------------------

class _FormattingToolbar extends StatelessWidget {
  final void Function(String prefix, [String? suffix]) onFormat;

  const _FormattingToolbar({required this.onFormat});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: Grid.half),
      child: Row(
        children: [
          _FormatButton(
            icon: LucideIcons.bold,
            tooltip: 'Bold',
            onTap: () => onFormat('**'),
          ),
          _FormatButton(
            icon: LucideIcons.italic,
            tooltip: 'Italic',
            onTap: () => onFormat('_'),
          ),
          _FormatButton(
            icon: LucideIcons.strikethrough,
            tooltip: 'Strikethrough',
            onTap: () => onFormat('~~'),
          ),
          _FormatButton(
            icon: LucideIcons.code,
            tooltip: 'Code',
            onTap: () => onFormat('`'),
          ),
        ],
      ),
    );
  }
}

class _FormatButton extends StatelessWidget {
  final IconData icon;
  final String tooltip;
  final VoidCallback onTap;

  const _FormatButton({
    required this.icon,
    required this.tooltip,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: tooltip,
      child: InkWell(
        borderRadius: BorderRadius.circular(Radii.sm),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(Grid.xxs),
          child: Icon(icon, size: 18, color: context.colors.onSurfaceVariant),
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Compose action buttons
// ---------------------------------------------------------------------------

class _ComposeAction extends StatelessWidget {
  final IconData icon;
  final bool active;
  final VoidCallback onTap;

  const _ComposeAction({
    required this.icon,
    this.active = false,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 36,
      height: 36,
      child: IconButton(
        onPressed: onTap,
        icon: Icon(
          icon,
          size: 20,
          color: active
              ? context.colors.primary
              : context.colors.onSurfaceVariant,
        ),
        padding: EdgeInsets.zero,
        visualDensity: VisualDensity.compact,
      ),
    );
  }
}

class _SendButton extends StatelessWidget {
  final bool isSending;
  final VoidCallback onTap;

  const _SendButton({required this.isSending, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 36,
      height: 36,
      child: IconButton(
        onPressed: isSending ? null : onTap,
        style: IconButton.styleFrom(
          backgroundColor: context.colors.primary,
          disabledBackgroundColor: context.colors.primary.withValues(
            alpha: 0.5,
          ),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Radii.md),
          ),
        ),
        padding: EdgeInsets.zero,
        icon: isSending
            ? SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  color: context.colors.onPrimary,
                ),
              )
            : Icon(
                LucideIcons.sendHorizontal,
                size: 18,
                color: context.colors.onPrimary,
              ),
      ),
    );
  }
}
