import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../shared/relay/relay_client.dart';
import '../../shared/theme/theme.dart';
import '../profile/profile_avatar.dart';
import 'channel.dart';
import 'channels_provider.dart';

class ChannelsPage extends HookConsumerWidget {
  const ChannelsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final channelsAsync = ref.watch(channelsProvider);

    // Poll every 30s while this page is mounted, matching desktop's pattern.
    useEffect(() {
      final timer = Timer.periodic(
        const Duration(seconds: 30),
        (_) => ref.read(channelsProvider.notifier).refresh(),
      );
      return timer.cancel;
    }, const []);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Channels'),
        actions: const [ProfileAvatar()],
      ),
      body: channelsAsync.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, _) => _ErrorView(
          error: error,
          onRetry: () => ref.read(channelsProvider.notifier).refresh(),
        ),
        data: (channels) => _ChannelsList(channels: channels),
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
          : ListView.separated(
              padding: const EdgeInsets.symmetric(vertical: Grid.xxs),
              itemCount: channels.length,
              separatorBuilder: (_, _) =>
                  const Divider(height: 1, indent: Grid.xl),
              itemBuilder: (context, index) =>
                  _ChannelTile(channel: channels[index]),
            ),
    );
  }
}

class _ChannelTile extends StatelessWidget {
  final Channel channel;

  const _ChannelTile({required this.channel});

  @override
  Widget build(BuildContext context) {
    return ListTile(
      leading: Icon(
        _iconFor(channel),
        color: channel.isMember
            ? context.colors.primary
            : context.colors.outline,
      ),
      title: Text(channel.name, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: channel.description.isNotEmpty
          ? Text(
              channel.description,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: context.colors.onSurfaceVariant),
            )
          : null,
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (channel.isMember) ...[
            Container(
              padding: const EdgeInsets.symmetric(
                horizontal: Grid.xxs,
                vertical: Grid.quarter,
              ),
              decoration: BoxDecoration(
                color: context.colors.primaryContainer,
                borderRadius: BorderRadius.circular(Grid.half),
              ),
              child: Text(
                'Joined',
                style: context.textTheme.labelSmall?.copyWith(
                  color: context.colors.onPrimaryContainer,
                ),
              ),
            ),
            const SizedBox(width: Grid.xxs),
          ],
          Text(
            '${channel.memberCount}',
            style: context.textTheme.bodySmall?.copyWith(
              color: context.colors.outline,
            ),
          ),
          const SizedBox(width: Grid.quarter),
          Icon(LucideIcons.users, size: 14, color: context.colors.outline),
        ],
      ),
    );
  }

  IconData _iconFor(Channel channel) {
    if (channel.isPrivate) return LucideIcons.lock;
    if (channel.isForum) return LucideIcons.messageSquare;
    return LucideIcons.hash;
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
