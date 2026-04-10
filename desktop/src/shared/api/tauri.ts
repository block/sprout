import { invoke as tauriInvoke } from "@tauri-apps/api/core";

import type {
  AddChannelMembersInput,
  AddChannelMembersResult,
  BackendProviderCandidate,
  BackendProviderProbeResult,
  CanvasResponse,
  Channel,
  ChannelDetail,
  ChannelMember,
  ChannelType,
  CreateChannelInput,
  GetHomeFeedInput,
  HomeFeedResponse,
  Identity,
  MintTokenInput,
  MintTokenResponse,
  MintManagedAgentTokenInput,
  ManagedAgent,
  ManagedAgentBackend,
  RelayAgent,
  PresenceLookup,
  PresenceStatus,
  Profile,
  RelayEvent,
  SearchMessagesInput,
  SearchMessagesResponse,
  SendChannelMessageResult,
  SetCanvasInput,
  SetCanvasResult,
  SetPresenceResult,
  SetChannelPurposeInput,
  SetChannelTopicInput,
  Token,
  TokenScope,
  UpdateProfileInput,
  UpdateChannelInput,
  UserProfileSummary,
  UserSearchResult,
  UsersBatchResponse,
  CreateManagedAgentInput,
  AgentModelsResponse,
  UpdateManagedAgentInput,
  AcpProvider,
  CommandAvailability,
  OpenDmInput,
} from "@/shared/api/types";

type RawIdentity = {
  pubkey: string;
  display_name: string;
};

type RawProfile = {
  pubkey: string;
  display_name: string | null;
  avatar_url: string | null;
  about: string | null;
  nip05_handle: string | null;
};

type RawUserProfileSummary = {
  display_name: string | null;
  avatar_url: string | null;
  nip05_handle: string | null;
};

type RawUsersBatchResponse = {
  profiles: Record<string, RawUserProfileSummary>;
  missing: string[];
};

type RawUserSearchResult = {
  pubkey: string;
  display_name: string | null;
  avatar_url: string | null;
  nip05_handle: string | null;
};

type RawSearchUsersResponse = {
  users: RawUserSearchResult[];
};

type RawPresenceLookup = Record<string, PresenceStatus>;

type RawSetPresenceResult = {
  status: PresenceStatus;
  ttl_seconds: number;
};

type RawChannel = {
  id: string;
  name: string;
  channel_type: ChannelType;
  visibility: "open" | "private";
  description: string;
  topic: string | null;
  purpose: string | null;
  member_count: number;
  last_message_at: string | null;
  archived_at: string | null;
  participants: string[];
  participant_pubkeys: string[];
  is_member?: boolean;
  ttl_seconds: number | null;
  ttl_deadline: string | null;
};

type RawChannelDetail = RawChannel & {
  created_by: string;
  created_at: string;
  updated_at: string;
  topic_set_by: string | null;
  topic_set_at: string | null;
  purpose_set_by: string | null;
  purpose_set_at: string | null;
  topic_required: boolean;
  max_members: number | null;
  nip29_group_id: string | null;
};

type RawChannelMember = {
  pubkey: string;
  role: ChannelMember["role"];
  joined_at: string;
  display_name: string | null;
};

type RawChannelMembersResponse = {
  members: RawChannelMember[];
  next_cursor: string | null;
};

type RawAddChannelMembersResult = {
  added: string[];
  errors: Array<{
    pubkey: string;
    error: string;
  }>;
};

type RawFeedItem = {
  id: string;
  kind: number;
  pubkey: string;
  content: string;
  created_at: number;
  channel_id: string | null;
  channel_name: string;
  tags: string[][];
  category: "mention" | "needs_action" | "activity" | "agent_activity";
};

type RawHomeFeedResponse = {
  feed: {
    mentions: RawFeedItem[];
    needs_action: RawFeedItem[];
    activity: RawFeedItem[];
    agent_activity: RawFeedItem[];
  };
  meta: {
    since: number;
    total: number;
    generated_at: number;
  };
};

