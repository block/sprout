import { hexToBytes } from "@noble/hashes/utils.js";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { finalizeEvent } from "nostr-tools/pure";

import type { RelayEvent } from "@/shared/api/types";

type TestIdentity = {
  privateKey: string;
  pubkey: string;
  username: string;
};

type E2eConfig = {
  mode?: "mock" | "relay";
  mock?: {
    mintTokenError?: string;
    seededTokens?: RawMockTokenSeed[];
  };
  relayHttpUrl?: string;
  relayWsUrl?: string;
  identity?: TestIdentity;
};

type RawMockTokenSeed = {
  id: string;
  name: string;
  scopes: string[];
  channel_ids: string[];
  created_at: string;
  expires_at: string | null;
  last_used_at: string | null;
  revoked_at: string | null;
  token?: string;
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

type PresenceStatus = "online" | "away" | "offline";

type RawPresenceLookup = Record<string, PresenceStatus>;

type RawSetPresenceResponse = {
  status: PresenceStatus;
  ttl_seconds: number;
};

type RawOpenDmResponse = {
  channel_id: string;
  created: boolean;
  participants: string[];
};

type RawChannel = {
  id: string;
  name: string;
  channel_type: "stream" | "forum" | "dm";
  visibility: "open" | "private";
  description: string;
  topic: string | null;
  purpose: string | null;
  member_count: number;
  last_message_at: string | null;
  archived_at: string | null;
  participants: string[];
  participant_pubkeys: string[];
};

type RawChannelWithMembership = RawChannel & {
  is_member: boolean;
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
  role: "owner" | "admin" | "member" | "guest" | "bot";
  joined_at: string;
  display_name: string | null;
};

type RawChannelMembersResponse = {
  members: RawChannelMember[];
  next_cursor: string | null;
};

type RawAddChannelMembersResponse = {
  added: string[];
  errors: Array<{
    pubkey: string;
    error: string;
  }>;
};

type MockChannel = RawChannelDetail & {
  members: RawChannelMember[];
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
  channel_id: string;
  channel_name: string;
  created_at: number;
  score: number;
};

type RawSearchResponse = {
  hits: RawSearchHit[];
  found: number;
};

type RawSendChannelMessageResponse = {
  event_id: string;
  parent_event_id: string | null;
  root_event_id: string | null;
  depth: number;
  created_at: number;
};

type RawToken = {
  id: string;
  name: string;
  scopes: string[];
  channel_ids: string[];
  created_at: string;
  expires_at: string | null;
  last_used_at: string | null;
  revoked_at: string | null;
};

type RawListTokensResponse = {
  tokens: RawToken[];
};

type RawMintTokenResponse = RawToken & {
  token: string;
};

type RawRevokeAllTokensResponse = {
  revoked_count: number;
};

type RawRelayAgent = {
  pubkey: string;
  name: string;
  agent_type: string;
  channels: string[];
  capabilities: string[];
  status: PresenceStatus;
};

type RawManagedAgent = {
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
  status: "running" | "stopped" | "deployed" | "not_deployed";
  pid: number | null;
  created_at: string;
  updated_at: string;
  last_started_at: string | null;
  last_stopped_at: string | null;
  last_exit_code: number | null;
  last_error: string | null;
  log_path: string;
  start_on_app_launch: boolean;
  backend:
    | { type: "local" }
    | { type: "provider"; id: string; config: Record<string, unknown> };
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

type RawPersona = {
  id: string;
  display_name: string;
  avatar_url: string | null;
  system_prompt: string;
  is_builtin: boolean;
  created_at: string;
  updated_at: string;
};

type MockToken = RawToken & {
  token: string;
};

type MockManagedAgent = RawManagedAgent & {
  private_key_nsec: string;
  api_token: string | null;
  log_lines: string[];
};

type WsHandler = (message: unknown) => void;
const GLOBAL_MOCK_SUBSCRIPTION = "*";

type MockSocket = {
  handler: WsHandler;
  subscriptions: Map<string, string>;
};

declare global {
  interface Window {
    __SPROUT_E2E__?: E2eConfig;
    __SPROUT_E2E_COMMANDS__?: string[];
    __SPROUT_E2E_WEBVIEW_ZOOM__?: number;
    __SPROUT_E2E_EMIT_MOCK_MESSAGE__?: (input: {
      channelName: string;
      content: string;
      pubkey?: string;
    }) => RelayEvent;
    __SPROUT_E2E_EMIT_MOCK_TYPING__?: (input: {
      channelName: string;
      pubkey?: string;
    }) => RelayEvent;
    __SPROUT_E2E_PUSH_MOCK_FEED_ITEM__?: (item: RawFeedItem) => RawFeedItem;
  }
}

const DEFAULT_RELAY_HTTP_URL = "http://localhost:3000";
const DEFAULT_RELAY_WS_URL = "ws://localhost:3000";
const DEFAULT_MOCK_IDENTITY = {
  pubkey: "deadbeef".repeat(8),
  display_name: "npub1mock...",
};
const DEFAULT_REAL_IDENTITY = {
  privateKey:
    "3dbaebadb5dfd777ff25149ee230d907a15a9e1294b40b830661e65bb42f6c03",
  pubkey: "e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34",
  username: "tyler",
} satisfies TestIdentity;

const ALICE_PUBKEY =
  "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f";
const BOB_PUBKEY =
  "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260";
const CHARLIE_PUBKEY =
  "554cef57437abac34522ac2c9f0490d685b72c80478cf9f7ed6f9570ee8624ea";
const OUTSIDER_PUBKEY =
  "df8e91b86fda13a9a67896df77232f7bdab2ba9c3e165378e1ba3d24c13a328e";
const MOCK_IDENTITY_PUBKEY = DEFAULT_MOCK_IDENTITY.pubkey;
const MOCK_PRESENCE_TTL_SECONDS = 90;

const mockDisplayNames = new Map<string, string>([
  [MOCK_IDENTITY_PUBKEY, DEFAULT_MOCK_IDENTITY.display_name],
  [ALICE_PUBKEY, "alice"],
  [BOB_PUBKEY, "bob"],
  [CHARLIE_PUBKEY, "charlie"],
  [OUTSIDER_PUBKEY, "outsider"],
  [DEFAULT_REAL_IDENTITY.pubkey, DEFAULT_REAL_IDENTITY.username],
]);

function isoMinutesAgo(minutesAgo: number): string {
  return new Date(Date.now() - minutesAgo * 60_000).toISOString();
}

function cloneMembers(members: RawChannelMember[]): RawChannelMember[] {
  return members.map((member) => ({ ...member }));
}

function toRawChannel(channel: MockChannel): RawChannelWithMembership {
  return {
    id: channel.id,
    name: channel.name,
    channel_type: channel.channel_type,
    visibility: channel.visibility,
    description: channel.description,
    topic: channel.topic,
    purpose: channel.purpose,
    member_count: channel.member_count,
    last_message_at: channel.last_message_at,
    archived_at: channel.archived_at,
    participants: [...channel.participants],
    participant_pubkeys: [...channel.participant_pubkeys],
    is_member: channel.members.some(
      (member) =>
        member.pubkey === MOCK_IDENTITY_PUBKEY ||
        member.pubkey === DEFAULT_REAL_IDENTITY.pubkey,
    ),
  };
}

function toRawChannelDetail(channel: MockChannel): RawChannelDetail {
  return {
    ...toRawChannel(channel),
    created_by: channel.created_by,
    created_at: channel.created_at,
    updated_at: channel.updated_at,
    topic_set_by: channel.topic_set_by,
    topic_set_at: channel.topic_set_at,
    purpose_set_by: channel.purpose_set_by,
    purpose_set_at: channel.purpose_set_at,
    topic_required: channel.topic_required,
    max_members: channel.max_members,
    nip29_group_id: channel.nip29_group_id,
  };
}

function createMockMember(
  pubkey: string,
  role: RawChannelMember["role"],
  joinedMinutesAgo: number,
): RawChannelMember {
  return {
    pubkey,
    role,
    joined_at: isoMinutesAgo(joinedMinutesAgo),
    display_name: mockDisplayNames.get(pubkey) ?? null,
  };
}

function createMockChannel(
  seed: Omit<
    MockChannel,
    | "created_at"
    | "member_count"
    | "members"
    | "updated_at"
    | "participant_pubkeys"
    | "participants"
  > & {
    created_minutes_ago: number;
    members: RawChannelMember[];
    participant_pubkeys?: string[];
    participants?: string[];
    updated_minutes_ago?: number;
  },
): MockChannel {
  return {
    ...seed,
    created_at: isoMinutesAgo(seed.created_minutes_ago),
    member_count: seed.members.length,
    members: cloneMembers(seed.members),
    participant_pubkeys: [...(seed.participant_pubkeys ?? [])],
    participants: [...(seed.participants ?? [])],
    updated_at: isoMinutesAgo(
      seed.updated_minutes_ago ?? seed.created_minutes_ago,
    ),
  };
}

function syncMockChannel(channel: MockChannel) {
  channel.member_count = channel.members.length;

  if (channel.channel_type !== "dm") {
    return;
  }

  channel.participant_pubkeys = channel.members.map((member) => member.pubkey);
  channel.participants = channel.members.map(
    (member) => member.display_name ?? member.pubkey.slice(0, 8),
  );
}

function touchMockChannel(channel: MockChannel) {
  channel.updated_at = new Date().toISOString();
}

function normalizeParticipantPubkeys(pubkeys: string[]) {
  return [...new Set(pubkeys.map((pubkey) => pubkey.toLowerCase()))].sort();
}

function findMockDmByParticipantPubkeys(pubkeys: string[]) {
  const normalizedPubkeys = normalizeParticipantPubkeys(pubkeys);

  return (
    mockChannels.find((channel) => {
      if (channel.channel_type !== "dm") {
        return false;
      }

      const channelPubkeys = normalizeParticipantPubkeys(
        channel.participant_pubkeys,
      );

      return (
        channelPubkeys.length === normalizedPubkeys.length &&
        channelPubkeys.every(
          (pubkey, index) => pubkey === normalizedPubkeys[index],
        )
      );
    }) ?? null
  );
}

function getMockIdentity() {
  return {
    pubkey: MOCK_IDENTITY_PUBKEY,
    displayName: DEFAULT_MOCK_IDENTITY.display_name,
  };
}

function cloneProfile(profile: RawProfile): RawProfile {
  return { ...profile };
}

function cloneToken(token: RawToken): RawToken {
  return {
    ...token,
    channel_ids: [...token.channel_ids],
    scopes: [...token.scopes],
  };
}

function cloneMintedToken(token: MockToken): RawMintTokenResponse {
  return {
    ...cloneToken(token),
    token: token.token,
  };
}

function cloneRelayAgent(agent: RawRelayAgent): RawRelayAgent {
  return {
    ...agent,
    channels: [...agent.channels],
    capabilities: [...agent.capabilities],
  };
}

function cloneManagedAgent(agent: MockManagedAgent): RawManagedAgent {
  return {
    pubkey: agent.pubkey,
    name: agent.name,
    persona_id: agent.persona_id,
    relay_url: agent.relay_url,
    acp_command: agent.acp_command,
    agent_command: agent.agent_command,
    agent_args: [...agent.agent_args],
    mcp_command: agent.mcp_command,
    turn_timeout_seconds: agent.turn_timeout_seconds,
    idle_timeout_seconds: agent.idle_timeout_seconds ?? null,
    max_turn_duration_seconds: agent.max_turn_duration_seconds ?? null,
    parallelism: agent.parallelism,
    system_prompt: agent.system_prompt,
    model: agent.model,
    has_api_token: agent.has_api_token,
    status: agent.status,
    pid: agent.pid,
    created_at: agent.created_at,
    updated_at: agent.updated_at,
    last_started_at: agent.last_started_at,
    last_stopped_at: agent.last_stopped_at,
    last_exit_code: agent.last_exit_code,
    last_error: agent.last_error,
    log_path: agent.log_path,
    start_on_app_launch: agent.start_on_app_launch,
    backend: agent.backend ?? { type: "local" as const },
    backend_agent_id: agent.backend_agent_id ?? null,
  };
}

function toMockToken(seed: RawMockTokenSeed): MockToken {
  return {
    ...cloneToken(seed),
    token:
      seed.token ??
      `spr_tok_mock_${seed.id.replace(/[^a-zA-Z0-9]/g, "").slice(0, 24)}`,
  };
}

function resetMockTokens(config: E2eConfig | undefined) {
  mockTokens = (config?.mock?.seededTokens ?? []).map(toMockToken);
  mockMintTokenError = config?.mock?.mintTokenError ?? null;
}

function resetMockManagedAgents() {
  mockManagedAgents = [];
  syncMockRelayAgentsFromManagedAgents();
}

function resetMockPersonas() {
  const now = new Date().toISOString();
  mockPersonas = [
    {
      id: "builtin:orchestrator",
      display_name: "Orchestrator",
      avatar_url: null,
      system_prompt:
        "You are an orchestration agent. Coordinate multi-step work across specialized agents, keep the overall plan moving, and synthesize results into a clear final outcome. When another agent should take a task, @mention them explicitly with the assignment, expected deliverable, and any relevant constraints or deadlines.",
      is_builtin: true,
      created_at: now,
      updated_at: now,
    },
    {
      id: "builtin:researcher",
      display_name: "Researcher",
      avatar_url: null,
      system_prompt:
        "You are a research agent. Gather relevant information, compare sources, call out uncertainty, and return concise findings with evidence.",
      is_builtin: true,
      created_at: now,
      updated_at: now,
    },
    {
      id: "builtin:planner",
      display_name: "Planner",
      avatar_url: null,
      system_prompt:
        "You are a planning agent. Turn ambiguous requests into structured plans with milestones, dependencies, risks, and clear next actions. Do not implement the work yourself unless asked.",
      is_builtin: true,
      created_at: now,
      updated_at: now,
    },
    {
      id: "builtin:implementer",
      display_name: "Builder",
      avatar_url: null,
      system_prompt:
        "You are a builder agent. Execute tasks directly, make code and configuration changes carefully, validate the result, and explain important decisions and follow-up items.",
      is_builtin: true,
      created_at: now,
      updated_at: now,
    },
    {
      id: "builtin:refactor",
      display_name: "Refactor",
      avatar_url: null,
      system_prompt:
        "You are a refactoring agent. Improve structure, naming, duplication, and module boundaries without changing externally observable behavior. Keep changes incremental, preserve compatibility, and add or update validation when behavior could drift.",
      is_builtin: true,
      created_at: now,
      updated_at: now,
    },
    {
      id: "builtin:reviewer",
      display_name: "Reviewer",
      avatar_url: null,
      system_prompt:
        "You are a review agent. Inspect plans, code, and outputs for bugs, regressions, edge cases, security issues, and missing tests. Prioritize findings by severity, cite concrete evidence, and keep summaries secondary to the actual review.",
      is_builtin: true,
      created_at: now,
      updated_at: now,
    },
  ];
}

function getMockProfileByPubkey(pubkey: string): RawProfile | null {
  const normalizedPubkey = pubkey.toLowerCase();
  const existing = mockProfiles.get(normalizedPubkey);
  if (existing) {
    return existing;
  }

  if (!mockDisplayNames.has(normalizedPubkey)) {
    return null;
  }

  return {
    pubkey: normalizedPubkey,
    display_name: mockDisplayNames.get(normalizedPubkey) ?? null,
    avatar_url: null,
    about: null,
    nip05_handle: null,
  };
}

function listMockProfiles(): RawProfile[] {
  const pubkeys = new Set<string>([
    ...mockProfiles.keys(),
    ...mockDisplayNames.keys(),
    DEFAULT_REAL_IDENTITY.pubkey,
  ]);

  return [...pubkeys]
    .map((pubkey) => getMockProfileByPubkey(pubkey))
    .filter((profile): profile is RawProfile => profile !== null);
}

function listMockChannels(): RawChannelWithMembership[] {
  return mockChannels.map(toRawChannel);
}

function getMockChannel(channelId: string): MockChannel {
  const channel = mockChannels.find((candidate) => candidate.id === channelId);
  if (!channel) {
    throw new Error(`Channel ${channelId} not found.`);
  }

  return channel;
}

function getMockMemberPubkey(config: E2eConfig | undefined): string {
  return getIdentity(config)?.pubkey ?? getMockIdentity().pubkey;
}

function getMockMemberDisplayName(config: E2eConfig | undefined): string {
  return getIdentity(config)?.username ?? getMockIdentity().displayName;
}

function createCurrentMember(
  config: E2eConfig | undefined,
  role: RawChannelMember["role"],
): RawChannelMember {
  return {
    pubkey: getMockMemberPubkey(config),
    role,
    joined_at: new Date().toISOString(),
    display_name: getMockMemberDisplayName(config),
  };
}

const mockChannels: MockChannel[] = [
  createMockChannel({
    id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
    name: "general",
    channel_type: "stream",
    visibility: "open",
    description: "General discussion for everyone",
    topic: "Company-wide updates",
    purpose: "Coordinate day-to-day work and unblock the team.",
    last_message_at: isoMinutesAgo(5),
    archived_at: null,
    created_by: MOCK_IDENTITY_PUBKEY,
    topic_set_by: MOCK_IDENTITY_PUBKEY,
    topic_set_at: isoMinutesAgo(90),
    purpose_set_by: MOCK_IDENTITY_PUBKEY,
    purpose_set_at: isoMinutesAgo(80),
    topic_required: false,
    max_members: null,
    nip29_group_id: null,
    created_minutes_ago: 1440,
    updated_minutes_ago: 5,
    members: [
      createMockMember(MOCK_IDENTITY_PUBKEY, "owner", 1440),
      createMockMember(ALICE_PUBKEY, "admin", 1200),
      createMockMember(BOB_PUBKEY, "member", 960),
    ],
  }),
  createMockChannel({
    id: "9dae0116-799b-5071-a0a8-fdd30a91a35d",
    name: "random",
    channel_type: "stream",
    visibility: "open",
    description: "Off-topic, fun stuff",
    topic: null,
    purpose: null,
    last_message_at: null,
    archived_at: null,
    created_by: ALICE_PUBKEY,
    topic_set_by: null,
    topic_set_at: null,
    purpose_set_by: null,
    purpose_set_at: null,
    topic_required: false,
    max_members: null,
    nip29_group_id: null,
    created_minutes_ago: 1400,
    updated_minutes_ago: 1400,
    members: [
      createMockMember(ALICE_PUBKEY, "owner", 1400),
      createMockMember(MOCK_IDENTITY_PUBKEY, "member", 1300),
      createMockMember(BOB_PUBKEY, "member", 1000),
    ],
  }),
  createMockChannel({
    id: "b5e2f8a1-3c44-5912-9e67-4a8d1f2b3c4e",
    name: "design",
    channel_type: "stream",
    visibility: "open",
    description: "Design system and UX discussions",
    topic: null,
    purpose: null,
    last_message_at: isoMinutesAgo(120),
    archived_at: null,
    created_by: ALICE_PUBKEY,
    topic_set_by: null,
    topic_set_at: null,
    purpose_set_by: null,
    purpose_set_at: null,
    topic_required: false,
    max_members: null,
    nip29_group_id: null,
    created_minutes_ago: 1350,
    updated_minutes_ago: 120,
    members: [
      createMockMember(ALICE_PUBKEY, "owner", 1350),
      createMockMember(BOB_PUBKEY, "member", 1100),
    ],
  }),
  createMockChannel({
    id: "c6f3a9b2-4d55-5a23-bf78-5b9e2g3c5d6f",
    name: "sales",
    channel_type: "stream",
    visibility: "open",
    description: "Sales team coordination and pipeline updates",
    topic: "Q1 targets",
    purpose: null,
    last_message_at: isoMinutesAgo(30),
    archived_at: null,
    created_by: BOB_PUBKEY,
    topic_set_by: BOB_PUBKEY,
    topic_set_at: isoMinutesAgo(200),
    purpose_set_by: null,
    purpose_set_at: null,
    topic_required: false,
    max_members: null,
    nip29_group_id: null,
    created_minutes_ago: 1300,
    updated_minutes_ago: 30,
    members: [
      createMockMember(BOB_PUBKEY, "owner", 1300),
      createMockMember(CHARLIE_PUBKEY, "member", 900),
    ],
  }),
  createMockChannel({
    id: "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9",
    name: "engineering",
    channel_type: "stream",
    visibility: "open",
    description: "Engineering discussions",
    topic: "Desktop release train",
    purpose: "Track implementation details and release readiness.",
    last_message_at: isoMinutesAgo(42),
    archived_at: null,
    created_by: ALICE_PUBKEY,
    topic_set_by: ALICE_PUBKEY,
    topic_set_at: isoMinutesAgo(120),
    purpose_set_by: ALICE_PUBKEY,
    purpose_set_at: isoMinutesAgo(130),
    topic_required: false,
    max_members: null,
    nip29_group_id: null,
    created_minutes_ago: 1320,
    updated_minutes_ago: 42,
    members: [
      createMockMember(ALICE_PUBKEY, "owner", 1320),
      createMockMember(MOCK_IDENTITY_PUBKEY, "member", 1180),
      createMockMember(BOB_PUBKEY, "member", 900),
    ],
  }),
  createMockChannel({
    id: "94a444a4-c0a3-5966-ab05-530c6ddc2301",
    name: "agents",
    channel_type: "stream",
    visibility: "open",
    description: "AI agent testing and collaboration",
    topic: "Coordination board",
    purpose: "Track agent work and relay activity.",
    last_message_at: isoMinutesAgo(15),
    archived_at: null,
    created_by: MOCK_IDENTITY_PUBKEY,
    topic_set_by: MOCK_IDENTITY_PUBKEY,
    topic_set_at: isoMinutesAgo(60),
    purpose_set_by: MOCK_IDENTITY_PUBKEY,
    purpose_set_at: isoMinutesAgo(65),
    topic_required: false,
    max_members: null,
    nip29_group_id: null,
    created_minutes_ago: 1000,
    updated_minutes_ago: 15,
    members: [
      createMockMember(MOCK_IDENTITY_PUBKEY, "owner", 1000),
      createMockMember(CHARLIE_PUBKEY, "bot", 800),
    ],
  }),
  createMockChannel({
    id: "a27e1ee9-76a6-5bdf-a5d5-1d85610dad11",
    name: "watercooler",
    channel_type: "forum",
    visibility: "open",
    description: "Casual forum for async discussions",
    topic: null,
    purpose: null,
    last_message_at: null,
    archived_at: null,
    created_by: ALICE_PUBKEY,
    topic_set_by: null,
    topic_set_at: null,
    purpose_set_by: null,
    purpose_set_at: null,
    topic_required: false,
    max_members: null,
    nip29_group_id: null,
    created_minutes_ago: 900,
    updated_minutes_ago: 900,
    members: [
      createMockMember(ALICE_PUBKEY, "owner", 900),
      createMockMember(MOCK_IDENTITY_PUBKEY, "member", 750),
    ],
  }),
  createMockChannel({
    id: "1be1dcdb-4c31-5a8c-81de-ac102552ca10",
    name: "announcements",
    channel_type: "forum",
    visibility: "private",
    description: "Company announcements",
    topic: "Leadership updates",
    purpose: "Read-only announcements for the workspace.",
    last_message_at: null,
    archived_at: null,
    created_by: ALICE_PUBKEY,
    topic_set_by: ALICE_PUBKEY,
    topic_set_at: isoMinutesAgo(200),
    purpose_set_by: ALICE_PUBKEY,
    purpose_set_at: isoMinutesAgo(210),
    topic_required: false,
    max_members: null,
    nip29_group_id: null,
    created_minutes_ago: 880,
    updated_minutes_ago: 200,
    members: [
      createMockMember(ALICE_PUBKEY, "owner", 880),
      createMockMember(MOCK_IDENTITY_PUBKEY, "guest", 700),
    ],
  }),
  createMockChannel({
    id: "f48efb06-0c93-5025-aac9-2e646bb6bfa8",
    name: "alice-tyler",
    channel_type: "dm",
    visibility: "private",
    description: "DM between alice and tyler",
    topic: null,
    purpose: null,
    last_message_at: null,
    archived_at: null,
    created_by: ALICE_PUBKEY,
    topic_set_by: null,
    topic_set_at: null,
    purpose_set_by: null,
    purpose_set_at: null,
    topic_required: false,
    max_members: 2,
    nip29_group_id: null,
    created_minutes_ago: 720,
    updated_minutes_ago: 720,
    participants: ["alice", "tyler"],
    participant_pubkeys: [ALICE_PUBKEY, DEFAULT_REAL_IDENTITY.pubkey],
    members: [
      createMockMember(ALICE_PUBKEY, "member", 720),
      createMockMember(DEFAULT_REAL_IDENTITY.pubkey, "member", 720),
    ],
  }),
  createMockChannel({
    id: "7eb9f239-9393-50b0-bd76-d85eef0511c7",
    name: "bob-tyler",
    channel_type: "dm",
    visibility: "private",
    description: "DM between bob and tyler",
    topic: null,
    purpose: null,
    last_message_at: null,
    archived_at: null,
    created_by: BOB_PUBKEY,
    topic_set_by: null,
    topic_set_at: null,
    purpose_set_by: null,
    purpose_set_at: null,
    topic_required: false,
    max_members: 2,
    nip29_group_id: null,
    created_minutes_ago: 700,
    updated_minutes_ago: 700,
    participants: ["bob", "tyler"],
    participant_pubkeys: [BOB_PUBKEY, DEFAULT_REAL_IDENTITY.pubkey],
    members: [
      createMockMember(BOB_PUBKEY, "member", 700),
      createMockMember(DEFAULT_REAL_IDENTITY.pubkey, "member", 700),
    ],
  }),
];

const mockMessages = new Map<string, RelayEvent[]>();
const mockSockets = new Map<number, MockSocket>();
const realSockets = new Map<number, WebSocket>();
let mockTokens: MockToken[] = [];
let mockMintTokenError: string | null = null;
let mockManagedAgents: MockManagedAgent[] = [];
let mockPersonas: RawPersona[] = [];
let mockRelayAgents: RawRelayAgent[] = [
  {
    pubkey: ALICE_PUBKEY,
    name: "alice",
    agent_type: "goose",
    channels: ["general", "agents"],
    capabilities: ["search", "summaries", "workflows"],
    status: "online",
  },
  {
    pubkey: CHARLIE_PUBKEY,
    name: "charlie",
    agent_type: "codex",
    channels: ["general"],
    capabilities: ["code", "reviews"],
    status: "away",
  },
];
const mockProfiles = new Map<string, RawProfile>([
  [
    MOCK_IDENTITY_PUBKEY,
    {
      pubkey: MOCK_IDENTITY_PUBKEY,
      display_name: DEFAULT_MOCK_IDENTITY.display_name,
      avatar_url: null,
      about: null,
      nip05_handle: null,
    },
  ],
]);
const mockPresence = new Map<string, PresenceStatus>([
  [MOCK_IDENTITY_PUBKEY, "offline"],
  [DEFAULT_REAL_IDENTITY.pubkey, "offline"],
  [ALICE_PUBKEY, "online"],
  [BOB_PUBKEY, "away"],
  [CHARLIE_PUBKEY, "online"],
  [OUTSIDER_PUBKEY, "offline"],
]);
const mockFeedOverrides: RawHomeFeedResponse["feed"] = {
  mentions: [],
  needs_action: [],
  activity: [],
  agent_activity: [],
};

let installed = false;
let nextSocketId = 1;

function syncMockRelayAgentsFromManagedAgents() {
  const baseAgents = mockRelayAgents.filter(
    (agent) =>
      !mockManagedAgents.some((managed) => managed.pubkey === agent.pubkey),
  );
  const managedAgentsAsRelay: RawRelayAgent[] = mockManagedAgents.map(
    (agent) => ({
      pubkey: agent.pubkey,
      name: agent.name,
      agent_type: agent.agent_command,
      channels: ["agents"],
      capabilities: ["messages", "channels", "mcp"],
      status: agent.status === "running" ? "online" : "offline",
    }),
  );

  mockRelayAgents = [...baseAgents, ...managedAgentsAsRelay];
}

function getConfig(): E2eConfig | undefined {
  return window.__SPROUT_E2E__;
}

function isRelayMode(config: E2eConfig | undefined): boolean {
  return config?.mode === "relay";
}

function getRelayHttpUrl(config: E2eConfig | undefined): string {
  return config?.relayHttpUrl ?? DEFAULT_RELAY_HTTP_URL;
}

function getRelayWsUrl(config: E2eConfig | undefined): string {
  return config?.relayWsUrl ?? DEFAULT_RELAY_WS_URL;
}

function getIdentity(config: E2eConfig | undefined): TestIdentity | undefined {
  if (!isRelayMode(config)) {
    return undefined;
  }

  return config?.identity ?? DEFAULT_REAL_IDENTITY;
}

function ensureMockProfile(config: E2eConfig | undefined): RawProfile {
  const pubkey = getMockMemberPubkey(config);
  const existing = mockProfiles.get(pubkey);
  if (existing) {
    return existing;
  }

  const profile = {
    pubkey,
    display_name: getMockMemberDisplayName(config),
    avatar_url: null,
    about: null,
    nip05_handle: null,
  };
  mockProfiles.set(pubkey, profile);
  return profile;
}

function applyMockDisplayName(pubkey: string, displayName: string | null) {
  if (displayName) {
    mockDisplayNames.set(pubkey, displayName);
  } else {
    mockDisplayNames.delete(pubkey);
  }

  for (const channel of mockChannels) {
    for (const member of channel.members) {
      if (member.pubkey === pubkey) {
        member.display_name = displayName;
      }
    }
    syncMockChannel(channel);
  }
}

function getMockPresenceStatus(pubkey: string): PresenceStatus {
  return mockPresence.get(pubkey.toLowerCase()) ?? "offline";
}

function setMockPresenceStatus(pubkey: string, status: PresenceStatus) {
  mockPresence.set(pubkey.toLowerCase(), status);
}

function resolveHandler(handler: unknown): WsHandler {
  if (typeof handler === "function") {
    return handler as WsHandler;
  }

  if (
    typeof handler === "object" &&
    handler !== null &&
    "onmessage" in handler &&
    typeof handler.onmessage === "function"
  ) {
    return handler.onmessage as WsHandler;
  }

  throw new Error("Invalid websocket message handler.");
}

function sendWsText(handler: WsHandler, payload: unknown[]) {
  handler({
    type: "Text",
    data: JSON.stringify(payload),
  });
}

function sendWsClose(handler: WsHandler) {
  handler({
    type: "Close",
  });
}

function getChannelIdFromTags(tags: string[][]): string | undefined {
  return tags.find((tag) => tag[0] === "h")?.[1];
}

function getThreadReferenceFromTags(tags: string[][]) {
  const eventTags = tags.filter(
    (tag) => tag[0] === "e" && typeof tag[1] === "string",
  );

  if (eventTags.length === 0) {
    return {
      parentEventId: null,
      rootEventId: null,
    };
  }

  const rootTag = eventTags.find((tag) => tag[3] === "root");
  const replyTag =
    [...eventTags].reverse().find((tag) => tag[3] === "reply") ?? null;

  if (!replyTag) {
    return {
      parentEventId: null,
      rootEventId: null,
    };
  }

  return {
    parentEventId: replyTag[1] ?? null,
    rootEventId: rootTag?.[1] ?? replyTag[1] ?? null,
  };
}

function getMockMessageStore(channelId: string): RelayEvent[] {
  const existing = mockMessages.get(channelId);
  if (existing) {
    return existing;
  }

  const seeded: RelayEvent[] =
    channelId === "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50"
      ? [
          {
            id: "mock-general-welcome",
            pubkey: DEFAULT_MOCK_IDENTITY.pubkey,
            created_at: Math.floor(Date.now() / 1000),
            kind: 9,
            tags: [["h", channelId]],
            content: "Welcome to #general",
            sig: "mocksig".repeat(20).slice(0, 128),
          },
        ]
      : [];

  mockMessages.set(channelId, seeded);
  return seeded;
}

function emitMockHistory(socket: MockSocket, subId: string, channelId: string) {
  const events = getMockMessageStore(channelId);
  for (const event of events) {
    sendWsText(socket.handler, ["EVENT", subId, event]);
  }
  sendWsText(socket.handler, ["EOSE", subId]);
}

function emitMockLiveEvent(channelId: string, event: RelayEvent) {
  for (const socket of mockSockets.values()) {
    for (const [subId, subscribedChannelId] of socket.subscriptions) {
      if (
        subscribedChannelId === channelId ||
        subscribedChannelId === GLOBAL_MOCK_SUBSCRIPTION
      ) {
        sendWsText(socket.handler, ["EVENT", subId, event]);
      }
    }
  }
}

function recordMockMessage(channelId: string, event: RelayEvent) {
  const history = getMockMessageStore(channelId);
  history.push(event);

  const channel = mockChannels.find((candidate) => candidate.id === channelId);
  if (!channel) {
    return;
  }

  channel.last_message_at = new Date(event.created_at * 1_000).toISOString();
  touchMockChannel(channel);
}

function emitMockChannelMessage(
  channelId: string,
  content: string,
  pubkey?: string,
) {
  const event = createMockEvent(9, content, [["h", channelId]], pubkey);
  recordMockMessage(channelId, event);
  emitMockLiveEvent(channelId, event);
  return event;
}

function emitMockTypingIndicator(channelId: string, pubkey: string) {
  const event: RelayEvent = {
    id: crypto.randomUUID().replace(/-/g, ""),
    pubkey,
    created_at: Math.floor(Date.now() / 1000),
    kind: 20002,
    tags: [["h", channelId]],
    content: "",
    sig: "mocksig".repeat(20).slice(0, 128),
  };

  emitMockLiveEvent(channelId, event);
  return event;
}

function createMockEvent(
  kind: number,
  content: string,
  tags: string[][],
  pubkey = DEFAULT_MOCK_IDENTITY.pubkey,
): RelayEvent {
  return {
    id: crypto.randomUUID().replace(/-/g, ""),
    pubkey,
    created_at: Math.floor(Date.now() / 1000),
    kind,
    tags,
    content,
    sig: "mocksig".repeat(20).slice(0, 128),
  };
}

async function signWithIdentity(
  identity: TestIdentity,
  template: {
    kind: number;
    content: string;
    tags: string[][];
  },
) {
  const secretKey = hexToBytes(identity.privateKey);

  return finalizeEvent(
    {
      kind: template.kind,
      content: template.content,
      tags: template.tags,
      created_at: Math.floor(Date.now() / 1000),
    },
    secretKey,
  );
}

async function assertOk(response: Response) {
  if (response.ok) {
    return;
  }

  const body = await response.text();
  throw new Error(body || `Request failed with ${response.status}`);
}

function getRelayIdentity(config: E2eConfig | undefined): TestIdentity {
  const identity = getIdentity(config);
  if (!identity) {
    throw new Error("Relay identity required.");
  }

  return identity;
}

async function relayJsonRequest<T>(
  config: E2eConfig | undefined,
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const identity = getRelayIdentity(config);
  const headers = new Headers(init.headers);

  headers.set("X-Pubkey", identity.pubkey);
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(`${getRelayHttpUrl(config)}${path}`, {
    ...init,
    headers,
  });
  await assertOk(response);
  return response.json() as Promise<T>;
}

async function relayEmptyRequest(
  config: E2eConfig | undefined,
  path: string,
  init: RequestInit = {},
) {
  const identity = getRelayIdentity(config);
  const headers = new Headers(init.headers);

  headers.set("X-Pubkey", identity.pubkey);
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(`${getRelayHttpUrl(config)}${path}`, {
    ...init,
    headers,
  });
  await assertOk(response);
}

async function submitSignedEvent(
  config: E2eConfig | undefined,
  template: { kind: number; content: string; tags: string[][] },
): Promise<{ event_id: string; accepted: boolean; message: string }> {
  const identity = getRelayIdentity(config);
  const signed = await signWithIdentity(identity, template);
  return relayJsonRequest(config, "/api/events", {
    method: "POST",
    body: JSON.stringify(signed),
  });
}

async function handleGetChannels(config: E2eConfig | undefined) {
  const identity = getIdentity(config);
  if (!identity) {
    return listMockChannels();
  }

  return relayJsonRequest<RawChannel[]>(config, "/api/channels");
}

async function handleGetProfile(config: E2eConfig | undefined) {
  const identity = getIdentity(config);
  if (!identity) {
    return cloneProfile(ensureMockProfile(config));
  }

  return relayJsonRequest<RawProfile>(config, "/api/users/me/profile");
}

async function handleUpdateProfile(
  args: {
    displayName?: string;
    avatarUrl?: string;
    about?: string;
    nip05Handle?: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const profile = ensureMockProfile(config);
    const nextDisplayName = args.displayName?.trim();
    const nextAvatarUrl = args.avatarUrl?.trim();
    const nextAbout = args.about?.trim();
    const nextNip05Handle = args.nip05Handle?.trim();

    if (nextDisplayName && nextDisplayName !== profile.display_name) {
      profile.display_name = nextDisplayName;
      applyMockDisplayName(profile.pubkey, nextDisplayName);
    }
    if (nextAvatarUrl && nextAvatarUrl !== profile.avatar_url) {
      profile.avatar_url = nextAvatarUrl;
    }
    if (nextAbout && nextAbout !== profile.about) {
      profile.about = nextAbout;
    }
    if (
      typeof nextNip05Handle === "string" &&
      nextNip05Handle !== profile.nip05_handle
    ) {
      profile.nip05_handle =
        nextNip05Handle.length > 0 ? nextNip05Handle : null;
    }

    return cloneProfile(profile);
  }

  // Read-merge-write: fetch current profile, merge, sign kind:0.
  const current = await relayJsonRequest<RawProfile>(
    config,
    "/api/users/me/profile",
  );
  const profileContent = JSON.stringify({
    display_name: args.displayName ?? current.display_name ?? undefined,
    name: current.display_name ?? undefined,
    picture: args.avatarUrl ?? current.avatar_url ?? undefined,
    about: args.about ?? current.about ?? undefined,
    nip05: args.nip05Handle ?? current.nip05_handle ?? undefined,
  });
  await submitSignedEvent(config, {
    kind: 0,
    content: profileContent,
    tags: [],
  });

  return relayJsonRequest<RawProfile>(config, "/api/users/me/profile");
}

async function handleGetUserProfile(
  args: {
    pubkey?: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const pubkey = (args.pubkey ?? getMockMemberPubkey(config)).toLowerCase();
    const profile = getMockProfileByPubkey(pubkey);
    if (!profile) {
      throw new Error(`User ${pubkey} not found.`);
    }

    return cloneProfile(profile);
  }

  const path = args.pubkey
    ? `/api/users/${args.pubkey}/profile`
    : "/api/users/me/profile";
  return relayJsonRequest<RawProfile>(config, path);
}

async function handleGetUsersBatch(
  args: {
    pubkeys: string[];
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const profiles: RawUsersBatchResponse["profiles"] = {};
    const missing: string[] = [];

    for (const pubkey of args.pubkeys) {
      const normalizedPubkey = pubkey.toLowerCase();
      const profile = getMockProfileByPubkey(normalizedPubkey);

      if (!profile) {
        missing.push(pubkey);
        continue;
      }

      profiles[normalizedPubkey] = {
        display_name: profile.display_name,
        avatar_url: profile.avatar_url,
        nip05_handle: profile.nip05_handle,
      };
    }

    return {
      profiles,
      missing,
    };
  }

  return relayJsonRequest<RawUsersBatchResponse>(config, "/api/users/batch", {
    method: "POST",
    body: JSON.stringify({
      pubkeys: args.pubkeys,
    }),
  });
}

async function handleSearchUsers(
  args: {
    query: string;
    limit?: number;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const normalizedQuery = args.query.trim().toLowerCase();
    if (normalizedQuery.length === 0) {
      return { users: [] } satisfies RawSearchUsersResponse;
    }

    const results = listMockProfiles()
      .filter((profile) => {
        const displayName = profile.display_name?.toLowerCase() ?? "";
        const nip05Handle = profile.nip05_handle?.toLowerCase() ?? "";
        const pubkey = profile.pubkey.toLowerCase();
        return (
          displayName.includes(normalizedQuery) ||
          nip05Handle.includes(normalizedQuery) ||
          pubkey.includes(normalizedQuery)
        );
      })
      .sort((left, right) => {
        const leftName = left.display_name ?? left.nip05_handle ?? left.pubkey;
        const rightName =
          right.display_name ?? right.nip05_handle ?? right.pubkey;
        return leftName.localeCompare(rightName);
      })
      .slice(0, args.limit ?? 8)
      .map((profile) => ({
        pubkey: profile.pubkey,
        display_name: profile.display_name,
        avatar_url: profile.avatar_url,
        nip05_handle: profile.nip05_handle,
      }));

    return {
      users: results,
    } satisfies RawSearchUsersResponse;
  }

  const searchParams = new URLSearchParams();
  searchParams.set("q", args.query);
  searchParams.set("limit", String(args.limit ?? 8));
  return relayJsonRequest<RawSearchUsersResponse>(
    config,
    `/api/users/search?${searchParams.toString()}`,
  );
}

async function handleGetPresence(
  args: {
    pubkeys: string[];
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    return Object.fromEntries(
      args.pubkeys.map((pubkey) => [
        pubkey.toLowerCase(),
        getMockPresenceStatus(pubkey),
      ]),
    ) satisfies RawPresenceLookup;
  }

  if (args.pubkeys.length === 0) {
    return {} satisfies RawPresenceLookup;
  }

  const searchParams = new URLSearchParams();
  searchParams.set("pubkeys", args.pubkeys.join(","));
  return relayJsonRequest<RawPresenceLookup>(
    config,
    `/api/presence?${searchParams.toString()}`,
  );
}

async function handleSetPresence(
  args: {
    status: PresenceStatus;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    setMockPresenceStatus(getMockMemberPubkey(config), args.status);

    return {
      status: args.status,
      ttl_seconds: args.status === "offline" ? 0 : MOCK_PRESENCE_TTL_SECONDS,
    } satisfies RawSetPresenceResponse;
  }

  return relayJsonRequest<RawSetPresenceResponse>(config, "/api/presence", {
    method: "PUT",
    body: JSON.stringify({
      status: args.status,
    }),
  });
}

async function handleCreateChannel(
  args: {
    name: string;
    channelType: "stream" | "forum";
    visibility: "open" | "private";
    description?: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const owner = createCurrentMember(config, "owner");
    const channel = createMockChannel({
      id: crypto.randomUUID(),
      name: args.name,
      channel_type: args.channelType,
      visibility: args.visibility,
      description: args.description ?? "",
      topic: null,
      purpose: null,
      last_message_at: null,
      archived_at: null,
      created_by: owner.pubkey,
      topic_set_by: null,
      topic_set_at: null,
      purpose_set_by: null,
      purpose_set_at: null,
      topic_required: false,
      max_members: null,
      nip29_group_id: null,
      created_minutes_ago: 0,
      updated_minutes_ago: 0,
      members: [owner],
    });
    mockChannels.push(channel);
    return toRawChannel(channel);
  }

  const channelId = crypto.randomUUID();
  const tags: string[][] = [
    ["h", channelId],
    ["name", args.name],
    ["channel_type", args.channelType],
    ["visibility", args.visibility],
  ];
  if (args.description) {
    tags.push(["about", args.description]);
  }
  await submitSignedEvent(config, { kind: 9007, content: "", tags });

  // Fetch the created channel to return the expected shape.
  const channels = await relayJsonRequest<RawChannel[]>(
    config,
    "/api/channels",
  );
  const created = channels.find((ch) => ch.name === args.name);
  if (!created) {
    throw new Error(`Channel "${args.name}" not found after creation`);
  }
  return created;
}

async function handleOpenDm(
  args: {
    pubkeys: string[];
  },
  config: E2eConfig | undefined,
) {
  const normalizedPubkeys = normalizeParticipantPubkeys(args.pubkeys);
  if (normalizedPubkeys.length === 0) {
    throw new Error("Select at least one person to start a DM.");
  }

  const currentPubkey = getMockMemberPubkey(config).toLowerCase();
  const participantPubkeys = normalizeParticipantPubkeys([
    currentPubkey,
    ...normalizedPubkeys.filter((pubkey) => pubkey !== currentPubkey),
  ]);
  const existingChannel = findMockDmByParticipantPubkeys(participantPubkeys);
  if (existingChannel) {
    return toRawChannel(existingChannel);
  }

  const identity = getIdentity(config);
  if (!identity) {
    const members = participantPubkeys.map((pubkey) =>
      createMockMember(pubkey, "member", 0),
    );
    const channel = createMockChannel({
      id: crypto.randomUUID(),
      name:
        participantPubkeys.length === 2
          ? "DM"
          : `Group DM (${participantPubkeys.length})`,
      channel_type: "dm",
      visibility: "private",
      description: "Direct message conversation",
      topic: null,
      purpose: null,
      last_message_at: null,
      archived_at: null,
      created_by: getMockMemberPubkey(config),
      topic_set_by: null,
      topic_set_at: null,
      purpose_set_by: null,
      purpose_set_at: null,
      topic_required: false,
      max_members: participantPubkeys.length,
      nip29_group_id: null,
      created_minutes_ago: 0,
      updated_minutes_ago: 0,
      members,
    });
    syncMockChannel(channel);
    mockChannels.push(channel);
    return toRawChannel(channel);
  }

  const response = await relayJsonRequest<RawOpenDmResponse>(
    config,
    "/api/dms",
    {
      method: "POST",
      body: JSON.stringify({
        pubkeys: normalizedPubkeys,
      }),
    },
  );

  return relayJsonRequest<RawChannel>(
    config,
    `/api/channels/${response.channel_id}`,
  );
}

async function handleHideDm(
  args: { channelId: string },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const index = mockChannels.findIndex(
      (channel) => channel.id === args.channelId,
    );
    if (index === -1) {
      throw new Error(`DM ${args.channelId} not found.`);
    }
    // Remove from mock list (simulates hiding from sidebar).
    mockChannels.splice(index, 1);
    return;
  }

  await relayEmptyRequest(config, `/api/dms/${args.channelId}/hide`, {
    method: "POST",
  });
}

async function handleGetChannelDetails(
  args: { channelId: string },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    return toRawChannelDetail(getMockChannel(args.channelId));
  }

  return relayJsonRequest<RawChannelDetail>(
    config,
    `/api/channels/${args.channelId}`,
  );
}

