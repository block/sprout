import 'dart:convert';
import 'dart:math';

import 'package:shared_preferences/shared_preferences.dart';
import 'package:uuid/uuid.dart';

import 'read_state_format.dart';
import 'read_state_time.dart';

const _clientIdKeyPrefix = 'sprout.nip-rs.client-id';
const _slotIdKeyPrefix = 'sprout.nip-rs.slot-id';
const _uuid = Uuid();

String localReadStateKey(String pubkey) =>
    'sprout.channel-read-state.v2:$pubkey';

String localPublishableContextKey(String pubkey) =>
    'sprout.channel-read-state.publishable.v1:$pubkey';

String clientIdKey(String pubkey) => '$_clientIdKeyPrefix:$pubkey';

String slotIdKey(String pubkey) => '$_slotIdKeyPrefix:$pubkey';

class StoredReadState {
  final Map<String, int> contexts;
  final Set<String> publishableContextIds;

  StoredReadState({
    required Map<String, int> contexts,
    required Set<String> publishableContextIds,
  }) : contexts = Map.unmodifiable(contexts),
       publishableContextIds = Set.unmodifiable(publishableContextIds);
}

class ReadStateStorage {
  final SharedPreferences _prefs;

  ReadStateStorage(this._prefs);

  String getOrCreateClientId(String pubkey) {
    final key = clientIdKey(pubkey);
    final stored = _prefs.getString(key);
    if (_isValidClientId(stored)) {
      return stored!;
    }

    final generated = _uuid.v4();
    _prefs.setString(key, generated);
    return generated;
  }

  String getOrCreateSlotId(String pubkey) {
    final key = slotIdKey(pubkey);
    final stored = _prefs.getString(key);
    if (_isValidSlotId(stored)) {
      return stored!;
    }

    final generated = generateReadStateSlotId();
    _prefs.setString(key, generated);
    return generated;
  }

  void writeSlotId(String pubkey, String slotId) {
    _prefs.setString(slotIdKey(pubkey), slotId);
  }

  StoredReadState read(String pubkey) {
    return StoredReadState(
      contexts: _readContexts(pubkey),
      publishableContextIds: _readPublishableContextIds(pubkey),
    );
  }

  void write(
    String pubkey,
    Map<String, int> contexts,
    Set<String> publishableContextIds,
  ) {
    final state = <String, String>{};
    for (final entry in contexts.entries) {
      state[entry.key] = unixSecondsToDateTime(entry.value).toIso8601String();
    }

    _prefs.setString(localReadStateKey(pubkey), jsonEncode(state));
    _prefs.setString(
      localPublishableContextKey(pubkey),
      jsonEncode(publishableContextIds.toList()),
    );
  }

  Map<String, int> _readContexts(String pubkey) {
    final raw = _prefs.getString(localReadStateKey(pubkey));
    if (raw == null || raw.isEmpty) {
      return {};
    }

    final Object? parsed;
    try {
      parsed = jsonDecode(raw);
    } catch (_) {
      return {};
    }

    final record = asStringObjectMap(parsed);
    if (record == null) {
      return {};
    }

    final contexts = <String, int>{};
    for (final entry in record.entries) {
      final timestamp = isoToUnixSeconds(entry.value);
      if (timestamp == null) continue;

      final current = contexts[entry.key] ?? 0;
      if (timestamp > current) {
        contexts[entry.key] = timestamp;
      }
    }
    return contexts;
  }

  Set<String> _readPublishableContextIds(String pubkey) {
    final raw = _prefs.getString(localPublishableContextKey(pubkey));
    if (raw == null || raw.isEmpty) {
      return {};
    }

    final Object? parsed;
    try {
      parsed = jsonDecode(raw);
    } catch (_) {
      return {};
    }

    if (parsed is! List) {
      return {};
    }

    return {
      for (final value in parsed)
        if (value is String) value,
    };
  }
}

String generateReadStateSlotId() {
  final random = Random.secure();
  final bytes = List<int>.generate(16, (_) => random.nextInt(256));
  return bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
}

bool _isValidClientId(String? value) {
  return value != null && value.isNotEmpty && value.runes.length <= 64;
}

bool _isValidSlotId(String? value) {
  if (value == null || value.isEmpty || value.length > 64) {
    return false;
  }
  return isValidReadStateDTag('$readStateDTagPrefix$value');
}