type RawSearchHit = {
  event_id: string;
  content: string;
  kind: number;
  pubkey: string;
  channel_id: string | null;
  channel_name: string | null;
  created_at: number;
  score: number;
};

type RawSearchResponse = {
  hits: RawSearchHit[];
  found: number;
};

type RawSendChannelMessageResult = {
  event_id: string;
  parent_event_id: string | null;
  root_event_id: string | null;
  depth: number;
  created_at: number;
};

type RawToken = {
  id: string;
  name: string;
  scopes: TokenScope[];
  channel_ids: string[];
  created_at: string;
  expires_at: string | null;
  last_used_at: string | null;
  revoked_at: string | null;
};

type RawListTokensResponse = {
  tokens: RawToken[];
};

type RawMintTokenResponse = {
  id: string;
  token: string;
  name: string;
  scopes: TokenScope[];
  channel_ids: string[];
  created_at: string;
  expires_at: string | null;
};

type RawRelayAgent = {
  pubkey: string;
  name: string;
  agent_type: string;
  channels: string[];
  channel_ids: string[];
  capabilities: string[];
  status: RelayAgent["status"];
};

export type RawManagedAgent = {
  pubkey: string;
  name: string;
  persona_id: string | null;
  relay_url: string;
  acp_command: string;
  agent_command: string;
  agent_args: string[];
  mcp_command: string;
  turn_timeout_seconds: number;
  idle_timeout_seconds: number | null;
  max_turn_duration_seconds: number | null;
  parallelism: number;
  system_prompt: string | null;
  model: string | null;
  has_api_token: boolean;
  status: ManagedAgent["status"];
  pid: number | null;
  created_at: string;
  updated_at: string;
  last_started_at: string | null;
  last_stopped_at: string | null;
  last_exit_code: number | null;
  last_error: string | null;
  log_path: string;
  start_on_app_launch: boolean;
  backend: ManagedAgentBackend;
  backend_agent_id: string | null;
};

type RawCreateManagedAgentResponse = {
  agent: RawManagedAgent;
  private_key_nsec: string;
  api_token: string | null;
  profile_sync_error: string | null;
  spawn_error: string | null;
};

type RawMintManagedAgentTokenResponse = {
  agent: RawManagedAgent;
  token: string;
};

type RawManagedAgentLog = {
  content: string;
  log_path: string;
};

type RawAcpProvider = {
  id: string;
  label: string;
  command: string;
  binary_path: string;
  default_args: string[];
};

type RawCommandAvailability = {
  command: string;
  resolved_path: string | null;
  available: boolean;
};

type RawManagedAgentPrereqs = {
  admin: RawCommandAvailability;
  acp: RawCommandAvailability;
  mcp: RawCommandAvailability;
};

type RawCanvasResponse = {
  content: string | null;
  updated_at: number | null;
  author: string | null;
};

type RawSetCanvasResult = {
  ok: boolean;
  event_id: string;
};

function toTauriError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }

  if (typeof error === "string") {
    return new Error(error);
  }

  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return new Error(error.message);
  }

  try {
    return new Error(JSON.stringify(error));
  } catch {
    return new Error("Unknown Tauri error");
  }
}

export async function invokeTauri<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    throw toTauriError(error);
  }
}

function fromRawChannel(channel: RawChannel): Channel {
  return {
    id: channel.id,
    name: channel.name,
    channelType: channel.channel_type,
    visibility: channel.visibility,
    description: channel.description,
    topic: channel.topic,
    purpose: channel.purpose,
    memberCount: channel.member_count,
    lastMessageAt: channel.last_message_at,
    archivedAt: channel.archived_at,
    participants: channel.participants,
    participantPubkeys: channel.participant_pubkeys,
    isMember: channel.is_member ?? true,
    ttlSeconds: channel.ttl_seconds,
    ttlDeadline: channel.ttl_deadline,
  };
}

