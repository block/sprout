import 'package:flutter/material.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/theme/theme.dart';

class HomePage extends HookConsumerWidget {
  const HomePage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Sprout'),
        actions: [
          IconButton(
            icon: const Icon(LucideIcons.sun),
            onPressed: () => ref.read(themeProvider.notifier).toggleTheme(),
          ),
        ],
      ),
      body: Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text('Sprout', style: context.textTheme.headlineMedium),
            const SizedBox(height: Grid.xxs),
            Text(
              'Mobile',
              style: context.textTheme.bodyLarge?.copyWith(
                color: context.colors.secondary,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
