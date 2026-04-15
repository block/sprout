import 'dart:async';

import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import 'user_profile.dart';

/// In-memory cache of user profiles, fetched in batches from the relay.
class UserCacheNotifier extends Notifier<Map<String, UserProfile>> {
  final Set<String> _pending = {};
  Timer? _batchTimer;

  @override
  Map<String, UserProfile> build() {
    ref.watch(relayClientProvider);
    ref.onDispose(() {
      _batchTimer?.cancel();
      _batchTimer = null;
    });
    return {};
  }

  /// Request a profile for [pubkey]. Returns immediately from cache if
  /// available, otherwise schedules a batch fetch.
  UserProfile? get(String pubkey) {
    final cached = state[pubkey];
    if (cached != null) return cached;
    _scheduleFetch(pubkey);
    return null;
  }

  void _scheduleFetch(String pubkey) {
    if (_pending.contains(pubkey)) return;
    _pending.add(pubkey);

    // Batch: wait 50ms to collect multiple lookups into one request.
    _batchTimer ??= Timer(const Duration(milliseconds: 50), _flushPending);
  }

  Future<void> _flushPending() async {
    _batchTimer = null;
    if (_pending.isEmpty) return;

    final pubkeys = _pending.toList();
    _pending.clear();

    try {
      final client = ref.read(relayClientProvider);
      final json =
          await client.post('/api/users/batch', body: {'pubkeys': pubkeys})
              as Map<String, dynamic>;

      final profiles = json['profiles'] as Map<String, dynamic>? ?? {};
      final updated = Map<String, UserProfile>.from(state);

      for (final entry in profiles.entries) {
        final data = entry.value as Map<String, dynamic>;
        updated[entry.key.toLowerCase()] = UserProfile(
          pubkey: entry.key.toLowerCase(),
          displayName: data['display_name'] as String?,
          avatarUrl: data['avatar_url'] as String?,
        );
      }

      state = updated;
    } catch (_) {
      // Silently fail — we'll just show pubkeys.
    }
  }
}

final userCacheProvider =
    NotifierProvider<UserCacheNotifier, Map<String, UserProfile>>(
      UserCacheNotifier.new,
    );