function fromRawChannelDetail(channel: RawChannelDetail): ChannelDetail {
  return {
    ...fromRawChannel(channel),
    createdBy: channel.created_by,
    createdAt: channel.created_at,
    updatedAt: channel.updated_at,
    topicSetBy: channel.topic_set_by,
    topicSetAt: channel.topic_set_at,
    purposeSetBy: channel.purpose_set_by,
    purposeSetAt: channel.purpose_set_at,
    topicRequired: channel.topic_required,
    maxMembers: channel.max_members,
    nip29GroupId: channel.nip29_group_id,
  };
}

function fromRawChannelMember(member: RawChannelMember): ChannelMember {
  return {
    pubkey: member.pubkey,
    role: member.role,
    joinedAt: member.joined_at,
    displayName: member.display_name,
  };
}

function fromRawFeedItem(item: RawFeedItem) {
  return {
    id: item.id,
    kind: item.kind,
    pubkey: item.pubkey,
    content: item.content,
    createdAt: item.created_at,
    channelId: item.channel_id,
    channelName: item.channel_name,
    tags: item.tags,
    category: item.category,
  };
}

function fromRawSearchHit(hit: RawSearchHit) {
  return {
    eventId: hit.event_id,
    content: hit.content,
    kind: hit.kind,
    pubkey: hit.pubkey,
    channelId: hit.channel_id,
    channelName: hit.channel_name,
    createdAt: hit.created_at,
    score: hit.score,
  };
}

function fromRawProfile(profile: RawProfile): Profile {
  return {
    pubkey: profile.pubkey,
    displayName: profile.display_name,
    avatarUrl: profile.avatar_url,
    about: profile.about,
    nip05Handle: profile.nip05_handle,
  };
}

function fromRawUserProfileSummary(
  profile: RawUserProfileSummary,
): UserProfileSummary {
  return {
    displayName: profile.display_name,
    avatarUrl: profile.avatar_url,
    nip05Handle: profile.nip05_handle,
  };
}

function fromRawUserSearchResult(user: RawUserSearchResult): UserSearchResult {
  return {
    pubkey: user.pubkey,
    displayName: user.display_name,
    avatarUrl: user.avatar_url,
    nip05Handle: user.nip05_handle,
  };
}

export async function getIdentity(): Promise<Identity> {
  const identity = await invokeTauri<RawIdentity>("get_identity");

  return {
    pubkey: identity.pubkey,
    displayName: identity.display_name,
  };
}

export async function getProfile(): Promise<Profile> {
  const profile = await invokeTauri<RawProfile>("get_profile");
  return fromRawProfile(profile);
}

export async function updateProfile(
  input: UpdateProfileInput,
): Promise<Profile> {
  const profile = await invokeTauri<RawProfile>("update_profile", input);
  return fromRawProfile(profile);
}

export async function getUserProfile(pubkey?: string): Promise<Profile> {
  const profile = await invokeTauri<RawProfile>("get_user_profile", { pubkey });
  return fromRawProfile(profile);
}

export async function getUsersBatch(
  pubkeys: string[],
): Promise<UsersBatchResponse> {
  const response = await invokeTauri<RawUsersBatchResponse>("get_users_batch", {
    pubkeys,
  });

  return {
    profiles: Object.fromEntries(
      Object.entries(response.profiles).map(([pubkey, profile]) => [
        pubkey,
        fromRawUserProfileSummary(profile),
      ]),
    ),
    missing: response.missing,
  };
}

export async function searchUsers(
  query: string,
  limit = 8,
): Promise<UserSearchResult[]> {
  const response = await invokeTauri<RawSearchUsersResponse>("search_users", {
    query,
    limit,
  });
  return response.users.map(fromRawUserSearchResult);
}

export async function getPresence(pubkeys: string[]): Promise<PresenceLookup> {
  const response = await invokeTauri<RawPresenceLookup>("get_presence", {
    pubkeys,
  });

  return Object.fromEntries(
    Object.entries(response).map(([pubkey, status]) => [
      pubkey.toLowerCase(),
      status,
    ]),
  );
}

