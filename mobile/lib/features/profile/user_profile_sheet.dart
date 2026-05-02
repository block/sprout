import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';
import '../../shared/utils/string_utils.dart';
import '../channels/channel_detail_page.dart';
import '../channels/channel_management_provider.dart';
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
          Grid.sm,
          0,
          Grid.sm,
          MediaQuery.viewInsetsOf(context).bottom + Grid.xs,
        ),
        child: SafeArea(
          top: false,
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxHeight: MediaQuery.sizeOf(context).height * 0.7,
            ),
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  // Avatar with presence overlay — centered
                  Center(
                    child: _ProfileAvatar(
                      avatarUrl: avatarUrl,
                      initial: initial,
                      presenceColor: presenceColor,
                    ),
                  ),
                  const SizedBox(height: Grid.xs),

                  // Display name — centered, large
                  Center(
                    child: Text(
                      displayName ?? shortPubkey(pubkey),
                      style: context.textTheme.headlineSmall?.copyWith(
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                  ),

                  // NIP-05 handle — centered, secondary
                  if (nip05 != null && nip05.isNotEmpty) ...[
                    const SizedBox(height: Grid.half),
                    Center(
                      child: Text(
                        nip05,
                        style: context.textTheme.bodyMedium?.copyWith(
                          color: context.colors.onSurfaceVariant,
                        ),
                      ),
                    ),
                  ],

                  const SizedBox(height: Grid.xs),

                  // Info rows — left-aligned with icons
                  _InfoRow(
                    icon: LucideIcons.circle,
                    iconColor: presenceColor,
                    iconSize: 10,
                    text: presenceLabel,
                  ),

                  if (userStatus != null && !userStatus.isEmpty)
                    _InfoRow(
                      icon: LucideIcons.messageCircle,
                      text:
                          '${userStatus.emoji.isNotEmpty ? '${userStatus.emoji} ' : ''}${userStatus.text}',
                    ),

                  _InfoRow(
                    icon: LucideIcons.key,
                    text: shortPubkey(pubkey),
                    textStyle: context.textTheme.bodySmall?.copyWith(
                      color: context.colors.onSurfaceVariant,
                      fontFamily: 'monospace',
                    ),
                    onTap: () {
                      Clipboard.setData(ClipboardData(text: pubkey));
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(
                          content: Text('Public key copied'),
                          duration: Duration(seconds: 2),
                        ),
                      );
                    },
                  ),

                  // About / bio section
                  if (about.isNotEmpty) ...[
                    const SizedBox(height: Grid.xxs),
                    Divider(
                      color: context.colors.outlineVariant.withValues(
                        alpha: 0.3,
                      ),
                    ),
                    const SizedBox(height: Grid.xxs),
                    Text(
                      about,
                      style: context.textTheme.bodyMedium?.copyWith(
                        color: context.colors.onSurfaceVariant,
                      ),
                    ),
                  ],

                  const SizedBox(height: Grid.xs),

                  // Action button — Message
                  SizedBox(
                    width: double.infinity,
                    child: FilledButton.icon(
                      onPressed: () async {
                        Navigator.of(context).pop();
                        try {
                          final channel = await ref
                              .read(channelActionsProvider)
                              .openDm(pubkeys: [pk]);
                          if (!context.mounted) return;
                          await Navigator.of(context).push(
                            MaterialPageRoute<void>(
                              builder: (_) =>
                                  ChannelDetailPage(channel: channel),
                            ),
                          );
                        } catch (_) {
                          // Silently fail — user tapped but DM open failed.
                        }
                      },
                      icon: const Icon(LucideIcons.messageSquare, size: 18),
                      label: const Text('Message'),
                      style: FilledButton.styleFrom(
                        padding: const EdgeInsets.symmetric(
                          vertical: Grid.twelve,
                        ),
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(Radii.lg),
                        ),
                      ),
                    ),
                  ),

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

/// A row displaying an icon + text, used for profile info items.
class _InfoRow extends StatelessWidget {
  final IconData icon;
  final String text;
  final Color? iconColor;
  final double iconSize;
  final TextStyle? textStyle;
  final VoidCallback? onTap;

  const _InfoRow({
    required this.icon,
    required this.text,
    this.iconColor,
    this.iconSize = 16,
    this.textStyle,
    this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final child = Padding(
      padding: const EdgeInsets.symmetric(vertical: Grid.half + 2),
      child: Row(
        children: [
          SizedBox(
            width: 24,
            child: Icon(
              icon,
              size: iconSize,
              color: iconColor ?? context.colors.onSurfaceVariant,
            ),
          ),
          const SizedBox(width: Grid.xxs),
          Expanded(
            child: Text(
              text,
              style:
                  textStyle ??
                  context.textTheme.bodyMedium?.copyWith(
                    color: context.colors.onSurface,
                  ),
            ),
          ),
          if (onTap != null)
            Icon(
              LucideIcons.copy,
              size: 14,
              color: context.colors.onSurfaceVariant,
            ),
        ],
      ),
    );

    if (onTap != null) {
      return GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: child,
      );
    }
    return child;
  }
}

class _ProfileAvatar extends HookWidget {
  final String? avatarUrl;
  final String initial;
  final Color presenceColor;

  const _ProfileAvatar({
    required this.avatarUrl,
    required this.initial,
    required this.presenceColor,
  });

  @override
  Widget build(BuildContext context) {
    final failed = useState(false);

    useEffect(() {
      failed.value = false;
      return null;
    }, [avatarUrl]);

    final url = avatarUrl;
    final showImage = url != null && !failed.value;

    return SizedBox(
      width: 120,
      height: 120,
      child: Stack(
        children: [
          CircleAvatar(
            radius: 56,
            backgroundColor: context.colors.primaryContainer,
            backgroundImage: showImage ? NetworkImage(url) : null,
            onBackgroundImageError: showImage
                ? (_, _) => failed.value = true
                : null,
            child: !showImage
                ? Text(
                    initial,
                    style: context.textTheme.headlineLarge?.copyWith(
                      color: context.colors.onPrimaryContainer,
                      fontWeight: FontWeight.w600,
                    ),
                  )
                : null,
          ),
          // Presence dot overlay
          Positioned(
            bottom: 4,
            right: 4,
            child: Container(
              width: 18,
              height: 18,
              decoration: BoxDecoration(
                color: presenceColor,
                shape: BoxShape.circle,
                border: Border.all(color: context.colors.surface, width: 3),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
