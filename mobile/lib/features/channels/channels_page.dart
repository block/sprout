import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';
import '../profile/profile_avatar.dart';
import '../profile/profile_provider.dart';
import '../settings/settings_page.dart';
import 'channel.dart';
import 'channel_detail_page.dart';
import 'channel_management_provider.dart';
import 'channels_provider.dart';
import 'dm_presence_avatar.dart';

enum _QuickAction { createChannel, createForum, newDm }

class ChannelsPage extends HookConsumerWidget {
  const ChannelsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final channelsAsync = ref.watch(channelsProvider);
    final sessionState = ref.watch(relaySessionProvider);
    final currentPubkey = ref
        .watch(profileProvider)
        .whenData((value) => value?.pubkey)
        .value;

    // Cache the last successfully loaded channels so the UI never flashes
    // back to a loading state when the provider rebuilds.
    final cachedChannels = useRef<List<Channel>?>(null);
    if (channelsAsync.asData?.value case final data?) {
      cachedChannels.value = data;
    }
    final channels = cachedChannels.value;

    Future<void> openChannel(Channel channel) async {
      if (!context.mounted) return;
      await Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (_) => ChannelDetailPage(channel: channel),
        ),
      );
    }

    Future<void> browseChannels() async {
      final channels = channelsAsync.asData?.value;
      if (channels == null || channels.isEmpty) {
        return;
      }

      final selected = await showModalBottomSheet<Channel>(
        context: context,
        isScrollControlled: true,
        showDragHandle: true,
        builder: (_) => _BrowseChannelsSheet(channels: channels),
      );
      if (selected != null && context.mounted) {
        await openChannel(selected);
      }
    }

    Future<void> openQuickActions() async {
      final action = await showModalBottomSheet<_QuickAction>(
        context: context,
        showDragHandle: true,
        builder: (_) => const _QuickActionsSheet(),
      );

      if (!context.mounted || action == null) {
        return;
      }

      switch (action) {
        case _QuickAction.createChannel:
        case _QuickAction.createForum:
          final created = await showModalBottomSheet<Channel>(
            context: context,
            isScrollControlled: true,
            showDragHandle: true,
            builder: (_) => _CreateChannelSheet(
              channelType: action == _QuickAction.createForum
                  ? 'forum'
                  : 'stream',
            ),
          );
          if (created != null && context.mounted) {
            await openChannel(created);
          }
        case _QuickAction.newDm:
          final opened = await showModalBottomSheet<Channel>(
            context: context,
            isScrollControlled: true,
            showDragHandle: true,
            builder: (_) =>
                _NewDirectMessageSheet(currentPubkey: currentPubkey),
          );
          if (opened != null && context.mounted) {
            await openChannel(opened);
          }
      }
    }

    return Scaffold(
      appBar: AppBar(
        titleSpacing: Grid.xs,
        title: Row(
          children: [
            Expanded(
              child: GestureDetector(
                onTap: channelsAsync.hasValue ? browseChannels : null,
                child: Container(
                  height: 36,
                  padding: const EdgeInsets.symmetric(horizontal: Grid.twelve),
                  decoration: BoxDecoration(
                    color: context.colors.surfaceContainerHighest,
                    borderRadius: BorderRadius.circular(Radii.lg),
                    border: Border.all(color: context.colors.outlineVariant),
                  ),
                  child: Row(
                    children: [
                      Icon(
                        LucideIcons.search,
                        size: 16,
                        color: context.colors.onSurfaceVariant,
                      ),
                      const SizedBox(width: Grid.xxs),
                      Text(
                        'Search',
                        style: context.textTheme.bodyMedium?.copyWith(
                          color: context.colors.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
            const SizedBox(width: Grid.twelve),
            ProfileAvatar(
              onTap: () => Navigator.of(context).push(
                MaterialPageRoute<void>(builder: (_) => const SettingsPage()),
              ),
            ),
          ],
        ),
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: openQuickActions,
        tooltip: 'Create or start conversation',
        shape: const CircleBorder(),
        child: const Icon(LucideIcons.plus),
      ),
      body: channels != null
          ? Column(
              children: [
                _ConnectionBanner(status: sessionState.status),
                Expanded(
                  child: _ChannelsList(
                    channels: channels,
                    currentPubkey: currentPubkey,
                    onSelectChannel: openChannel,
                  ),
                ),
              ],
            )
          : channelsAsync.hasError
          ? _ErrorView(
              error: channelsAsync.error!,
              onRetry: () => ref.read(channelsProvider.notifier).refresh(),
            )
          : const _ConnectionBanner(status: SessionStatus.connecting),
    );
  }
}

class _ChannelsList extends HookConsumerWidget {
  final List<Channel> channels;
  final String? currentPubkey;
  final Future<void> Function(Channel channel) onSelectChannel;

  const _ChannelsList({
    required this.channels,
    required this.currentPubkey,
    required this.onSelectChannel,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final visibleChannels = channels
        .where((channel) => channel.isMember && !channel.isArchived)
        .toList();
    final streamChannels = visibleChannels
        .where((channel) => channel.isStream)
        .toList();
    final forumChannels = visibleChannels
        .where((channel) => channel.isForum)
        .toList();
    final dmChannels = visibleChannels
        .where((channel) => channel.isDm)
        .toList();

    final channelsExpanded = useState(true);
    final forumsExpanded = useState(true);
    final dmsExpanded = useState(true);

    return RefreshIndicator(
      onRefresh: () => ref.read(channelsProvider.notifier).refresh(),
      child: ListView(
        padding: const EdgeInsets.only(top: Grid.xxs, bottom: 80),
        children: [
          if (visibleChannels.isEmpty)
            const _EmptyState()
          else ...[
            _ChannelSection(
              title: 'Channels',
              icon: LucideIcons.hash,
              expanded: channelsExpanded.value,
              onToggle: () => channelsExpanded.value = !channelsExpanded.value,
              channels: streamChannels,
              currentPubkey: currentPubkey,
              emptyLabel: 'No stream channels yet',
              onSelectChannel: onSelectChannel,
            ),
            _ChannelSection(
              title: 'Forums',
              icon: LucideIcons.messageSquareText,
              expanded: forumsExpanded.value,
              onToggle: () => forumsExpanded.value = !forumsExpanded.value,
              channels: forumChannels,
              currentPubkey: currentPubkey,
              emptyLabel: 'No forums yet',
              onSelectChannel: onSelectChannel,
            ),
            _ChannelSection(
              title: 'DMs',
              icon: LucideIcons.messagesSquare,
              expanded: dmsExpanded.value,
              onToggle: () => dmsExpanded.value = !dmsExpanded.value,
              channels: dmChannels,
              currentPubkey: currentPubkey,
              emptyLabel: 'No direct messages yet',
              onSelectChannel: onSelectChannel,
            ),
          ],
        ],
      ),
    );
  }
}

class _ChannelSection extends StatelessWidget {
  final String title;
  final IconData icon;
  final bool expanded;
  final VoidCallback onToggle;
  final List<Channel> channels;
  final String? currentPubkey;
  final String emptyLabel;
  final Future<void> Function(Channel channel) onSelectChannel;

  const _ChannelSection({
    required this.title,
    required this.icon,
    required this.expanded,
    required this.onToggle,
    required this.channels,
    required this.currentPubkey,
    required this.emptyLabel,
    required this.onSelectChannel,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SectionHeader(
          label: title,
          icon: icon,
          expanded: expanded,
          onToggle: onToggle,
        ),
        if (expanded) ...[
          if (channels.isEmpty)
            Padding(
              padding: const EdgeInsets.only(
                left: Grid.xs + Grid.xxs,
                right: Grid.xs,
                top: Grid.half,
                bottom: Grid.half,
              ),
              child: Text(
                emptyLabel,
                style: context.textTheme.bodySmall?.copyWith(
                  color: context.colors.outline,
                ),
              ),
            )
          else
            for (final channel in channels)
              _ChannelTile(
                channel: channel,
                currentPubkey: currentPubkey,
                onTap: () => onSelectChannel(channel),
              ),
        ],
      ],
    );
  }
}

class _EmptyState extends StatelessWidget {
  const _EmptyState();

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: MediaQuery.sizeOf(context).height * 0.55,
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              LucideIcons.messagesSquare,
              size: Grid.xl,
              color: context.colors.outline,
            ),
            const SizedBox(height: Grid.xs),
            Text(
              'No conversations yet',
              style: context.textTheme.bodyLarge?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SectionHeader extends StatelessWidget {
  final String label;
  final IconData icon;
  final bool expanded;
  final VoidCallback onToggle;

  const _SectionHeader({
    required this.label,
    required this.icon,
    required this.expanded,
    required this.onToggle,
  });

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onToggle,
      behavior: HitTestBehavior.opaque,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          Grid.xs,
          Grid.twelve,
          Grid.xs,
          Grid.half,
        ),
        child: Row(
          children: [
            Icon(icon, size: 14, color: context.colors.outline),
            const SizedBox(width: Grid.half),
            Text(
              label.toUpperCase(),
              style: context.textTheme.labelSmall?.copyWith(
                color: context.colors.outline,
                fontWeight: FontWeight.w600,
                letterSpacing: 0.8,
              ),
            ),
            const Spacer(),
            Icon(
              expanded ? LucideIcons.chevronDown : LucideIcons.chevronRight,
              size: 14,
              color: context.colors.outline,
            ),
          ],
        ),
      ),
    );
  }
}

class _ChannelTile extends ConsumerWidget {
  final Channel channel;
  final String? currentPubkey;
  final VoidCallback onTap;

  const _ChannelTile({
    required this.channel,
    required this.currentPubkey,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final hasActivity = channel.lastMessageAt != null;

    return InkWell(
      borderRadius: BorderRadius.circular(Radii.md),
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.only(
          left: Grid.xs + Grid.xxs,
          right: Grid.xs,
          top: Grid.xxs + Grid.quarter,
          bottom: Grid.xxs + Grid.quarter,
        ),
        child: Row(
          children: [
            if (channel.isDm)
              DmPresenceAvatar(channel: channel, currentPubkey: currentPubkey)
            else
              Icon(
                channelIcon(channel),
                size: 18,
                color: hasActivity
                    ? context.colors.onSurface
                    : context.colors.outline,
              ),
            const SizedBox(width: Grid.xxs),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    channel.displayLabel(currentPubkey: currentPubkey),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: context.textTheme.bodyMedium?.copyWith(
                      color: hasActivity
                          ? context.colors.onSurface
                          : context.colors.onSurfaceVariant,
                    ),
                  ),
                ],
              ),
            ),
            if (!channel.isMember && !channel.isDm)
              Padding(
                padding: const EdgeInsets.only(right: Grid.xxs),
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: Grid.half + 2,
                    vertical: 3,
                  ),
                  decoration: BoxDecoration(
                    color: context.colors.primary.withValues(alpha: 0.1),
                    borderRadius: BorderRadius.circular(Radii.sm),
                  ),
                  child: Text(
                    'Open',
                    style: context.textTheme.labelSmall?.copyWith(
                      color: context.colors.primary,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ),
            if (channel.isEphemeral) ...[
              const SizedBox(width: Grid.xxs),
              _EphemeralBadge(channel: channel),
            ],
          ],
        ),
      ),
    );
  }
}