async function handleGetChannelMembers(
  args: { channelId: string },
  config: E2eConfig | undefined,
): Promise<RawChannelMembersResponse> {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    return {
      members: cloneMembers(channel.members),
      next_cursor: null,
    };
  }

  return relayJsonRequest<RawChannelMembersResponse>(
    config,
    `/api/channels/${args.channelId}/members`,
  );
}

async function handleUpdateChannel(
  args: {
    channelId: string;
    name?: string;
    description?: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    if (args.name !== undefined) {
      channel.name = args.name;
    }
    if (args.description !== undefined) {
      channel.description = args.description;
    }
    touchMockChannel(channel);
    return toRawChannelDetail(channel);
  }

  const tags: string[][] = [["h", args.channelId]];
  if (args.name !== undefined) {
    tags.push(["name", args.name]);
  }
  if (args.description !== undefined) {
    tags.push(["about", args.description]);
  }
  await submitSignedEvent(config, { kind: 9002, content: "", tags });

  return relayJsonRequest<RawChannelDetail>(
    config,
    `/api/channels/${args.channelId}`,
  );
}

async function handleSetChannelTopic(
  args: {
    channelId: string;
    topic: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    const nextTopic = args.topic.trim();

    channel.topic = nextTopic.length > 0 ? nextTopic : null;
    channel.topic_set_by = getMockMemberPubkey(config);
    channel.topic_set_at = new Date().toISOString();
    touchMockChannel(channel);
    return;
  }

  await submitSignedEvent(config, {
    kind: 9002,
    content: "",
    tags: [
      ["h", args.channelId],
      ["topic", args.topic],
    ],
  });
}

