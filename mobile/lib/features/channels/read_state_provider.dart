import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';
import 'package:flutter/widgets.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:nostr/nostr.dart' as nostr;
import 'package:shared_preferences/shared_preferences.dart';

import '../../shared/crypto/nip44.dart';
import '../../shared/relay/app_lifecycle_provider.dart';
import '../../shared/relay/nostr_models.dart';
import '../../shared/relay/relay_provider.dart';
import '../../shared/relay/relay_session.dart';
import '../../shared/theme/theme_provider.dart';
import 'read_state_sync.dart';

// Re-export so existing importers of ReadSyncState keep working.
export 'read_state_sync.dart' show ReadSyncState;

// ── Notifier ─────────────────────────────────────────────────────────────────

class ReadStateSyncNotifier extends Notifier<ReadSyncState> {
  // Mutable engine state (not in the immutable state object).
  String _clientId = '';
  String _slotId = '';
  Map<String, int> _ownBlob = {};
  Map<String, int> _lastPublished = {};
  int _maxFetchedCreatedAt = 0;
  Timer? _debounceTimer;
  Timer? _republishTimer;
  bool _isPublishing = false;
  bool _needsRepublish = false;
  bool _disposed = false;
  void Function()? _unsubscribe;
  Uint8List? _conversationKey;
  String? _privkeyHex;
  String? _pubkeyHex;

  SharedPreferences get _prefs => ref.read(savedPrefsProvider);

  @override
  ReadSyncState build() {
    // Watch relay config so notifier rebuilds on workspace switch,
    // clearing stale keys/crypto state per identity.
    ref.watch(relayConfigProvider);

    // Watch session status so we re-initialize (re-subscribe + catch-up
    // fetch) after websocket reconnect (phone wake, network restore).
    final sessionState = ref.watch(relaySessionProvider);

    // Flush on background, re-initialize on foreground resume.
    ref.listen(appLifecycleProvider, (prev, next) {
      if (next == AppLifecycleState.paused ||
          next == AppLifecycleState.detached) {
        flushNow();
      }
    });

    ref.onDispose(_cleanup);

    // Read cached state synchronously for first paint.
    // Full initialization happens asynchronously when connected.
    if (sessionState.status == SessionStatus.connected) {
      Future.microtask(_initialize);
    }

    return const ReadSyncState();
  }

  Future<void> _initialize() async {
    _disposed = false;

    // Get keypair from relay config.
    final config = ref.read(relayConfigProvider);
    final nsec = config.nsec;
    if (nsec == null || nsec.isEmpty) {
      state = state.copyWith(isInitialized: true);
      return;
    }

    _privkeyHex = nostr.Nip19.decodePrivkey(nsec);
    if (_privkeyHex == null || _privkeyHex!.isEmpty) {
      state = state.copyWith(isInitialized: true);
      return;
    }
    _pubkeyHex = nostr.Keychain(_privkeyHex!).public;
    _conversationKey = getConversationKey(_privkeyHex!, _pubkeyHex!);

    // Read sync enabled preference (scoped by pubkey).
    final syncEnabled = _prefs.getString(syncEnabledKey(_pubkeyHex!)) == 'true';

    // Read cached read state from disk (scoped by pubkey).
    final cached = _readCachedState();

    state = state.copyWith(mergedState: cached, syncEnabled: syncEnabled);

    // Get or create persistent IDs (scoped by pubkey).
    _clientId = _getOrCreateStorageValue(clientIdKey(_pubkeyHex!));
    _slotId = _getOrCreateStorageValue(slotIdKey(_pubkeyHex!));

    // Fetch 7-day horizon of read-state events.
    final since = DateTime.now().millisecondsSinceEpoch ~/ 1000 - 7 * 86400;
    final session = ref.read(relaySessionProvider.notifier);

    List<NostrEvent> events;
    try {
      events = await session.fetchReadStateEvents(_pubkeyHex!, since: since);
    } catch (_) {
      state = state.copyWith(isInitialized: true);
      return;
    }

    if (_disposed) return;

    // Decode all blobs.
    final decoded = <DecodedBlob>[];
    for (final event in events) {
      final result = decryptAndValidateBlob(event, _conversationKey!);
      if (result != null) decoded.add(result);
    }

    if (_disposed) return;

    // Find our own blobs (matching client_id).
    final ownBlobs = decoded.where((d) => d.clientId == _clientId).toList();

    // Handle duplicate client_id: keep highest created_at, delete rest.
    if (ownBlobs.length > 1) {
      ownBlobs.sort((a, b) => b.event.createdAt.compareTo(a.event.createdAt));
      for (var i = 1; i < ownBlobs.length; i++) {
        final stale = ownBlobs[i];
        final staleD = validateDTag(stale.event);
        if (staleD != null) {
          try {
            _publishDeleteEvent(staleD);
          } catch (_) {
            // Best-effort cleanup.
          }
        }
      }
    }

    // Slot-ID conflict detection: another client owns our slot.
    final ownDTag = 'read-state:$_slotId';
    final conflict = decoded.any((d) {
      final dTag = validateDTag(d.event);
      return dTag == ownDTag && d.clientId != _clientId;
    });
    if (conflict) {
      _slotId = randomHex(32);
      _prefs.setString(slotIdKey(_pubkeyHex!), _slotId);
    }

    // Track max created_at for clock-skew protection.
    for (final d in decoded) {
      _maxFetchedCreatedAt = max(_maxFetchedCreatedAt, d.event.createdAt);
    }

    // Set own blob from primary (highest created_at).
    if (ownBlobs.isNotEmpty) {
      _ownBlob = Map.from(ownBlobs.first.contexts);
      _lastPublished = Map.from(ownBlobs.first.contexts);
    }

    // Merge ALL blobs (ours + other clients) + local cache.
    var effective = <String, int>{};
    for (final d in decoded) {
      effective = mergeContexts(effective, d.contexts);
    }
    effective = mergeContexts(effective, cached);
    effective = mergeContexts(effective, _ownBlob);

    if (!_disposed) {
      // Functional merge with any marks that arrived during init.
      state = state.copyWith(
        mergedState: mergeContexts(state.mergedState, effective),
        isInitialized: true,
      );
      _writeCachedState(state.mergedState);
    }

    // Subscribe to live read-state events.
    if (_disposed) return;
    try {
      _unsubscribe = await session.subscribeToReadState(
        _pubkeyHex!,
        _handleIncomingEvent,
      );
    } catch (_) {
      // Non-fatal: sync still works via fetch.
    }
  }

