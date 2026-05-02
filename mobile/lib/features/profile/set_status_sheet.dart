import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/theme/theme.dart';
import 'user_status.dart';
import 'user_status_provider.dart';

// ---------------------------------------------------------------------------
// Curated emoji list — matches desktop SetStatusDialog
// ---------------------------------------------------------------------------

const _emojiOptions = [
  (emoji: '\u{1F5E3}\u{FE0F}', label: 'In a meeting'),
  (emoji: '\u{1F68C}', label: 'Commuting'),
  (emoji: '\u{1F912}', label: 'Out sick'),
  (emoji: '\u{1F3D6}\u{FE0F}', label: 'Vacationing'),
  (emoji: '\u{1F3E0}', label: 'Working remotely'),
  (emoji: '\u{1F354}', label: 'Lunch'),
  (emoji: '\u{1F3AF}', label: 'Focus'),
  (emoji: '\u{1F4AA}', label: 'Exercising'),
];

const _presets = [
  (text: 'In a meeting', emoji: '\u{1F5E3}\u{FE0F}'),
  (text: 'Commuting', emoji: '\u{1F68C}'),
  (text: 'Out sick', emoji: '\u{1F912}'),
  (text: 'Vacationing', emoji: '\u{1F3D6}\u{FE0F}'),
  (text: 'Working remotely', emoji: '\u{1F3E0}'),
];

// ---------------------------------------------------------------------------
// Public helper to show the sheet
// ---------------------------------------------------------------------------

void showSetStatusSheet(BuildContext context, {UserStatus? currentStatus}) {
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (_) => _SetStatusSheet(currentStatus: currentStatus),
  );
}

// ---------------------------------------------------------------------------
// Sheet widget
// ---------------------------------------------------------------------------

class _SetStatusSheet extends HookConsumerWidget {
  final UserStatus? currentStatus;

  const _SetStatusSheet({this.currentStatus});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final textController = useTextEditingController(
      text: currentStatus?.text ?? '',
    );
    final emoji = useState(currentStatus?.emoji ?? '');
    final text = useState(currentStatus?.text ?? '');
    final isSaving = useState(false);

    useEffect(() {
      void listener() => text.value = textController.text;
      textController.addListener(listener);
      return () => textController.removeListener(listener);
    }, [textController]);

    final hasContent = text.value.trim().isNotEmpty || emoji.value.isNotEmpty;
    final hasExistingStatus = currentStatus != null && !currentStatus!.isEmpty;

    Future<void> handleSave() async {
      if (isSaving.value) return;
      isSaving.value = true;
      try {
        await ref
            .read(userStatusProvider.notifier)
            .setStatus(text.value, emoji.value);
        if (context.mounted) Navigator.of(context).pop();
      } finally {
        isSaving.value = false;
      }
    }

    Future<void> handleClear() async {
      if (isSaving.value) return;
      isSaving.value = true;
      try {
        await ref.read(userStatusProvider.notifier).clearStatus();
        if (context.mounted) Navigator.of(context).pop();
      } finally {
        isSaving.value = false;
      }
    }

    return Padding(
      padding: EdgeInsets.fromLTRB(
        Grid.xs,
        0,
        Grid.xs,
        MediaQuery.viewInsetsOf(context).bottom +
            MediaQuery.viewPaddingOf(context).bottom +
            Grid.xs,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Set a status', style: context.textTheme.titleMedium),
          const SizedBox(height: Grid.half),
          Text(
            'Let others know what you\u2019re up to.',
            style: context.textTheme.bodySmall?.copyWith(
              color: context.colors.onSurfaceVariant,
            ),
          ),
          const SizedBox(height: Grid.twelve),

          // Text input with emoji preview
          Row(
            children: [
              Container(
                width: 40,
                height: 40,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  border: Border.all(color: context.colors.outlineVariant),
                  borderRadius: BorderRadius.circular(Radii.md),
                ),
                child: Text(
                  emoji.value.isNotEmpty ? emoji.value : '\u{1F4AC}',
                  style: const TextStyle(fontSize: 18),
                ),
              ),
              const SizedBox(width: Grid.xxs),
              Expanded(
                child: TextField(
                  controller: textController,
                  autofocus: true,
                  decoration: const InputDecoration(
                    hintText: 'What\u2019s your status?',
                    border: OutlineInputBorder(),
                    contentPadding: EdgeInsets.symmetric(
                      horizontal: Grid.twelve,
                      vertical: Grid.xxs,
                    ),
                  ),
                  textInputAction: TextInputAction.done,
                  onSubmitted: (_) {
                    if (hasContent) handleSave();
                  },
                ),
              ),
            ],
          ),
          const SizedBox(height: Grid.twelve),

          // Emoji grid
          Wrap(
            spacing: Grid.half,
            runSpacing: Grid.half,
            children: [
              for (final option in _emojiOptions)
                _EmojiButton(
                  emoji: option.emoji,
                  label: option.label,
                  selected: emoji.value == option.emoji,
                  onTap: () {
                    emoji.value = emoji.value == option.emoji
                        ? ''
                        : option.emoji;
                  },
                ),
            ],
          ),
          const SizedBox(height: Grid.twelve),

          // Presets
          Wrap(
            spacing: Grid.half,
            runSpacing: Grid.half,
            children: [
              for (final preset in _presets)
                ActionChip(
                  label: Text('${preset.emoji} ${preset.text}'),
                  labelStyle: context.textTheme.labelSmall,
                  onPressed: () {
                    textController.text = preset.text;
                    emoji.value = preset.emoji;
                  },
                ),
            ],
          ),
          const SizedBox(height: Grid.xs),

          // Action buttons
          Row(
            children: [
              if (hasExistingStatus)
                TextButton(
                  onPressed: isSaving.value ? null : handleClear,
                  child: const Text('Clear status'),
                ),
              const Spacer(),
              TextButton(
                onPressed: isSaving.value
                    ? null
                    : () => Navigator.of(context).pop(),
                child: const Text('Cancel'),
              ),
              const SizedBox(width: Grid.xxs),
              FilledButton(
                onPressed: hasContent && !isSaving.value ? handleSave : null,
                child: const Text('Save'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _EmojiButton extends StatelessWidget {
  final String emoji;
  final String label;
  final bool selected;
  final VoidCallback onTap;

  const _EmojiButton({
    required this.emoji,
    required this.label,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: label,
      child: InkWell(
        borderRadius: BorderRadius.circular(Radii.md),
        onTap: onTap,
        child: Container(
          width: 36,
          height: 36,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: selected
                ? context.colors.secondaryContainer
                : Colors.transparent,
            borderRadius: BorderRadius.circular(Radii.md),
            border: selected ? Border.all(color: context.colors.outline) : null,
          ),
          child: Text(emoji, style: const TextStyle(fontSize: 18)),
        ),
      ),
    );
  }
}