class _QuickActionsSheet extends StatelessWidget {
  const _QuickActionsSheet();

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(Grid.xs, 0, Grid.xs, Grid.xs),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            ListTile(
              leading: const Icon(LucideIcons.hash),
              title: const Text('Create channel'),
              subtitle: const Text('Start a new stream channel'),
              onTap: () =>
                  Navigator.of(context).pop(_QuickAction.createChannel),
            ),
            ListTile(
              leading: const Icon(LucideIcons.messageSquareText),
              title: const Text('Create forum'),
              subtitle: const Text('Set up a threaded discussion space'),
              onTap: () => Navigator.of(context).pop(_QuickAction.createForum),
            ),
            ListTile(
              leading: const Icon(LucideIcons.messagesSquare),
              title: const Text('New direct message'),
              subtitle: const Text('Open a DM with one or more people'),
              onTap: () => Navigator.of(context).pop(_QuickAction.newDm),
            ),
          ],
        ),
      ),
    );
  }
}

class _CreateChannelSheet extends HookConsumerWidget {
  final String channelType;

  const _CreateChannelSheet({required this.channelType});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final nameController = useTextEditingController();
    final descriptionController = useTextEditingController();
    final visibility = useState('open');
    final isSubmitting = useState(false);
    final errorMessage = useState<String?>(null);

