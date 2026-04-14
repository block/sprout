import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';

class SettingsPage extends HookConsumerWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final config = ref.watch(relayConfigProvider);
    final urlController = useTextEditingController(text: config.baseUrl);
    final tokenController = useTextEditingController(
      text: config.apiToken ?? '',
    );
    final pubkeyController = useTextEditingController(
      text: config.devPubkey ?? '',
    );

    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        padding: const EdgeInsets.all(Grid.xs),
        children: [
          Text('Relay Connection', style: context.textTheme.titleMedium),
          const SizedBox(height: Grid.twelve),
          TextField(
            controller: urlController,
            decoration: const InputDecoration(
              labelText: 'Relay URL',
              hintText: 'http://localhost:3000',
              prefixIcon: Icon(LucideIcons.server),
            ),
            keyboardType: TextInputType.url,
            autocorrect: false,
          ),
          const SizedBox(height: Grid.twelve),
          TextField(
            controller: tokenController,
            decoration: const InputDecoration(
              labelText: 'API Token (optional)',
              hintText: 'sprout_...',
              prefixIcon: Icon(LucideIcons.key),
            ),
            obscureText: true,
            autocorrect: false,
          ),
          const SizedBox(height: Grid.twelve),
          TextField(
            controller: pubkeyController,
            decoration: InputDecoration(
              labelText: 'Dev Pubkey (hex, for local relay)',
              hintText: '3bf0c63...',
              prefixIcon: const Icon(LucideIcons.userRound),
            ),
            autocorrect: false,
          ),
          const SizedBox(height: Grid.xs),
          FilledButton(
            onPressed: () {
              final token = tokenController.text.trim();
              final pubkey = pubkeyController.text.trim();
              ref
                  .read(relayConfigProvider.notifier)
                  .update(
                    baseUrl: urlController.text.trim(),
                    apiToken: token.isEmpty ? null : token,
                    devPubkey: pubkey.isEmpty ? null : pubkey,
                  );
              ScaffoldMessenger.of(context).showSnackBar(
                const SnackBar(content: Text('Relay config updated')),
              );
            },
            child: const Text('Save'),
          ),
          const SizedBox(height: Grid.sm),
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
}
