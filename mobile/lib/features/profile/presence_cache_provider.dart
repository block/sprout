import 'dart:async';

import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';

/// In-memory cache of other users' presence, fetched in batches.
/// Periodically refreshes to keep presence status up to date.
class PresenceCacheNotifier extends Notifier<Map<String, String>> {
  static const _refreshInterval = Duration(seconds: 30);

  final Set<String> _tracked = {};
  final Set<String> _pending = {};
  Timer? _batchTimer;
  Timer? _refreshTimer;

  @override
  Map<String, String> build() {
    ref.watch(relayClientProvider);
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

    _tracked.addAll(normalized);
    _ensureRefreshTimer();

    if (uncached.isEmpty) return;
    _pending.addAll(uncached);
    _batchTimer ??= Timer(const Duration(milliseconds: 50), _flushPending);
  }

  void _ensureRefreshTimer() {
    _refreshTimer ??= Timer.periodic(_refreshInterval, (_) => _refreshAll());
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
