import 'package:flutter_secure_storage/flutter_secure_storage.dart';

class StoredCredentials {
  final String relayUrl;
  final String token;
  final String? pubkey;

  const StoredCredentials({
    required this.relayUrl,
    required this.token,
    this.pubkey,
  });
}

class CredentialStorage {
  static const _keyRelayUrl = 'sprout_relay_url';
  static const _keyToken = 'sprout_token';
  static const _keyPubkey = 'sprout_pubkey';

  final FlutterSecureStorage _storage;

  CredentialStorage([FlutterSecureStorage? storage])
    : _storage = storage ?? const FlutterSecureStorage();

  Future<StoredCredentials?> load() async {
    final relayUrl = await _storage.read(key: _keyRelayUrl);
    final token = await _storage.read(key: _keyToken);
    if (relayUrl == null || token == null) return null;

    final pubkey = await _storage.read(key: _keyPubkey);
    return StoredCredentials(relayUrl: relayUrl, token: token, pubkey: pubkey);
  }

  Future<void> save(StoredCredentials credentials) async {
    await _storage.write(key: _keyRelayUrl, value: credentials.relayUrl);
    await _storage.write(key: _keyToken, value: credentials.token);
    if (credentials.pubkey != null) {
      await _storage.write(key: _keyPubkey, value: credentials.pubkey);
    }
  }

  Future<void> clear() async {
    await _storage.delete(key: _keyRelayUrl);
    await _storage.delete(key: _keyToken);
    await _storage.delete(key: _keyPubkey);
  }
}
