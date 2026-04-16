import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'feed_item.dart';

class ActivityNotifier extends AsyncNotifier<HomeFeedResponse> {
  @override
  Future<HomeFeedResponse> build() {
    ref.watch(relayClientProvider);
    // Re-fetch when websocket reconnects so feed stays fresh.
    ref.watch(relaySessionProvider);
    return _fetch();
  }

  Future<HomeFeedResponse> _fetch() async {
    final client = ref.read(relayClientProvider);
    final json =
        await client.get(
              '/api/feed',
              queryParams: {
                'limit': '20',
                'types': 'mentions,needs_action,activity,agent_activity',
              },
            )
            as Map<String, dynamic>;
    return HomeFeedResponse.fromJson(json);
  }

  Future<void> refresh() async {
    state = await AsyncValue.guard(_fetch);
  }
}

final activityProvider =
    AsyncNotifierProvider<ActivityNotifier, HomeFeedResponse>(
      ActivityNotifier.new,
    );
