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
import 'channel.dart';
import 'channel_detail_page.dart';
import 'channel_management_provider.dart';
import 'channels_provider.dart';

enum _BrowseMode { channels, forums }

enum _QuickAction { createChannel, createForum, newDm }

class ChannelsPage extends ConsumerWidget {
  const ChannelsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final channelsAsync = ref.watch(channelsProvider);
    final sessionState = ref.watch(relaySessionProvider);
    final currentPubkey = ref
        .watch(profileProvider)
        .whenData((value) => value?.pubkey)
        .value;

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
        title: const Text('Sprout'),
        actions: [
          IconButton(
            onPressed: channelsAsync.hasValue ? browseChannels : null,
            tooltip: 'Browse channels',
            icon: const Icon(LucideIcons.search),
          ),
          IconButton(
            onPressed: openQuickActions,
            tooltip: 'Create or start conversation',
            icon: const Icon(LucideIcons.plus),
          ),
          const ProfileAvatar(),
        ],
      ),
      body: Column(
        children: [
          _ConnectionBanner(status: sessionState.status),
          Expanded(
            child: channelsAsync.when(
              loading: () => const Center(child: CircularProgressIndicator()),
              error: (error, _) => _ErrorView(
                error: error,
                onRetry: () => ref.read(channelsProvider.notifier).refresh(),
              ),
              data: (channels) => _ChannelsList(
                channels: channels,
                currentPubkey: currentPubkey,
                onSelectChannel: openChannel,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ChannelsList extends ConsumerWidget {
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

    return RefreshIndicator(
      onRefresh: () => ref.read(channelsProvider.notifier).refresh(),
      child: ListView(
        padding: const EdgeInsets.only(top: Grid.xxs, bottom: Grid.xs),
        children: [
          if (visibleChannels.isEmpty)
            const _EmptyState()
          else ...[
            _ChannelSection(
              title: 'Channels',
              channels: streamChannels,
              currentPubkey: currentPubkey,
              emptyLabel: 'No stream channels yet',
              onSelectChannel: onSelectChannel,
            ),
            _ChannelSection(
              title: 'Forums',
              channels: forumChannels,
              currentPubkey: currentPubkey,
              emptyLabel: 'No forums yet',
              onSelectChannel: onSelectChannel,
            ),
            _ChannelSection(
              title: 'DMs',
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
  final List<Channel> channels;
  final String? currentPubkey;
  final String emptyLabel;
  final Future<void> Function(Channel channel) onSelectChannel;

  const _ChannelSection({
    required this.title,
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
        _SectionHeader(label: title, count: channels.length),
        if (channels.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: Grid.xs,
              vertical: Grid.half,
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
  final int count;

  const _SectionHeader({required this.label, required this.count});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Grid.xs,
        vertical: Grid.xxs,
      ),
      child: Row(
        children: [
          Text(
            label,
            style: context.textTheme.labelMedium?.copyWith(
              color: context.colors.onSurfaceVariant,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(width: Grid.xxs),
          Text(
            '$count',
            style: context.textTheme.labelSmall?.copyWith(
              color: context.colors.outline,
            ),
          ),
        ],
      ),
    );
  }
}

class _ChannelTile extends StatelessWidget {
  final Channel channel;
  final String? currentPubkey;
  final VoidCallback onTap;

  const _ChannelTile({
    required this.channel,
    required this.currentPubkey,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final hasActivity = channel.lastMessageAt != null;

    return InkWell(
      borderRadius: BorderRadius.circular(Radii.md),
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Grid.xs,
          vertical: Grid.xxs + Grid.quarter,
        ),
        child: Row(
          children: [
            Icon(
              _iconFor(channel),
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
                  if (channel.isDm && channel.name.trim().isNotEmpty)
                    Text(
                      channel.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: context.textTheme.labelSmall?.copyWith(
                        color: context.colors.outline,
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
            if (channel.lastMessageAt != null) ...[
              const SizedBox(width: Grid.xxs),
              Text(
                _relativeTime(channel.lastMessageAt!),
                style: context.textTheme.labelSmall?.copyWith(
                  color: context.colors.outline,
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  IconData _iconFor(Channel channel) => channelIcon(channel);

  static String _relativeTime(DateTime dt) {
    final diff = DateTime.now().difference(dt);
    if (diff.inMinutes < 1) return 'now';
    if (diff.inMinutes < 60) return '${diff.inMinutes}m';
    if (diff.inHours < 24) return '${diff.inHours}h';
    if (diff.inDays < 7) return '${diff.inDays}d';
    if (diff.inDays < 365) return '${(diff.inDays / 7).floor()}w';
    return '${(diff.inDays / 365).floor()}y';
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
            Text('New', style: context.textTheme.titleMedium),
            const SizedBox(height: Grid.xxs),
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

    final kindLabel = channelType == 'forum' ? 'forum' : 'channel';

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
            Text('Create $kindLabel', style: context.textTheme.titleMedium),
            const SizedBox(height: Grid.xxs),
            Text(
              channelType == 'forum'
                  ? 'Forums organize threaded discussions around a topic.'
                  : 'Channels are real-time streams for team conversation.',
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: Grid.xs),
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
            SegmentedButton<String>(
              segments: const [
                ButtonSegment<String>(value: 'open', label: Text('Open')),
                ButtonSegment<String>(value: 'private', label: Text('Private')),
              ],
              selected: {visibility.value},
              onSelectionChanged: isSubmitting.value
                  ? null
                  : (selection) => visibility.value = selection.first,
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
    final mode = useState(_BrowseMode.channels);
    final query = useState('');
    final busyChannelId = useState<String?>(null);

    final normalizedQuery = query.value.trim().toLowerCase();
    final browsableChannels = channels.where((channel) {
      if (channel.isDm) {
        return false;
      }
      if (mode.value == _BrowseMode.channels && !channel.isStream) {
        return false;
      }
      if (mode.value == _BrowseMode.forums && !channel.isForum) {
        return false;
      }
      final visible = channel.isArchived
          ? channel.isMember
          : channel.visibility == 'open' || channel.isMember;
      if (!visible) {
        return false;
      }
      if (normalizedQuery.isEmpty) {
        return true;
      }
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
      if (busyChannelId.value != null) {
        return;
      }

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
            Text(
              mode.value == _BrowseMode.forums
                  ? 'Browse forums'
                  : 'Browse channels',
              style: context.textTheme.titleMedium,
            ),
            const SizedBox(height: Grid.xxs),
            SegmentedButton<_BrowseMode>(
              segments: const [
                ButtonSegment<_BrowseMode>(
                  value: _BrowseMode.channels,
                  label: Text('Channels'),
                ),
                ButtonSegment<_BrowseMode>(
                  value: _BrowseMode.forums,
                  label: Text('Forums'),
                ),
              ],
              selected: {mode.value},
              onSelectionChanged: (selection) => mode.value = selection.first,
            ),
            const SizedBox(height: Grid.xxs),
            TextField(
              decoration: InputDecoration(
                prefixIcon: const Icon(LucideIcons.search),
                hintText: mode.value == _BrowseMode.forums
                    ? 'Search forums by name or description'
                    : 'Search channels by name or description',
              ),
              onChanged: (value) => query.value = value,
            ),
            const SizedBox(height: Grid.xs),
            SizedBox(
              height: 360,
              child: browsableChannels.isEmpty
                  ? Padding(
                      padding: const EdgeInsets.symmetric(vertical: Grid.sm),
                      child: Center(
                        child: Text(
                          'No matching spaces',
                          style: context.textTheme.bodyMedium?.copyWith(
                            color: context.colors.onSurfaceVariant,
                          ),
                        ),
                      ),
                    )
                  : ListView(
                      shrinkWrap: true,
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
            Text('New direct message', style: context.textTheme.titleMedium),
            const SizedBox(height: Grid.xxs),
            Text(
              'Pick 1 to 8 people. If the conversation already exists, mobile will reopen it.',
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: Grid.xs),
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
      padding: const EdgeInsets.only(top: Grid.half, bottom: Grid.half),
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
