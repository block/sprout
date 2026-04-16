import 'dart:async';

import 'package:flutter/widgets.dart';
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
///
/// Sends a heartbeat every 60s while the app is active. Watches
/// [appLifecycleProvider] to send "away" when backgrounded.
class PresenceNotifier extends AsyncNotifier<String> {
  static const _heartbeatInterval = Duration(seconds: 60);

  Timer? _heartbeatTimer;

  @override
  Future<String> build() {
    ref.watch(relayClientProvider);
    ref.watch(profileProvider);

    final lifecycle = ref.watch(appLifecycleProvider);

    ref.onDispose(() {
      _heartbeatTimer?.cancel();
      _heartbeatTimer = null;
    });

    if (lifecycle == AppLifecycleState.resumed) {
      _startHeartbeat();
      return _setPresence('online');
    } else if (lifecycle == AppLifecycleState.paused ||
        lifecycle == AppLifecycleState.detached) {
      _heartbeatTimer?.cancel();
      _heartbeatTimer = null;
      return _setPresence('away');
    }

    return _fetch();
  }

  void _startHeartbeat() {
    _heartbeatTimer?.cancel();
    _heartbeatTimer = Timer.periodic(_heartbeatInterval, (_) {
      _setPresence('online');
    });
  }

  Future<String> _setPresence(String status) async {
    final client = ref.read(relayClientProvider);
    try {
      await client.post('/api/presence', body: {'status': status});
    } catch (_) {
      // Optimistically report the requested status even if the POST fails —
      // the heartbeat will retry on the next tick.
    }
    return status;
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
