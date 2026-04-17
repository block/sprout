import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';
import '../profile/presence_cache_provider.dart';
import '../profile/profile_provider.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';
import 'channel.dart';
import 'channel_management_provider.dart';
import 'channel_messages_provider.dart';
import 'channel_typing_provider.dart';
import 'channels_provider.dart';
import 'compose_bar.dart';
import 'message_content.dart';
import 'send_message_provider.dart';
import 'thread_detail_page.dart';
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
    // Only show channel-level typing (exclude thread-scoped entries).
    final typingEntries = ref
        .watch(channelTypingProvider(channel.id))
        .where((e) => e.threadHeadId == null)
        .toList();
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
        title: resolvedChannel.isDm
            ? _DmAppBarTitle(
                channel: resolvedChannel,
                currentPubkey: currentPubkey,
              )
            : Row(
                children: [
                  Icon(
                    channelIcon(resolvedChannel),
                    size: 18,
                    color: context.colors.onSurfaceVariant,
                  ),
                  const SizedBox(width: Grid.half),
                  Expanded(
                    child: Text(
                      resolvedChannel.displayLabel(
                        currentPubkey: currentPubkey,
                      ),
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
                      final messages = formatTimeline(
                        events,
                        currentPubkey: currentPubkey,
                      );
                      final entries = buildMainTimelineEntries(messages);
                      return _MessageList(
                        entries: entries,
                        allMessages: messages,
                        channelId: channel.id,
                        currentPubkey: currentPubkey,
                        isMember: resolvedChannel.isMember,
                        isArchived: resolvedChannel.isArchived,
                      );
                    },
                  ),
          ),
          if (!resolvedChannel.isForum && typingEntries.isNotEmpty)
            _TypingIndicator(entries: typingEntries),
          if (!resolvedChannel.isForum &&
              resolvedChannel.isMember &&
              !resolvedChannel.isArchived)
            ComposeBar(
              channelId: channel.id,
              channelName: resolvedChannel.name,
              onSend: (content) => ref
                  .read(sendMessageProvider)
                  .call(channelId: channel.id, content: content),
            )
          else if (!resolvedChannel.isForum &&
              !resolvedChannel.isDm &&
              (!resolvedChannel.isMember || resolvedChannel.isArchived))
            _ReadOnlyNotice(channel: resolvedChannel),
        ],
      ),
    );
  }
}

class _MessageList extends HookConsumerWidget {
  final List<MainTimelineEntry> entries;
  final List<TimelineMessage> allMessages;
  final String channelId;
  final String? currentPubkey;
  final bool isMember;
  final bool isArchived;

