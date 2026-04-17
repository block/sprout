import 'package:flutter/material.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/theme/theme.dart';
import '../profile/presence_cache_provider.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';
import 'channel.dart';

/// Resolved DM presence data for the other participant in a DM channel.
class DmPresenceData {
  final UserProfile? profile;
  final String presence;
  final String initial;
  final String? avatarUrl;

  const DmPresenceData({
    this.profile,
    required this.presence,
    required this.initial,
    this.avatarUrl,
  });
}

/// Resolve the other participant's presence data from a DM channel.
///
/// Triggers lazy fetches for profile and presence if not cached.
DmPresenceData resolveDmPresence(
  WidgetRef ref,
  Channel channel,
  String? currentPubkey,
) {
  final profiles = ref.watch(userCacheProvider);
  final presenceMap = ref.watch(presenceCacheProvider);
  final normalizedCurrent = currentPubkey?.toLowerCase();

  String? otherPubkey;
  for (final pk in channel.participantPubkeys) {
    if (pk.toLowerCase() != normalizedCurrent) {
      otherPubkey = pk.toLowerCase();
      break;
    }
  }

  final profile = otherPubkey != null ? profiles[otherPubkey] : null;

  if (otherPubkey != null) {
    if (profile == null) {
      ref.read(userCacheProvider.notifier).preload([otherPubkey]);
    }
    ref.read(presenceCacheProvider.notifier).track([otherPubkey]);
  }

  final avatarUrl = profile?.avatarUrl;
  final initial =
      profile?.initial ??
      (channel.participants.isNotEmpty
          ? channel.participants.first[0].toUpperCase()
          : '?');
  final presence = otherPubkey != null
      ? (presenceMap[otherPubkey] ?? 'offline')
      : 'offline';

  return DmPresenceData(
    profile: profile,
    presence: presence,
    initial: initial,
    avatarUrl: avatarUrl,
  );
}

/// Presence indicator dot color.
Color presenceColor(BuildContext context, String presence) {
  return switch (presence) {
    'online' => context.appColors.success,
    'away' => context.appColors.warning,
    _ => context.colors.outline,
  };
}

/// Compact DM avatar with presence dot, for use in channel lists.
class DmPresenceAvatar extends ConsumerWidget {
  final Channel channel;
  final String? currentPubkey;
  final double size;
  final double dotSize;

  const DmPresenceAvatar({
    super.key,
    required this.channel,
    required this.currentPubkey,
    this.size = 22,
    this.dotSize = 9,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final data = resolveDmPresence(ref, channel, currentPubkey);
    final radius = size / 2 - 1;

    return SizedBox(
      width: size,
      height: size,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          CircleAvatar(
            radius: radius,
            backgroundColor: context.colors.primaryContainer,
            backgroundImage: data.avatarUrl != null
                ? NetworkImage(data.avatarUrl!)
                : null,
            child: data.avatarUrl == null
                ? Text(
                    data.initial,
                    style: context.textTheme.labelSmall?.copyWith(
                      fontSize: radius * 0.9,
                      color: context.colors.onPrimaryContainer,
                      fontWeight: FontWeight.w600,
                    ),
                  )
                : null,
          ),
          Positioned(
            right: -1,
            bottom: -1,
            child: Container(
              width: dotSize,
              height: dotSize,
              decoration: BoxDecoration(
                color: presenceColor(context, data.presence),
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
}
