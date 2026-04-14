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
              borderRadius: BorderRadius.circular(12),
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
