import type { UserNote, UserNotesResponse } from "@/shared/api/socialTypes";

import { invokeTauri } from "./tauri";

type RawUserNote = {
  id: string;
  pubkey: string;
  created_at: number;
  content: string;
};

type RawUserNotesCursor = {
  before: number;
  before_id: string;
};

type RawUserNotesResponse = {
  notes: RawUserNote[];
  next_cursor: RawUserNotesCursor | null;
};

function fromRawUserNote(note: RawUserNote): UserNote {
  return {
    id: note.id,
    pubkey: note.pubkey,
    createdAt: note.created_at,
    content: note.content,
  };
}

export async function getUserNotes(
  pubkey: string,
  options?: {
    limit?: number;
    before?: number;
    beforeId?: string;
  },
): Promise<UserNotesResponse> {
  const response = await invokeTauri<RawUserNotesResponse>("get_user_notes", {
    pubkey,
    limit: options?.limit ?? null,
    before: options?.before ?? null,
    beforeId: options?.beforeId ?? null,
  });

  return {
    notes: response.notes.map(fromRawUserNote),
    nextCursor: response.next_cursor
      ? {
          before: response.next_cursor.before,
          beforeId: response.next_cursor.before_id,
        }
      : null,
  };
}
