import { invoke } from "@tauri-apps/api/core";

import type {
  Channel,
  ChannelType,
  CreateChannelInput,
  GetHomeFeedInput,
  HomeFeedResponse,
  Identity,
  RelayEvent,
} from "@/shared/api/types";

type RawIdentity = {
  pubkey: string;
  display_name: string;
};

type RawChannel = {
  id: string;
  name: string;
  channel_type: ChannelType;
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

function fromRawChannel(channel: RawChannel): Channel {
  return {
    id: channel.id,
    name: channel.name,
    channelType: channel.channel_type,
    description: channel.description,
    participants: channel.participants,
    participantPubkeys: channel.participant_pubkeys,
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

export async function getIdentity(): Promise<Identity> {
  const identity = await invoke<RawIdentity>("get_identity");

  return {
    pubkey: identity.pubkey,
    displayName: identity.display_name,
  };
}

export function getRelayWsUrl(): Promise<string> {
  return invoke<string>("get_relay_ws_url");
}

export async function getChannels(): Promise<Channel[]> {
  const channels = await invoke<RawChannel[]>("get_channels");
  return channels.map(fromRawChannel);
}

export async function createChannel(
  input: CreateChannelInput,
): Promise<Channel> {
  const channel = await invoke<RawChannel>("create_channel", input);
  return fromRawChannel(channel);
}

export async function getHomeFeed(
  input: GetHomeFeedInput = {},
): Promise<HomeFeedResponse> {
  const response = await invoke<RawHomeFeedResponse>("get_feed", input);

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

export async function signRelayEvent(input: {
  kind: number;
  content: string;
  tags: string[][];
}): Promise<RelayEvent> {
  const eventJson = await invoke<string>("sign_event", input);
  return JSON.parse(eventJson) as RelayEvent;
}

export async function createAuthEvent(input: {
  challenge: string;
  relayUrl: string;
}): Promise<RelayEvent> {
  const eventJson = await invoke<string>("create_auth_event", input);
  return JSON.parse(eventJson) as RelayEvent;
}
