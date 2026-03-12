export type TimelineMessage = {
  id: string;
  pubkey?: string;
  author: string;
  avatarUrl?: string | null;
  role?: string;
  time: string;
  body: string;
  accent?: boolean;
  pending?: boolean;
  highlighted?: boolean;
  kind?: number;
  tags?: string[][];
};