export async function setPresence(
  status: PresenceStatus,
): Promise<SetPresenceResult> {
  const response = await invokeTauri<RawSetPresenceResult>("set_presence", {
    status,
  });

  return {
    status: response.status,
    ttlSeconds: response.ttl_seconds,
  };
}

export function getRelayWsUrl(): Promise<string> {
  return invokeTauri<string>("get_relay_ws_url");
}

export function getRelayHttpUrl(): Promise<string> {
  return invokeTauri<string>("get_relay_http_url");
}

export async function getChannels(): Promise<Channel[]> {
  const channels = await invokeTauri<RawChannel[]>("get_channels");
  return channels.map(fromRawChannel);
}

export async function createChannel(
  input: CreateChannelInput,
): Promise<Channel> {
  return fromRawChannel(await invokeTauri<RawChannel>("create_channel", input));
}

export async function openDm(input: OpenDmInput): Promise<Channel> {
  return fromRawChannel(await invokeTauri<RawChannel>("open_dm", input));
}

export async function hideDm(channelId: string): Promise<void> {
  await invokeTauri<void>("hide_dm", { channelId });
}

export async function getChannelDetails(
  channelId: string,
): Promise<ChannelDetail> {
  const channel = await invokeTauri<RawChannelDetail>("get_channel_details", {
    channelId,
  });
  return fromRawChannelDetail(channel);
}

export async function getChannelMembers(
  channelId: string,
): Promise<ChannelMember[]> {
  const response = await invokeTauri<RawChannelMembersResponse>(
    "get_channel_members",
    {
      channelId,
    },
  );
  return response.members.map(fromRawChannelMember);
}

export async function updateChannel(
  input: UpdateChannelInput,
): Promise<ChannelDetail> {
  const channel = await invokeTauri<RawChannelDetail>("update_channel", input);
  return fromRawChannelDetail(channel);
}

export async function setChannelTopic(
  input: SetChannelTopicInput,
): Promise<void> {
  await invokeTauri("set_channel_topic", input);
}

export async function setChannelPurpose(
  input: SetChannelPurposeInput,
): Promise<void> {
  await invokeTauri("set_channel_purpose", input);
}

export async function archiveChannel(channelId: string): Promise<void> {
  await invokeTauri("archive_channel", { channelId });
}

export async function unarchiveChannel(channelId: string): Promise<void> {
  await invokeTauri("unarchive_channel", { channelId });
}

export async function deleteChannel(channelId: string): Promise<void> {
  await invokeTauri("delete_channel", { channelId });
}

export async function addChannelMembers(
  input: AddChannelMembersInput,
): Promise<AddChannelMembersResult> {
  return invokeTauri<RawAddChannelMembersResult>("add_channel_members", input);
}

export async function removeChannelMember(
  channelId: string,
  pubkey: string,
): Promise<void> {
  await invokeTauri("remove_channel_member", { channelId, pubkey });
}

export async function joinChannel(channelId: string): Promise<void> {
  await invokeTauri("join_channel", { channelId });
}

export async function leaveChannel(channelId: string): Promise<void> {
  await invokeTauri("leave_channel", { channelId });
}

export async function getCanvas(channelId: string): Promise<CanvasResponse> {
  const response = await invokeTauri<RawCanvasResponse>("get_canvas", {
    channelId,
  });
  return {
    content: response.content,
    updatedAt: response.updated_at,
    author: response.author,
  };
}

export async function setCanvas(
  input: SetCanvasInput,
): Promise<SetCanvasResult> {
  const response = await invokeTauri<RawSetCanvasResult>("set_canvas", {
    channelId: input.channelId,
    content: input.content,
  });
  return {
    ok: response.ok,
    eventId: response.event_id,
  };
}

