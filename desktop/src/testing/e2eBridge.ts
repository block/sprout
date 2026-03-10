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

type RawChannel = {
  id: string;
  name: string;
  channel_type: "stream" | "forum" | "dm";
  description: string;
  participants: string[];
  participant_pubkeys: string[];
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

type WsHandler = (message: unknown) => void;

type MockSocket = {
  handler: WsHandler;
  subscriptions: Map<string, string>;
};

declare global {
  interface Window {
    __SPROUT_E2E__?: E2eConfig;
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

const mockChannels: RawChannel[] = [
  {
    id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
    name: "general",
    channel_type: "stream",
    description: "General discussion for everyone",
    participants: [],
    participant_pubkeys: [],
  },
  {
    id: "9dae0116-799b-5071-a0a8-fdd30a91a35d",
    name: "random",
    channel_type: "stream",
    description: "Off-topic, fun stuff",
    participants: [],
    participant_pubkeys: [],
  },
  {
    id: "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9",
    name: "engineering",
    channel_type: "stream",
    description: "Engineering discussions",
    participants: [],
    participant_pubkeys: [],
  },
  {
    id: "94a444a4-c0a3-5966-ab05-530c6ddc2301",
    name: "agents",
    channel_type: "stream",
    description: "AI agent testing and collaboration",
    participants: [],
    participant_pubkeys: [],
  },
  {
    id: "a27e1ee9-76a6-5bdf-a5d5-1d85610dad11",
    name: "watercooler",
    channel_type: "forum",
    description: "Casual forum for async discussions",
    participants: [],
    participant_pubkeys: [],
  },
  {
    id: "1be1dcdb-4c31-5a8c-81de-ac102552ca10",
    name: "announcements",
    channel_type: "forum",
    description: "Company announcements",
    participants: [],
    participant_pubkeys: [],
  },
  {
    id: "f48efb06-0c93-5025-aac9-2e646bb6bfa8",
    name: "alice-tyler",
    channel_type: "dm",
    description: "DM between alice and tyler",
    participants: ["alice", "tyler"],
    participant_pubkeys: [
      "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f",
      "e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34",
    ],
  },
  {
    id: "7eb9f239-9393-50b0-bd76-d85eef0511c7",
    name: "bob-tyler",
    channel_type: "dm",
    description: "DM between bob and tyler",
    participants: ["bob", "tyler"],
    participant_pubkeys: [
      "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260",
      "e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34",
    ],
  },
];

const mockMessages = new Map<string, RelayEvent[]>();
const mockSockets = new Map<number, MockSocket>();
const realSockets = new Map<number, WebSocket>();

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
  return tags.find((tag) => tag[0] === "e")?.[1];
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
            tags: [["e", channelId]],
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

async function handleGetChannels(config: E2eConfig | undefined) {
  const identity = getIdentity(config);
  if (!identity) {
    return mockChannels;
  }

  const response = await fetch(`${getRelayHttpUrl(config)}/api/channels`, {
    headers: {
      "X-Pubkey": identity.pubkey,
    },
  });
  await assertOk(response);
  return response.json();
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
    const channel: RawChannel = {
      id: crypto.randomUUID(),
      name: args.name,
      channel_type: args.channelType,
      description: args.description ?? "",
      participants: [],
      participant_pubkeys: [],
    };
    mockChannels.push(channel);
    return channel;
  }

  const response = await fetch(`${getRelayHttpUrl(config)}/api/channels`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Pubkey": identity.pubkey,
    },
    body: JSON.stringify({
      name: args.name,
      channel_type: args.channelType,
      visibility: args.visibility,
      description: args.description,
    }),
  });
  await assertOk(response);
  return response.json();
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
    const filter = rest[1] as { "#e"?: string[] };
    const channelId = filter["#e"]?.[0];
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

    const history = getMockMessageStore(channelId);
    history.push(event);
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
  mockIPC(async (command, payload) => {
    const activeConfig = getConfig();
    const identity = getIdentity(activeConfig);

    switch (command) {
      case "get_identity":
        if (identity) {
          return {
            pubkey: identity.pubkey,
            display_name: identity.username,
          };
        }

        return DEFAULT_MOCK_IDENTITY;
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
