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
  isMember: boolean;
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
  avatarUrl: string | null;
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

export type SendChannelMessageResult = {
  eventId: string;
  parentEventId: string | null;
  rootEventId: string | null;
  depth: number;
  createdAt: number;
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

export type TokenScope =
  | "messages:read"
  | "messages:write"
  | "channels:read"
  | "channels:write"
  | "users:read"
  | "users:write"
  | "files:read"
  | "files:write";

export type Token = {
  id: string;
  name: string;
  scopes: TokenScope[];
  channelIds: string[];
  createdAt: string;
  expiresAt: string | null;
  lastUsedAt: string | null;
  revokedAt: string | null;
};

export type MintTokenInput = {
  name: string;
  scopes: TokenScope[];
  channelIds?: string[];
  expiresInDays?: number;
};

export type MintTokenResponse = {
  id: string;
  token: string;
  name: string;
  scopes: TokenScope[];
  channelIds: string[];
  createdAt: string;
  expiresAt: string | null;
};

export type RelayAgent = {
  pubkey: string;
  name: string;
  agentType: string;
  channels: string[];
  capabilities: string[];
  status: "online" | "away" | "offline";
};

export type ManagedAgent = {
  pubkey: string;
  name: string;
  relayUrl: string;
  acpCommand: string;
  agentCommand: string;
  agentArgs: string[];
  mcpCommand: string;
  turnTimeoutSeconds: number;
  parallelism: number;
  systemPrompt: string | null;
  hasApiToken: boolean;
  status: "running" | "stopped";
  pid: number | null;
  createdAt: string;
  updatedAt: string;
  lastStartedAt: string | null;
  lastStoppedAt: string | null;
  lastExitCode: number | null;
  lastError: string | null;
  logPath: string;
};

export type CreateManagedAgentInput = {
  name: string;
  relayUrl?: string;
  acpCommand?: string;
  agentCommand?: string;
  agentArgs?: string[];
  mcpCommand?: string;
  turnTimeoutSeconds?: number;
  parallelism?: number;
  systemPrompt?: string;
  mintToken?: boolean;
  tokenScopes?: TokenScope[];
  tokenName?: string;
  spawnAfterCreate?: boolean;
};

export type CreateManagedAgentResponse = {
  agent: ManagedAgent;
  privateKeyNsec: string;
  apiToken: string | null;
  profileSyncError: string | null;
  spawnError: string | null;
};

export type MintManagedAgentTokenInput = {
  pubkey: string;
  tokenName?: string;
  scopes?: TokenScope[];
};

export type MintManagedAgentTokenResponse = {
  agent: ManagedAgent;
  token: string;
};

export type ManagedAgentLog = {
  content: string;
  logPath: string;
};

export type AcpProvider = {
  id: string;
  label: string;
  command: string;
  binaryPath: string;
  defaultArgs: string[];
};

export type CommandAvailability = {
  command: string;
  resolvedPath: string | null;
  available: boolean;
};

export type ManagedAgentPrereqs = {
  admin: CommandAvailability;
  acp: CommandAvailability;
  mcp: CommandAvailability;
};
