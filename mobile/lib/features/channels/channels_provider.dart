import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'channel.dart';

class ChannelsNotifier extends AsyncNotifier<List<Channel>> {
  @override
  Future<List<Channel>> build() {
    // Watch relayClientProvider here so we auto-refetch when config changes.
    ref.watch(relayClientProvider);
    return _fetch();
  }

  Future<List<Channel>> _fetch() async {
    final client = ref.read(relayClientProvider);
    final json = await client.get('/api/channels') as List<dynamic>;
    final channels = json
        .cast<Map<String, dynamic>>()
        .map(Channel.fromJson)
        .where((c) => !c.isDm) // exclude DMs from channel list
        .toList();
    channels.sort((a, b) => a.name.compareTo(b.name));
    return channels;
  }

  Future<void> refresh() async {
    state = await AsyncValue.guard(_fetch);
  }
}

final channelsProvider = AsyncNotifierProvider<ChannelsNotifier, List<Channel>>(
  ChannelsNotifier.new,
);
