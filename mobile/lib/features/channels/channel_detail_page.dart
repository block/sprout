import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';
import '../profile/profile_provider.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';
import 'channel.dart';
import 'channel_management_provider.dart';
import 'channel_messages_provider.dart';
import 'channel_typing_provider.dart';
import 'channels_provider.dart';
import 'message_content.dart';
import 'send_message_provider.dart';
import 'timeline_message.dart';

/// Fetch channel members and preload their profiles into the user cache.
Future<void> _preloadMembers(WidgetRef ref, String channelId) async {
  // Capture references before async gap to avoid using disposed ref.
  final client = ref.read(relayClientProvider);
  final notifier = ref.read(userCacheProvider.notifier);
  try {
    final json =
        await client.get('/api/channels/$channelId/members')
            as Map<String, dynamic>;
    final members = json['members'] as List<dynamic>? ?? [];
    final pubkeys = members
        .map((m) => (m as Map<String, dynamic>)['pubkey'] as String)
        .toList();
    if (pubkeys.isNotEmpty) {
      notifier.preload(pubkeys);
    }
  } catch (_) {
    // Non-fatal — mentions will just fall back to cache from messages.
  }
}

class ChannelDetailPage extends HookConsumerWidget {
  final Channel channel;

  const ChannelDetailPage({super.key, required this.channel});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final detailsAsync = ref.watch(channelDetailsProvider(channel.id));
    final channelsAsync = ref.watch(channelsProvider);
    final messagesState = ref.watch(channelMessagesProvider(channel.id));
    final typingEntries = ref.watch(channelTypingProvider(channel.id));
    final currentPubkey = ref
        .watch(profileProvider)
        .whenData((value) => value?.pubkey)
        .value;
    final baseChannel =
        channelsAsync
            .whenData(
              (channels) => channels.firstWhere(
                (candidate) => candidate.id == channel.id,
                orElse: () => channel,
              ),
            )
            .value ??
        channel;
    final resolvedChannel =
        detailsAsync.whenData(baseChannel.mergeDetails).value ?? baseChannel;

    // Preload channel member profiles so @mentions resolve correctly.
    useEffect(() {
      _preloadMembers(ref, channel.id);
      return null;
    }, [channel.id]);

    return Scaffold(
      appBar: AppBar(
        title: Row(
          children: [
            Icon(
              channelIcon(resolvedChannel),
              size: 18,
              color: context.colors.onSurfaceVariant,
            ),
            const SizedBox(width: Grid.half),
            Expanded(
              child: Text(
                resolvedChannel.displayLabel(currentPubkey: currentPubkey),
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
        actions: [
          IconButton(
            onPressed: () {
              showModalBottomSheet<void>(
                context: context,
                isScrollControlled: true,
                showDragHandle: true,
                builder: (_) => _MembersSheet(
                  channel: resolvedChannel,
                  currentPubkey: currentPubkey,
                ),
              );
            },
            tooltip: 'View members',
            icon: const Icon(LucideIcons.users),
          ),
          if (!resolvedChannel.isDm)
            IconButton(
              onPressed: () async {
                final shouldClose = await showModalBottomSheet<bool>(
                  context: context,
                  isScrollControlled: true,
                  showDragHandle: true,
                  builder: (_) => _ManageChannelSheet(channel: resolvedChannel),
                );
                if (shouldClose == true && context.mounted) {
                  Navigator.of(context).pop();
                }
              },
              tooltip: 'Manage channel',
              icon: const Icon(LucideIcons.ellipsis),
            ),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: resolvedChannel.isForum
                ? _ForumPlaceholder(channel: resolvedChannel)
                : messagesState.when(
                    loading: () =>
                        const Center(child: CircularProgressIndicator()),
                    error: (e, _) => Center(
                      child: Text(
                        'Failed to load messages',
                        style: context.textTheme.bodyMedium?.copyWith(
                          color: context.colors.error,
                        ),
                      ),
                    ),
                    data: (events) {
                      final messages = formatTimeline(events);
                      return _MessageList(
                        messages: messages,
                        channelId: channel.id,
                      );
                    },
                  ),
          ),
          if (!resolvedChannel.isForum && typingEntries.isNotEmpty)
            _TypingIndicator(entries: typingEntries),
          if (!resolvedChannel.isForum &&
              resolvedChannel.isMember &&
              !resolvedChannel.isArchived)
            _ComposeBar(channelId: channel.id)
          else if (!resolvedChannel.isForum &&
              !resolvedChannel.isDm &&
              (!resolvedChannel.isMember || resolvedChannel.isArchived))
            _ReadOnlyNotice(channel: resolvedChannel),
        ],
      ),
    );
  }
}

class _MessageList extends ConsumerWidget {
  final List<TimelineMessage> messages;
  final String channelId;

  const _MessageList({required this.messages, required this.channelId});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (messages.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              LucideIcons.messageSquare,
              size: Grid.xl,
              color: context.colors.outline,
            ),
            const SizedBox(height: Grid.xxs),
            Text(
              'No messages yet',
              style: context.textTheme.bodyLarge?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: Grid.half),
            Text(
              'Be the first to say something!',
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.outline,
              ),
            ),
          ],
        ),
      );
    }

    // Build channel names map once for all message bubbles.
    final channelsAsync = ref.watch(channelsProvider);
    final channelNamesMap = <String, String>{};
    channelsAsync.whenData((channels) {
      for (final ch in channels) {
        channelNamesMap[ch.name.toLowerCase()] = ch.id;
      }
    });

    return ListView.builder(
      reverse: true,
      padding: const EdgeInsets.symmetric(
        horizontal: Grid.xs,
        vertical: Grid.xxs,
      ),
      itemCount: messages.length,
      itemBuilder: (context, index) {
        // Reversed list: index 0 = newest (bottom of screen).
        final chronIdx = messages.length - 1 - index;
        final message = messages[chronIdx];

        if (message.isSystem) {
          return _SystemMessageRow(message: message);
        }

        // The message visually above is the one earlier in time.
        final prevMessage = chronIdx > 0 ? messages[chronIdx - 1] : null;
        final showAuthor =
            prevMessage == null ||
            prevMessage.isSystem ||
            prevMessage.pubkey.toLowerCase() != message.pubkey.toLowerCase() ||
            (message.createdAt - prevMessage.createdAt) > 300;

        return _MessageBubble(
          message: message,
          showAuthor: showAuthor,
          channelNames: channelNamesMap,
          currentChannelId: channelId,
        );
      },
    );
  }
}

