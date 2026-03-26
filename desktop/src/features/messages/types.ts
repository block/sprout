export type TimelineReaction = {
  emoji: string;
  count: number;
  reactedByCurrentUser?: boolean;
  users: Array<{ pubkey: string; displayName: string }>;
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