    Future<void> submit() async {
      final name = nameController.text.trim();
      if (name.isEmpty || isSubmitting.value) {
        return;
      }

      isSubmitting.value = true;
      errorMessage.value = null;
      try {
        final created = await ref
            .read(channelActionsProvider)
            .createChannel(
              name: name,
              channelType: channelType,
              visibility: visibility.value,
              description: descriptionController.text.trim(),
            );
        if (context.mounted) {
          Navigator.of(context).pop(created);
        }
      } catch (error) {
        errorMessage.value = error.toString();
      } finally {
        isSubmitting.value = false;
      }
    }

    return Padding(
      padding: EdgeInsets.fromLTRB(
        Grid.xs,
        0,
        Grid.xs,
        MediaQuery.viewInsetsOf(context).bottom + Grid.xs,
      ),
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextField(
              controller: nameController,
              enabled: !isSubmitting.value,
              decoration: InputDecoration(
                labelText: 'Name',
                hintText: channelType == 'forum'
                    ? 'design-discussions'
                    : 'release-notes',
              ),
              textInputAction: TextInputAction.next,
            ),
            const SizedBox(height: Grid.xxs),
            TextField(
              controller: descriptionController,
              enabled: !isSubmitting.value,
              decoration: const InputDecoration(
                labelText: 'Description',
                hintText: 'What this space is for',
              ),
              minLines: 2,
              maxLines: 3,
            ),
            const SizedBox(height: Grid.xxs),
            SwitchListTile(
              title: const Text('Private'),
              contentPadding: EdgeInsets.zero,
              value: visibility.value == 'private',
              onChanged: isSubmitting.value
                  ? null
                  : (on) => visibility.value = on ? 'private' : 'open',
            ),
            if (errorMessage.value case final error?) ...[
              const SizedBox(height: Grid.xxs),
              Text(
                error,
                style: context.textTheme.bodySmall?.copyWith(
                  color: context.colors.error,
                ),
              ),
            ],
            const SizedBox(height: Grid.xs),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: isSubmitting.value
                      ? null
                      : () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: Grid.half),
                FilledButton(
                  onPressed: isSubmitting.value ? null : submit,
                  child: Text(isSubmitting.value ? 'Creating…' : 'Create'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _BrowseChannelsSheet extends HookConsumerWidget {
  final List<Channel> channels;

  const _BrowseChannelsSheet({required this.channels});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final query = useState('');
    final busyChannelId = useState<String?>(null);

    final normalizedQuery = query.value.trim().toLowerCase();
    final browsableChannels = channels.where((channel) {
      if (channel.isDm) return false;
      final visible = channel.isArchived
          ? channel.isMember
          : channel.visibility == 'open' || channel.isMember;
      if (!visible) return false;
      if (normalizedQuery.isEmpty) return true;
      return channel.name.toLowerCase().contains(normalizedQuery) ||
          channel.description.toLowerCase().contains(normalizedQuery);
    }).toList();

    final notJoined = browsableChannels
        .where((channel) => !channel.isMember)
        .toList();
    final joined = browsableChannels
        .where((channel) => channel.isMember)
        .toList();

    Future<void> openOrJoin(Channel channel) async {
      if (busyChannelId.value != null) return;

      if (channel.isMember) {
        Navigator.of(context).pop(channel);
        return;
      }

      busyChannelId.value = channel.id;
      try {
        await ref.read(channelActionsProvider).joinChannel(channel.id);
        final refreshed = await ref.read(channelsProvider.future);
        final joinedChannel = refreshed.firstWhere(
          (candidate) => candidate.id == channel.id,
          orElse: () => channel,
        );
        if (context.mounted) {
          Navigator.of(context).pop(joinedChannel);
        }
      } catch (error) {
        if (!context.mounted) return;
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(error.toString())));
      } finally {
        busyChannelId.value = null;
      }
    }

    return DraggableScrollableSheet(
      initialChildSize: 0.75,
      minChildSize: 0.4,
      maxChildSize: 0.95,
      expand: false,
      builder: (context, scrollController) => Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              Grid.xs,
              Grid.twelve,
              Grid.xs,
              Grid.xxs,
            ),
            child: GestureDetector(
              onTap: () {},
              child: Container(
                height: 36,
                padding: const EdgeInsets.symmetric(horizontal: Grid.twelve),
                decoration: BoxDecoration(
                  color: context.colors.surfaceContainerHighest,
                  borderRadius: BorderRadius.circular(Radii.lg),
                  border: Border.all(color: context.colors.outlineVariant),
                ),
                child: Row(
                  children: [
                    Icon(
                      LucideIcons.search,
                      size: 16,
                      color: context.colors.onSurfaceVariant,
                    ),
                    const SizedBox(width: Grid.xxs),
                    Expanded(
                      child: TextField(
                        autofocus: true,
                        decoration: InputDecoration(
                          hintText: 'Search channels, forums…',
                          hintStyle: context.textTheme.bodyMedium?.copyWith(
                            color: context.colors.onSurfaceVariant,
                          ),
                          border: InputBorder.none,
                          enabledBorder: InputBorder.none,
                          focusedBorder: InputBorder.none,
                          isDense: true,
                          contentPadding: EdgeInsets.zero,
                        ),
                        style: context.textTheme.bodyMedium,
                        onChanged: (value) => query.value = value,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          Expanded(
            child: browsableChannels.isEmpty
                ? Center(
                    child: Text(
                      'No matching results',
                      style: context.textTheme.bodyMedium?.copyWith(
                        color: context.colors.onSurfaceVariant,
                      ),
                    ),
                  )
                : ListView(
                    controller: scrollController,
                    padding: const EdgeInsets.only(top: Grid.xxs),
                    children: [
                      if (notJoined.isNotEmpty) ...[
                        _MiniHeader(
                          label: '${notJoined.length} available to join',
                        ),
                        for (final channel in notJoined)
                          _BrowseTile(
                            channel: channel,
                            isBusy: busyChannelId.value == channel.id,
                            onTap: () => openOrJoin(channel),
                          ),
                      ],
                      if (joined.isNotEmpty) ...[
                        _MiniHeader(label: '${joined.length} joined'),
                        for (final channel in joined)
                          _BrowseTile(
                            channel: channel,
                            isBusy: false,
                            onTap: () => openOrJoin(channel),
                          ),
                      ],
                    ],
                  ),
          ),
        ],
      ),
    );
  }
}

class _BrowseTile extends StatelessWidget {
  final Channel channel;
  final bool isBusy;
  final VoidCallback onTap;

  const _BrowseTile({
    required this.channel,
    required this.isBusy,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return ListTile(
      leading: Icon(
        channel.isForum ? LucideIcons.messageSquareText : LucideIcons.hash,
      ),
      title: Text(channel.name),
      subtitle: Text(
        channel.description.isEmpty ? 'No description' : channel.description,
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: FilledButton.tonal(
        onPressed: isBusy ? null : onTap,
        child: Text(
          isBusy
              ? 'Joining…'
              : channel.isMember
              ? 'Open'
              : 'Join',
        ),
      ),
      onTap: isBusy ? null : onTap,
    );
  }
}

class _NewDirectMessageSheet extends HookConsumerWidget {
  final String? currentPubkey;

  const _NewDirectMessageSheet({required this.currentPubkey});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final queryController = useTextEditingController();
    final query = useState('');
    final debouncedQuery = useState('');
    final selectedUsers = useState<List<DirectoryUser>>([]);
    final isSubmitting = useState(false);
    final submitError = useState<String?>(null);

    useEffect(() {
      final timer = Timer(const Duration(milliseconds: 250), () {
        debouncedQuery.value = query.value.trim();
      });
      return timer.cancel;
    }, [query.value]);

    final searchFuture = useMemoized(() {
      if (debouncedQuery.value.isEmpty || selectedUsers.value.length >= 8) {
        return Future.value(const <DirectoryUser>[]);
      }
      return ref
          .read(channelActionsProvider)
          .searchUsers(debouncedQuery.value, limit: 8);
    }, [debouncedQuery.value, selectedUsers.value.length]);
    final searchResults = useFuture(searchFuture);

    final selectedPubkeys = selectedUsers.value
        .map((user) => user.pubkey.toLowerCase())
        .toSet();
    final availableResults =
        searchResults.data
            ?.where(
              (user) =>
                  !selectedPubkeys.contains(user.pubkey.toLowerCase()) &&
                  user.pubkey.toLowerCase() != currentPubkey?.toLowerCase(),
            )
            .toList() ??
        const <DirectoryUser>[];
    final canSubmit = !isSubmitting.value && selectedUsers.value.isNotEmpty;

    Future<void> submit() async {
      if (selectedUsers.value.isEmpty || isSubmitting.value) {
        return;
      }

      isSubmitting.value = true;
      submitError.value = null;
      try {
        final channel = await ref
            .read(channelActionsProvider)
            .openDm(
              pubkeys: selectedUsers.value.map((user) => user.pubkey).toList(),
            );
        if (context.mounted) {
          Navigator.of(context).pop(channel);
        }
      } catch (error) {
        submitError.value = error.toString();
      } finally {
        isSubmitting.value = false;
      }
    }

    return Padding(
      padding: EdgeInsets.fromLTRB(
        Grid.xs,
        0,
        Grid.xs,
        MediaQuery.viewInsetsOf(context).bottom + Grid.xs,
      ),
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextField(
              controller: queryController,
              decoration: const InputDecoration(
                prefixIcon: Icon(LucideIcons.search),
                hintText: 'Search by name, NIP-05, or pubkey',
              ),
              enabled: !isSubmitting.value,
              onChanged: (value) => query.value = value,
            ),
            if (selectedUsers.value.isNotEmpty) ...[
              const SizedBox(height: Grid.xxs),
              Wrap(
                spacing: Grid.half,
                runSpacing: Grid.half,
                children: [
                  for (final user in selectedUsers.value)
                    InputChip(
                      label: Text(user.label),
                      onDeleted: isSubmitting.value
                          ? null
                          : () {
                              selectedUsers.value = [
                                for (final candidate in selectedUsers.value)
                                  if (candidate.pubkey != user.pubkey)
                                    candidate,
                              ];
                            },
                    ),
                ],
              ),
            ],
            const SizedBox(height: Grid.xs),
            SizedBox(
              height: 280,
              child: Builder(
                builder: (context) {
                  if (selectedUsers.value.length >= 8) {
                    return const Center(
                      child: Text(
                        'Direct messages support up to 9 people including you.',
                      ),
                    );
                  }
                  if (debouncedQuery.value.isEmpty) {
                    return const Center(
                      child: Text(
                        'Search for someone to start a conversation.',
                      ),
                    );
                  }
                  if (searchResults.connectionState ==
                      ConnectionState.waiting) {
                    return const Center(child: CircularProgressIndicator());
                  }
                  if (availableResults.isEmpty) {
                    return const Center(child: Text('No matching users.'));
                  }
                  return ListView(
                    shrinkWrap: true,
                    children: [
                      for (final user in availableResults)
                        ListTile(
                          leading: CircleAvatar(
                            backgroundImage: user.avatarUrl != null
                                ? NetworkImage(user.avatarUrl!)
                                : null,
                            child: user.avatarUrl == null
                                ? Text(user.label.substring(0, 1).toUpperCase())
                                : null,
                          ),
                          title: Text(user.label),
                          subtitle: Text(user.secondaryLabel),
                          onTap: () {
                            selectedUsers.value = [
                              ...selectedUsers.value,
                              user,
                            ];
                            queryController.clear();
                            query.value = '';
                            debouncedQuery.value = '';
                          },
                        ),
                    ],
                  );
                },
              ),
            ),
            if (submitError.value case final error?) ...[
              const SizedBox(height: Grid.xxs),
              Text(
                error,
                style: context.textTheme.bodySmall?.copyWith(
                  color: context.colors.error,
                ),
              ),
            ],
            const SizedBox(height: Grid.xs),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: isSubmitting.value
                      ? null
                      : () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: Grid.half),
                FilledButton(
                  onPressed: canSubmit ? submit : null,
                  child: Text(isSubmitting.value ? 'Opening…' : 'Open DM'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _MiniHeader extends StatelessWidget {
  final String label;

  const _MiniHeader({required this.label});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Grid.xs,
        vertical: Grid.half,
      ),
      child: Text(
        label,
        style: context.textTheme.labelSmall?.copyWith(
          color: context.colors.outline,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}

class _EphemeralBadge extends StatelessWidget {
  final Channel channel;

  const _EphemeralBadge({required this.channel});

  @override
  Widget build(BuildContext context) {
    final label = _label();

    return Tooltip(
      message: 'Ephemeral channel — cleans up after inactivity',
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
        decoration: BoxDecoration(
          color: const Color(0xFFF59E0B).withValues(alpha: 0.1),
          borderRadius: BorderRadius.circular(Radii.sm),
          border: Border.all(
            color: const Color(0xFFF59E0B).withValues(alpha: 0.2),
          ),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(LucideIcons.clock, size: 10, color: _amberColor(context)),
            if (label != null) ...[
              const SizedBox(width: 3),
              Text(
                label,
                style: context.textTheme.labelSmall?.copyWith(
                  fontSize: 10,
                  color: _amberColor(context),
                  fontWeight: FontWeight.w500,
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Color _amberColor(BuildContext context) {
    return context.colors.brightness == Brightness.light
        ? const Color(0xFFB45309)
        : const Color(0xFFFCD34D);
  }

  String? _label() {
    final deadline = channel.ttlDeadline;
    if (deadline == null) return null;
    final diff = deadline.difference(DateTime.now());
    if (diff.isNegative) return 'due';
    if (diff.inMinutes < 60) return '${diff.inMinutes}m';
    if (diff.inHours < 24) return '${diff.inHours}h';
    return '${diff.inDays}d';
  }
}

class _ConnectionBanner extends StatelessWidget {
  final SessionStatus status;

  const _ConnectionBanner({required this.status});

  @override
  Widget build(BuildContext context) {
    if (status == SessionStatus.connected ||
        status == SessionStatus.disconnected) {
      return const SizedBox.shrink();
    }

    final isConnecting = status == SessionStatus.connecting;
    final message = isConnecting ? 'Connecting…' : 'Reconnecting…';

    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: Grid.xs,
        vertical: Grid.quarter + 2,
      ),
      color: context.colors.surfaceContainerHighest,
      child: Row(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          SizedBox(
            width: 12,
            height: 12,
            child: CircularProgressIndicator(
              strokeWidth: 2,
              color: context.colors.onSurfaceVariant,
            ),
          ),
          const SizedBox(width: Grid.xxs),
          Text(
            message,
            style: context.textTheme.labelSmall?.copyWith(
              color: context.colors.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}

class _ErrorView extends StatelessWidget {
  final Object error;
  final VoidCallback onRetry;

  const _ErrorView({required this.error, required this.onRetry});

  static String _userMessage(Object error) {
    if (error is RelayException) {
      if (error.statusCode == 401) {
        return 'Not authorized. Check your API token.';
      }
      if (error.statusCode == 403) {
        return 'Access denied.';
      }
      return 'Server error (${error.statusCode}). Try again later.';
    }
    if (error is SocketException) {
      return 'Could not reach the relay server.';
    }
    return 'Something went wrong. Check your connection.';
  }

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Grid.sm),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              LucideIcons.wifiOff,
              size: Grid.xl,
              color: context.colors.error,
            ),
            const SizedBox(height: Grid.xs),
            Text(
              'Could not load channels',
              style: context.textTheme.titleMedium,
            ),
            const SizedBox(height: Grid.xxs),
            Text(
              _userMessage(error),
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
              textAlign: TextAlign.center,
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: Grid.xs),
            FilledButton.icon(
              onPressed: onRetry,
              icon: const Icon(LucideIcons.refreshCw),
              label: const Text('Retry'),
            ),
          ],
        ),
      ),
    );
  }
}