export async function getHomeFeed(
  input: GetHomeFeedInput = {},
): Promise<HomeFeedResponse> {
  const response = await invokeTauri<RawHomeFeedResponse>("get_feed", input);

  return {
    feed: {
      mentions: response.feed.mentions.map(fromRawFeedItem),
      needsAction: response.feed.needs_action.map(fromRawFeedItem),
      activity: response.feed.activity.map(fromRawFeedItem),
      agentActivity: response.feed.agent_activity.map(fromRawFeedItem),
    },
    meta: {
      since: response.meta.since,
      total: response.meta.total,
      generatedAt: response.meta.generated_at,
    },
  };
}

export async function searchMessages(
  input: SearchMessagesInput,
): Promise<SearchMessagesResponse> {
  const response = await invokeTauri<RawSearchResponse>(
    "search_messages",
    input,
  );

  return {
    hits: response.hits.map(fromRawSearchHit),
    found: response.found,
  };
}

export async function getEventById(eventId: string): Promise<RelayEvent> {
  const eventJson = await invokeTauri<string>("get_event", { eventId });
  return JSON.parse(eventJson) as RelayEvent;
}

export async function sendChannelMessage(
  channelId: string,
  content: string,
  parentEventId?: string | null,
  mediaTags?: string[][],
  mentionPubkeys?: string[],
  kind?: number,
): Promise<SendChannelMessageResult> {
  const response = await invokeTauri<RawSendChannelMessageResult>(
    "send_channel_message",
    {
      channelId,
      content,
      parentEventId,
      mediaTags: mediaTags ?? null,
      mentionPubkeys: mentionPubkeys ?? null,
      kind: kind ?? null,
    },
  );

  return {
    eventId: response.event_id,
    parentEventId: response.parent_event_id,
    rootEventId: response.root_event_id,
    depth: response.depth,
    createdAt: response.created_at,
  };
}

export type BlobDescriptor = {
  url: string;
  sha256: string;
  size: number;
  type: string;
  uploaded: number;
  dim?: string;
  blurhash?: string;
  thumb?: string;
  duration?: number;
};

export async function uploadMedia(
  filePath: string,
  isTemp: boolean,
): Promise<BlobDescriptor> {
  return invokeTauri<BlobDescriptor>("upload_media", {
    filePath,
    isTemp,
  });
}

export async function pickAndUploadMedia(): Promise<BlobDescriptor | null> {
  return invokeTauri<BlobDescriptor | null>("pick_and_upload_media", {});
}

export async function uploadMediaBytes(
  data: number[],
): Promise<BlobDescriptor> {
  return invokeTauri<BlobDescriptor>("upload_media_bytes", { data });
}

export async function editMessage(
  channelId: string,
  eventId: string,
  content: string,
): Promise<void> {
  await invokeTauri("edit_message", { channelId, eventId, content });
}

export async function deleteMessage(eventId: string): Promise<void> {
  await invokeTauri("delete_message", { eventId });
}

export async function addReaction(
  eventId: string,
  emoji: string,
): Promise<void> {
  await invokeTauri("add_reaction", { eventId, emoji });
}

export async function removeReaction(
  eventId: string,
  emoji: string,
): Promise<void> {
  await invokeTauri("remove_reaction", { eventId, emoji });
}

export async function signRelayEvent(input: {
  kind: number;
  content: string;
  tags: string[][];
}): Promise<RelayEvent> {
  const eventJson = await invokeTauri<string>("sign_event", input);
  return JSON.parse(eventJson) as RelayEvent;
}

export async function createAuthEvent(input: {
  challenge: string;
  relayUrl: string;
}): Promise<RelayEvent> {
  const eventJson = await invokeTauri<string>("create_auth_event", input);
  return JSON.parse(eventJson) as RelayEvent;
}

function fromRawToken(token: RawToken): Token {
  return {
    id: token.id,
    name: token.name,
    scopes: token.scopes,
    channelIds: token.channel_ids,
    createdAt: token.created_at,
    expiresAt: token.expires_at,
    lastUsedAt: token.last_used_at,
    revokedAt: token.revoked_at,
  };
}