async function handleSetChannelPurpose(
  args: {
    channelId: string;
    purpose: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    const nextPurpose = args.purpose.trim();

    channel.purpose = nextPurpose.length > 0 ? nextPurpose : null;
    channel.purpose_set_by = getMockMemberPubkey(config);
    channel.purpose_set_at = new Date().toISOString();
    touchMockChannel(channel);
    return;
  }

  await submitSignedEvent(config, {
    kind: 9002,
    content: "",
    tags: [
      ["h", args.channelId],
      ["purpose", args.purpose],
    ],
  });
}

async function handleArchiveChannel(
  args: { channelId: string },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    channel.archived_at = new Date().toISOString();
    touchMockChannel(channel);
    return;
  }

  await submitSignedEvent(config, {
    kind: 9002,
    content: "",
    tags: [
      ["h", args.channelId],
      ["archived", "true"],
    ],
  });
}

async function handleUnarchiveChannel(
  args: { channelId: string },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    channel.archived_at = null;
    touchMockChannel(channel);
    return;
  }

  await submitSignedEvent(config, {
    kind: 9002,
    content: "",
    tags: [
      ["h", args.channelId],
      ["archived", "false"],
    ],
  });
}

async function handleDeleteChannel(
  args: { channelId: string },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const index = mockChannels.findIndex(
      (channel) => channel.id === args.channelId,
    );
    if (index === -1) {
      throw new Error(`Channel ${args.channelId} not found.`);
    }

    mockChannels.splice(index, 1);
    mockMessages.delete(args.channelId);
    return;
  }

  await submitSignedEvent(config, {
    kind: 9008,
    content: "",
    tags: [["h", args.channelId]],
  });
}

