import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/theme/theme.dart';

class ActivityPage extends StatelessWidget {
  const ActivityPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Activity')),
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              LucideIcons.bell,
              size: Grid.xl,
              color: context.colors.outline,
            ),
            const SizedBox(height: Grid.xs),
            Text(
              'No activity yet',
              style: context.textTheme.bodyLarge?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: Grid.half),
            Text(
              'Mentions, replies, and reactions will show up here.',
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.outline,
              ),
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
    );
  }
}