function fromRawRelayAgent(agent: RawRelayAgent): RelayAgent {
  return {
    pubkey: agent.pubkey,
    name: agent.name,
    agentType: agent.agent_type,
    channels: agent.channels,
    channelIds: agent.channel_ids ?? [],
    capabilities: agent.capabilities,
    status: agent.status,
  };
}

export function fromRawManagedAgent(agent: RawManagedAgent): ManagedAgent {
  return {
    pubkey: agent.pubkey,
    name: agent.name,
    personaId: agent.persona_id,
    relayUrl: agent.relay_url,
    acpCommand: agent.acp_command,
    agentCommand: agent.agent_command,
    agentArgs: agent.agent_args,
    mcpCommand: agent.mcp_command,
    turnTimeoutSeconds: agent.turn_timeout_seconds,
    idleTimeoutSeconds: agent.idle_timeout_seconds,
    maxTurnDurationSeconds: agent.max_turn_duration_seconds,
    parallelism: agent.parallelism,
    systemPrompt: agent.system_prompt,
    model: agent.model,
    hasApiToken: agent.has_api_token,
    status: agent.status,
    pid: agent.pid,
    createdAt: agent.created_at,
    updatedAt: agent.updated_at,
    lastStartedAt: agent.last_started_at,
    lastStoppedAt: agent.last_stopped_at,
    lastExitCode: agent.last_exit_code,
    lastError: agent.last_error,
    logPath: agent.log_path,
    startOnAppLaunch: agent.start_on_app_launch,
    backend: agent.backend,
    backendAgentId: agent.backend_agent_id,
  };
}

function fromRawAcpProvider(provider: RawAcpProvider): AcpProvider {
  return {
    id: provider.id,
    label: provider.label,
    command: provider.command,
    binaryPath: provider.binary_path,
    defaultArgs: provider.default_args,
  };
}

function fromRawCommandAvailability(
  command: RawCommandAvailability,
): CommandAvailability {
  return {
    command: command.command,
    resolvedPath: command.resolved_path,
    available: command.available,
  };
}

export async function listTokens(): Promise<Token[]> {
  const response = await invokeTauri<RawListTokensResponse>("list_tokens");
  return response.tokens.map(fromRawToken);
}

export async function mintToken(
  input: MintTokenInput,
): Promise<MintTokenResponse> {
  const response = await invokeTauri<RawMintTokenResponse>("mint_token", {
    name: input.name,
    scopes: input.scopes,
    channelIds: input.channelIds,
    expiresInDays: input.expiresInDays,
  });
  return {
    id: response.id,
    token: response.token,
    name: response.name,
    scopes: response.scopes,
    channelIds: response.channel_ids,
    createdAt: response.created_at,
    expiresAt: response.expires_at,
  };
}

export async function revokeToken(tokenId: string): Promise<void> {
  await invokeTauri("revoke_token", { tokenId });
}

export async function revokeAllTokens(): Promise<{ revokedCount: number }> {
  const response = await invokeTauri<{ revoked_count: number }>(
    "revoke_all_tokens",
  );
  return { revokedCount: response.revoked_count };
}

export async function listRelayAgents(): Promise<RelayAgent[]> {
  return (await invokeTauri<RawRelayAgent[]>("list_relay_agents")).map(
    fromRawRelayAgent,
  );
}

export async function listManagedAgents(): Promise<ManagedAgent[]> {
  return (await invokeTauri<RawManagedAgent[]>("list_managed_agents")).map(
    fromRawManagedAgent,
  );
}