async function handleAddChannelMembers(
  args: {
    channelId: string;
    pubkeys: string[];
    role?: RawChannelMember["role"];
  },
  config: E2eConfig | undefined,
): Promise<RawAddChannelMembersResponse> {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    const added: string[] = [];
    const errors: RawAddChannelMembersResponse["errors"] = [];

    for (const pubkey of args.pubkeys) {
      if (channel.members.some((member) => member.pubkey === pubkey)) {
        errors.push({
          pubkey,
          error: "Already a member.",
        });
        continue;
      }

      channel.members.push({
        pubkey,
        role: args.role ?? "member",
        joined_at: new Date().toISOString(),
        display_name: mockDisplayNames.get(pubkey) ?? null,
      });
      added.push(pubkey);
    }

    syncMockChannel(channel);
    touchMockChannel(channel);
    return {
      added,
      errors,
    };
  }

  const added: string[] = [];
  const errors: RawAddChannelMembersResponse["errors"] = [];
  for (const pubkey of args.pubkeys) {
    try {
      const tags: string[][] = [
        ["h", args.channelId],
        ["p", pubkey],
      ];
      if (args.role) {
        tags.push(["role", args.role]);
      }
      await submitSignedEvent(config, { kind: 9000, content: "", tags });
      added.push(pubkey);
    } catch (e) {
      errors.push({ pubkey, error: String(e) });
    }
  }
  return { added, errors };
}