  const _MessageList({
    required this.entries,
    required this.allMessages,
    required this.channelId,
    required this.currentPubkey,
    required this.isMember,
    required this.isArchived,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Pagination: fetch older messages when scrolling near the top.
    final scrollController = useScrollController();
    final isLoadingOlder = useState(false);

    useEffect(() {
      void onScroll() {
        if (isLoadingOlder.value) return;
        final notifier = ref.read(channelMessagesProvider(channelId).notifier);
        if (notifier.reachedOldest) return;
        // In a reversed ListView, maxScrollExtent is the oldest messages.
        final pos = scrollController.position;
        if (pos.pixels >= pos.maxScrollExtent - 200) {
          isLoadingOlder.value = true;
          notifier.fetchOlder().whenComplete(
            () => isLoadingOlder.value = false,
          );
        }
      }

      scrollController.addListener(onScroll);
      return () => scrollController.removeListener(onScroll);
    }, [scrollController]);

    if (entries.isEmpty) {
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
      controller: scrollController,
      reverse: true,
      padding: const EdgeInsets.symmetric(
        horizontal: Grid.xs,
        vertical: Grid.xxs,
      ),
      itemCount: entries.length + (isLoadingOlder.value ? 1 : 0),
      itemBuilder: (context, index) {
        // Loading indicator at the top (last index in reversed list).
        if (index >= entries.length) {
          return const Padding(
            padding: EdgeInsets.symmetric(vertical: Grid.xs),
            child: Center(
              child: SizedBox(
                width: 20,
                height: 20,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            ),
          );
        }

        // Reversed list: index 0 = newest (bottom of screen).
        final chronIdx = entries.length - 1 - index;
        final entry = entries[chronIdx];
        final message = entry.message;

        if (message.isSystem) {
          return _SystemMessageRow(message: message);
        }

        // The message visually above is the one earlier in time.
        final prevEntry = chronIdx > 0 ? entries[chronIdx - 1] : null;
        final prevMessage = prevEntry?.message;
        final showAuthor =
            prevMessage == null ||
            prevMessage.isSystem ||
            prevMessage.pubkey.toLowerCase() != message.pubkey.toLowerCase() ||
            (message.createdAt - prevMessage.createdAt) > 300;

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _MessageBubble(
              message: message,
              showAuthor: showAuthor,
              channelNames: channelNamesMap,
              currentChannelId: channelId,
              currentPubkey: currentPubkey,
              allMessages: allMessages,
              isMember: isMember,
              isArchived: isArchived,
            ),
            if (entry.summary != null)
              _ThreadSummaryRow(
                summary: entry.summary!,
                message: message,
                allMessages: allMessages,
                channelId: channelId,
                currentPubkey: currentPubkey,
                isMember: isMember,
                isArchived: isArchived,
              ),
          ],
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
// Thread summary row (shown below messages that have replies)
// ---------------------------------------------------------------------------

class _ThreadSummaryRow extends ConsumerWidget {
  final ThreadSummary summary;
  final TimelineMessage message;
  final List<TimelineMessage> allMessages;
  final String channelId;
  final String? currentPubkey;
  final bool isMember;
  final bool isArchived;

  const _ThreadSummaryRow({
    required this.summary,
    required this.message,
    required this.allMessages,
    required this.channelId,
    required this.currentPubkey,
    required this.isMember,
    required this.isArchived,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final userCache = ref.watch(userCacheProvider);

    return GestureDetector(
      onTap: () {
        Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (_) => ThreadDetailPage(
              threadHead: message,
              allMessages: allMessages,
              channelId: channelId,
              currentPubkey: currentPubkey,
              isMember: isMember,
              isArchived: isArchived,
            ),
          ),
        );
      },
      child: Padding(
        padding: const EdgeInsets.only(
          left: 36,
          top: Grid.half,
          bottom: Grid.half,
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            // Stacked participant avatars.
            SizedBox(
              width: 20.0 + (summary.participantPubkeys.length - 1) * 12.0,
              height: 20,
              child: Stack(
                children: [
                  for (var i = 0; i < summary.participantPubkeys.length; i++)
                    Positioned(
                      left: i * 12.0,
                      child: _SmallAvatar(
                        pubkey: summary.participantPubkeys[i],
                        userCache: userCache,
                      ),
                    ),
                ],
              ),
            ),
            const SizedBox(width: Grid.xxs),
            Text(
              '${summary.replyCount} ${summary.replyCount == 1 ? 'reply' : 'replies'}',
              style: context.textTheme.labelMedium?.copyWith(
                color: context.colors.primary,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(width: Grid.half),
            Icon(
              LucideIcons.chevronRight,
              size: 14,
              color: context.colors.primary,
            ),
          ],
        ),
      ),
    );
  }
}

class _SmallAvatar extends StatelessWidget {
  final String pubkey;
  final Map<String, UserProfile> userCache;

  const _SmallAvatar({required this.pubkey, required this.userCache});

  @override
  Widget build(BuildContext context) {
    final profile = userCache[pubkey.toLowerCase()];
    final avatarUrl = profile?.avatarUrl;
    final initial =
        profile?.initial ?? (pubkey.isNotEmpty ? pubkey[0].toUpperCase() : '?');

    return Container(
      width: 20,
      height: 20,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        border: Border.all(color: context.colors.surface, width: 1.5),
      ),
      child: CircleAvatar(
        radius: 9,
        backgroundColor: context.colors.primaryContainer,
        backgroundImage: avatarUrl != null ? NetworkImage(avatarUrl) : null,
        child: avatarUrl == null
            ? Text(
                initial,
                style: TextStyle(
                  fontSize: 8,
                  fontWeight: FontWeight.w600,
                  color: context.colors.onPrimaryContainer,
                ),
              )
            : null,
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
  final String? currentPubkey;
  final List<TimelineMessage>? allMessages;
  final bool isMember;
  final bool isArchived;

  const _MessageBubble({
    required this.message,
    required this.showAuthor,
    required this.channelNames,
    required this.currentChannelId,
    required this.currentPubkey,
    this.allMessages,
    this.isMember = false,
    this.isArchived = false,
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

    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onLongPress: () => _showMessageActions(
        context: context,
        ref: ref,
        message: message,
        channelId: currentChannelId,
        isOwnMessage:
            currentPubkey?.toLowerCase() == message.pubkey.toLowerCase(),
        allMessages: allMessages,
        currentPubkey: currentPubkey,
        isMember: isMember,
        isArchived: isArchived,
      ),
      child: Padding(
        padding: EdgeInsets.only(top: showAuthor ? Grid.xs : Grid.quarter),
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
                      if (channelId == currentChannelId) return;
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
                  if (message.reactions.isNotEmpty)
                    _ReactionRow(
                      reactions: message.reactions,
                      onToggle: (emoji) {
                        final actions = ref.read(channelActionsProvider);
                        final reaction = message.reactions.firstWhere(
                          (r) => r.emoji == emoji,
                        );
                        if (reaction.reactedByCurrentUser &&
                            reaction.currentUserReactionId != null) {
                          actions.removeReaction(
                            reaction.currentUserReactionId!,
                            emoji,
                          );
                        } else {
                          actions.addReaction(message.id, emoji);
                        }
                      },
                    ),
                ],
              ),
            ),
          ],
        ),
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
// Reaction pills row
// ---------------------------------------------------------------------------

class _ReactionRow extends StatelessWidget {
  final List<TimelineReaction> reactions;
  final void Function(String emoji) onToggle;

  const _ReactionRow({required this.reactions, required this.onToggle});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: Grid.half),
      child: Wrap(
        spacing: Grid.half,
        runSpacing: Grid.half,
        children: [
          for (final reaction in reactions)
            GestureDetector(
              onTap: () => onToggle(reaction.emoji),
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: Grid.xxs,
                  vertical: Grid.quarter,
                ),
                decoration: BoxDecoration(
                  color: reaction.reactedByCurrentUser
                      ? context.colors.primary.withValues(alpha: 0.12)
                      : context.colors.surfaceContainerHighest,
                  borderRadius: BorderRadius.circular(Radii.lg),
                  border: Border.all(
                    color: reaction.reactedByCurrentUser
                        ? context.colors.primary.withValues(alpha: 0.4)
                        : context.colors.outlineVariant,
                  ),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(reaction.emoji, style: const TextStyle(fontSize: 14)),
                    if (reaction.count > 1) ...[
                      const SizedBox(width: Grid.quarter),
                      Text(
                        '${reaction.count}',
                        style: context.textTheme.labelSmall?.copyWith(
                          color: reaction.reactedByCurrentUser
                              ? context.colors.primary
                              : context.colors.onSurfaceVariant,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Message actions (long-press sheet)
// ---------------------------------------------------------------------------

const _quickEmojis = ['👍', '❤️', '😂', '🎉', '👀', '🙏'];

void _showMessageActions({
  required BuildContext context,
  required WidgetRef ref,
  required TimelineMessage message,
  required String channelId,
  required bool isOwnMessage,
  List<TimelineMessage>? allMessages,
  String? currentPubkey,
  bool isMember = false,
  bool isArchived = false,
}) {
  showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (sheetContext) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(Grid.xs, 0, Grid.xs, Grid.xs),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            // Quick emoji row
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                for (final emoji in _quickEmojis)
                  GestureDetector(
                    onTap: () {
                      Navigator.of(sheetContext).pop();
                      ref
                          .read(channelActionsProvider)
                          .addReaction(message.id, emoji);
                    },
                    child: Container(
                      width: 44,
                      height: 44,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: Theme.of(
                          sheetContext,
                        ).colorScheme.surfaceContainerHighest,
                        shape: BoxShape.circle,
                      ),
                      child: Text(emoji, style: const TextStyle(fontSize: 20)),
                    ),
                  ),
              ],
            ),
            const SizedBox(height: Grid.xs),
            if (allMessages != null)
              ListTile(
                leading: const Icon(LucideIcons.messageSquareReply),
                title: const Text('Reply in thread'),
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  Navigator.of(context).push(
                    MaterialPageRoute<void>(
                      builder: (_) => ThreadDetailPage(
                        threadHead: message,
                        allMessages: allMessages,
                        channelId: channelId,
                        currentPubkey: currentPubkey,
                        isMember: isMember,
                        isArchived: isArchived,
                      ),
                    ),
                  );
                },
              ),
            ListTile(
              leading: const Icon(LucideIcons.copy),
              title: const Text('Copy text'),
              onTap: () {
                Navigator.of(sheetContext).pop();
                // Copy to clipboard
                final data = ClipboardData(text: message.content);
                Clipboard.setData(data);
              },
            ),
            if (isOwnMessage) ...[
              ListTile(
                leading: const Icon(LucideIcons.pencil),
                title: const Text('Edit message'),
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  _showEditSheet(
                    context: context,
                    ref: ref,
                    message: message,
                    channelId: channelId,
                  );
                },
              ),
              ListTile(
                leading: Icon(
                  LucideIcons.trash2,
                  color: Theme.of(sheetContext).colorScheme.error,
                ),
                title: Text(
                  'Delete message',
                  style: TextStyle(
                    color: Theme.of(sheetContext).colorScheme.error,
                  ),
                ),
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  _confirmDelete(
                    context: context,
                    ref: ref,
                    messageId: message.id,
                  );
                },
              ),
            ],
          ],
        ),
      ),
    ),
  );
}

void _showEditSheet({
  required BuildContext context,
  required WidgetRef ref,
  required TimelineMessage message,
  required String channelId,
}) {
  final controller = TextEditingController(text: message.content);
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (sheetContext) => Padding(
      padding: EdgeInsets.fromLTRB(
        Grid.xs,
        0,
        Grid.xs,
        MediaQuery.viewInsetsOf(sheetContext).bottom + Grid.xs,
      ),
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: controller,
              autofocus: true,
              minLines: 1,
              maxLines: 5,
              decoration: const InputDecoration(hintText: 'Edit message'),
            ),
            const SizedBox(height: Grid.xxs),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.of(sheetContext).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: Grid.half),
                FilledButton(
                  onPressed: () {
                    final text = controller.text.trim();
                    if (text.isEmpty || text == message.content) {
                      Navigator.of(sheetContext).pop();
                      return;
                    }
                    ref
                        .read(channelActionsProvider)
                        .editMessage(
                          channelId: channelId,
                          eventId: message.id,
                          content: text,
                        );
                    Navigator.of(sheetContext).pop();
                  },
                  child: const Text('Save'),
                ),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}

