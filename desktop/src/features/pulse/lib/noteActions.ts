import { nip19 } from "nostr-tools";

import type { UserNote } from "@/shared/api/socialTypes";

export function buildNoteShareUri(note: Pick<UserNote, "id" | "pubkey">) {
  return `nostr:${nip19.neventEncode({
    id: note.id,
    author: note.pubkey,
  })}`;
}

export function toggleNoteIdInSet(
  current: ReadonlySet<string>,
  noteId: string,
  enabled: boolean,
) {
  const next = new Set(current);
  if (enabled) {
    next.add(noteId);
  } else {
    next.delete(noteId);
  }
  return next;
}