  void _handleIncomingEvent(NostrEvent event) {
    if (_disposed || _conversationKey == null) return;

    final decoded = decryptAndValidateBlob(event, _conversationKey!);
    if (decoded == null) return;

    _maxFetchedCreatedAt = max(_maxFetchedCreatedAt, event.createdAt);

    if (decoded.clientId == _clientId) {
      // Own blob echoed back from relay.
      _ownBlob = mergeContexts(_ownBlob, decoded.contexts);
      state = state.copyWith(
        mergedState: mergeContexts(state.mergedState, decoded.contexts),
      );
      _writeCachedState(state.mergedState);
      return;
    }

    // Another client's blob — merge into state.
    final merged = mergeContexts(state.mergedState, decoded.contexts);
    state = state.copyWith(mergedState: merged);
    _writeCachedState(merged);

    // Check if we need to re-publish our blob (another client advanced us).
    var needsRepublish = false;
    for (final entry in decoded.contexts.entries) {
      if (entry.value > (_lastPublished[entry.key] ?? 0)) {
        needsRepublish = true;
        break;
      }
    }

    if (needsRepublish && state.syncEnabled) {
      _republishTimer?.cancel();
      _republishTimer = Timer(const Duration(seconds: 5), _publishOwnBlob);
    }
  }

  /// Mark a context (channel ID) as read at the given unix timestamp (seconds).
  void markContextRead(String contextId, int timestamp) {
    final existing = state.mergedState[contextId] ?? 0;
    if (timestamp <= existing) return;

    final newMerged = Map<String, int>.from(state.mergedState);
    newMerged[contextId] = timestamp;
    state = state.copyWith(mergedState: newMerged);
    _writeCachedState(newMerged);

    // Update own blob.
    final existingOwn = _ownBlob[contextId] ?? 0;
    if (timestamp > existingOwn) {
      _ownBlob = Map<String, int>.from(_ownBlob)..[contextId] = timestamp;
    }

    // Debounced publish.
    _debounceTimer?.cancel();
    _debounceTimer = Timer(const Duration(seconds: 5), _publishOwnBlob);
  }

  /// Flush pending state immediately (call on app background).
  Future<void> flushNow() async {
    _debounceTimer?.cancel();
    _debounceTimer = null;
    if (!contextsEqual(_ownBlob, _lastPublished)) {
      await _publishOwnBlob();
    }
  }

  /// Seed channels that have no read state yet. Call once after channels
  /// load to prevent everything showing as unread on first install.
  void seedChannelsAsRead(
    List<({String id, DateTime? lastMessageAt})> channels,
  ) {
    var changed = false;
    final newMerged = Map<String, int>.from(state.mergedState);
    for (final ch in channels) {
      if (ch.lastMessageAt == null) continue;
      if (newMerged.containsKey(ch.id)) continue;
      final ts = ch.lastMessageAt!.millisecondsSinceEpoch ~/ 1000;
      newMerged[ch.id] = ts;
      _ownBlob = Map<String, int>.from(_ownBlob)..[ch.id] = ts;
      changed = true;
    }
    if (changed) {
      state = state.copyWith(mergedState: newMerged);
      _writeCachedState(newMerged);
    }
  }

