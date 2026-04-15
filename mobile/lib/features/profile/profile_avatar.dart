import 'package:flutter/material.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/theme/theme.dart';
import 'profile_provider.dart';
import 'user_profile.dart';

/// User avatar with a presence dot indicator, for use in the app bar.
class ProfileAvatar extends ConsumerWidget {
  const ProfileAvatar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final profileAsync = ref.watch(profileProvider);
    final presence =
        ref.watch(presenceProvider).whenData((v) => v).value ?? 'offline';

    return profileAsync.when(
      loading: () => const SizedBox(
        width: 32,
        height: 32,
        child: CircularProgressIndicator(strokeWidth: 2),
      ),
      error: (_, _) => _buildAvatar(context, null, presence),
      data: (profile) => _buildAvatar(context, profile, presence),
    );
  }

  Widget _buildAvatar(
    BuildContext context,
    UserProfile? profile,
    String presence,
  ) {
    return Padding(
      padding: const EdgeInsets.only(right: Grid.xxs),
      child: Stack(
        children: [
          CircleAvatar(
            radius: 16,
            backgroundColor: context.colors.primaryContainer,
            backgroundImage: profile?.avatarUrl != null
                ? NetworkImage(profile!.avatarUrl!)
                : null,
            child: profile?.avatarUrl == null
                ? Text(
                    profile?.initial ?? '?',
                    style: context.textTheme.labelMedium?.copyWith(
                      color: context.colors.onPrimaryContainer,
                    ),
                  )
                : null,
          ),
          Positioned(
            right: 0,
            bottom: 0,
            child: Container(
              width: 10,
              height: 10,
              decoration: BoxDecoration(
                color: _presenceColor(context, presence),
                shape: BoxShape.circle,
                border: Border.all(
                  color: context.theme.scaffoldBackgroundColor,
                  width: 1.5,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Color _presenceColor(BuildContext context, String presence) {
    return switch (presence) {
      'online' => context.appColors.success,
      'away' => context.appColors.warning,
      _ => context.colors.outline,
    };
  }
}