void _confirmDelete({
  required BuildContext context,
  required WidgetRef ref,
  required String messageId,
}) {
  showDialog<void>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      title: const Text('Delete message'),
      content: const Text('This cannot be undone.'),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () {
            Navigator.of(dialogContext).pop();
            ref.read(channelActionsProvider).deleteMessage(messageId);
          },
          style: FilledButton.styleFrom(
            backgroundColor: Theme.of(dialogContext).colorScheme.error,
          ),
          child: const Text('Delete'),
        ),
      ],
    ),
  );
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

class _DmAppBarTitle extends ConsumerWidget {
  final Channel channel;
  final String? currentPubkey;

  const _DmAppBarTitle({required this.channel, required this.currentPubkey});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
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
    final presenceLabel = switch (presence) {
      'online' => 'Online',
      'away' => 'Away',
      _ => 'Offline',
    };

    return Row(
      children: [
        SizedBox(
          width: 30,
          height: 30,
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              CircleAvatar(
                radius: 14,
                backgroundColor: context.colors.primaryContainer,
                backgroundImage: avatarUrl != null
                    ? NetworkImage(avatarUrl)
                    : null,
                child: avatarUrl == null
                    ? Text(
                        initial,
                        style: context.textTheme.labelSmall?.copyWith(
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
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(
                    color: switch (presence) {
                      'online' => context.appColors.success,
                      'away' => context.appColors.warning,
                      _ => context.colors.outline,
                    },
                    shape: BoxShape.circle,
                    border: Border.all(
                      color:
                          context.theme.appBarTheme.backgroundColor ??
                          context.theme.scaffoldBackgroundColor,
                      width: 1.5,
                    ),
                  ),
                ),
              ),
            ],
          ),
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
                style: context.textTheme.titleSmall,
              ),
              Text(
                presenceLabel,
                style: context.textTheme.labelSmall?.copyWith(
                  color: context.colors.outline,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
