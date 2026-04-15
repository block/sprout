import 'package:flutter/material.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/auth/auth.dart';
import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';

class SettingsPage extends HookConsumerWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final config = ref.watch(relayConfigProvider);
    final selectedAccent = ref.watch(accentProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        padding: const EdgeInsets.all(Grid.xs),
        children: [
          // Connection info
          Text('Connection', style: context.textTheme.titleMedium),
          const SizedBox(height: Grid.twelve),
          ListTile(
            leading: const Icon(LucideIcons.server),
            title: const Text('Connected to'),
            subtitle: Text(
              config.baseUrl,
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
            ),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(Radii.md),
              side: BorderSide(color: context.colors.outlineVariant),
            ),
          ),
          const SizedBox(height: Grid.twelve),
          OutlinedButton.icon(
            onPressed: () => _confirmSignOut(context, ref),
            icon: const Icon(LucideIcons.logOut),
            label: const Text('Sign Out'),
            style: OutlinedButton.styleFrom(
              foregroundColor: context.colors.error,
            ),
          ),

          const SizedBox(height: Grid.sm),

          // Appearance
          Text('Appearance', style: context.textTheme.titleMedium),
          const SizedBox(height: Grid.twelve),
          SegmentedButton<ThemeMode>(
            segments: const [
              ButtonSegment(
                value: ThemeMode.light,
                icon: Icon(LucideIcons.sun),
                label: Text('Light'),
              ),
              ButtonSegment(
                value: ThemeMode.system,
                icon: Icon(LucideIcons.monitor),
                label: Text('System'),
              ),
              ButtonSegment(
                value: ThemeMode.dark,
                icon: Icon(LucideIcons.moon),
                label: Text('Dark'),
              ),
            ],
            selected: {ref.watch(themeProvider)},
            onSelectionChanged: (modes) {
              ref.read(themeProvider.notifier).setThemeMode(modes.first);
            },
          ),

          const SizedBox(height: Grid.xs),

          // Accent color picker
          Text('Accent Color', style: context.textTheme.titleSmall),
          const SizedBox(height: Grid.xxs),
          Wrap(
            spacing: Grid.xxs,
            runSpacing: Grid.xxs,
            children: [
              // Default (Mauve) swatch
              _AccentSwatch(
                color: context.colors.brightness == Brightness.light
                    ? const Color(0xFF8839EF)
                    : const Color(0xFFC6A0F6),
                label: 'Mauve',
                selected: selectedAccent == defaultAccentIndex,
                onTap: () => ref
                    .read(accentProvider.notifier)
                    .setAccent(defaultAccentIndex),
              ),
              // 8 accent colors from desktop
              for (var i = 0; i < accentColors.length; i++)
                _AccentSwatch(
                  color: context.colors.brightness == Brightness.light
                      ? accentColors[i].light
                      : accentColors[i].dark,
                  label: accentColors[i].name,
                  selected: selectedAccent == i,
                  onTap: () => ref.read(accentProvider.notifier).setAccent(i),
                ),
            ],
          ),
        ],
      ),
    );
  }

  void _confirmSignOut(BuildContext context, WidgetRef ref) {
    showDialog<void>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Sign Out'),
        content: const Text(
          'You will need to scan a new pairing code from your '
          'desktop app to reconnect.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () {
              Navigator.of(ctx).pop();
              ref.read(authProvider.notifier).signOut();
            },
            style: FilledButton.styleFrom(
              backgroundColor: context.colors.error,
            ),
            child: const Text('Sign Out'),
          ),
        ],
      ),
    );
  }
}

class _AccentSwatch extends StatelessWidget {
  const _AccentSwatch({
    required this.color,
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final Color color;
  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: label,
      child: GestureDetector(
        onTap: onTap,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 150),
          width: 36,
          height: 36,
          decoration: BoxDecoration(
            color: color,
            borderRadius: BorderRadius.circular(Radii.md),
            border: selected
                ? Border.all(color: context.colors.onSurface, width: 2.5)
                : Border.all(color: color.withValues(alpha: 0.4), width: 1),
          ),
          child: selected
              ? Icon(LucideIcons.check, size: 16, color: _contrastColor(color))
              : null,
        ),
      ),
    );
  }

  static Color _contrastColor(Color bg) {
    final lum = bg.computeLuminance();
    final contrastWithBlack = (lum + 0.05) / 0.05;
    final contrastWithWhite = 1.05 / (lum + 0.05);
    return contrastWithBlack >= contrastWithWhite
        ? const Color(0xFF000000)
        : const Color(0xFFFFFFFF);
  }
}
