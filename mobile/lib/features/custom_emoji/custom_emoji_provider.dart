import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:nostr/nostr.dart' as nostr;

import '../../shared/relay/relay.dart';
import 'custom_emoji.dart';

/// Workspace custom-emoji palette (NIP-30, per-user kind:30030 sets unioned).
///
/// On build, fetches every member's set and collapses to one entry per
/// shortcode (see [unionCustomEmoji]). Re-fetches when the relay session
/// reconnects. The palette is the single source of truth consumed by the
/// renderer, picker, autocomplete, and reaction/send tag plumbing.
class CustomEmojiPaletteNotifier extends AsyncNotifier<List<CustomEmoji>> {
  @override
  Future<List<CustomEmoji>> build() {
    ref.watch(relayClientProvider);
    ref.watch(relaySessionProvider);
    return _fetch();
  }

  Future<List<CustomEmoji>> _fetch() async {
    final sessionState = ref.read(relaySessionProvider);
    if (sessionState.status != SessionStatus.connected) return [];

    try {
      final session = ref.read(relaySessionProvider.notifier);
      final events = await session.fetchHistory(
        const NostrFilter(
          kinds: [kindEmojiSet],
          tags: {
            '#d': [customEmojiSetDTag],
          },
          // One 30030 per member; the relay keeps only the latest per
          // (pubkey, d_tag), so this bounds member count, not history.
          limit: 500,
        ),
      );
      return unionCustomEmoji(events);
    } catch (_) {
      return [];
    }
  }

  /// Force a re-fetch of the workspace palette.
  Future<void> refresh() async {
    state = const AsyncValue.loading();
    state = await AsyncValue.guard(_fetch);
  }

  /// The caller's OWN current set (latest 30030 under the d-tag).
  Future<List<CustomEmoji>> fetchOwnEmoji() async {
    final me = _myPubkey();
    if (me == null) return [];
    final sessionState = ref.read(relaySessionProvider);
    if (sessionState.status != SessionStatus.connected) return [];

    final session = ref.read(relaySessionProvider.notifier);
    final events = await session.fetchHistory(
      NostrFilter(
        kinds: const [kindEmojiSet],
        authors: [me],
        tags: const {
          '#d': [customEmojiSetDTag],
        },
        limit: 1,
      ),
    );
    if (events.isEmpty) return [];
    final latest = events.reduce((a, b) => a.createdAt >= b.createdAt ? a : b);
    return customEmojiFromEvent(latest);
  }

  /// Add/update a custom emoji in the caller's OWN set (read-modify-write).
  /// [url] should be a Blossom blob URL. Returns the normalized shortcode.
  /// Throws [ArgumentError] for an invalid shortcode.
  Future<String> setEmoji(String shortcode, String url) async {
    final normalized = normalizeShortcode(shortcode);
    if (normalized == null) {
      throw ArgumentError(
        'Invalid emoji name. Use letters, numbers, hyphen, or underscore.',
      );
    }
    final own = await fetchOwnEmoji();
    final next = own.where((e) => e.shortcode != normalized).toList()
      ..add(CustomEmoji(shortcode: normalized, url: url));
    await _publishOwnSet(next);
    await refresh();
    return normalized;
  }

  /// Remove a custom emoji from the caller's OWN set by shortcode.
  Future<void> removeEmoji(String shortcode) async {
    final normalized = normalizeShortcode(shortcode);
    if (normalized == null) return;
    final own = await fetchOwnEmoji();
    final next = own.where((e) => e.shortcode != normalized).toList();
    if (next.length == own.length) return; // not present — nothing to republish
    await _publishOwnSet(next);
    await refresh();
  }

  Future<void> _publishOwnSet(List<CustomEmoji> emojis) async {
    final config = ref.read(relayConfigProvider);
    final nsec = config.nsec;
    if (nsec == null || nsec.isEmpty) {
      throw StateError('Cannot publish emoji set: no signing key available');
    }
    final tags = <List<String>>[
      ['d', customEmojiSetDTag],
      for (final e in emojis) ['emoji', e.shortcode, e.url],
    ];
    final privkeyHex = nostr.Nip19.decode(payload: nsec).data;
    final event = nostr.Event.from(
      kind: kindEmojiSet,
      content: '',
      tags: tags,
      secretKey: privkeyHex,
      verify: false,
    );
    final session = ref.read(relaySessionProvider.notifier);
    await session.publish(NostrEvent.fromJson(event.toMap()));
  }

  String? _myPubkey() {
    final config = ref.read(relayConfigProvider);
    final nsec = config.nsec;
    if (nsec == null || nsec.isEmpty) return null;
    try {
      final privkeyHex = nostr.Nip19.decode(payload: nsec).data;
      return nostr.Keys(privkeyHex).public.toLowerCase();
    } catch (_) {
      return null;
    }
  }
}

final customEmojiPaletteProvider =
    AsyncNotifierProvider<CustomEmojiPaletteNotifier, List<CustomEmoji>>(
      CustomEmojiPaletteNotifier.new,
    );

/// Synchronous read of the current palette (empty while loading/error).
/// Convenience for widgets that only need the resolved list, not the async
/// state — renderer, autocomplete, reaction resolution.
final customEmojiListProvider = Provider<List<CustomEmoji>>((ref) {
  return ref.watch(customEmojiPaletteProvider).value ?? const [];
});
