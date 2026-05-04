import 'package:flutter/material.dart';

import '../../shared/theme/theme.dart';

/// A centered label between two horizontal dividers, used to separate
/// messages by calendar day ("TODAY", "YESTERDAY", full dates).
class DayDivider extends StatelessWidget {
  final String label;

  const DayDivider({super.key, required this.label});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Grid.xxs),
      child: Row(
        children: [
          Expanded(child: Divider(color: context.colors.outlineVariant)),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Grid.xxs),
            child: Text(
              label.toUpperCase(),
              style: context.textTheme.labelSmall?.copyWith(
                color: context.colors.onSurfaceVariant,
                letterSpacing: 2.0,
              ),
            ),
          ),
          Expanded(child: Divider(color: context.colors.outlineVariant)),
        ],
      ),
    );
  }
}
