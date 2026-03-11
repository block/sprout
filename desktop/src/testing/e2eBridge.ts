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
  relayHttpUrl?: string;
  relayWsUrl?: string;
  identity?: TestIdentity;
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
  nip05_handle: string | null;
};

type RawUsersBatchResponse = {
  profiles: Record<string, RawUserProfileSummary>;
  missing: string[];
};

type PresenceStatus = "online" | "away" | "offline";

type RawPresenceLookup = Record<string, PresenceStatus>;

type RawSetPresenceResponse = {
  status: PresenceStatus;
  ttl_seconds: number;
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

type WsHandler = (message: unknown) => void;

type MockSocket = {
  handler: WsHandler;
  subscriptions: Map<string, string>;
};

declare global {
  interface Window {
    __SPROUT_E2E__?: E2eConfig;
    __SPROUT_E2E_COMMANDS__?: string[];
    __SPROUT_E2E_EMIT_MOCK_MESSAGE__?: (input: {
      channelName: string;
      content: string;
    }) => RelayEvent;
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

function toRawChannel(channel: MockChannel): RawChannel {
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

function getMockIdentity() {
  return {
    pubkey: MOCK_IDENTITY_PUBKEY,
    displayName: DEFAULT_MOCK_IDENTITY.display_name,
  };
}

function cloneProfile(profile: RawProfile): RawProfile {
  return { ...profile };
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

function listMockChannels(): RawChannel[] {
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
      createMockMember(BOB_PUBKEY, "member", 1000),
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

let installed = false;
let nextSocketId = 1;

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
            kind: 40001,
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
      if (subscribedChannelId === channelId) {
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

function emitMockChannelMessage(channelId: string, content: string) {
  const event = createMockEvent(40001, content, [["h", channelId]]);
  recordMockMessage(channelId, event);
  emitMockLiveEvent(channelId, event);
  return event;
}

function createMockEvent(
  kind: number,
  content: string,
  tags: string[][],
): RelayEvent {
  return {
    id: crypto.randomUUID().replace(/-/g, ""),
    pubkey: DEFAULT_MOCK_IDENTITY.pubkey,
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

  await relayEmptyRequest(config, "/api/users/me/profile", {
    method: "PUT",
    body: JSON.stringify({
      display_name: args.displayName,
      avatar_url: args.avatarUrl,
      about: args.about,
      nip05_handle: args.nip05Handle,
    }),
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

  return relayJsonRequest<RawChannel>(config, "/api/channels", {
    method: "POST",
    body: JSON.stringify({
      name: args.name,
      channel_type: args.channelType,
      visibility: args.visibility,
      description: args.description,
    }),
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

  return relayJsonRequest<RawChannelDetail>(
    config,
    `/api/channels/${args.channelId}`,
    {
      method: "PUT",
      body: JSON.stringify({
        name: args.name,
        description: args.description,
      }),
    },
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

  await relayEmptyRequest(config, `/api/channels/${args.channelId}/topic`, {
    method: "PUT",
    body: JSON.stringify({
      topic: args.topic,
    }),
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

  await relayEmptyRequest(config, `/api/channels/${args.channelId}/purpose`, {
    method: "PUT",
    body: JSON.stringify({
      purpose: args.purpose,
    }),
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

  await relayEmptyRequest(config, `/api/channels/${args.channelId}/archive`, {
    method: "POST",
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

  await relayEmptyRequest(config, `/api/channels/${args.channelId}/unarchive`, {
    method: "POST",
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

  await relayEmptyRequest(config, `/api/channels/${args.channelId}`, {
    method: "DELETE",
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

  return relayJsonRequest<RawAddChannelMembersResponse>(
    config,
    `/api/channels/${args.channelId}/members`,
    {
      method: "POST",
      body: JSON.stringify({
        pubkeys: args.pubkeys,
        role: args.role,
      }),
    },
  );
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

  await relayEmptyRequest(
    config,
    `/api/channels/${args.channelId}/members/${args.pubkey}`,
    {
      method: "DELETE",
    },
  );
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

  await relayEmptyRequest(config, `/api/channels/${args.channelId}/join`, {
    method: "POST",
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

  await relayEmptyRequest(config, `/api/channels/${args.channelId}/leave`, {
    method: "POST",
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

    const mentions = includeType("mentions")
      ? [
          {
            id: "mock-feed-mention",
            kind: 40001,
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
        ].slice(0, limit)
      : [];

    const needsAction = includeType("needs_action")
      ? [
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
        ].slice(0, limit)
      : [];

    const activity = includeType("activity")
      ? [
          {
            id: "mock-feed-self-activity",
            kind: 40001,
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
            kind: 40001,
            pubkey:
              "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260",
            content: "Engineering shipped the desktop build.",
            created_at: now - 42 * 60,
            channel_id: "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9",
            channel_name: "engineering",
            tags: [["e", "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9"]],
            category: "activity" as const,
          },
        ].slice(0, limit)
      : [];

    const agentActivity = includeType("agent_activity")
      ? [
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
        ].slice(0, limit)
      : [];

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
        kind: 40001,
        pubkey: DEFAULT_MOCK_IDENTITY.pubkey,
        channel_id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
        channel_name: "general",
        created_at: now - 60,
        score: 8.5,
      },
      {
        event_id: "mock-engineering-shipped",
        content: "Engineering shipped the desktop build.",
        kind: 40001,
        pubkey:
          "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260",
        channel_id: "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9",
        channel_name: "engineering",
        created_at: now - 42 * 60,
        score: 7.2,
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
        kind: 40001,
        tags: [["e", "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9"]],
        content: "Engineering shipped the desktop build.",
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
    if (!channelId) {
      sendWsText(socket.handler, ["EOSE", subId]);
      return;
    }

    if (subId.startsWith("live-")) {
      socket.subscriptions.set(subId, channelId);
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

  mockWindows("main");
  window.__SPROUT_E2E_COMMANDS__ = [];
  window.__SPROUT_E2E_EMIT_MOCK_MESSAGE__ = ({ channelName, content }) => {
    const channel = mockChannels.find(
      (candidate) => candidate.name === channelName,
    );
    if (!channel) {
      throw new Error(`Mock channel ${channelName} not found.`);
    }

    return emitMockChannelMessage(channel.id, content);
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
      case "get_channels":
        return handleGetChannels(activeConfig);
      case "get_feed":
        return handleGetFeed(
          (payload as Parameters<typeof handleGetFeed>[0]) ?? {},
          activeConfig,
        );
      case "create_channel":
        return handleCreateChannel(
          payload as Parameters<typeof handleCreateChannel>[0],
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
      default:
        throw new Error(`Unsupported mocked Tauri command: ${command}`);
    }
  });

  installed = true;
}