// ---------------------------------------------------------------------------
// System message row
// ---------------------------------------------------------------------------

class _SystemMessageRow extends ConsumerWidget {
  final TimelineMessage message;

  const _SystemMessageRow({required this.message});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final systemEvent = message.systemEvent;
    if (systemEvent == null) return const SizedBox.shrink();

    final userCache = ref.watch(userCacheProvider);

    String resolveLabel(String? pubkey) {
      if (pubkey == null) return 'Someone';
      final profile =
          userCache[pubkey.toLowerCase()] ??
          ref.read(userCacheProvider.notifier).get(pubkey.toLowerCase());
      return profile?.label ?? _shortPubkey(pubkey);
    }

    final description = systemEvent.describe(resolveLabel);

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Grid.half),
      child: Row(
        children: [
          Container(
            width: 20,
            height: 20,
            decoration: BoxDecoration(
              color: context.colors.surfaceContainerHighest,
              shape: BoxShape.circle,
            ),
            child: Icon(
              LucideIcons.arrowLeftRight,
              size: 12,
              color: context.colors.outline,
            ),
          ),
          const SizedBox(width: Grid.xxs),
          Expanded(
            child: Text(
              description,
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.outline,
              ),
            ),
          ),
          Text(
            _formatTime(message.createdAt),
            style: context.textTheme.labelSmall?.copyWith(
              color: context.colors.outline.withValues(alpha: 0.6),
            ),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// User message bubble
// ---------------------------------------------------------------------------

class _MessageBubble extends ConsumerWidget {
  final TimelineMessage message;
  final bool showAuthor;
  final Map<String, String> channelNames;
  final String currentChannelId;

  const _MessageBubble({
    required this.message,
    required this.showAuthor,
    required this.channelNames,
    required this.currentChannelId,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Watch only this user's profile to avoid rebuilding on unrelated cache changes.
    final pk = message.pubkey.toLowerCase();
    final profile =
        ref.watch(userCacheProvider.select((cache) => cache[pk])) ??
        ref.read(userCacheProvider.notifier).get(pk);
    final displayName = profile?.label ?? _shortPubkey(message.pubkey);

    // Build mention names map from event p-tags.
    final userCache = ref.watch(userCacheProvider);
    final mentionNames = <String, String>{};
    for (final mpk in message.mentionPubkeys) {
      final p = userCache[mpk.toLowerCase()];
      if (p?.displayName != null) {
        mentionNames[mpk.toLowerCase()] = p!.displayName!;
      }
    }

    return Padding(
      padding: EdgeInsets.only(top: showAuthor ? Grid.xxs : Grid.quarter),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (showAuthor)
            _UserAvatar(profile: profile, pubkey: message.pubkey)
          else
            const SizedBox(width: 28),
          const SizedBox(width: Grid.xxs),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (showAuthor)
                  Padding(
                    padding: const EdgeInsets.only(bottom: Grid.quarter),
                    child: Row(
                      children: [
                        Text(
                          displayName,
                          style: context.textTheme.labelMedium?.copyWith(
                            fontWeight: FontWeight.w600,
                            color: context.colors.onSurface,
                          ),
                        ),
                        const SizedBox(width: Grid.xxs),
                        Text(
                          _formatTime(message.createdAt),
                          style: context.textTheme.labelSmall?.copyWith(
                            color: context.colors.outline,
                          ),
                        ),
                        if (message.edited) ...[
                          const SizedBox(width: Grid.half),
                          Text(
                            '(edited)',
                            style: context.textTheme.labelSmall?.copyWith(
                              color: context.colors.outline,
                              fontStyle: FontStyle.italic,
                            ),
                          ),
                        ],
                      ],
                    ),
                  ),
                MessageContent(
                  content: message.content,
                  mentionNames: mentionNames,
                  channelNames: channelNames,
                  onChannelTap: (channelId) {
                    if (channelId == currentChannelId) {
                      return;
                    }
                    final channelsAsync = ref.read(channelsProvider);
                    final channels = channelsAsync.hasValue
                        ? channelsAsync.value
                        : null;
                    Channel? targetChannel;
                    for (final channel in channels ?? const <Channel>[]) {
                      if (channel.id == channelId) {
                        targetChannel = channel;
                        break;
                      }
                    }
                    if (targetChannel == null) return;
                    Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) =>
                            ChannelDetailPage(channel: targetChannel!),
                      ),
                    );
                  },
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _UserAvatar extends StatelessWidget {
  final UserProfile? profile;
  final String pubkey;

  const _UserAvatar({required this.profile, required this.pubkey});

  @override
  Widget build(BuildContext context) {
    final initial =
        profile?.initial ?? (pubkey.isNotEmpty ? pubkey[0].toUpperCase() : '?');
    final avatarUrl = profile?.avatarUrl;

    return CircleAvatar(
      radius: 14,
      backgroundColor: context.colors.primaryContainer,
      backgroundImage: avatarUrl != null ? NetworkImage(avatarUrl) : null,
      child: avatarUrl == null
          ? Text(
              initial,
              style: context.textTheme.labelSmall?.copyWith(
                color: context.colors.onPrimaryContainer,
                fontWeight: FontWeight.w600,
              ),
            )
          : null,
    );
  }
}

// ---------------------------------------------------------------------------
// Channel management
// ---------------------------------------------------------------------------

IconData channelIcon(Channel channel) {
  if (channel.isDm) return LucideIcons.messagesSquare;
  if (channel.isPrivate) return LucideIcons.lock;
  if (channel.isForum) return LucideIcons.messageSquareText;
  return LucideIcons.hash;
}

class _ForumPlaceholder extends StatelessWidget {
  final Channel channel;

  const _ForumPlaceholder({required this.channel});

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: Grid.sm),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              LucideIcons.messageSquareText,
              size: Grid.xl,
              color: context.colors.outline,
            ),
            const SizedBox(height: Grid.xs),
            Text(
              'Forum threads are not on mobile yet',
              style: context.textTheme.titleMedium,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: Grid.xxs),
            Text(
              'You can still view channel context, canvas, and members from the actions above.',
              style: context.textTheme.bodyMedium?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
              textAlign: TextAlign.center,
            ),
            if (channel.description.trim().isNotEmpty) ...[
              const SizedBox(height: Grid.xs),
              Text(
                channel.description,
                style: context.textTheme.bodySmall?.copyWith(
                  color: context.colors.outline,
                ),
                textAlign: TextAlign.center,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _ReadOnlyNotice extends StatelessWidget {
  final Channel channel;

  const _ReadOnlyNotice({required this.channel});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: EdgeInsets.only(
        left: Grid.xs,
        right: Grid.xs,
        top: Grid.xxs,
        bottom: MediaQuery.viewPaddingOf(context).bottom + Grid.xxs,
      ),
      decoration: BoxDecoration(
        border: Border(top: BorderSide(color: context.colors.outlineVariant)),
        color: context.colors.surface,
      ),
      child: Text(
        channel.isArchived
            ? 'This ${channel.isForum ? 'forum' : 'channel'} is archived and read-only on mobile.'
            : 'Join this ${channel.isForum ? 'forum' : 'channel'} from Manage to participate.',
        style: context.textTheme.bodySmall?.copyWith(
          color: context.colors.onSurfaceVariant,
        ),
        textAlign: TextAlign.center,
      ),
    );
  }
}

class _MembersSheet extends HookConsumerWidget {
  final Channel channel;
  final String? currentPubkey;

  const _MembersSheet({required this.channel, required this.currentPubkey});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final membersAsync = ref.watch(channelMembersProvider(channel.id));
    final allMembers = membersAsync.asData?.value ?? const <ChannelMember>[];
    final people = allMembers.where((member) => !member.isBot).toList();
    final userCache = ref.watch(userCacheProvider);

    // Preload profiles for all members so avatars appear.
    useEffect(() {
      if (people.isNotEmpty) {
        ref
            .read(userCacheProvider.notifier)
            .preload(people.map((m) => m.pubkey).toList());
      }
      return null;
    }, [people.length]);

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
              if (!channel.isDm) ...[
                const SizedBox(height: Grid.xxs),
                Text(
                  channel.isArchived
                      ? 'Archived channels are read-only on mobile. Member and bot management stay on desktop.'
                      : 'Member and bot management stay on desktop.',
                  style: context.textTheme.bodySmall?.copyWith(
                    color: context.colors.outline,
                  ),
                ),
              ],
              if (!channel.isDm) ...[const Divider(height: Grid.sm)],
              SizedBox(
                height: 280,
                child: membersAsync.when(
                  data: (_) => people.isEmpty
                      ? Center(
                          child: Text(
                            'No people found.',
                            style: context.textTheme.bodySmall?.copyWith(
                              color: context.colors.outline,
                            ),
                          ),
                        )
                      : ListView(
                          shrinkWrap: true,
                          children: [
                            for (final member in people)
                              Builder(
                                builder: (context) {
                                  final profile =
                                      userCache[member.pubkey.toLowerCase()];
                                  final avatarUrl = profile?.avatarUrl;
                                  final label = member.labelFor(currentPubkey);
                                  final initial = label
                                      .substring(0, 1)
                                      .toUpperCase();
                                  return ListTile(
                                    contentPadding: EdgeInsets.zero,
                                    leading: _MemberAvatar(
                                      avatarUrl: avatarUrl,
                                      initial: initial,
                                    ),
                                    title: Text(label),
                                    subtitle: Text(member.role),
                                  );
                                },
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

class _ManageChannelSheet extends HookConsumerWidget {
  final Channel channel;

  const _ManageChannelSheet({required this.channel});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final canvasAsync = ref.watch(channelCanvasProvider(channel.id));
    final isEditingCanvas = useState(false);
    final isSavingCanvas = useState(false);
    final isBusy = useState(false);
    final actionError = useState<String?>(null);
    final canvasController = useTextEditingController();

    useEffect(() {
      final canvas = canvasAsync.asData?.value;
      if (!isEditingCanvas.value) {
        canvasController.text = canvas?.content ?? '';
      }
      return null;
    }, [canvasAsync.asData?.value.content, isEditingCanvas.value]);

    final canJoin =
        channel.visibility == 'open' &&
        !channel.isArchived &&
        !channel.isMember &&
        !channel.isDm;
    final canLeave = channel.isMember && !channel.isArchived && !channel.isDm;
    final canEditCanvas = channel.isMember && !channel.isArchived;

    Future<void> joinChannel() async {
      if (isBusy.value) return;
      isBusy.value = true;
      actionError.value = null;
      try {
        await ref.read(channelActionsProvider).joinChannel(channel.id);
        if (context.mounted) {
          Navigator.of(context).pop(false);
        }
      } catch (error) {
        actionError.value = error.toString();
      } finally {
        isBusy.value = false;
      }
    }

    Future<void> leaveChannel() async {
      if (isBusy.value) return;
      isBusy.value = true;
      actionError.value = null;
      try {
        await ref.read(channelActionsProvider).leaveChannel(channel.id);
        if (context.mounted) {
          Navigator.of(context).pop(true);
        }
      } catch (error) {
        actionError.value = error.toString();
      } finally {
        isBusy.value = false;
      }
    }

    Future<void> saveCanvas() async {
      if (isSavingCanvas.value) {
        return;
      }
      isSavingCanvas.value = true;
      actionError.value = null;
      try {
        await ref
            .read(channelActionsProvider)
            .setCanvas(
              channelId: channel.id,
              content: canvasController.text.trim(),
            );
        if (context.mounted) {
          isEditingCanvas.value = false;
        }
      } catch (error) {
        actionError.value = error.toString();
      } finally {
        isSavingCanvas.value = false;
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
        child: ListView(
          shrinkWrap: true,
          children: [
            Text('Manage channel', style: context.textTheme.titleMedium),
            const SizedBox(height: Grid.xxs),
            Text(
              'Basic management for ${channel.name}.',
              style: context.textTheme.bodySmall?.copyWith(
                color: context.colors.onSurfaceVariant,
              ),
            ),
            if (actionError.value case final error?) ...[
              const SizedBox(height: Grid.xs),
              Text(
                error,
                style: context.textTheme.bodySmall?.copyWith(
                  color: context.colors.error,
                ),
              ),
            ],
            if (canJoin || canLeave) ...[
              const SizedBox(height: Grid.xs),
              Wrap(
                spacing: Grid.xxs,
                children: [
                  if (canJoin)
                    FilledButton.tonal(
                      onPressed: isBusy.value ? null : joinChannel,
                      child: Text(isBusy.value ? 'Joining…' : 'Join channel'),
                    ),
                  if (canLeave)
                    OutlinedButton(
                      onPressed: isBusy.value ? null : leaveChannel,
                      child: Text(isBusy.value ? 'Leaving…' : 'Leave channel'),
                    ),
                ],
              ),
            ],
            const SizedBox(height: Grid.sm),
            Text('Context', style: context.textTheme.labelLarge),
            const SizedBox(height: Grid.xxs),
            _ContextCard(
              label: 'Description',
              value: channel.description,
              emptyLabel: 'No description set',
            ),
            const SizedBox(height: Grid.xxs),
            _ContextCard(
              label: 'Topic',
              value: channel.topic,
              emptyLabel: 'No topic set',
            ),
            const SizedBox(height: Grid.xxs),
            _ContextCard(
              label: 'Purpose',
              value: channel.purpose,
              emptyLabel: 'No purpose set',
            ),
            if (!channel.isDm) ...[
              const SizedBox(height: Grid.sm),
              Text('Canvas', style: context.textTheme.labelLarge),
              const SizedBox(height: Grid.xxs),
              canvasAsync.when(
                data: (canvas) {
                  if (isEditingCanvas.value) {
                    return Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        TextField(
                          controller: canvasController,
                          maxLines: 8,
                          minLines: 6,
                          decoration: const InputDecoration(
                            hintText: 'Write your canvas content in Markdown…',
                          ),
                        ),
                        const SizedBox(height: Grid.xxs),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.end,
                          children: [
                            TextButton(
                              onPressed: isSavingCanvas.value
                                  ? null
                                  : () {
                                      isEditingCanvas.value = false;
                                      canvasController.text =
                                          canvas.content ?? '';
                                    },
                              child: const Text('Cancel'),
                            ),
                            const SizedBox(width: Grid.half),
                            FilledButton(
                              onPressed: isSavingCanvas.value
                                  ? null
                                  : saveCanvas,
                              child: Text(
                                isSavingCanvas.value
                                    ? 'Saving…'
                                    : 'Save canvas',
                              ),
                            ),
                          ],
                        ),
                      ],
                    );
                  }

                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Container(
                        width: double.infinity,
                        padding: const EdgeInsets.all(Grid.xs),
                        decoration: BoxDecoration(
                          color: context.colors.surfaceContainerHighest,
                          borderRadius: BorderRadius.circular(Radii.md),
                        ),
                        child: Text(
                          canvas.content?.trim().isNotEmpty == true
                              ? canvas.content!
                              : 'No canvas set for this channel.',
                          style: context.textTheme.bodyMedium?.copyWith(
                            color: context.colors.onSurfaceVariant,
                          ),
                        ),
                      ),
                      const SizedBox(height: Grid.xxs),
                      Align(
                        alignment: Alignment.centerRight,
                        child: FilledButton.tonal(
                          onPressed: canEditCanvas
                              ? () => isEditingCanvas.value = true
                              : null,
                          child: Text(
                            canvas.content?.trim().isNotEmpty == true
                                ? 'Edit canvas'
                                : 'Create canvas',
                          ),
                        ),
                      ),
                    ],
                  );
                },
                loading: () => const Center(child: CircularProgressIndicator()),
                error: (error, _) => Text(
                  error.toString(),
                  style: context.textTheme.bodySmall?.copyWith(
                    color: context.colors.error,
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _ContextCard extends StatelessWidget {
  final String label;
  final String? value;
  final String emptyLabel;

  const _ContextCard({
    required this.label,
    required this.value,
    required this.emptyLabel,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(Grid.xs),
      decoration: BoxDecoration(
        color: context.colors.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(Radii.md),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            style: context.textTheme.labelSmall?.copyWith(
              color: context.colors.outline,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: Grid.half),
          Text(
            value?.trim().isNotEmpty == true ? value!.trim() : emptyLabel,
            style: context.textTheme.bodyMedium?.copyWith(
              color: context.colors.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Typing indicator
// ---------------------------------------------------------------------------

class _TypingIndicator extends ConsumerWidget {
  final List<TypingEntry> entries;

  const _TypingIndicator({required this.entries});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final userCache = ref.watch(userCacheProvider);
    final names = entries.map((e) {
      final profile =
          userCache[e.pubkey.toLowerCase()] ??
          ref.read(userCacheProvider.notifier).get(e.pubkey.toLowerCase());
      return profile?.label ?? _shortPubkey(e.pubkey);
    }).toList();
    final text = switch (names.length) {
      1 => '${names[0]} is typing…',
      2 => '${names[0]} and ${names[1]} are typing…',
      _ => '${names[0]} and ${names.length - 1} others are typing…',
    };

    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: Grid.xs,
        vertical: Grid.quarter + 2,
      ),
      child: Text(
        text,
        style: context.textTheme.labelSmall?.copyWith(
          color: context.colors.outline,
          fontStyle: FontStyle.italic,
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Compose bar
// ---------------------------------------------------------------------------

class _ComposeBar extends HookConsumerWidget {
  final String channelId;

  const _ComposeBar({required this.channelId});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = useTextEditingController();
    final isSending = useState(false);

    Future<void> send() async {
      final text = controller.text.trim();
      if (text.isEmpty || isSending.value) return;

      isSending.value = true;
      try {
        await ref
            .read(sendMessageProvider)
            .call(channelId: channelId, content: text);
        if (context.mounted) controller.clear();
      } finally {
        if (context.mounted) isSending.value = false;
      }
    }

    return Container(
      decoration: BoxDecoration(
        border: Border(top: BorderSide(color: context.colors.outlineVariant)),
        color: context.colors.surface,
      ),
      padding: EdgeInsets.only(
        left: Grid.xs,
        right: Grid.xxs,
        top: Grid.xxs,
        bottom: MediaQuery.viewPaddingOf(context).bottom + Grid.xxs,
      ),
      child: Row(
        children: [
          Expanded(
            child: TextField(
              controller: controller,
              textInputAction: TextInputAction.send,
              onSubmitted: (_) => send(),
              minLines: 1,
              maxLines: 5,
              decoration: InputDecoration(
                hintText: 'Message…',
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(Radii.lg),
                  borderSide: BorderSide(color: context.colors.outlineVariant),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(Radii.lg),
                  borderSide: BorderSide(color: context.colors.outlineVariant),
                ),
                focusedBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(Radii.lg),
                  borderSide: BorderSide(color: context.colors.primary),
                ),
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: Grid.twelve,
                  vertical: Grid.xxs,
                ),
                isDense: true,
              ),
            ),
          ),
          const SizedBox(width: Grid.half),
          IconButton(
            onPressed: isSending.value ? null : send,
            icon: isSending.value
                ? SizedBox(
                    width: 20,
                    height: 20,
                    child: CircularProgressIndicator(
                      strokeWidth: 2,
                      color: context.colors.primary,
                    ),
                  )
                : Icon(
                    LucideIcons.sendHorizontal,
                    color: context.colors.primary,
                  ),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

String _shortPubkey(String pubkey) {
  if (pubkey.length > 12) return '${pubkey.substring(0, 8)}…';
  return pubkey;
}

String _formatTime(int createdAt) {
  final dt = DateTime.fromMillisecondsSinceEpoch(
    createdAt * 1000,
    isUtc: true,
  ).toLocal();
  final now = DateTime.now();
  final diff = now.difference(dt);

  if (diff.inDays > 0) {
    return '${dt.month}/${dt.day} ${_pad(dt.hour)}:${_pad(dt.minute)}';
  }
  return '${_pad(dt.hour)}:${_pad(dt.minute)}';
}

String _pad(int n) => n.toString().padLeft(2, '0');
