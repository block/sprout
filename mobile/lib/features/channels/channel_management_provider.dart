import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/auth/auth.dart';
import '../../shared/relay/relay.dart';
import '../profile/profile_provider.dart';
import 'channel.dart';
import 'channels_provider.dart';

@immutable
class ChannelMember {
  final String pubkey;
  final String role;
  final DateTime joinedAt;
  final String? displayName;

  const ChannelMember({
    required this.pubkey,
    required this.role,
    required this.joinedAt,
    this.displayName,
  });

  factory ChannelMember.fromJson(Map<String, dynamic> json) => ChannelMember(
    pubkey: json['pubkey'] as String,
    role: json['role'] as String? ?? 'member',
    joinedAt: DateTime.parse(json['joined_at'] as String),
    displayName: json['display_name'] as String?,
  );

  bool get isBot => role == 'bot';

  String labelFor(String? currentPubkey) {
    if (currentPubkey != null &&
        currentPubkey.toLowerCase() == pubkey.toLowerCase()) {
      return 'You';
    }
    if (displayName case final name? when name.trim().isNotEmpty) {
      return name.trim();
    }
    return pubkey.length > 8 ? '${pubkey.substring(0, 8)}…' : pubkey;
  }
}

@immutable
class ChannelCanvas {
  final String? content;
  final DateTime? updatedAt;
  final String? authorPubkey;

  const ChannelCanvas({
    required this.content,
    required this.updatedAt,
    required this.authorPubkey,
  });

  factory ChannelCanvas.fromJson(Map<String, dynamic> json) => ChannelCanvas(
    content: json['content'] as String?,
    updatedAt: json['updated_at'] != null
        ? DateTime.fromMillisecondsSinceEpoch(
            (json['updated_at'] as int) * 1000,
            isUtc: true,
          )
        : null,
    authorPubkey: json['author'] as String?,
  );
}

@immutable
class DirectoryUser {
  final String pubkey;
  final String? displayName;
  final String? avatarUrl;
  final String? nip05Handle;

  const DirectoryUser({
    required this.pubkey,
    this.displayName,
    this.avatarUrl,
    this.nip05Handle,
  });

  factory DirectoryUser.fromJson(Map<String, dynamic> json) => DirectoryUser(
    pubkey: json['pubkey'] as String,
    displayName: json['display_name'] as String?,
    avatarUrl: json['avatar_url'] as String?,
    nip05Handle: json['nip05_handle'] as String?,
  );

  String get label {
    final display = displayName?.trim();
    if (display != null && display.isNotEmpty) {
      return display;
    }
    final nip05 = nip05Handle?.trim();
    if (nip05 != null && nip05.isNotEmpty) {
      return nip05;
    }
    return pubkey.length > 8 ? '${pubkey.substring(0, 8)}…' : pubkey;
  }

  String get secondaryLabel {
    final nip05 = nip05Handle?.trim();
    if (nip05 != null && nip05.isNotEmpty && nip05 != label) {
      return nip05;
    }
    return pubkey.length > 16 ? '${pubkey.substring(0, 16)}…' : pubkey;
  }
}

final currentPubkeyProvider = Provider<String?>((ref) {
  final profile = ref.watch(profileProvider).whenData((value) => value).value;
  final profilePubkey = profile?.pubkey.trim();
  if (profilePubkey != null && profilePubkey.isNotEmpty) {
    return profilePubkey.toLowerCase();
  }

  final authState = ref.watch(authProvider).whenData((value) => value).value;
  final credentialPubkey = authState?.credentials?.pubkey?.trim();
  if (credentialPubkey != null && credentialPubkey.isNotEmpty) {
    return credentialPubkey.toLowerCase();
  }

  return null;
});

final channelDetailsProvider = FutureProvider.family<ChannelDetails, String>((
  ref,
  channelId,
) async {
  final client = ref.watch(relayClientProvider);
  final json =
      await client.get('/api/channels/$channelId') as Map<String, dynamic>;
  return ChannelDetails.fromJson(json);
});

final channelMembersProvider =
    FutureProvider.family<List<ChannelMember>, String>((ref, channelId) async {
      final client = ref.watch(relayClientProvider);
      final json =
          await client.get('/api/channels/$channelId/members')
              as Map<String, dynamic>;
      final members = json['members'] as List<dynamic>? ?? const [];
      return members
          .cast<Map<String, dynamic>>()
          .map(ChannelMember.fromJson)
          .toList();
    });

final channelCanvasProvider = FutureProvider.family<ChannelCanvas, String>((
  ref,
  channelId,
) async {
  final client = ref.watch(relayClientProvider);
  final json =
      await client.get('/api/channels/$channelId/canvas')
          as Map<String, dynamic>;
  return ChannelCanvas.fromJson(json);
});

class ChannelActions {
  final Ref _ref;
  final RelayClient _client;
  final SignedEventRelay _signedEventRelay;
  final String? _currentPubkey;

  ChannelActions({
    required Ref ref,
    required RelayClient client,
    required SignedEventRelay signedEventRelay,
    required String? currentPubkey,
  }) : _ref = ref,
       _client = client,
       _signedEventRelay = signedEventRelay,
       _currentPubkey = currentPubkey;

