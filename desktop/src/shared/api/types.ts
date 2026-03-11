export type ChannelType = "stream" | "forum" | "dm";
export type ChannelVisibility = "open" | "private";
export type ChannelRole = "owner" | "admin" | "member" | "guest" | "bot";

export type Channel = {
  id: string;
  name: string;
  channelType: ChannelType;
  visibility: ChannelVisibility;
  description: string;
  topic: string | null;
  purpose: string | null;
  memberCount: number;
  lastMessageAt: string | null;
  archivedAt: string | null;
  participants: string[];
  participantPubkeys: string[];
};

export type ChannelDetail = Channel & {
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  topicSetBy: string | null;
  topicSetAt: string | null;
  purposeSetBy: string | null;
  purposeSetAt: string | null;
  topicRequired: boolean;
  maxMembers: number | null;
  nip29GroupId: string | null;
};

export type ChannelMember = {
  pubkey: string;
  role: ChannelRole;
  joinedAt: string;
  displayName: string | null;
};

export type CreateChannelInput = {
  name: string;
  channelType: Exclude<ChannelType, "dm">;
  visibility: ChannelVisibility;
  description?: string;
};

export type UpdateChannelInput = {
  channelId: string;
  name?: string;
  description?: string;
};

export type SetChannelTopicInput = {
  channelId: string;
  topic: string;
};

export type SetChannelPurposeInput = {
  channelId: string;
  purpose: string;
};

export type AddChannelMembersInput = {
  channelId: string;
  pubkeys: string[];
  role?: Exclude<ChannelRole, "owner">;
};

export type AddChannelMembersResult = {
  added: string[];
  errors: Array<{
    pubkey: string;
    error: string;
  }>;
};

export type Identity = {
  pubkey: string;
  displayName: string;
};

export type Profile = {
  pubkey: string;
  displayName: string | null;
  avatarUrl: string | null;
  about: string | null;
  nip05Handle: string | null;
};

export type UserProfileSummary = {
  displayName: string | null;
  nip05Handle: string | null;
};

export type UsersBatchResponse = {
  profiles: Record<string, UserProfileSummary>;
  missing: string[];
};

export type UpdateProfileInput = {
  displayName?: string;
  avatarUrl?: string;
  about?: string;
  nip05Handle?: string;
};

export type PresenceStatus = "online" | "away" | "offline";

export type PresenceLookup = Record<string, PresenceStatus>;

export type SetPresenceResult = {
  status: PresenceStatus;
  ttlSeconds: number;
};

export type RelayEvent = {
  id: string;
  pubkey: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
  sig: string;
  pending?: boolean;
};

export type FeedItemCategory =
  | "mention"
  | "needs_action"
  | "activity"
  | "agent_activity";

export type FeedItem = {
  id: string;
  kind: number;
  pubkey: string;
  content: string;
  createdAt: number;
  channelId: string | null;
  channelName: string;
  tags: string[][];
  category: FeedItemCategory;
};

export type HomeFeed = {
  mentions: FeedItem[];
  needsAction: FeedItem[];
  activity: FeedItem[];
  agentActivity: FeedItem[];
};

export type HomeFeedMeta = {
  since: number;
  total: number;
  generatedAt: number;
};

export type HomeFeedResponse = {
  feed: HomeFeed;
  meta: HomeFeedMeta;
};

export type GetHomeFeedInput = {
  since?: number;
  limit?: number;
  types?: string;
};

export type SearchMessagesInput = {
  q: string;
  limit?: number;
};

export type SearchHit = {
  eventId: string;
  content: string;
  kind: number;
  pubkey: string;
  channelId: string;
  channelName: string;
  createdAt: number;
  score: number;
};

export type SearchMessagesResponse = {
  hits: SearchHit[];
  found: number;
};
