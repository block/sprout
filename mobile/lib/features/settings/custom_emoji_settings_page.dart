import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/media_upload.dart';
import '../../shared/theme/theme.dart';
import '../../shared/widgets/frosted_app_bar.dart';
import '../../shared/widgets/frosted_scaffold.dart';
import '../custom_emoji/custom_emoji.dart';
import '../custom_emoji/custom_emoji_provider.dart';
import '../custom_emoji/custom_emoji_render.dart';

/// Manage the signing user's own custom emoji (NIP-30 kind:30030 set):
/// upload an image, name it, and publish — or remove an existing one.
///
/// Reads/writes only the caller's own set via [CustomEmojiPaletteNotifier];
/// the workspace palette is the union of every member's set and is owned by
/// the renderer/picker surfaces.
class CustomEmojiSettingsPage extends HookConsumerWidget {
  const CustomEmojiSettingsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final notifier = ref.watch(customEmojiPaletteProvider.notifier);
    final ownFuture = useState<Future<List<CustomEmoji>>>(
      notifier.fetchOwnEmoji(),
    );
    final own = useFuture(ownFuture.value);
    final busy = useState(false);

    void reload() => ownFuture.value = notifier.fetchOwnEmoji();

    Future<void> add() async {
      if (busy.value) return;
      busy.value = true;
      try {
        final blob = await ref
            .read(mediaUploadServiceProvider)
            .pickAndUploadImage();
        if (blob == null || !context.mounted) return;
        final shortcode = await showDialog<String>(
          context: context,
          builder: (_) => const _ShortcodeDialog(),
        );
        if (shortcode == null || !context.mounted) return;
        await notifier.setEmoji(shortcode, blob.url);
        if (context.mounted) reload();
      } catch (error) {
        if (context.mounted) _showError(context, error);
      } finally {
        if (context.mounted) busy.value = false;
      }
    }

    Future<void> remove(CustomEmoji emoji) async {
      if (busy.value) return;
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (_) => AlertDialog(
          title: Text('Remove :${emoji.shortcode}:'),
          content: const Text(
            'This removes the emoji from your set. Messages that already use '
            'it keep working.',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(false),
              child: const Text('Cancel'),
            ),
            TextButton(
              onPressed: () => Navigator.of(context).pop(true),
              child: const Text('Remove'),
            ),
          ],
        ),
      );
      if (confirmed != true || !context.mounted) return;
      busy.value = true;
      try {
        await notifier.removeEmoji(emoji.shortcode);
        if (context.mounted) reload();
      } catch (error) {
        if (context.mounted) _showError(context, error);
      } finally {
        if (context.mounted) busy.value = false;
      }
    }

    return FrostedScaffold(
      appBar: FrostedAppBar(
        title: const Text('Custom Emoji'),
        actions: [
          IconButton(
            tooltip: 'Add emoji',
            icon: busy.value
                ? const SizedBox(
                    width: 18,
                    height: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(LucideIcons.plus),
            onPressed: busy.value ? null : add,
          ),
        ],
      ),
      body: _Body(
        snapshot: own,
        onAdd: add,
        onRemove: remove,
        adding: busy.value,
      ),
    );
  }
}

class _Body extends StatelessWidget {
  final AsyncSnapshot<List<CustomEmoji>> snapshot;
  final VoidCallback onAdd;
  final void Function(CustomEmoji) onRemove;
  final bool adding;

  const _Body({
    required this.snapshot,
    required this.onAdd,
    required this.onRemove,
    required this.adding,
  });

  @override
  Widget build(BuildContext context) {
    final emoji = snapshot.data;
    if (emoji == null) {
      return const Center(child: CircularProgressIndicator());
    }
    if (emoji.isEmpty) {
      return _Empty(onAdd: onAdd, adding: adding);
    }
    return ListView.separated(
      padding: EdgeInsets.only(
        top: frostedAppBarHeight(context) + Grid.xs,
        left: Grid.xs,
        right: Grid.xs,
        bottom: Grid.xs,
      ),
      itemCount: emoji.length,
      separatorBuilder: (_, _) => const SizedBox(height: Grid.xxs),
      itemBuilder: (context, i) {
        final e = emoji[i];
        return ListTile(
          leading: CustomEmojiImage(
            shortcode: e.shortcode,
            url: e.url,
            size: 28,
          ),
          title: Text(':${e.shortcode}:'),
          trailing: IconButton(
            icon: const Icon(LucideIcons.trash2, size: 18),
            color: context.colors.error,
            onPressed: adding ? null : () => onRemove(e),
          ),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Radii.md),
            side: BorderSide(color: context.colors.outlineVariant),
          ),
        );
      },
    );
  }
}

class _Empty extends StatelessWidget {
  final VoidCallback onAdd;
  final bool adding;

  const _Empty({required this.onAdd, required this.adding});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Grid.md),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              LucideIcons.smilePlus,
              size: 40,
              color: context.colors.onSurfaceVariant,
            ),
            const SizedBox(height: Grid.twelve),
            Text('No custom emoji yet', style: context.textTheme.titleMedium),
            const SizedBox(height: Grid.half),
            Text(
              'Upload an image and give it a :shortcode: to use it anywhere '
              'in this workspace.',
              textAlign: TextAlign.center,
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: Grid.xs),
            FilledButton.icon(
              onPressed: adding ? null : onAdd,
              icon: const Icon(LucideIcons.plus),
              label: const Text('Add emoji'),
            ),
          ],
        ),
      ),
    );
  }
}

/// Prompt for a shortcode after an image upload. Validates with the shared
/// [normalizeShortcode] so the name matches what the relay stores.
class _ShortcodeDialog extends HookWidget {
  const _ShortcodeDialog();

  @override
  Widget build(BuildContext context) {
    final controller = useTextEditingController();
    final error = useState<String?>(null);

    void confirm() {
      final normalized = normalizeShortcode(controller.text);
      if (normalized == null) {
        error.value =
            'Use letters, numbers, hyphen, or underscore (no spaces).';
        return;
      }
      Navigator.of(context).pop(normalized);
    }

    return AlertDialog(
      title: const Text('Name your emoji'),
      content: TextField(
        controller: controller,
        autofocus: true,
        autocorrect: false,
        decoration: InputDecoration(
          labelText: 'Shortcode',
          prefixText: ':',
          suffixText: ':',
          errorText: error.value,
        ),
        onChanged: (_) {
          if (error.value != null) error.value = null;
        },
        onSubmitted: (_) => confirm(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        TextButton(onPressed: confirm, child: const Text('Add')),
      ],
    );
  }
}

void _showError(BuildContext context, Object error) {
  final message = error is ArgumentError
      ? (error.message?.toString() ?? 'Invalid emoji')
      : error.toString().replaceFirst('Exception: ', '');
  ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(message)));
}
