import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';
import '../profile/user_cache_provider.dart';
import '../profile/user_profile.dart';
import 'channel.dart';
import 'channel_messages_provider.dart';
import 'channel_typing_provider.dart';
import 'send_message_provider.dart';
import 'timeline_message.dart';

/// Fetch channel members and preload their profiles into the user cache.
Future<void> _preloadMembers(WidgetRef ref, String channelId) async {
  try {
    final client = ref.read(relayClientProvider);
    final json =
        await client.get('/api/channels/$channelId/members')
            as Map<String, dynamic>;
    final members = json['members'] as List<dynamic>? ?? [];
    final pubkeys = members
        .map((m) => (m as Map<String, dynamic>)['pubkey'] as String)
        .toList();
    if (pubkeys.isNotEmpty) {
      ref.read(userCacheProvider.notifier).preload(pubkeys);
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
    final messagesState = ref.watch(channelMessagesProvider(channel.id));
    final typingEntries = ref.watch(channelTypingProvider(channel.id));

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
              channel.isPrivate ? LucideIcons.lock : LucideIcons.hash,
              size: 18,
              color: context.colors.onSurfaceVariant,
            ),
            const SizedBox(width: Grid.half),
            Expanded(
              child: Text(channel.name, overflow: TextOverflow.ellipsis),
            ),
          ],
        ),
      ),
      body: Column(
        children: [
          Expanded(
            child: messagesState.when(
              loading: () => const Center(child: CircularProgressIndicator()),
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
                return _MessageList(messages: messages, channelId: channel.id);
              },
            ),
          ),
          if (typingEntries.isNotEmpty)
            _TypingIndicator(entries: typingEntries),
          _ComposeBar(channelId: channel.id),
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

        return _MessageBubble(message: message, showAuthor: showAuthor);
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

  const _MessageBubble({required this.message, required this.showAuthor});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final userCache = ref.watch(userCacheProvider);
    final profile =
        userCache[message.pubkey.toLowerCase()] ??
        ref.read(userCacheProvider.notifier).get(message.pubkey.toLowerCase());
    final displayName = profile?.label ?? _shortPubkey(message.pubkey);

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
                Text(
                  message.content,
                  style: context.textTheme.bodyMedium?.copyWith(
                    color: context.colors.onSurface,
                  ),
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
        controller.clear();
      } finally {
        isSending.value = false;
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
