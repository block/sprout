import 'package:hooks_riverpod/hooks_riverpod.dart';

import '../../shared/relay/relay.dart';
import '../profile/user_cache_provider.dart';
import 'pulse_models.dart';

final globalNotesProvider = FutureProvider<List<UserNote>>((ref) async {
  final session = ref.watch(relaySessionProvider.notifier);
  final events = await session.fetchHistory(NostrFilters.globalNotes());
  return _notesFromEvents(events);
});

final notesTimelineProvider =
    FutureProvider.family<List<UserNote>, List<String>>((ref, pubkeys) async {
      if (pubkeys.isEmpty) return const [];
      final session = ref.watch(relaySessionProvider.notifier);
      final events = await session.fetchHistory(
        NostrFilters.notesTimeline(
          pubkeys.map((p) => p.toLowerCase()).toList(),
        ),
      );
      return _notesFromEvents(events);
    });

final likedNotesProvider = FutureProvider<List<UserNote>>((ref) async {
  final pubkey = ref.watch(myPubkeyProvider);
  if (pubkey == null) return const [];
  final session = ref.watch(relaySessionProvider.notifier);
  final reactions = await session.fetchHistory(
    NostrFilters.userReactions(pubkey),
  );
  final ids = <String>[];
  final seen = <String>{};
  for (final reaction in reactions) {
    if (reaction.content != '+') continue;
    final noteId = _lastETag(reaction.tags);
    if (noteId != null && seen.add(noteId)) ids.add(noteId);
  }
  if (ids.isEmpty) return const [];
  final notes = await session.fetchHistory(NostrFilters.notesByIds(ids));
  return _notesFromEvents(notes);
});

final noteReactionsProvider =
    FutureProvider.family<Map<String, PulseReactionState>, List<String>>((
      ref,
      noteIds,
    ) async {
      if (noteIds.isEmpty) return const {};
      final currentPubkey = ref.watch(myPubkeyProvider)?.toLowerCase();
      final session = ref.watch(relaySessionProvider.notifier);
      final events = await session.fetchHistory(
        NostrFilters.noteReactions(noteIds),
      );
      final pubkeysByNote = <String, Set<String>>{};
      final currentReactionIds = <String, String>{};

      for (final event in events) {
        if (event.content != '+') continue;
        final noteId = _lastETag(event.tags);
        if (noteId == null) continue;
        final pubkey = event.pubkey.toLowerCase();
        pubkeysByNote.putIfAbsent(noteId, () => <String>{}).add(pubkey);
        if (currentPubkey != null && pubkey == currentPubkey) {
          currentReactionIds[noteId] = event.id;
        }
      }

      return {
        for (final noteId in noteIds)
          noteId: PulseReactionState(
            count: pubkeysByNote[noteId]?.length ?? 0,
            reactedByCurrentUser: currentReactionIds.containsKey(noteId),
            currentUserReactionId: currentReactionIds[noteId],
          ),
      };
    });

final contactListProvider = FutureProvider.family<List<ContactEntry>, String>((
  ref,
  pubkey,
) async {
  if (pubkey.isEmpty) return const [];
  final session = ref.watch(relaySessionProvider.notifier);
  final events = await session.fetchHistory(
    NostrFilters.contactList(pubkey.toLowerCase()),
  );
  if (events.isEmpty) return const [];
  return _contactsFromTags(events.first.tags);
});

final agentPubkeysProvider = FutureProvider<List<String>>((ref) async {
  final session = ref.watch(relaySessionProvider.notifier);
  final events = await session.fetchHistory(NostrFilters.agentProfiles());
  final pubkeys = <String>{};
  for (final event in events) {
    pubkeys.add(event.pubkey.toLowerCase());
    final p = event.getTagValue('p');
    if (p != null) pubkeys.add(p.toLowerCase());
  }
  return pubkeys.toList();
});

final agentNotesProvider = FutureProvider<List<UserNote>>((ref) async {
  final pubkeys = await ref.watch(agentPubkeysProvider.future);
  if (pubkeys.isEmpty) return const [];
  return ref.watch(notesTimelineProvider(pubkeys).future);
});

List<UserNote> _notesFromEvents(List<NostrEvent> events) {
  final notes =
      events
          .where((event) => event.kind == EventKind.note)
          .map(UserNote.fromEvent)
          .toList()
        ..sort((a, b) => b.createdAt.compareTo(a.createdAt));
  return notes;
}

List<ContactEntry> _contactsFromTags(List<List<String>> tags) => [
  for (final tag in tags)
    if (tag.length >= 2 && tag[0] == 'p')
      ContactEntry(
        pubkey: tag[1].toLowerCase(),
        relayUrl: tag.length >= 3 && tag[2].isNotEmpty ? tag[2] : null,
        petname: tag.length >= 4 && tag[3].isNotEmpty ? tag[3] : null,
      ),
];

String? _lastETag(List<List<String>> tags) {
  for (final tag in tags.reversed) {
    if (tag.length >= 2 && tag[0] == 'e') return tag[1];
  }
  return null;
}

void preloadPulseProfiles(WidgetRef ref, List<UserNote> notes) {
  final pubkeys = <String>{};
  for (final note in notes) {
    pubkeys.add(note.pubkey);
    pubkeys.addAll(note.mentionPubkeys);
  }
  ref.read(userCacheProvider.notifier).preload(pubkeys.toList());
}