  Future<Channel> createChannel({
    required String name,
    required String channelType,
    required String visibility,
    String? description,
  }) async {
    final channelId = _newUuidV4();
    final tags = <List<String>>[
      ['h', channelId],
      ['name', name],
      ['visibility', visibility],
      ['channel_type', channelType],
      if (description case final about? when about.trim().isNotEmpty)
        ['about', about.trim()],
    ];
    await _signedEventRelay.submit(kind: 9007, content: '', tags: tags);
    return _refreshChannelsAndRead(channelId);
  }

  Future<Channel> openDm({required List<String> pubkeys}) async {
    final json =
        await _client.post('/api/dms', body: {'pubkeys': pubkeys})
            as Map<String, dynamic>;
    final channelId = json['channel_id'] as String?;
    if (channelId == null || channelId.isEmpty) {
      throw Exception('Relay did not return a DM channel id');
    }
    return _refreshChannelsAndRead(channelId);
  }

  Future<void> joinChannel(String channelId) async {
    await _signedEventRelay.submit(
      kind: 9021,
      content: '',
      tags: [
        ['h', channelId],
      ],
    );
    await _refreshChannelState(channelId);
  }

  Future<void> leaveChannel(String channelId) async {
    await _signedEventRelay.submit(
      kind: 9022,
      content: '',
      tags: [
        ['h', channelId],
      ],
    );
    await _refreshChannelState(channelId);
  }

  Future<void> setCanvas({
    required String channelId,
    required String content,
  }) async {
    await _signedEventRelay.submit(
      kind: 40100,
      content: content,
      tags: [
        ['h', channelId],
      ],
    );
    _ref.invalidate(channelCanvasProvider(channelId));
  }

  Future<List<DirectoryUser>> searchUsers(String query, {int limit = 8}) async {
    final trimmed = query.trim();
    if (trimmed.isEmpty) {
      return const [];
    }

    final json =
        await _client.get(
              '/api/users/search',
              queryParams: {'q': trimmed, 'limit': '$limit'},
            )
            as Map<String, dynamic>;
    final users = json['users'] as List<dynamic>? ?? const [];
    return users
        .cast<Map<String, dynamic>>()
        .map(DirectoryUser.fromJson)
        .where(
          (user) =>
              _currentPubkey == null ||
              user.pubkey.toLowerCase() != _currentPubkey,
        )
        .toList();
  }

  Future<Channel> _refreshChannelsAndRead(String channelId) async {
    await _ref.read(channelsProvider.notifier).refresh();
    final channels = await _ref.read(channelsProvider.future);
    return channels.firstWhere(
      (channel) => channel.id == channelId,
      orElse: () =>
          throw Exception('Channel was created but is not visible yet'),
    );
  }

  Future<void> _refreshChannelState(String channelId) async {
    await _ref.read(channelsProvider.notifier).refresh();
    _ref.invalidate(channelDetailsProvider(channelId));
    _ref.invalidate(channelMembersProvider(channelId));
    _ref.invalidate(channelCanvasProvider(channelId));
  }

  String _newUuidV4() {
    final bytes = List<int>.generate(16, (_) => _random.nextInt(256));
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    final hex = bytes
        .map((byte) => byte.toRadixString(16).padLeft(2, '0'))
        .join();
    return '${hex.substring(0, 8)}-'
        '${hex.substring(8, 12)}-'
        '${hex.substring(12, 16)}-'
        '${hex.substring(16, 20)}-'
        '${hex.substring(20, 32)}';
  }

  Future<void> addReaction(String eventId, String emoji) async {
    await _signedEventRelay.submit(
      kind: EventKind.reaction,
      content: emoji,
      tags: [
        ['e', eventId],
      ],
    );
  }

  Future<void> removeReaction(String reactionEventId, String emoji) async {
    await _signedEventRelay.submit(
      kind: EventKind.deletion,
      content: '',
      tags: [
        ['e', reactionEventId],
      ],
    );
  }

  Future<void> editMessage({
    required String channelId,
    required String eventId,
    required String content,
  }) async {
    await _signedEventRelay.submit(
      kind: EventKind.streamMessageEdit,
      content: content,
      tags: [
        ['h', channelId],
        ['e', eventId],
      ],
    );
  }

  Future<void> deleteMessage(String eventId) async {
    await _signedEventRelay.submit(
      kind: EventKind.deletion,
      content: '',
      tags: [
        ['e', eventId],
      ],
    );
  }

  static final Random _random = Random.secure();
}

final channelActionsProvider = Provider<ChannelActions>((ref) {
  final client = ref.watch(relayClientProvider);
  final relayConfig = ref.watch(relayConfigProvider);
  final currentPubkey = ref.watch(currentPubkeyProvider);
  return ChannelActions(
    ref: ref,
    client: client,
    signedEventRelay: SignedEventRelay(client: client, nsec: relayConfig.nsec),
    currentPubkey: currentPubkey,
  );
});