export async function createManagedAgent(input: CreateManagedAgentInput) {
  const response = await invokeTauri<RawCreateManagedAgentResponse>(
    "create_managed_agent",
    {
      input: {
        name: input.name,
        personaId: input.personaId,
        relayUrl: input.relayUrl,
        acpCommand: input.acpCommand,
        agentCommand: input.agentCommand,
        agentArgs: input.agentArgs,
        mcpCommand: input.mcpCommand,
        turnTimeoutSeconds: input.turnTimeoutSeconds,
        idleTimeoutSeconds: input.idleTimeoutSeconds,
        maxTurnDurationSeconds: input.maxTurnDurationSeconds,
        parallelism: input.parallelism,
        systemPrompt: input.systemPrompt,
        avatarUrl: input.avatarUrl,
        model: input.model,
        mintToken: input.mintToken,
        tokenScopes: input.tokenScopes,
        tokenName: input.tokenName,
        spawnAfterCreate: input.spawnAfterCreate,
        startOnAppLaunch: input.startOnAppLaunch,
        backend: input.backend,
      },
    },
  );

  return {
    agent: fromRawManagedAgent(response.agent),
    privateKeyNsec: response.private_key_nsec,
    apiToken: response.api_token,
    profileSyncError: response.profile_sync_error,
    spawnError: response.spawn_error,
  };
}

export async function startManagedAgent(pubkey: string): Promise<ManagedAgent> {
  const response = await invokeTauri<RawManagedAgent>("start_managed_agent", {
    pubkey,
  });
  return fromRawManagedAgent(response);
}

export async function stopManagedAgent(pubkey: string): Promise<ManagedAgent> {
  const response = await invokeTauri<RawManagedAgent>("stop_managed_agent", {
    pubkey,
  });
  return fromRawManagedAgent(response);
}

export async function deleteManagedAgent(
  pubkey: string,
  forceRemoteDelete?: boolean,
): Promise<void> {
  await invokeTauri("delete_managed_agent", {
    pubkey,
    forceRemoteDelete: forceRemoteDelete ?? null,
  });
}

export async function mintManagedAgentToken(input: MintManagedAgentTokenInput) {
  const response = await invokeTauri<RawMintManagedAgentTokenResponse>(
    "mint_managed_agent_token",
    {
      input: {
        pubkey: input.pubkey,
        tokenName: input.tokenName,
        scopes: input.scopes,
      },
    },
  );

  return {
    agent: fromRawManagedAgent(response.agent),
    token: response.token,
  };
}

export async function getManagedAgentLog(pubkey: string, lineCount?: number) {
  const response = await invokeTauri<RawManagedAgentLog>(
    "get_managed_agent_log",
    {
      pubkey,
      lineCount,
    },
  );

  return {
    content: response.content,
    logPath: response.log_path,
  };
}

export async function discoverAcpProviders(): Promise<AcpProvider[]> {
  return (await invokeTauri<RawAcpProvider[]>("discover_acp_providers")).map(
    fromRawAcpProvider,
  );
}

export async function discoverManagedAgentPrereqs(input: {
  acpCommand?: string;
  mcpCommand?: string;
}) {
  const response = await invokeTauri<RawManagedAgentPrereqs>(
    "discover_managed_agent_prereqs",
    {
      input: {
        acpCommand: input.acpCommand,
        mcpCommand: input.mcpCommand,
      },
    },
  );

  return {
    admin: fromRawCommandAvailability(response.admin),
    acp: fromRawCommandAvailability(response.acp),
    mcp: fromRawCommandAvailability(response.mcp),
  };
}

// ── Model discovery ───────────────────────────────────────────────────────────

export async function getAgentModels(pubkey: string) {
  return invokeTauri<AgentModelsResponse>("get_agent_models", { pubkey });
}

export async function updateManagedAgent(
  input: UpdateManagedAgentInput,
): Promise<ManagedAgent> {
  const response = await invokeTauri<RawManagedAgent>("update_managed_agent", {
    input,
  });
  return fromRawManagedAgent(response);
}

// ── Backend provider discovery ────────────────────────────────────────────────

export async function discoverBackendProviders(): Promise<
  BackendProviderCandidate[]
> {
  return invokeTauri<BackendProviderCandidate[]>("discover_backend_providers");
}

export async function probeBackendProvider(
  binaryPath: string,
): Promise<BackendProviderProbeResult> {
  return invokeTauri<BackendProviderProbeResult>("probe_backend_provider", {
    binaryPath,
  });
}
