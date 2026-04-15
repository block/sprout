import 'dart:io';

import 'package:flutter/material.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/relay.dart';
import '../../shared/theme/theme.dart';
import '../profile/profile_avatar.dart';
import 'channel.dart';
import 'channel_detail_page.dart';
import 'channels_provider.dart';

class ChannelsPage extends ConsumerWidget {
  const ChannelsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final channelsAsync = ref.watch(channelsProvider);
    final sessionState = ref.watch(relaySessionProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Sprout'),
        actions: const [ProfileAvatar()],
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
              data: (channels) => _ChannelsList(channels: channels),
            ),
          ),
        ],
      ),
    );
  }
}

class _ChannelsList extends ConsumerWidget {
  final List<Channel> channels;

  const _ChannelsList({required this.channels});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return RefreshIndicator(
      onRefresh: () => ref.read(channelsProvider.notifier).refresh(),
      child: channels.isEmpty
          ? ListView(
              children: [
                SizedBox(
                  height: MediaQuery.sizeOf(context).height * 0.6,
                  child: Center(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          LucideIcons.hash,
                          size: Grid.xl,
                          color: context.colors.outline,
                        ),
                        const SizedBox(height: Grid.xs),
                        Text(
                          'No channels yet',
                          style: context.textTheme.bodyLarge?.copyWith(
                            color: context.colors.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            )
          : ListView.builder(
              padding: const EdgeInsets.only(top: Grid.xxs),
              itemCount: channels.length + 1, // +1 for section header
              itemBuilder: (context, index) {
                if (index == 0) {
                  return _SectionHeader(
                    label: 'Channels',
                    count: channels.length,
                  );
                }
                return _ChannelTile(channel: channels[index - 1]);
              },
            ),
    );
  }
}

/// Slack-style section header: "Channels  12"
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

/// Compact Slack-style channel row:
///   # channel-name                      3m
class _ChannelTile extends StatelessWidget {
  final Channel channel;

  const _ChannelTile({required this.channel});

  @override
  Widget build(BuildContext context) {
    final hasActivity = channel.lastMessageAt != null;

    return InkWell(
      borderRadius: BorderRadius.circular(Radii.md),
      onTap: () {
        Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (_) => ChannelDetailPage(channel: channel),
          ),
        );
      },
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Grid.xs,
          vertical: Grid.xxs + Grid.quarter,
        ),
        child: Row(
          children: [
            // Channel type icon
            Icon(
              _iconFor(channel),
              size: 18,
              color: hasActivity
                  ? context.colors.onSurface
                  : context.colors.outline,
            ),
            const SizedBox(width: Grid.xxs),

            // Channel name
            Expanded(
              child: Text(
                channel.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: context.textTheme.bodyMedium?.copyWith(
                  color: hasActivity
                      ? context.colors.onSurface
                      : context.colors.onSurfaceVariant,
                ),
              ),
            ),

            // Ephemeral badge
            if (channel.isEphemeral) ...[
              const SizedBox(width: Grid.xxs),
              _EphemeralBadge(channel: channel),
            ],

            // Relative timestamp
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

  IconData _iconFor(Channel channel) {
    if (channel.isPrivate) return LucideIcons.lock;
    if (channel.isForum) return LucideIcons.messageSquare;
    return LucideIcons.hash;
  }

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

/// Small amber clock badge matching the desktop's EphemeralChannelBadge.
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
        ? const Color(0xFFB45309) // amber-700
        : const Color(0xFFFCD34D); // amber-300
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

/// Slim banner shown when the websocket is reconnecting or disconnected.
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
