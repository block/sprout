import 'dart:convert';
import 'dart:math';

import 'package:flutter/foundation.dart';

import '../../shared/crypto/nip44.dart';
import '../../shared/relay/nostr_models.dart';

// ── Storage keys (scoped by pubkey for multi-workspace isolation) ────────────

String clientIdKey(String pubkey) => 'sprout.read-state-client-id.v1.$pubkey';
String slotIdKey(String pubkey) => 'sprout.read-state-slot-id.v1.$pubkey';
String syncEnabledKey(String pubkey) =>
    'sprout.read-state-sync-enabled.v1.$pubkey';
String cacheKey(String pubkey) => 'sprout.channel-read-state.v1.$pubkey';

// ── State ────────────────────────────────────────────────────────────────────

@immutable
class ReadSyncState {
  final Map<String, int> mergedState;
  final bool syncEnabled;
  final bool isInitialized;

  const ReadSyncState({
    this.mergedState = const {},
    this.syncEnabled = false,
    this.isInitialized = false,
  });

  ReadSyncState copyWith({
    Map<String, int>? mergedState,
    bool? syncEnabled,
    bool? isInitialized,
  }) => ReadSyncState(
    mergedState: mergedState ?? this.mergedState,
    syncEnabled: syncEnabled ?? this.syncEnabled,
    isInitialized: isInitialized ?? this.isInitialized,
  );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

String randomHex(int length) {
  final rng = Random.secure();
  final bytes = List<int>.generate(length ~/ 2, (_) => rng.nextInt(256));
  return bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
}

Map<String, int> mergeContexts(
  Map<String, int> base,
  Map<String, int> incoming,
) {
  final result = Map<String, int>.from(base);
  for (final entry in incoming.entries) {
    result[entry.key] = max(result[entry.key] ?? 0, entry.value);
  }
  return result;
}

bool contextsEqual(Map<String, int> a, Map<String, int> b) {
  if (a.length != b.length) return false;
  for (final key in a.keys) {
    if (a[key] != b[key]) return false;
  }
  return true;
}

// ── Validation per NIP-RS Content Validation ─────────────────────────────────

String? validateDTag(NostrEvent event) {
  final dTags = event.tags.where((t) => t.length >= 2 && t[0] == 'd').toList();
  if (dTags.length != 1) return null;
  final dValue = dTags[0][1];
  if (!dValue.startsWith('read-state:')) return null;
  final slotId = dValue.substring('read-state:'.length);
  if (slotId.isEmpty || slotId.length > 64) return null;
  for (var i = 0; i < slotId.length; i++) {
    if (slotId.codeUnitAt(i) > 127) return null;
  }
  return dValue;
}

bool validateTTag(NostrEvent event) {
  final tTags = event.tags
      .where((t) => t.length >= 2 && t[0] == 't' && t[1] == 'read-state')
      .toList();
  return tTags.length == 1;
}

/// Decoded read-state blob from a kind:30078 event.
class DecodedBlob {
  final Map<String, int> contexts;
  final String clientId;
  final NostrEvent event;

  DecodedBlob({
    required this.contexts,
    required this.clientId,
    required this.event,
  });
}

DecodedBlob? decryptAndValidateBlob(
  NostrEvent event,
  Uint8List conversationKey,
) {
  if (validateDTag(event) == null) return null;
  if (!validateTTag(event)) return null;

  String plaintext;
  try {
    plaintext = nip44Decrypt(conversationKey, event.content);
  } catch (_) {
    return null;
  }

  Map<String, dynamic> parsed;
  try {
    parsed = jsonDecode(plaintext) as Map<String, dynamic>;
  } catch (_) {
    return null;
  }

  // Validate v
  final v = parsed['v'];
  if (v is! int || v != 1) return null;

  // Validate client_id
  final clientId = parsed['client_id'];
  if (clientId is! String || clientId.isEmpty || clientId.length > 64) {
    return null;
  }

  // Validate contexts
  final rawContexts = parsed['contexts'];
  if (rawContexts is! Map<String, dynamic>) return null;
  if (rawContexts.length > 10000) return null;

  final contexts = <String, int>{};
  for (final entry in rawContexts.entries) {
    if (utf8.encode(entry.key).length > 256) continue;
    final ts = entry.value;
    if (ts is! int || ts < 0 || ts > 4294967295) continue;
    contexts[entry.key] = ts;
  }

  return DecodedBlob(contexts: contexts, clientId: clientId, event: event);
}

/// Parse a cached read-state JSON string back into a `Map<String, int>`.
/// Handles both the old ISO-string format and the new unix-seconds format.
Map<String, int> parseCachedReadState(String? raw) {
  if (raw == null || raw.isEmpty) return {};
  try {
    final parsed = jsonDecode(raw);
    if (parsed is! Map<String, dynamic>) return {};

    final result = <String, int>{};
    for (final entry in parsed.entries) {
      if (entry.value is int) {
        result[entry.key] = entry.value as int;
      } else if (entry.value is String) {
        final ms = DateTime.tryParse(
          entry.value as String,
        )?.millisecondsSinceEpoch;
        if (ms != null) result[entry.key] = ms ~/ 1000;
      }
    }
    return result;
  } catch (_) {
    return {};
  }
}
