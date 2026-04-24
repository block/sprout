import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/theme/theme.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';
import 'channel_management_provider.dart';
import 'channel_messages_provider.dart';
import 'channel_typing_provider.dart';
import 'channels_provider.dart';
import 'compose_bar.dart';
import 'message_content.dart';
import 'send_message_provider.dart';
import 'timeline_message.dart';

/// Full-screen thread detail page.
///
/// Shows the thread head message, direct replies, typing indicators scoped to
/// the thread, and a compose bar for replying.
class ThreadDetailPage extends HookConsumerWidget {
  final TimelineMessage threadHead;
  final List<TimelineMessage> allMessages;
  final String channelId;
  final String? currentPubkey;
  final bool isMember;
  final bool isArchived;

  const ThreadDetailPage({
    super.key,
    required this.threadHead,
    required this.allMessages,
    required this.channelId,
    required this.currentPubkey,
    required this.isMember,
    required this.isArchived,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Re-derive replies from live message state so new replies appear.
    final messagesState = ref.watch(channelMessagesProvider(channelId));
    final liveMessages = messagesState.whenData((events) {
      return formatTimeline(events, currentPubkey: currentPubkey);
    });

    final allMsgs = liveMessages.value ?? allMessages;

    // Index all messages by parentId so we can find direct children of any
    // message and compute thread summaries for nested threads.
    final childrenByParent = <String, List<TimelineMessage>>{};
    for (final msg in allMsgs) {
      final pid = msg.parentId;
      if (pid == null) continue;
      childrenByParent.putIfAbsent(pid, () => []).add(msg);
    }

    final replies = childrenByParent[threadHead.id] ?? const [];

    // Thread-scoped typing indicators (exclude self).
    final allTyping = ref.watch(channelTypingProvider(channelId));
    final threadTyping = allTyping
        .where((e) => e.threadHeadId == threadHead.id)
        .where(
          (e) =>
              currentPubkey == null ||
              e.pubkey.toLowerCase() != currentPubkey?.toLowerCase(),
        )
        .toList();

    // Resolve thread head from live data (reactions/edits may have changed).
    final liveHead =
        allMsgs.where((m) => m.id == threadHead.id).firstOrNull ?? threadHead;

    // The root of the entire thread chain. If the current thread head is
    // itself a root message its rootId is null, so fall back to its own id.
    final effectiveRootId = threadHead.rootId ?? threadHead.id;

    // Channel names for message content rendering.
    final channelsAsync = ref.watch(channelsProvider);
    final channelNamesMap = <String, String>{};
    channelsAsync.whenData((channels) {
      for (final ch in channels) {
        channelNamesMap[ch.name.toLowerCase()] = ch.id;
      }
    });

    return Scaffold(
      appBar: AppBar(title: const Text('Thread')),
      body: Column(
        children: [
          Expanded(
            child: ListView.builder(
              padding: const EdgeInsets.symmetric(
                horizontal: Grid.xs,
                vertical: Grid.xxs,
              ),
              itemCount: replies.length + 1, // +1 for thread head
              itemBuilder: (context, index) {
                if (index == 0) {
                  // Thread head.
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _ThreadMessage(
                        message: liveHead,
                        channelNames: channelNamesMap,
                        channelId: channelId,
                        currentPubkey: currentPubkey,
                        showAuthor: true,
                        allMessages: allMsgs,
                        isMember: isMember,
                        isArchived: isArchived,
                      ),
                      Padding(
                        padding: const EdgeInsets.symmetric(vertical: Grid.xxs),
                        child: Row(
                          children: [
                            Text(
                              '${replies.length} ${replies.length == 1 ? 'reply' : 'replies'}',
                              style: context.textTheme.labelMedium?.copyWith(
                                color: context.colors.onSurfaceVariant,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(width: Grid.xxs),
                            Expanded(
                              child: Divider(
                                color: context.colors.outlineVariant,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  );
                }

                final reply = replies[index - 1];
                final prevReply = index > 1 ? replies[index - 2] : null;
                final showAuthor =
                    prevReply == null ||
                    prevReply.pubkey.toLowerCase() !=
                        reply.pubkey.toLowerCase() ||
                    (reply.createdAt - prevReply.createdAt) > 300;

                // Check if this reply itself has children (nested thread).
                final nestedChildren = childrenByParent[reply.id];
                final nestedSummary =
                    nestedChildren != null && nestedChildren.isNotEmpty
                    ? _buildNestedSummary(reply.id, nestedChildren)
                    : null;

                return Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    _ThreadMessage(
                      message: reply,
                      channelNames: channelNamesMap,
                      channelId: channelId,
                      currentPubkey: currentPubkey,
                      showAuthor: showAuthor,
                      allMessages: allMsgs,
                      isMember: isMember,
                      isArchived: isArchived,
                    ),
                    if (nestedSummary != null)
                      _NestedThreadSummaryRow(
                        summary: nestedSummary,
                        replyMessage: reply,
                        allMessages: allMsgs,
                        channelId: channelId,
                        currentPubkey: currentPubkey,
                        isMember: isMember,
                        isArchived: isArchived,
                      ),
                  ],
                );
              },
            ),
          ),
          if (threadTyping.isNotEmpty)
            _ThreadTypingIndicator(entries: threadTyping),
          if (isMember && !isArchived)
            ComposeBar(
              channelId: channelId,
              hintText: 'Reply in thread\u2026',
              threadHeadId: threadHead.id,
              rootId: effectiveRootId,
              onSend:
                  (
                    content,
                    mentionPubkeys, {
                    mediaTags = const <List<String>>[],
                  }) => ref
                      .read(sendMessageProvider)
                      .call(
                        channelId: channelId,
                        content: content,
                        mentionPubkeys: mentionPubkeys,
                        parentEventId: threadHead.id,
                        rootEventId: effectiveRootId,
                        mediaTags: mediaTags,
                      ),
            ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Nested thread summary helpers
// ---------------------------------------------------------------------------

/// Build a lightweight summary for a nested thread (reply that has its own
/// replies). Same logic as the top-level [ThreadSummary] but kept local to
/// avoid coupling.
ThreadSummary _buildNestedSummary(
  String messageId,
  List<TimelineMessage> children,
) {
  final seen = <String>{};
  final participants = <String>[];
  for (var i = children.length - 1; i >= 0 && participants.length < 3; i--) {
    final pk = children[i].pubkey.toLowerCase();
    if (seen.add(pk)) participants.add(pk);
  }
  return ThreadSummary(
    threadHeadId: messageId,
    replyCount: children.length,
    participantPubkeys: participants.reversed.toList(),
  );
}

/// Tappable summary row shown below a reply that itself has replies.
/// Pushes a new [ThreadDetailPage] for the nested thread.
class _NestedThreadSummaryRow extends ConsumerWidget {
  final ThreadSummary summary;
  final TimelineMessage replyMessage;
  final List<TimelineMessage> allMessages;
  final String channelId;
  final String? currentPubkey;
  final bool isMember;
  final bool isArchived;

  const _NestedThreadSummaryRow({
    required this.summary,
    required this.replyMessage,
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
              threadHead: replyMessage,
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
              width:
                  20.0 +
                  (summary.participantPubkeys.length - 1).clamp(0, 2) * 12.0,
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
// Thread message row (reusable for head and replies)
// ---------------------------------------------------------------------------

class _ThreadMessage extends ConsumerWidget {
  final TimelineMessage message;
  final Map<String, String> channelNames;
  final String channelId;
  final String? currentPubkey;
  final bool showAuthor;
  final List<TimelineMessage>? allMessages;
  final bool isMember;
  final bool isArchived;

  const _ThreadMessage({
    required this.message,
    required this.channelNames,
    required this.channelId,
    required this.currentPubkey,
    required this.showAuthor,
    this.allMessages,
    this.isMember = false,
    this.isArchived = false,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pk = message.pubkey.toLowerCase();
    final profile =
        ref.watch(userCacheProvider.select((cache) => cache[pk])) ??
        ref.read(userCacheProvider.notifier).get(pk);
    final displayName = profile?.label ?? _shortPubkey(message.pubkey);

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
      onLongPress: () => _showThreadMessageActions(
        context: context,
        ref: ref,
        message: message,
        channelId: channelId,
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
              _Avatar(profile: profile, pubkey: message.pubkey)
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
                    tags: message.tags,
                    onChannelTap: (_) {},
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

// ---------------------------------------------------------------------------
// Thread compose bar
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Thread-scoped typing indicator
// ---------------------------------------------------------------------------

class _ThreadTypingIndicator extends ConsumerWidget {
  final List<TypingEntry> entries;

  const _ThreadTypingIndicator({required this.entries});

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
      1 => '${names[0]} is typing...',
      2 => '${names[0]} and ${names[1]} are typing...',
      _ => '${names[0]} and ${names.length - 1} others are typing...',
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
// Thread message actions (long-press sheet — reactions, copy, edit, delete)
// ---------------------------------------------------------------------------

const _quickEmojis = [
  '\u{1F44D}',
  '\u{2764}\u{FE0F}',
  '\u{1F602}',
  '\u{1F389}',
  '\u{1F440}',
  '\u{1F64F}',
];

void _showThreadMessageActions({
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
                Clipboard.setData(ClipboardData(text: message.content));
              },
            ),
            if (isOwnMessage) ...[
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
                            ref
                                .read(channelActionsProvider)
                                .deleteMessage(message.id);
                          },
                          style: FilledButton.styleFrom(
                            backgroundColor: Theme.of(
                              dialogContext,
                            ).colorScheme.error,
                          ),
                          child: const Text('Delete'),
                        ),
                      ],
                    ),
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

// ---------------------------------------------------------------------------
// Shared helpers (duplicated here to keep the file self-contained)
// ---------------------------------------------------------------------------

class _Avatar extends StatelessWidget {
  final UserProfile? profile;
  final String pubkey;

  const _Avatar({required this.profile, required this.pubkey});

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

String _shortPubkey(String pubkey) {
  if (pubkey.length > 12) return '${pubkey.substring(0, 8)}...';
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
