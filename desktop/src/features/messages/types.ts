export type TimelineReaction = {
  emoji: string;
  count: number;
  reactedByCurrentUser?: boolean;
  users: Array<{ pubkey: string; displayName: string }>;
};

/** Sidebar thread summary for a root message (main timeline). */
export type ThreadConversationHint = {
  replyCount: number;
  /** Unique reply authors (up to 5) for avatar stack. */
  participantPubkeys: string[];
  /** Parallel display names (tooltips / initials fallback). */
  participantLabels: string[];
};

export type TimelineMessage = {
  id: string;
  createdAt: number;
  pubkey?: string;
  author: string;
  avatarUrl?: string | null;
  role?: string;
  time: string;
  body: string;
  parentId?: string | null;
  rootId?: string | null;
  depth: number;
  accent?: boolean;
  pending?: boolean;
  edited?: boolean;
  highlighted?: boolean;
  kind?: number;
  tags?: string[][];
  reactions?: TimelineReaction[];
};