async function handleRemoveChannelMember(
  args: {
    channelId: string;
    pubkey: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    channel.members = channel.members.filter(
      (member) => member.pubkey !== args.pubkey,
    );
    syncMockChannel(channel);
    touchMockChannel(channel);
    return;
  }

  await submitSignedEvent(config, {
    kind: 9001,
    content: "",
    tags: [
      ["h", args.channelId],
      ["p", args.pubkey],
    ],
  });
}

async function handleJoinChannel(
  args: {
    channelId: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    const currentPubkey = getMockMemberPubkey(config);

    if (channel.members.some((member) => member.pubkey === currentPubkey)) {
      return;
    }

    channel.members.push(createCurrentMember(config, "member"));
    syncMockChannel(channel);
    touchMockChannel(channel);
    return;
  }

  await submitSignedEvent(config, {
    kind: 9021,
    content: "",
    tags: [["h", args.channelId]],
  });
}

async function handleLeaveChannel(
  args: {
    channelId: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const channel = getMockChannel(args.channelId);
    const currentPubkey = getMockMemberPubkey(config);

    channel.members = channel.members.filter(
      (member) => member.pubkey !== currentPubkey,
    );
    syncMockChannel(channel);
    touchMockChannel(channel);
    return;
  }

  await submitSignedEvent(config, {
    kind: 9022,
    content: "",
    tags: [["h", args.channelId]],
  });
}

async function handleGetFeed(
  args: {
    since?: number;
    limit?: number;
    types?: string;
  },
  config: E2eConfig | undefined,
): Promise<RawHomeFeedResponse> {
  const identity = getIdentity(config);
  if (!identity) {
    const now = Math.floor(Date.now() / 1000);
    const limit = args.limit ?? 50;
    const wantedTypes =
      args.types
        ?.split(",")
        .map((value) => value.trim())
        .filter((value) => value.length > 0) ?? [];
    const includeType = (type: string) =>
      wantedTypes.length === 0 || wantedTypes.includes(type);

    const defaultFeed: RawHomeFeedResponse["feed"] = {
      mentions: [
        {
          id: "mock-feed-mention",
          kind: 9,
          pubkey:
            "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f",
          content: "Please review the release checklist.",
          created_at: now - 90,
          channel_id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
          channel_name: "general",
          tags: [
            ["e", "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50"],
            ["p", DEFAULT_MOCK_IDENTITY.pubkey],
          ],
          category: "mention" as const,
        },
      ],
      needs_action: [
        {
          id: "mock-feed-reminder",
          kind: 40007,
          pubkey:
            "0000000000000000000000000000000000000000000000000000000000000000",
          content: "Reminder: update the launch plan before lunch.",
          created_at: now - 15 * 60,
          channel_id: "94a444a4-c0a3-5966-ab05-530c6ddc2301",
          channel_name: "agents",
          tags: [
            ["e", "94a444a4-c0a3-5966-ab05-530c6ddc2301"],
            ["p", DEFAULT_MOCK_IDENTITY.pubkey],
          ],
          category: "needs_action" as const,
        },
      ],
      activity: [
        {
          id: "mock-feed-self-activity",
          kind: 9,
          pubkey: DEFAULT_MOCK_IDENTITY.pubkey,
          content: "I posted a note about the launch checklist.",
          created_at: now - 25 * 60,
          channel_id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
          channel_name: "general",
          tags: [["e", "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50"]],
          category: "activity" as const,
        },
        {
          id: "mock-feed-activity",
          kind: 9,
          pubkey:
            "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260",
          content: "Engineering shipped the desktop build.",
          created_at: now - 42 * 60,
          channel_id: "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9",
          channel_name: "engineering",
          tags: [["e", "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9"]],
          category: "activity" as const,
        },
      ],
      agent_activity: [
        {
          id: "mock-feed-agent",
          kind: 43003,
          pubkey:
            "db0b028cd36f4d3e36c8300cce87252c1f7fc9495ffecc53f393fcac341ffd36",
          content: "Agent progress: channel index complete.",
          created_at: now - 2 * 60 * 60,
          channel_id: "94a444a4-c0a3-5966-ab05-530c6ddc2301",
          channel_name: "agents",
          tags: [["e", "94a444a4-c0a3-5966-ab05-530c6ddc2301"]],
          category: "agent_activity" as const,
        },
      ],
    };

    const mergeFeedCategory = (
      category: keyof RawHomeFeedResponse["feed"],
    ): RawFeedItem[] =>
      includeType(category)
        ? [...mockFeedOverrides[category], ...defaultFeed[category]]
            .sort((left, right) => right.created_at - left.created_at)
            .slice(0, limit)
        : [];

    const mentions = mergeFeedCategory("mentions");
    const needsAction = mergeFeedCategory("needs_action");
    const activity = mergeFeedCategory("activity");
    const agentActivity = mergeFeedCategory("agent_activity");

    return {
      feed: {
        mentions,
        needs_action: needsAction,
        activity,
        agent_activity: agentActivity,
      },
      meta: {
        since: args.since ?? now - 7 * 24 * 60 * 60,
        total:
          mentions.length +
          needsAction.length +
          activity.length +
          agentActivity.length,
        generated_at: now,
      },
    };
  }

  const url = new URL("/api/feed", getRelayHttpUrl(config));
  if (args.since !== undefined) {
    url.searchParams.set("since", String(args.since));
  }
  if (args.limit !== undefined) {
    url.searchParams.set("limit", String(args.limit));
  }
  if (args.types) {
    url.searchParams.set("types", args.types);
  }

  const response = await fetch(url, {
    headers: {
      "X-Pubkey": identity.pubkey,
    },
  });
  await assertOk(response);
  return response.json();
}

async function handleListTokens(
  config: E2eConfig | undefined,
): Promise<RawListTokensResponse> {
  const identity = getIdentity(config);
  if (!identity) {
    return {
      tokens: mockTokens.map(cloneToken),
    };
  }

  return relayJsonRequest<RawListTokensResponse>(config, "/api/tokens");
}

async function handleMintToken(
  args: {
    name: string;
    scopes: string[];
    channelIds?: string[];
    expiresInDays?: number;
  },
  config: E2eConfig | undefined,
): Promise<RawMintTokenResponse> {
  const identity = getIdentity(config);
  if (!identity) {
    if (mockMintTokenError) {
      throw mockMintTokenError;
    }

    const now = new Date();
    const token: MockToken = {
      id: crypto.randomUUID(),
      name: args.name,
      scopes: [...args.scopes],
      channel_ids: [...(args.channelIds ?? [])],
      created_at: now.toISOString(),
      expires_at:
        typeof args.expiresInDays === "number"
          ? new Date(
              now.getTime() + args.expiresInDays * 24 * 60 * 60 * 1_000,
            ).toISOString()
          : null,
      last_used_at: null,
      revoked_at: null,
      token: `spr_tok_mock_${crypto.randomUUID().replace(/-/g, "")}`,
    };

    mockTokens.unshift(token);
    return cloneMintedToken(token);
  }

  return relayJsonRequest<RawMintTokenResponse>(config, "/api/tokens", {
    method: "POST",
    body: JSON.stringify({
      name: args.name,
      scopes: args.scopes,
      channel_ids: args.channelIds,
      expires_in_days: args.expiresInDays,
    }),
  });
}

async function handleRevokeToken(
  args: { tokenId: string },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const token = mockTokens.find((candidate) => candidate.id === args.tokenId);
    if (!token) {
      throw new Error(`Token ${args.tokenId} not found.`);
    }

    token.revoked_at = new Date().toISOString();
    return;
  }

  await relayEmptyRequest(config, `/api/tokens/${args.tokenId}`, {
    method: "DELETE",
  });
}

