import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/theme/theme.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';
import 'channel.dart';
import 'channel_management_provider.dart';

class MembersSheet extends HookConsumerWidget {
  final Channel channel;
  final String? currentPubkey;

  const MembersSheet({
    super.key,
    required this.channel,
    required this.currentPubkey,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final membersAsync = ref.watch(channelMembersProvider(channel.id));
    final allMembers = membersAsync.asData?.value ?? const <ChannelMember>[];
    final people = allMembers.where((member) => !member.isBot).toList();
    final bots = allMembers.where((member) => member.isBot).toList();
    final userCache = ref.watch(userCacheProvider);

    // Determine if the current user can manage members.
    final currentMember = allMembers.cast<ChannelMember?>().firstWhere(
      (m) => m!.pubkey.toLowerCase() == currentPubkey?.toLowerCase(),
      orElse: () => null,
    );
    final canManage =
        currentMember != null &&
        currentMember.isElevated &&
        !channel.isArchived;

    // Preload profiles for all members so avatars appear.
    useEffect(() {
      if (allMembers.isNotEmpty) {
        ref
            .read(userCacheProvider.notifier)
            .preload(allMembers.map((m) => m.pubkey).toList());
      }
      return null;
    }, [allMembers.length]);

    return Padding(
      padding: EdgeInsets.fromLTRB(
        Grid.xs,
        0,
        Grid.xs,
        MediaQuery.viewInsetsOf(context).bottom + Grid.xs,
      ),
      child: SafeArea(
        top: false,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Members', style: context.textTheme.titleMedium),
              const SizedBox(height: Grid.xxs),
              Text(
                'People in ${channel.displayLabel(currentPubkey: currentPubkey)}.',
                style: context.textTheme.bodySmall?.copyWith(
                  color: context.colors.onSurfaceVariant,
                ),
              ),
              if (!channel.isDm) ...[const Divider(height: Grid.sm)],
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 400),
                child: membersAsync.when(
                  data: (_) => ListView(
                    shrinkWrap: true,
                    children: [
                      if (people.isNotEmpty) ...[
                        _SectionLabel(label: 'People — ${people.length}'),
                        for (final member in people)
                          _MemberTile(
                            member: member,
                            currentPubkey: currentPubkey,
                            profile: userCache[member.pubkey.toLowerCase()],
                            canManage: canManage,
                            isSelf:
                                member.pubkey.toLowerCase() ==
                                currentPubkey?.toLowerCase(),
                            channelId: channel.id,
                          ),
                      ],
                      if (bots.isNotEmpty) ...[
                        const SizedBox(height: Grid.xxs),
                        _SectionLabel(label: 'Bots — ${bots.length}'),
                        for (final bot in bots)
                          _MemberTile(
                            member: bot,
                            currentPubkey: currentPubkey,
                            profile: userCache[bot.pubkey.toLowerCase()],
                            canManage: canManage,
                            isSelf: false,
                            channelId: channel.id,
                          ),
                      ],
                      if (people.isEmpty && bots.isEmpty)
                        Center(
                          child: Text(
                            'No members found.',
                            style: context.textTheme.bodySmall?.copyWith(
                              color: context.colors.outline,
                            ),
                          ),
                        ),
                    ],
                  ),
                  loading: () =>
                      const Center(child: CircularProgressIndicator()),
                  error: (error, _) => Center(
                    child: Text(
                      error.toString(),
                      style: context.textTheme.bodySmall?.copyWith(
                        color: context.colors.error,
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SectionLabel extends StatelessWidget {
  final String label;

  const _SectionLabel({required this.label});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: Grid.half, bottom: Grid.half),
      child: Text(
        label.toUpperCase(),
        style: context.textTheme.labelSmall?.copyWith(
          color: context.colors.outline,
          fontWeight: FontWeight.w600,
          letterSpacing: 0.8,
        ),
      ),
    );
  }
}

const _changeableRoles = ['admin', 'member', 'guest'];

class _MemberTile extends ConsumerWidget {
  final ChannelMember member;
  final String? currentPubkey;
  final UserProfile? profile;
  final bool canManage;
  final bool isSelf;
  final String channelId;

  const _MemberTile({
    required this.member,
    required this.currentPubkey,
    required this.profile,
    required this.canManage,
    required this.isSelf,
    required this.channelId,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final label = member.labelFor(currentPubkey);
    final initial = label.substring(0, 1).toUpperCase();
    final showMenu = canManage && !isSelf && !member.isOwner;

    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: _MemberAvatar(avatarUrl: profile?.avatarUrl, initial: initial),
      title: Text(label),
      subtitle: Text(
        _roleLabel(member.role),
        style: context.textTheme.bodySmall?.copyWith(
          color: context.colors.outline,
        ),
      ),
      trailing: showMenu
          ? IconButton(
              icon: const Icon(LucideIcons.ellipsis, size: 18),
              onPressed: () => _showMemberActions(context, ref),
              visualDensity: VisualDensity.compact,
            )
          : null,
    );
  }

  String _roleLabel(String role) {
    if (role.isEmpty) return 'Member';
    return '${role[0].toUpperCase()}${role.substring(1)}';
  }

  void _showMemberActions(BuildContext context, WidgetRef ref) {
    final label = member.labelFor(currentPubkey);
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (_) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Grid.xs),
              child: Text(label, style: context.textTheme.titleSmall),
            ),
            const SizedBox(height: Grid.xxs),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Grid.xs),
              child: Text(
                'Change role',
                style: context.textTheme.labelMedium?.copyWith(
                  color: context.colors.outline,
                ),
              ),
            ),
            const SizedBox(height: Grid.half),
            for (final role in _changeableRoles)
              ListTile(
                title: Text(_roleLabel(role)),
                trailing: role == member.role
                    ? Icon(
                        LucideIcons.check,
                        size: 16,
                        color: context.colors.primary,
                      )
                    : null,
                enabled: role != member.role,
                onTap: role == member.role
                    ? null
                    : () async {
                        Navigator.of(context).pop();
                        await ref
                            .read(channelActionsProvider)
                            .changeMemberRole(
                              channelId: channelId,
                              pubkey: member.pubkey,
                              role: role,
                            );
                      },
              ),
            const Divider(),
            ListTile(
              leading: Icon(
                LucideIcons.userMinus,
                size: 18,
                color: context.colors.error,
              ),
              title: Text(
                'Remove from channel',
                style: TextStyle(color: context.colors.error),
              ),
              onTap: () async {
                Navigator.of(context).pop();
                final confirmed = await showDialog<bool>(
                  context: context,
                  builder: (context) => AlertDialog(
                    title: const Text('Remove member'),
                    content: Text('Remove $label from this channel?'),
                    actions: [
                      TextButton(
                        onPressed: () => Navigator.of(context).pop(false),
                        child: const Text('Cancel'),
                      ),
                      TextButton(
                        onPressed: () => Navigator.of(context).pop(true),
                        child: Text(
                          'Remove',
                          style: TextStyle(color: context.colors.error),
                        ),
                      ),
                    ],
                  ),
                );
                if (confirmed == true) {
                  await ref
                      .read(channelActionsProvider)
                      .removeMember(
                        channelId: channelId,
                        pubkey: member.pubkey,
                      );
                }
              },
            ),
            const SizedBox(height: Grid.xxs),
          ],
        ),
      ),
    );
  }
}

class _MemberAvatar extends HookWidget {
  final String? avatarUrl;
  final String initial;

  const _MemberAvatar({required this.avatarUrl, required this.initial});

  @override
  Widget build(BuildContext context) {
    final failed = useState(false);

    useEffect(() {
      failed.value = false;
      return null;
    }, [avatarUrl]);

    final url = avatarUrl;
    if (url == null || failed.value) {
      return CircleAvatar(child: Text(initial));
    }
    return CircleAvatar(
      backgroundImage: NetworkImage(url),
      onBackgroundImageError: (_, _) => failed.value = true,
      child: null,
    );
  }
}
