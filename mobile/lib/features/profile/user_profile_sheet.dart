import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';
import '../../shared/utils/string_utils.dart';
import 'presence_cache_provider.dart';
import 'user_cache_provider.dart';
import 'user_status_cache_provider.dart';

/// Show a user profile bottom sheet for the given [pubkey].
void showUserProfileSheet(BuildContext context, String pubkey) {
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (_) => UserProfileSheet(pubkey: pubkey),
  );
}

class UserProfileSheet extends HookConsumerWidget {
  final String pubkey;

  const UserProfileSheet({super.key, required this.pubkey});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pk = pubkey.toLowerCase();

    // Watch cached profile, presence, and user status.
    final profile =
        ref.watch(userCacheProvider.select((cache) => cache[pk])) ??
        ref.read(userCacheProvider.notifier).get(pk);
    final presenceMap = ref.watch(presenceCacheProvider);
    final presence = presenceMap[pk] ?? 'offline';
    final statusCache = ref.watch(userStatusCacheProvider);
    final userStatus = statusCache[pk];

    // Fetch about from the individual profile endpoint.
    final aboutFuture = useMemoized(
      () => ref
          .read(relayClientProvider)
          .get('/api/users/$pk/profile')
          .then(
            (json) => (json as Map<String, dynamic>)['about'] as String? ?? '',
          )
          .catchError((_) => ''),
      [pk],
    );
    final aboutSnapshot = useFuture(aboutFuture);
    final about = aboutSnapshot.data ?? profile?.about ?? '';

    // Ensure presence and status are tracked.
    useEffect(() {
      ref.read(presenceCacheProvider.notifier).track([pk]);
      ref.read(userStatusCacheProvider.notifier).track([pk]);
      ref.read(userCacheProvider.notifier).preload([pk]);
      return null;
    }, [pk]);

    final displayName = profile?.displayName;
    final avatarUrl = profile?.avatarUrl;
    final nip05 = profile?.nip05Handle;
    final initial =
        profile?.initial ?? (pubkey.isNotEmpty ? pubkey[0].toUpperCase() : '?');

    final presenceColor = switch (presence) {
      'online' => context.appColors.success,
      'away' => context.appColors.warning,
      _ => context.colors.outline,
    };
    final presenceLabel = switch (presence) {
      'online' => 'Online',
      'away' => 'Away',
      _ => 'Offline',
    };

    return SizedBox(
      width: double.infinity,
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          Grid.xs,
          0,
          Grid.xs,
          MediaQuery.viewInsetsOf(context).bottom + Grid.xs,
        ),
        child: SafeArea(
          top: false,
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxHeight: MediaQuery.sizeOf(context).height * 0.6,
            ),
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  // Avatar
                  _ProfileAvatar(avatarUrl: avatarUrl, initial: initial),
                  const SizedBox(height: Grid.twelve),

                  // Display name or truncated pubkey
                  Text(
                    displayName ?? shortPubkey(pubkey),
                    style: context.textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),

                  // NIP-05 handle
                  if (nip05 != null && nip05.isNotEmpty) ...[
                    const SizedBox(height: Grid.quarter),
                    Text(
                      nip05,
                      style: context.textTheme.bodySmall?.copyWith(
                        color: context.colors.onSurfaceVariant,
                      ),
                    ),
                  ],

                  // Truncated pubkey (only if display name is shown)
                  if (displayName != null) ...[
                    const SizedBox(height: Grid.quarter),
                    Text(
                      shortPubkey(pubkey),
                      style: context.textTheme.labelSmall?.copyWith(
                        color: context.colors.onSurfaceVariant.withValues(
                          alpha: 0.6,
                        ),
                        fontFamily: 'monospace',
                      ),
                    ),
                  ],

                  const SizedBox(height: Grid.xxs),

                  // Presence indicator
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        width: 8,
                        height: 8,
                        decoration: BoxDecoration(
                          color: presenceColor,
                          shape: BoxShape.circle,
                        ),
                      ),
                      const SizedBox(width: Grid.half),
                      Text(
                        presenceLabel,
                        style: context.textTheme.bodySmall?.copyWith(
                          color: context.colors.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),

                  // Custom status
                  if (userStatus != null && !userStatus.isEmpty) ...[
                    const SizedBox(height: Grid.half),
                    Text(
                      '${userStatus.emoji.isNotEmpty ? '${userStatus.emoji} ' : ''}${userStatus.text}',
                      style: context.textTheme.bodySmall?.copyWith(
                        color: context.colors.onSurfaceVariant,
                      ),
                      textAlign: TextAlign.center,
                    ),
                  ],

                  // About / bio
                  if (about.isNotEmpty) ...[
                    const SizedBox(height: Grid.xxs),
                    Text(
                      about,
                      style: context.textTheme.bodySmall?.copyWith(
                        color: context.colors.onSurfaceVariant,
                      ),
                      textAlign: TextAlign.center,
                    ),
                  ],

                  const SizedBox(height: Grid.xxs),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ProfileAvatar extends HookWidget {
  final String? avatarUrl;
  final String initial;

  const _ProfileAvatar({required this.avatarUrl, required this.initial});

  @override
  Widget build(BuildContext context) {
    final failed = useState(false);

    useEffect(() {
      failed.value = false;
      return null;
    }, [avatarUrl]);

    final url = avatarUrl;
    final showImage = url != null && !failed.value;

    return CircleAvatar(
      radius: 48,
      backgroundColor: context.colors.primaryContainer,
      backgroundImage: showImage ? NetworkImage(url) : null,
      onBackgroundImageError: showImage ? (_, _) => failed.value = true : null,
      child: !showImage
          ? Text(
              initial,
              style: context.textTheme.headlineMedium?.copyWith(
                color: context.colors.onPrimaryContainer,
                fontWeight: FontWeight.w600,
              ),
            )
          : null,
    );
  }
}