async function handleRevokeAllTokens(
  config: E2eConfig | undefined,
): Promise<RawRevokeAllTokensResponse> {
  const identity = getIdentity(config);
  if (!identity) {
    const now = new Date().toISOString();
    let revokedCount = 0;

    for (const token of mockTokens) {
      if (token.revoked_at) {
        continue;
      }

      token.revoked_at = now;
      revokedCount += 1;
    }

    return {
      revoked_count: revokedCount,
    };
  }

  return relayJsonRequest<RawRevokeAllTokensResponse>(config, "/api/tokens", {
    method: "DELETE",
  });
}

async function handleListRelayAgents(): Promise<RawRelayAgent[]> {
  syncMockRelayAgentsFromManagedAgents();
  return mockRelayAgents.map(cloneRelayAgent);
}

async function handleDiscoverAcpProviders(): Promise<RawAcpProvider[]> {
  return [
    {
      id: "goose",
      label: "Goose",
      command: "goose",
      binary_path: "/usr/local/bin/goose",
      default_args: ["acp"],
    },
    {
      id: "codex",
      label: "Codex",
      command: "codex-acp",
      binary_path: "/usr/local/bin/codex-acp",
      default_args: [],
    },
  ];
}

async function handleDiscoverManagedAgentPrereqs(args: {
  input?: {
    acpCommand?: string;
    mcpCommand?: string;
  };
}): Promise<RawManagedAgentPrereqs> {
  return {
    admin: {
      command: "sprout-admin",
      resolved_path: "/Users/wesb/dev/sprout/target/debug/sprout-admin",
      available: true,
    },
    acp: {
      command: args.input?.acpCommand ?? "sprout-acp",
      resolved_path: "/Users/wesb/dev/sprout/target/debug/sprout-acp",
      available: true,
    },
    mcp: {
      command: args.input?.mcpCommand ?? "sprout-mcp-server",
      resolved_path: "/Users/wesb/dev/sprout/target/debug/sprout-mcp-server",
      available: true,
    },
  };
}

async function handleListManagedAgents(): Promise<RawManagedAgent[]> {
  return mockManagedAgents.map(cloneManagedAgent);
}

async function handleListPersonas(): Promise<RawPersona[]> {
  return mockPersonas.map((persona) => ({ ...persona }));
}

async function handleCreatePersona(args: {
  input: {
    displayName: string;
    avatarUrl?: string;
    systemPrompt: string;
  };
}): Promise<RawPersona> {
  const now = new Date().toISOString();
  const persona: RawPersona = {
    id: crypto.randomUUID(),
    display_name: args.input.displayName.trim(),
    avatar_url: args.input.avatarUrl?.trim() || null,
    system_prompt: args.input.systemPrompt.trim(),
    is_builtin: false,
    created_at: now,
    updated_at: now,
  };
  mockPersonas.push(persona);
  return { ...persona };
}

async function handleUpdatePersona(args: {
  input: {
    id: string;
    displayName: string;
    avatarUrl?: string;
    systemPrompt: string;
  };
}): Promise<RawPersona> {
  const persona = mockPersonas.find(
    (candidate) => candidate.id === args.input.id,
  );
  if (!persona) {
    throw new Error(`Persona ${args.input.id} not found.`);
  }
  if (persona.is_builtin) {
    throw new Error("Built-in personas cannot be edited.");
  }

  persona.display_name = args.input.displayName.trim();
  persona.avatar_url = args.input.avatarUrl?.trim() || null;
  persona.system_prompt = args.input.systemPrompt.trim();
  persona.updated_at = new Date().toISOString();

  return { ...persona };
}

async function handleDeletePersona(args: { id: string }): Promise<void> {
  const persona = mockPersonas.find((candidate) => candidate.id === args.id);
  if (!persona) {
    throw new Error(`Persona ${args.id} not found.`);
  }
  if (persona.is_builtin) {
    throw new Error("Built-in personas cannot be deleted.");
  }

  mockPersonas = mockPersonas.filter((candidate) => candidate.id !== args.id);
  const now = new Date().toISOString();
  for (const agent of mockManagedAgents) {
    if (agent.persona_id === args.id) {
      agent.persona_id = null;
      agent.updated_at = now;
    }
  }
}

async function handleParsePersonaFiles(args: {
  fileBytes: number[];
  fileName: string;
}): Promise<{
  personas: {
    display_name: string;
    system_prompt: string;
    avatar_data_url: string | null;
    source_file: string;
  }[];
  skipped: { source_file: string; reason: string }[];
}> {
  // In test mode, return canned data — we can't actually parse PNG chunks in JS
  return {
    personas: [
      {
        display_name: "Imported Persona",
        system_prompt: "You are an imported test persona.",
        avatar_data_url: null,
        source_file: args.fileName,
      },
    ],
    skipped: [],
  };
}

async function handleExportPersonaToJson(args: {
  id: string;
}): Promise<boolean> {
  // In test mode, just verify the persona exists
  const persona = mockPersonas.find((p) => p.id === args.id);
  if (!persona) throw new Error(`Persona ${args.id} not found.`);
  return true; // Simulate successful save
}

async function handleCreateManagedAgent(args: {
  input: {
    name: string;
    personaId?: string;
    relayUrl?: string;
    acpCommand?: string;
    agentCommand?: string;
    agentArgs?: string[];
    mcpCommand?: string;
    turnTimeoutSeconds?: number;
    idleTimeoutSeconds?: number;
    maxTurnDurationSeconds?: number;
    parallelism?: number;
    systemPrompt?: string;
    avatarUrl?: string;
    model?: string;
    mintToken?: boolean;
    tokenScopes?: string[];
    tokenName?: string;
    spawnAfterCreate?: boolean;
    startOnAppLaunch?: boolean;
    backend?:
      | { type: "local" }
      | { type: "provider"; id: string; config: Record<string, unknown> };
  };
}): Promise<RawCreateManagedAgentResponse> {
  const name = args.input.name.trim();
  const now = new Date().toISOString();
  const pubkey = crypto
    .randomUUID()
    .replace(/-/g, "")
    .padEnd(64, "0")
    .slice(0, 64);
  const token =
    args.input.mintToken === false
      ? null
      : `spr_tok_mock_${crypto.randomUUID().replace(/-/g, "")}`;
  const managedAgent: MockManagedAgent = {
    pubkey,
    name,
    persona_id: args.input.personaId ?? null,
    relay_url: args.input.relayUrl ?? DEFAULT_RELAY_WS_URL,
    acp_command: args.input.acpCommand ?? "sprout-acp",
    agent_command: args.input.agentCommand ?? "goose",
    agent_args:
      args.input.agentArgs && args.input.agentArgs.length > 0
        ? [...args.input.agentArgs]
        : ["acp"],
    mcp_command: args.input.mcpCommand ?? "sprout-mcp-server",
    turn_timeout_seconds: args.input.turnTimeoutSeconds ?? 320,
    idle_timeout_seconds: args.input.idleTimeoutSeconds ?? null,
    max_turn_duration_seconds: args.input.maxTurnDurationSeconds ?? null,
    parallelism: args.input.parallelism ?? 1,
    system_prompt: args.input.systemPrompt?.trim() || null,
    model: args.input.model?.trim() || null,
    has_api_token: token !== null,
    status: args.input.spawnAfterCreate ? "running" : "stopped",
    pid: args.input.spawnAfterCreate ? 42000 + mockManagedAgents.length : null,
    created_at: now,
    updated_at: now,
    last_started_at: args.input.spawnAfterCreate ? now : null,
    last_stopped_at: null,
    last_exit_code: null,
    last_error: null,
    log_path: `/tmp/mock-agent-${pubkey}.log`,
    start_on_app_launch: args.input.startOnAppLaunch ?? true,
    backend: args.input.backend ?? { type: "local" as const },
    backend_agent_id: null,
    private_key_nsec: `nsec1mock${pubkey.slice(0, 20)}`,
    api_token: token,
    log_lines: [
      `sprout-acp starting: relay=${args.input.relayUrl ?? DEFAULT_RELAY_WS_URL} agent_pubkey=${pubkey} parallelism=${args.input.parallelism ?? 1}`,
      args.input.systemPrompt?.trim()
        ? `system prompt override configured (${args.input.systemPrompt.trim().length} chars)`
        : "system prompt override not set",
      args.input.spawnAfterCreate
        ? "connected to relay at ws://localhost:3000"
        : "profile created; harness not started",
    ],
  };

  mockManagedAgents.unshift(managedAgent);
  syncMockRelayAgentsFromManagedAgents();

  return {
    agent: cloneManagedAgent(managedAgent),
    private_key_nsec: managedAgent.private_key_nsec,
    api_token: managedAgent.api_token,
    profile_sync_error: null,
    spawn_error: null,
  };
}

function getMockManagedAgent(pubkey: string): MockManagedAgent {
  const agent = mockManagedAgents.find(
    (candidate) => candidate.pubkey === pubkey,
  );
  if (!agent) {
    throw new Error(`Managed agent ${pubkey} not found.`);
  }

  return agent;
}

async function handleStartManagedAgent(args: {
  pubkey: string;
}): Promise<RawManagedAgent> {
  const agent = getMockManagedAgent(args.pubkey);
  const now = new Date().toISOString();
  agent.status = "running";
  agent.pid = agent.pid ?? 42000 + mockManagedAgents.indexOf(agent);
  agent.updated_at = now;
  agent.last_started_at = now;
  agent.last_error = null;
  agent.log_lines.push(`started mock harness at ${now}`);
  syncMockRelayAgentsFromManagedAgents();
  return cloneManagedAgent(agent);
}

async function handleStopManagedAgent(args: {
  pubkey: string;
}): Promise<RawManagedAgent> {
  const agent = getMockManagedAgent(args.pubkey);
  const now = new Date().toISOString();
  agent.status = "stopped";
  agent.pid = null;
  agent.updated_at = now;
  agent.last_stopped_at = now;
  agent.log_lines.push(`stopped mock harness at ${now}`);
  syncMockRelayAgentsFromManagedAgents();
  return cloneManagedAgent(agent);
}

async function handleDeleteManagedAgent(args: {
  pubkey: string;
  forceRemoteDelete?: boolean | null;
}): Promise<void> {
  // Model the backend invariant: reject deletion of deployed remote agents
  // unless force_remote_delete is true.
  const agent = mockManagedAgents.find((a) => a.pubkey === args.pubkey);
  if (
    agent &&
    agent.backend.type === "provider" &&
    agent.backend_agent_id != null &&
    !args.forceRemoteDelete
  ) {
    throw new Error(
      "cannot delete a deployed remote agent without force_remote_delete: true",
    );
  }
  mockManagedAgents = mockManagedAgents.filter(
    (candidate) => candidate.pubkey !== args.pubkey,
  );
  syncMockRelayAgentsFromManagedAgents();
}

async function handleSetManagedAgentStartOnAppLaunch(args: {
  pubkey: string;
  startOnAppLaunch: boolean;
}): Promise<RawManagedAgent> {
  const agent = getMockManagedAgent(args.pubkey);
  agent.start_on_app_launch = args.startOnAppLaunch;
  agent.updated_at = new Date().toISOString();
  return cloneManagedAgent(agent);
}

async function handleMintManagedAgentToken(args: {
  input: {
    pubkey: string;
    tokenName?: string;
    scopes?: string[];
  };
}): Promise<RawMintManagedAgentTokenResponse> {
  const agent = getMockManagedAgent(args.input.pubkey);
  const now = new Date().toISOString();
  agent.api_token = `spr_tok_mock_${crypto.randomUUID().replace(/-/g, "")}`;
  agent.has_api_token = true;
  agent.updated_at = now;
  agent.log_lines.push(
    `minted token ${args.input.tokenName ?? `${agent.name}-token`} at ${now}`,
  );

  return {
    agent: cloneManagedAgent(agent),
    token: agent.api_token ?? "",
  };
}

