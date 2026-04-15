import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'user_profile.dart';

class ProfileNotifier extends AsyncNotifier<UserProfile?> {
  @override
  Future<UserProfile?> build() {
    ref.watch(relayClientProvider);
    return _fetch();
  }

  Future<UserProfile?> _fetch() async {
    final client = ref.read(relayClientProvider);
    try {
      final json =
          await client.get('/api/users/me/profile') as Map<String, dynamic>;
      return UserProfile.fromJson(json);
    } on RelayException catch (e) {
      // 404 means user has no profile yet — not an error.
      if (e.statusCode == 404) return null;
      rethrow;
    }
  }

  Future<void> refresh() async {
    state = await AsyncValue.guard(_fetch);
  }
}

final profileProvider = AsyncNotifierProvider<ProfileNotifier, UserProfile?>(
  ProfileNotifier.new,
);

/// Presence status for the current user.
class PresenceNotifier extends AsyncNotifier<String> {
  @override
  Future<String> build() {
    ref.watch(relayClientProvider);
    ref.watch(profileProvider);
    return _fetch();
  }

  Future<String> _fetch() async {
    final profile = ref.read(profileProvider).whenData((v) => v).value;
    if (profile == null) return 'offline';
    final client = ref.read(relayClientProvider);
    final json =
        await client.get(
              '/api/presence',
              queryParams: {'pubkeys': profile.pubkey},
            )
            as Map<String, dynamic>;
    return (json[profile.pubkey] as String?) ?? 'offline';
  }

  Future<void> refresh() async {
    state = await AsyncValue.guard(_fetch);
  }
}

final presenceProvider = AsyncNotifierProvider<PresenceNotifier, String>(
  PresenceNotifier.new,
);
