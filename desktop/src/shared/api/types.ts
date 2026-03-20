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

export type OpenDmInput = {
  pubkeys: string[];
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

export type UserSearchResult = {
  pubkey: string;
  displayName: string | null;
  avatarUrl: string | null;
  nip05Handle: string | null;
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

export type ManagedAgentBackend =
  | { type: "local" }
  | { type: "provider"; id: string; config: Record<string, unknown> };

export type ManagedAgent = {
  pubkey: string;
  name: string;
  personaId: string | null;
  relayUrl: string;
  acpCommand: string;
  agentCommand: string;
  agentArgs: string[];
  mcpCommand: string;
  turnTimeoutSeconds: number;
  parallelism: number;
  systemPrompt: string | null;
  model: string | null;
  hasApiToken: boolean;
  status: "running" | "stopped" | "deployed" | "not_deployed";
  pid: number | null;
  createdAt: string;
  updatedAt: string;
  lastStartedAt: string | null;
  lastStoppedAt: string | null;
  lastExitCode: number | null;
  lastError: string | null;
  logPath: string;
  startOnAppLaunch: boolean;
  backend: ManagedAgentBackend;
  backend_agent_id: string | null;
};

export type BackendProviderCandidate = {
  id: string;
  binaryPath: string;
};

export type BackendProviderProbeResult = {
  ok: boolean;
  name?: string;
  version?: string;
  description?: string;
  config_schema?: Record<string, unknown>;
};

export type CreateManagedAgentInput = {
  name: string;
  personaId?: string;
  relayUrl?: string;
  acpCommand?: string;
  agentCommand?: string;
  agentArgs?: string[];
  mcpCommand?: string;
  turnTimeoutSeconds?: number;
  parallelism?: number;
  systemPrompt?: string;
  avatarUrl?: string;
  model?: string;
  mintToken?: boolean;
  tokenScopes?: TokenScope[];
  tokenName?: string;
  spawnAfterCreate?: boolean;
  startOnAppLaunch?: boolean;
  backend?: ManagedAgentBackend;
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

// ── Model discovery types ─────────────────────────────────────────────────────

export type AgentModelsResponse = {
  agentName: string;
  agentVersion: string;
  models: AgentModelInfo[];
  agentDefaultModel: string | null;
  selectedModel: string | null;
  supportsSwitching: boolean;
};

export type AgentModelInfo = {
  id: string;
  name: string | null;
  description: string | null;
};

export type UpdateManagedAgentInput = {
  pubkey: string;
  model?: string | null;
  systemPrompt?: string | null;
};

export type AgentPersona = {
  id: string;
  displayName: string;
  avatarUrl: string | null;
  systemPrompt: string;
  isBuiltIn: boolean;
  createdAt: string;
  updatedAt: string;
};

export type CreatePersonaInput = {
  displayName: string;
  avatarUrl?: string;
  systemPrompt: string;
};

export type UpdatePersonaInput = {
  id: string;
  displayName: string;
  avatarUrl?: string;
  systemPrompt: string;
};

// ── Forum types ───────────────────────────────────────────────────────────────

export type ThreadSummary = {
  replyCount: number;
  descendantCount: number;
  lastReplyAt: number | null;
  participants: string[];
};

export type ForumPost = {
  eventId: string;
  pubkey: string;
  content: string;
  kind: number;
  createdAt: number;
  channelId: string;
  tags: string[][];
  threadSummary: ThreadSummary | null;
};

export type ForumPostsResponse = {
  posts: ForumPost[];
  nextCursor: number | null;
};

export type ThreadReply = {
  eventId: string;
  pubkey: string;
  content: string;
  kind: number;
  createdAt: number;
  channelId: string;
  tags: string[][];
  parentEventId: string | null;
  rootEventId: string | null;
  depth: number;
};

export type ForumThreadResponse = {
  post: ForumPost;
  replies: ThreadReply[];
  totalReplies: number;
  nextCursor: string | null;
};