async function handleGetManagedAgentLog(args: {
  pubkey: string;
  lineCount?: number;
}): Promise<RawManagedAgentLog> {
  const agent = getMockManagedAgent(args.pubkey);
  const count = args.lineCount ?? 120;
  return {
    content: agent.log_lines.slice(-count).join("\n"),
    log_path: agent.log_path,
  };
}

async function handleUpdateManagedAgent(args: {
  input: {
    pubkey: string;
    model?: string | null;
    systemPrompt?: string | null;
  };
}): Promise<RawManagedAgent> {
  const agent = getMockManagedAgent(args.input.pubkey);
  if (args.input.model !== undefined) {
    agent.model = args.input.model;
  }
  if (args.input.systemPrompt !== undefined) {
    agent.system_prompt = args.input.systemPrompt;
  }
  agent.updated_at = new Date().toISOString();
  return cloneManagedAgent(agent);
}

async function handleSearchMessages(
  args: {
    q: string;
    limit?: number;
  },
  config: E2eConfig | undefined,
): Promise<RawSearchResponse> {
  const identity = getIdentity(config);
  if (!identity) {
    const query = args.q.trim().toLowerCase();
    const limit = args.limit ?? 20;
    const now = Math.floor(Date.now() / 1000);

    const mockHits: RawSearchHit[] = [
      {
        event_id: "mock-general-welcome",
        content: "Welcome to #general",
        kind: 9,
        pubkey: DEFAULT_MOCK_IDENTITY.pubkey,
        channel_id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
        channel_name: "general",
        created_at: now - 60,
        score: 8.5,
      },
      {
        event_id: "mock-engineering-shipped",
        content: "Engineering shipped the desktop build.",
        kind: 9,
        pubkey:
          "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260",
        channel_id: "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9",
        channel_name: "engineering",
        created_at: now - 42 * 60,
        score: 7.2,
      },
      {
        event_id: "mock-design-critique",
        content: "Design critique notes for the browse flow.",
        kind: 9,
        pubkey:
          "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f",
        channel_id: "b5e2f8a1-3c44-5912-9e67-4a8d1f2b3c4e",
        channel_name: "design",
        created_at: now - 75 * 60,
        score: 6.6,
      },
      {
        event_id: "mock-forum-release-thread",
        content: "Release checklist: async feedback thread.",
        kind: 45001,
        pubkey:
          "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f",
        channel_id: "a27e1ee9-76a6-5bdf-a5d5-1d85610dad11",
        channel_name: "watercooler",
        created_at: now - 90 * 60,
        score: 5.8,
      },
    ];

    const hits = mockHits
      .filter((hit) => {
        if (!query) {
          return true;
        }

        return (
          hit.content.toLowerCase().includes(query) ||
          hit.channel_name.toLowerCase().includes(query)
        );
      })
      .slice(0, limit);

    return {
      hits,
      found: hits.length,
    };
  }

  const url = new URL("/api/search", getRelayHttpUrl(config));
  url.searchParams.set("q", args.q);
  if (args.limit !== undefined) {
    url.searchParams.set("limit", String(args.limit));
  }

  const response = await fetch(url, {
    headers: {
      "X-Pubkey": identity.pubkey,
    },
  });
  await assertOk(response);
  return response.json();
}

async function handleSendChannelMessage(
  args: {
    channelId: string;
    content: string;
    parentEventId?: string | null;
    kind?: number | null;
    mentionPubkeys?: string[];
  },
  config: E2eConfig | undefined,
): Promise<RawSendChannelMessageResponse> {
  const kind = args.kind ?? 9;
  const identity = getIdentity(config);
  if (!identity) {
    const createdAt = Math.floor(Date.now() / 1000);

    if (!args.parentEventId) {
      const selfLower = getMockMemberPubkey(config).toLowerCase();
      const seen = new Set<string>([selfLower]);
      const tags: string[][] = [["h", args.channelId]];
      for (const pk of args.mentionPubkeys ?? []) {
        const lower = pk.toLowerCase();
        if (!seen.has(lower)) {
          seen.add(lower);
          tags.push(["p", lower]);
        }
      }

      const event = createMockEvent(kind, args.content, tags);
      recordMockMessage(args.channelId, event);
      emitMockLiveEvent(args.channelId, event);

      return {
        event_id: event.id,
        parent_event_id: null,
        root_event_id: null,
        depth: 0,
        created_at: createdAt,
      };
    }

    const history = getMockMessageStore(args.channelId);
    const parentEvent = history.find(
      (event) => event.id === args.parentEventId,
    );
    const parentThread = parentEvent
      ? getThreadReferenceFromTags(parentEvent.tags)
      : {
          parentEventId: null,
          rootEventId: null,
        };
    const rootEventId = parentThread.rootEventId ?? args.parentEventId;
    const depth = parentEvent
      ? (() => {
          let currentEvent: RelayEvent | undefined = parentEvent;
          let nextDepth = 1;

          while (currentEvent) {
            const reference = getThreadReferenceFromTags(currentEvent.tags);
            if (!reference.parentEventId) {
              return nextDepth;
            }

            nextDepth += 1;
            currentEvent = history.find(
              (event) => event.id === reference.parentEventId,
            );
          }

          return nextDepth;
        })()
      : 1;

    const event: RelayEvent = {
      id: crypto.randomUUID().replace(/-/g, ""),
      pubkey: getMockMemberPubkey(config),
      created_at: createdAt,
      kind,
      tags: (() => {
        const authorPubkey = getMockMemberPubkey(config);
        const tags: string[][] = [["h", args.channelId]];
        // Best-effort client-side normalization (relay is authoritative).
        const selfLower = authorPubkey.toLowerCase();
        const seen = new Set<string>([selfLower]);
        if (rootEventId === args.parentEventId) {
          tags.push(["e", rootEventId, "", "reply"]);
        } else {
          tags.push(["e", rootEventId, "", "root"]);
          tags.push(["e", args.parentEventId, "", "reply"]);
        }
        for (const pk of args.mentionPubkeys ?? []) {
          const lower = pk.toLowerCase();
          if (!seen.has(lower)) {
            seen.add(lower);
            tags.push(["p", lower]);
          }
        }
        return tags;
      })(),
      content: args.content.trim(),
      sig: "mocksig".repeat(20).slice(0, 128),
    };

    recordMockMessage(args.channelId, event);
    emitMockLiveEvent(args.channelId, event);

    return {
      event_id: event.id,
      parent_event_id: args.parentEventId,
      root_event_id: rootEventId,
      depth,
      created_at: createdAt,
    };
  }

  const relayIdentity = getRelayIdentity(config);
  const tags: string[][] = [["h", args.channelId]];

  const selfLower = relayIdentity.pubkey.toLowerCase();
  const seen = new Set<string>([selfLower]);

  if (args.parentEventId) {
    tags.push(["e", args.parentEventId, "", "root"]);
    tags.push(["e", args.parentEventId, "", "reply"]);
  }

  for (const pk of args.mentionPubkeys ?? []) {
    const lower = pk.toLowerCase();
    if (!seen.has(lower)) {
      seen.add(lower);
      tags.push(["p", lower]);
    }
  }

  const result = await submitSignedEvent(config, {
    kind,
    content: args.content.trim(),
    tags,
  });

  return {
    event_id: result.event_id,
    parent_event_id: args.parentEventId ?? null,
    root_event_id: args.parentEventId ?? null,
    depth: args.parentEventId ? 1 : 0,
    created_at: Math.floor(Date.now() / 1000),
  };
}

async function handleGetEvent(
  args: {
    eventId: string;
  },
  config: E2eConfig | undefined,
) {
  const identity = getIdentity(config);
  if (!identity) {
    const knownEvents: RelayEvent[] = [
      ...Array.from(mockMessages.values()).flat(),
      {
        id: "mock-engineering-shipped",
        pubkey:
          "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260",
        created_at: Math.floor(Date.now() / 1000) - 42 * 60,
        kind: 9,
        tags: [["e", "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9"]],
        content: "Engineering shipped the desktop build.",
        sig: "mocksig".repeat(20).slice(0, 128),
      },
      {
        id: "mock-design-critique",
        pubkey:
          "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f",
        created_at: Math.floor(Date.now() / 1000) - 75 * 60,
        kind: 9,
        tags: [["h", "b5e2f8a1-3c44-5912-9e67-4a8d1f2b3c4e"]],
        content: "Design critique notes for the browse flow.",
        sig: "mocksig".repeat(20).slice(0, 128),
      },
      {
        id: "mock-forum-release-thread",
        pubkey:
          "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f",
        created_at: Math.floor(Date.now() / 1000) - 90 * 60,
        kind: 45001,
        tags: [["e", "a27e1ee9-76a6-5bdf-a5d5-1d85610dad11"]],
        content: "Release checklist: async feedback thread.",
        sig: "mocksig".repeat(20).slice(0, 128),
      },
    ];
    const event = knownEvents.find((item) => item.id === args.eventId);
    if (!event) {
      throw new Error(`Event not found: ${args.eventId}`);
    }

    return JSON.stringify(event);
  }

  const response = await fetch(
    `${getRelayHttpUrl(config)}/api/events/${args.eventId}`,
    {
      headers: {
        "X-Pubkey": identity.pubkey,
      },
    },
  );
  await assertOk(response);
  return JSON.stringify(await response.json());
}

async function connectRealSocket(args: { url?: string; onMessage: unknown }) {
  const wsId = nextSocketId++;
  const ws = new WebSocket(args.url ?? DEFAULT_RELAY_WS_URL);
  const handler = resolveHandler(args.onMessage);

  realSockets.set(wsId, ws);
  ws.addEventListener("message", (event) => {
    handler({
      type: "Text",
      data: event.data,
    });
  });
  ws.addEventListener("close", () => {
    sendWsClose(handler);
    realSockets.delete(wsId);
  });
  ws.addEventListener("error", () => {
    handler({
      type: "Error",
    });
  });

  return await new Promise<number>((resolve) => {
    ws.addEventListener("open", () => resolve(wsId), { once: true });
    ws.addEventListener("error", () => resolve(wsId), { once: true });
  });
}

async function connectMockSocket(args: { onMessage: unknown }) {
  const wsId = nextSocketId++;
  const handler = resolveHandler(args.onMessage);

  mockSockets.set(wsId, {
    handler,
    subscriptions: new Map(),
  });

  window.setTimeout(() => {
    sendWsText(handler, ["AUTH", `mock-challenge-${wsId}`]);
  }, 0);

  return wsId;
}

async function sendToRealSocket(args: {
  id: number;
  message?: {
    type: "Text" | "Close";
    data?: string;
  };
}) {
  const socket = realSockets.get(args.id);
  if (!socket) {
    return;
  }

  if (args.message?.type === "Close") {
    socket.close();
    return;
  }

  if (args.message?.type === "Text") {
    socket.send(args.message.data ?? "");
  }
}

function sendToMockSocket(args: {
  id: number;
  message?: {
    type: "Text" | "Close";
    data?: string;
  };
}) {
  const socket = mockSockets.get(args.id);
  if (!socket || !args.message) {
    return;
  }

  if (args.message.type === "Close") {
    mockSockets.delete(args.id);
    sendWsClose(socket.handler);
    return;
  }

  if (args.message.type !== "Text" || !args.message.data) {
    return;
  }

  const [type, ...rest] = JSON.parse(args.message.data) as [
    string,
    ...unknown[],
  ];

  if (type === "AUTH") {
    const event = rest[0] as RelayEvent;
    sendWsText(socket.handler, ["OK", event.id, true, ""]);
    return;
  }

  if (type === "REQ") {
    const subId = rest[0] as string;
    const filter = rest[1] as { "#h"?: string[] };
    const channelId = filter["#h"]?.[0];

    if (subId.startsWith("live-")) {
      socket.subscriptions.set(subId, channelId ?? GLOBAL_MOCK_SUBSCRIPTION);
      sendWsText(socket.handler, ["EOSE", subId]);
      return;
    }

    if (!channelId) {
      sendWsText(socket.handler, ["EOSE", subId]);
      return;
    }

    emitMockHistory(socket, subId, channelId);
    return;
  }

  if (type === "CLOSE") {
    const subId = rest[0] as string;
    socket.subscriptions.delete(subId);
    return;
  }

  if (type === "EVENT") {
    const event = rest[0] as RelayEvent;
    const channelId = getChannelIdFromTags(event.tags);
    if (!channelId) {
      sendWsText(socket.handler, [
        "OK",
        event.id,
        false,
        "Missing channel tag.",
      ]);
      return;
    }

    recordMockMessage(channelId, event);
    emitMockLiveEvent(channelId, event);
    sendWsText(socket.handler, ["OK", event.id, true, ""]);
  }
}

