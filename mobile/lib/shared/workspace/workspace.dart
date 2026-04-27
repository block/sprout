import 'package:uuid/uuid.dart';

const _uuid = Uuid();

class Workspace {
  final String id;
  final String name;
  final String relayUrl;
  final String token;
  final String? pubkey;
  final String? nsec;
  final DateTime addedAt;

  const Workspace({
    required this.id,
    required this.name,
    required this.relayUrl,
    required this.token,
    this.pubkey,
    this.nsec,
    required this.addedAt,
  });

  factory Workspace.create({
    required String name,
    required String relayUrl,
    required String token,
    String? pubkey,
    String? nsec,
  }) {
    return Workspace(
      id: _uuid.v4(),
      name: name,
      relayUrl: relayUrl,
      token: token,
      pubkey: pubkey,
      nsec: nsec,
      addedAt: DateTime.now(),
    );
  }

  Workspace copyWith({
    String? name,
    String? relayUrl,
    String? token,
    String? pubkey,
    String? nsec,
  }) {
    return Workspace(
      id: id,
      name: name ?? this.name,
      relayUrl: relayUrl ?? this.relayUrl,
      token: token ?? this.token,
      pubkey: pubkey ?? this.pubkey,
      nsec: nsec ?? this.nsec,
      addedAt: addedAt,
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'name': name,
    'relayUrl': relayUrl,
    'token': token,
    if (pubkey != null) 'pubkey': pubkey,
    if (nsec != null) 'nsec': nsec,
    'addedAt': addedAt.toIso8601String(),
  };

  factory Workspace.fromJson(Map<String, dynamic> json) => Workspace(
    id: json['id'] as String,
    name: json['name'] as String,
    relayUrl: json['relayUrl'] as String,
    token: json['token'] as String,
    pubkey: json['pubkey'] as String?,
    nsec: json['nsec'] as String?,
    addedAt: DateTime.parse(json['addedAt'] as String),
  );

  /// Derive a human-friendly workspace name from a relay URL.
  static String nameFromUrl(String url) {
    try {
      final host = Uri.parse(url).host;
      if (host.contains('localhost') || host == '127.0.0.1') return 'Local Dev';
      final parts = host.split('.');
      if (parts.length > 2) return parts.first;
      return host;
    } catch (_) {
      return 'Workspace';
    }
  }
}