  /// Toggle sync on/off.
  void setSyncEnabled(bool enabled) {
    if (_pubkeyHex != null) {
      _prefs.setString(syncEnabledKey(_pubkeyHex!), enabled.toString());
    }
    state = state.copyWith(syncEnabled: enabled);
  }

  Future<void> _publishOwnBlob({int retryDepth = 0}) async {
    if (_pubkeyHex == null || !state.syncEnabled || !state.isInitialized) {
      return;
    }

    if (_isPublishing) {
      _needsRepublish = true;
      return;
    }
    _isPublishing = true;

    try {
      final session = ref.read(relaySessionProvider.notifier);
      final events = await session.fetchOwnReadStateBlob(_pubkeyHex!, _slotId);

      var fetchedOwnContexts = <String, int>{};
      var localMaxCreatedAt = _maxFetchedCreatedAt;

      for (final event in events) {
        localMaxCreatedAt = max(localMaxCreatedAt, event.createdAt);
        final decoded = decryptAndValidateBlob(event, _conversationKey!);
        if (decoded == null) continue;

        // Slot-ID conflict: another client claimed our slot.
        if (decoded.clientId != _clientId) {
          _slotId = randomHex(32);
          _prefs.setString(slotIdKey(_pubkeyHex!), _slotId);
          if (retryDepth >= 3) return;
          _isPublishing = false;
          return _publishOwnBlob(retryDepth: retryDepth + 1);
        }

        fetchedOwnContexts = mergeContexts(
          fetchedOwnContexts,
          decoded.contexts,
        );
      }

      _maxFetchedCreatedAt = localMaxCreatedAt;

      final merged = mergeContexts(fetchedOwnContexts, _ownBlob);
      _ownBlob = merged;

      if (contextsEqual(merged, _lastPublished)) return;

      final payload = jsonEncode({
        'v': 1,
        'client_id': _clientId,
        'contexts': merged,
      });

      final encrypted = nip44Encrypt(_conversationKey!, payload);

      // Clock skew: created_at = max(now, maxFetched + 1).
      final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      final createdAt = max(now, _maxFetchedCreatedAt + 1);

      final signedEvent = nostr.Event.from(
        kind: EventKind.readState,
        content: encrypted,
        tags: [
          ['d', 'read-state:$_slotId'],
          ['t', 'read-state'],
        ],
        privkey: _privkeyHex!,
        createdAt: createdAt,
      );

      // Convert nostr.Event to our NostrEvent for the session publish API.
      final nostrEvent = NostrEvent.fromJson(signedEvent.toJson());

      await session.publish(nostrEvent);
      _lastPublished = Map.from(merged);
    } catch (_) {
      // Log error — non-fatal.
    } finally {
      _isPublishing = false;
      if (_needsRepublish) {
        _needsRepublish = false;
        _publishOwnBlob();
      }
    }
  }

  void _publishDeleteEvent(String dTag) {
    if (_privkeyHex == null || _pubkeyHex == null) return;
    final signedEvent = nostr.Event.from(
      kind: 5,
      content: '',
      tags: [
        ['a', '${EventKind.readState}:$_pubkeyHex:$dTag'],
      ],
      privkey: _privkeyHex!,
    );
    try {
      final nostrEvent = NostrEvent.fromJson(signedEvent.toJson());
      final session = ref.read(relaySessionProvider.notifier);
      session.publish(nostrEvent);
    } catch (_) {
      // Best-effort.
    }
  }

  String _getOrCreateStorageValue(String key) {
    final existing = _prefs.getString(key);
    if (existing != null && existing.isNotEmpty) return existing;
    final value = randomHex(32);
    _prefs.setString(key, value);
    return value;
  }

  Map<String, int> _readCachedState() {
    if (_pubkeyHex == null) return {};
    return parseCachedReadState(_prefs.getString(cacheKey(_pubkeyHex!)));
  }

  void _writeCachedState(Map<String, int> s) {
    if (_pubkeyHex == null) return;
    _prefs.setString(cacheKey(_pubkeyHex!), jsonEncode(s));
  }

  void _cleanup() {
    _disposed = true;
    _debounceTimer?.cancel();
    _debounceTimer = null;
    _republishTimer?.cancel();
    _republishTimer = null;
    _unsubscribe?.call();
    _unsubscribe = null;
    _ownBlob = {};
    _lastPublished = {};
    _maxFetchedCreatedAt = 0;
    _isPublishing = false;
    _needsRepublish = false;
  }
}

final readStateSyncProvider =
    NotifierProvider<ReadStateSyncNotifier, ReadSyncState>(
      ReadStateSyncNotifier.new,
    );
