export type UserNote = {
  id: string;
  pubkey: string;
  createdAt: number;
  content: string;
};

export type UserNotesCursor = {
  before: number;
  beforeId: string;
};

export type UserNotesResponse = {
  notes: UserNote[];
  nextCursor: UserNotesCursor | null;
};
