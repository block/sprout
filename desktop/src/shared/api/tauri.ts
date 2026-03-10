import { invoke } from "@tauri-apps/api/core";

import type {
  Channel,
  ChannelType,
  CreateChannelInput,
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
