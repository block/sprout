import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';

/// In-memory cache of other users' presence, fetched in batches.
///
/// Subscribes to kind:20001 presence events over WebSocket for real-time
/// updates. Falls back to a 60-second REST poll as a backstop for REST-only
/// writers (ACP agents) and TTL expiry (crashed clients — Redis expires after
/// 90s, no WS event emitted).
class PresenceCacheNotifier extends Notifier<Map<String, String>> {
  // Backstop poll: catches REST-only writers and TTL expiry.
  // WS events handle the fast path. Matches desktop's 60s interval.
  static const _refreshInterval = Duration(seconds: 60);

  final Set<String> _tracked = {};
  final Set<String> _pending = {};
  Timer? _batchTimer;
  Timer? _refreshTimer;
  void Function()? _presenceUnsub;

  @override
  Map<String, String> build() {
    ref.watch(relayClientProvider);
    final sessionState = ref.watch(relaySessionProvider);

    ref.onDispose(() {
      _batchTimer?.cancel();
      _batchTimer = null;
      _refreshTimer?.cancel();
      _refreshTimer = null;
      _presenceUnsub?.call();
      _presenceUnsub = null;
    });

    if (sessionState.status == SessionStatus.connected) {
      _subscribePresenceUpdates();
    }

    return {};
  }

  /// Track presence for [pubkeys]. Fetches immediately if not cached,
  /// and includes them in periodic refreshes.
  void track(List<String> pubkeys) {
    final normalized = pubkeys.map((pk) => pk.toLowerCase()).toList();
    final uncached = normalized
        .where((pk) => !state.containsKey(pk) && !_pending.contains(pk))
        .toList();

    _tracked.addAll(normalized);
    _ensureRefreshTimer();

    if (uncached.isEmpty) return;
    _pending.addAll(uncached);
    _batchTimer ??= Timer(const Duration(milliseconds: 50), _flushPending);
  }

  void _ensureRefreshTimer() {
    _refreshTimer ??= Timer.periodic(_refreshInterval, (_) => _refreshAll());
  }

  /// Subscribe to kind:20001 presence events over WebSocket.
  ///
  /// On each event, updates the in-memory cache for that pubkey without
  /// triggering a REST refetch. Matches the desktop's
  /// `usePresenceSubscription()` pattern.
  void _subscribePresenceUpdates() async {
    _presenceUnsub?.call();
    _presenceUnsub = null;

    final session = ref.read(relaySessionProvider.notifier);
    try {
      _presenceUnsub = await session.subscribe(
        const NostrFilter(kinds: [EventKind.presenceUpdate], limit: 0),
        _handlePresenceEvent,
      );
    } catch (error) {
      debugPrint(
        '[PresenceCacheNotifier] presence subscription failed: $error',
      );
      // Backstop polling handles this case.
    }
  }

  void _handlePresenceEvent(NostrEvent event) {
    final pubkey = event.pubkey.toLowerCase();
    // Only update pubkeys we're tracking to avoid unbounded cache growth.
    if (!_tracked.contains(pubkey)) return;
    final status = event.content;
    if (status != 'online' && status != 'away' && status != 'offline') return;
    if (state[pubkey] == status) return;
    final updated = Map<String, String>.from(state);
    updated[pubkey] = status;
    state = updated;
  }

  Future<void> _refreshAll() async {
    if (_tracked.isEmpty) return;
    await _fetchPresence(_tracked.toList());
  }

  Future<void> _flushPending() async {
    _batchTimer = null;
    if (_pending.isEmpty) return;

    final pubkeys = _pending.toList();
    _pending.clear();
    await _fetchPresence(pubkeys);
  }

  Future<void> _fetchPresence(List<String> pubkeys) async {
    try {
      final client = ref.read(relayClientProvider);
      final json =
          await client.get(
                '/api/presence',
                queryParams: {'pubkeys': pubkeys.join(',')},
              )
              as Map<String, dynamic>;

      final updated = Map<String, String>.from(state);
      for (final pk in pubkeys) {
        updated[pk] = (json[pk] as String?) ?? 'offline';
      }
      state = updated;
    } catch (_) {
      // Silently fail — default to offline.
    }
  }
}

final presenceCacheProvider =
    NotifierProvider<PresenceCacheNotifier, Map<String, String>>(
      PresenceCacheNotifier.new,
    );
