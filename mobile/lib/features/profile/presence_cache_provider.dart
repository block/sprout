import 'dart:async';
import 'dart:math';

import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';

/// In-memory cache of other users' presence, fetched in batches.
/// Periodically refreshes to keep presence status up to date.
///
/// Tracked set is capped at [_maxTracked] entries; oldest entries are evicted
/// when the cap is exceeded. Fetch failures trigger exponential backoff up to
/// [_maxBackoff].
class PresenceCacheNotifier extends Notifier<Map<String, String>> {
  static const _refreshInterval = Duration(seconds: 30);
  static const _maxBackoff = Duration(minutes: 5);
  static const _maxTracked = 200;

  // List-backed so insertion order is preserved for eviction.
  final List<String> _tracked = [];
  final Set<String> _trackedSet = {};
  final Set<String> _pending = {};
  Timer? _batchTimer;
  Timer? _refreshTimer;
  int _consecutiveFailures = 0;

  @override
  Map<String, String> build() {
    ref.watch(relayClientProvider);
    _tracked.clear();
    _trackedSet.clear();
    _consecutiveFailures = 0;
    ref.onDispose(() {
      _batchTimer?.cancel();
      _batchTimer = null;
      _refreshTimer?.cancel();
      _refreshTimer = null;
    });
    return {};
  }

  /// Track presence for [pubkeys]. Fetches immediately if not cached,
  /// and includes them in periodic refreshes.
  void track(List<String> pubkeys) {
    final normalized = pubkeys.map((pk) => pk.toLowerCase()).toList();
    final uncached = normalized
        .where((pk) => !state.containsKey(pk) && !_pending.contains(pk))
        .toList();

    for (final pk in normalized) {
      if (!_trackedSet.contains(pk)) {
        _tracked.add(pk);
        _trackedSet.add(pk);
      }
    }

    // Evict oldest entries if over cap.
    while (_tracked.length > _maxTracked) {
      final evicted = _tracked.removeAt(0);
      _trackedSet.remove(evicted);
    }

    _ensureRefreshTimer();

    if (uncached.isEmpty) return;
    _pending.addAll(uncached);
    _batchTimer ??= Timer(const Duration(milliseconds: 50), _flushPending);
  }

  /// Stop tracking presence for [pubkeys]. Cancels the refresh timer if
  /// [_tracked] becomes empty.
  void untrack(List<String> pubkeys) {
    final normalized = pubkeys.map((pk) => pk.toLowerCase()).toList();
    for (final pk in normalized) {
      if (_trackedSet.remove(pk)) {
        _tracked.remove(pk);
      }
    }
    if (_tracked.isEmpty) {
      _refreshTimer?.cancel();
      _refreshTimer = null;
    }
  }

  void _ensureRefreshTimer() {
    if (_refreshTimer != null) return;
    _scheduleRefresh();
  }

  void _scheduleRefresh() {
    _refreshTimer?.cancel();
    final delay = _consecutiveFailures == 0
        ? _refreshInterval
        : _clampedBackoff(_consecutiveFailures);
    _refreshTimer = Timer(delay, () async {
      _refreshTimer = null;
      await _refreshAll();
      if (_tracked.isNotEmpty) _scheduleRefresh();
    });
  }

  Duration _clampedBackoff(int failures) {
    final seconds = _refreshInterval.inSeconds * pow(2, failures - 1);
    return Duration(seconds: min(seconds.toInt(), _maxBackoff.inSeconds));
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
      _consecutiveFailures = 0;
    } catch (_) {
      // Silently fail — default to offline.
      _consecutiveFailures++;
    }
  }
}

final presenceCacheProvider =
    NotifierProvider<PresenceCacheNotifier, Map<String, String>>(
      PresenceCacheNotifier.new,
    );