function disconnectMockSocket(id: number) {
  const socket = mockSockets.get(id);
  if (!socket) {
    return;
  }

  mockSockets.delete(id);
  sendWsClose(socket.handler);
}

export function maybeInstallE2eTauriMocks() {
  if (installed) {
    return;
  }

  const config = getConfig();
  if (!config) {
    return;
  }

  resetMockTokens(config);
  resetMockManagedAgents();
  resetMockPersonas();
  mockWindows("main");
  window.__SPROUT_E2E_COMMANDS__ = [];
  window.__SPROUT_E2E_WEBVIEW_ZOOM__ = 1;
  window.__SPROUT_E2E_EMIT_MOCK_MESSAGE__ = ({
    channelName,
    content,
    pubkey,
  }) => {
    const channel = mockChannels.find(
      (candidate) => candidate.name === channelName,
    );
    if (!channel) {
      throw new Error(`Mock channel ${channelName} not found.`);
    }

    return emitMockChannelMessage(channel.id, content, pubkey);
  };
  window.__SPROUT_E2E_EMIT_MOCK_TYPING__ = ({ channelName, pubkey }) => {
    const channel = mockChannels.find(
      (candidate) => candidate.name === channelName,
    );
    if (!channel) {
      throw new Error(`Mock channel ${channelName} not found.`);
    }

    return emitMockTypingIndicator(channel.id, pubkey ?? CHARLIE_PUBKEY);
  };
  window.__SPROUT_E2E_PUSH_MOCK_FEED_ITEM__ = (item) => {
    const category = item.category === "mention" ? "mentions" : item.category;
    mockFeedOverrides[category].unshift(item);
    window.dispatchEvent(new CustomEvent("sprout:e2e-home-feed-updated"));
    return item;
  };
  mockIPC(async (command, payload) => {
    const activeConfig = getConfig();
    const identity = getIdentity(activeConfig);
    window.__SPROUT_E2E_COMMANDS__?.push(command);

    switch (command) {
      case "get_identity":
        if (identity) {
          return {
            pubkey: identity.pubkey,
            display_name: identity.username,
          };
        }

        return DEFAULT_MOCK_IDENTITY;
      case "get_profile":
        return handleGetProfile(activeConfig);
      case "update_profile":
        return handleUpdateProfile(
          payload as Parameters<typeof handleUpdateProfile>[0],
          activeConfig,
        );
      case "get_user_profile":
        return handleGetUserProfile(
          (payload as Parameters<typeof handleGetUserProfile>[0]) ?? {},
          activeConfig,
        );
      case "get_users_batch":
        return handleGetUsersBatch(
          payload as Parameters<typeof handleGetUsersBatch>[0],
          activeConfig,
        );
      case "search_users":
        return handleSearchUsers(
          payload as Parameters<typeof handleSearchUsers>[0],
          activeConfig,
        );
      case "get_presence":
        return handleGetPresence(
          (payload as Parameters<typeof handleGetPresence>[0]) ?? {
            pubkeys: [],
          },
          activeConfig,
        );
      case "set_presence":
        return handleSetPresence(
          payload as Parameters<typeof handleSetPresence>[0],
          activeConfig,
        );
      case "get_relay_ws_url":
        return getRelayWsUrl(activeConfig);
      case "discover_acp_providers":
        return handleDiscoverAcpProviders();
      case "discover_backend_providers":
        return [];
      case "probe_backend_provider":
        return { ok: false, error: "mock: no providers available" };
      case "discover_managed_agent_prereqs":
        return handleDiscoverManagedAgentPrereqs(
          payload as Parameters<typeof handleDiscoverManagedAgentPrereqs>[0],
        );
      case "get_channels":
        return handleGetChannels(activeConfig);
      case "get_feed":
        return handleGetFeed(
          (payload as Parameters<typeof handleGetFeed>[0]) ?? {},
          activeConfig,
        );
      case "list_tokens":
        return handleListTokens(activeConfig);
      case "mint_token":
        return handleMintToken(
          payload as Parameters<typeof handleMintToken>[0],
          activeConfig,
        );
      case "revoke_token":
        return handleRevokeToken(
          payload as Parameters<typeof handleRevokeToken>[0],
          activeConfig,
        );
      case "revoke_all_tokens":
        return handleRevokeAllTokens(activeConfig);
      case "list_relay_agents":
        return handleListRelayAgents();
      case "list_personas":
        return handleListPersonas();
      case "create_persona":
        return handleCreatePersona(
          payload as Parameters<typeof handleCreatePersona>[0],
        );
      case "update_persona":
        return handleUpdatePersona(
          payload as Parameters<typeof handleUpdatePersona>[0],
        );
      case "delete_persona":
        return handleDeletePersona(
          payload as Parameters<typeof handleDeletePersona>[0],
        );
      case "parse_persona_files":
        return handleParsePersonaFiles(
          payload as { fileBytes: number[]; fileName: string },
        );
      case "export_persona_to_json":
        return handleExportPersonaToJson(payload as { id: string });
      case "list_managed_agents":
        return handleListManagedAgents();
      case "create_managed_agent":
        return handleCreateManagedAgent(
          payload as Parameters<typeof handleCreateManagedAgent>[0],
        );
      case "start_managed_agent":
        return handleStartManagedAgent(
          payload as Parameters<typeof handleStartManagedAgent>[0],
        );
      case "stop_managed_agent":
        return handleStopManagedAgent(
          payload as Parameters<typeof handleStopManagedAgent>[0],
        );
      case "set_managed_agent_start_on_app_launch":
        return handleSetManagedAgentStartOnAppLaunch(
          payload as Parameters<
            typeof handleSetManagedAgentStartOnAppLaunch
          >[0],
        );
      case "delete_managed_agent":
        return handleDeleteManagedAgent(
          payload as Parameters<typeof handleDeleteManagedAgent>[0],
        );
      case "mint_managed_agent_token":
        return handleMintManagedAgentToken(
          payload as Parameters<typeof handleMintManagedAgentToken>[0],
        );
      case "get_managed_agent_log":
        return handleGetManagedAgentLog(
          payload as Parameters<typeof handleGetManagedAgentLog>[0],
        );
      case "get_agent_models":
        return {
          agentName: "mock-agent",
          agentVersion: "0.0.0",
          models: [],
          agentDefaultModel: null,
          selectedModel: null,
          supportsSwitching: false,
        };
      case "update_managed_agent":
        return handleUpdateManagedAgent(
          payload as Parameters<typeof handleUpdateManagedAgent>[0],
        );
      case "create_channel":
        return handleCreateChannel(
          payload as Parameters<typeof handleCreateChannel>[0],
          activeConfig,
        );
      case "open_dm":
        return handleOpenDm(
          payload as Parameters<typeof handleOpenDm>[0],
          activeConfig,
        );
      case "hide_dm":
        return handleHideDm(
          payload as Parameters<typeof handleHideDm>[0],
          activeConfig,
        );
      case "get_channel_details":
        return handleGetChannelDetails(
          payload as Parameters<typeof handleGetChannelDetails>[0],
          activeConfig,
        );
      case "get_channel_members":
        return handleGetChannelMembers(
          payload as Parameters<typeof handleGetChannelMembers>[0],
          activeConfig,
        );
      case "update_channel":
        return handleUpdateChannel(
          payload as Parameters<typeof handleUpdateChannel>[0],
          activeConfig,
        );
      case "set_channel_topic":
        return handleSetChannelTopic(
          payload as Parameters<typeof handleSetChannelTopic>[0],
          activeConfig,
        );
      case "set_channel_purpose":
        return handleSetChannelPurpose(
          payload as Parameters<typeof handleSetChannelPurpose>[0],
          activeConfig,
        );
      case "archive_channel":
        return handleArchiveChannel(
          payload as Parameters<typeof handleArchiveChannel>[0],
          activeConfig,
        );
      case "unarchive_channel":
        return handleUnarchiveChannel(
          payload as Parameters<typeof handleUnarchiveChannel>[0],
          activeConfig,
        );
      case "delete_channel":
        return handleDeleteChannel(
          payload as Parameters<typeof handleDeleteChannel>[0],
          activeConfig,
        );
      case "add_channel_members":
        return handleAddChannelMembers(
          payload as Parameters<typeof handleAddChannelMembers>[0],
          activeConfig,
        );
      case "remove_channel_member":
        return handleRemoveChannelMember(
          payload as Parameters<typeof handleRemoveChannelMember>[0],
          activeConfig,
        );
      case "join_channel":
        return handleJoinChannel(
          payload as Parameters<typeof handleJoinChannel>[0],
          activeConfig,
        );
      case "leave_channel":
        return handleLeaveChannel(
          payload as Parameters<typeof handleLeaveChannel>[0],
          activeConfig,
        );
      case "search_messages":
        return handleSearchMessages(
          payload as Parameters<typeof handleSearchMessages>[0],
          activeConfig,
        );
      case "send_channel_message":
        return handleSendChannelMessage(
          payload as Parameters<typeof handleSendChannelMessage>[0],
          activeConfig,
        );
      case "get_event":
        return handleGetEvent(
          payload as Parameters<typeof handleGetEvent>[0],
          activeConfig,
        );
      case "sign_event":
        if (identity) {
          return JSON.stringify(
            await signWithIdentity(identity, {
              kind: (payload as { kind: number }).kind,
              content: (payload as { content: string }).content,
              tags: (payload as { tags: string[][] }).tags,
            }),
          );
        }

        return JSON.stringify(
          createMockEvent(
            (payload as { kind: number }).kind,
            (payload as { content: string }).content,
            (payload as { tags: string[][] }).tags,
          ),
        );
      case "create_auth_event":
        if (identity) {
          return JSON.stringify(
            await signWithIdentity(identity, {
              kind: 22242,
              content: "",
              tags: [
                ["relay", (payload as { relayUrl: string }).relayUrl],
                ["challenge", (payload as { challenge: string }).challenge],
              ],
            }),
          );
        }

        return JSON.stringify(
          createMockEvent(22242, "", [
            ["relay", (payload as { relayUrl: string }).relayUrl],
            ["challenge", (payload as { challenge: string }).challenge],
          ]),
        );
      case "plugin:websocket|connect":
        if (isRelayMode(activeConfig)) {
          return connectRealSocket(
            payload as Parameters<typeof connectRealSocket>[0],
          );
        }

        return connectMockSocket(
          payload as Parameters<typeof connectMockSocket>[0],
        );
      case "plugin:websocket|send":
        if (isRelayMode(activeConfig)) {
          return sendToRealSocket(
            payload as Parameters<typeof sendToRealSocket>[0],
          );
        }

        return sendToMockSocket(
          payload as Parameters<typeof sendToMockSocket>[0],
        );
      case "plugin:websocket|disconnect":
        if (isRelayMode(activeConfig)) {
          realSockets.get((payload as { id: number }).id)?.close();
          realSockets.delete((payload as { id: number }).id);
          return;
        }

        return disconnectMockSocket((payload as { id: number }).id);
      case "plugin:webview|set_webview_zoom":
        window.__SPROUT_E2E_WEBVIEW_ZOOM__ = (
          payload as { value: number }
        ).value;
        return;
      default:
        throw new Error(`Unsupported mocked Tauri command: ${command}`);
    }
  });

  installed = true;
}
