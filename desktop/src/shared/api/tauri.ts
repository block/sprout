import { invoke } from "@tauri-apps/api/core";

import type {
  AddChannelMembersInput,
  AddChannelMembersResult,
  Channel,
  ChannelDetail,
  ChannelMember,
  ChannelType,
  CreateChannelInput,
  GetHomeFeedInput,
  HomeFeedResponse,
  Identity,
  Profile,
  RelayEvent,
  SearchMessagesInput,
  SearchMessagesResponse,
  SetChannelPurposeInput,
  SetChannelTopicInput,
  UpdateProfileInput,
  UpdateChannelInput,
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
  channel_id: string;
  channel_name: string;
  created_at: number;
  score: number;
};

type RawSearchResponse = {
  hits: RawSearchHit[];
  found: number;
};

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

export async function getIdentity(): Promise<Identity> {
  const identity = await invoke<RawIdentity>("get_identity");

  return {
    pubkey: identity.pubkey,
    displayName: identity.display_name,
  };
}

export async function getProfile(): Promise<Profile> {
  const profile = await invoke<RawProfile>("get_profile");
  return fromRawProfile(profile);
}

export async function updateProfile(
  input: UpdateProfileInput,
): Promise<Profile> {
  const profile = await invoke<RawProfile>("update_profile", input);
  return fromRawProfile(profile);
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

export async function getChannelDetails(
  channelId: string,
): Promise<ChannelDetail> {
  const channel = await invoke<RawChannelDetail>("get_channel_details", {
    channelId,
  });
  return fromRawChannelDetail(channel);
}

export async function getChannelMembers(
  channelId: string,
): Promise<ChannelMember[]> {
  const response = await invoke<RawChannelMembersResponse>(
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
  const channel = await invoke<RawChannelDetail>("update_channel", input);
  return fromRawChannelDetail(channel);
}

export async function setChannelTopic(
  input: SetChannelTopicInput,
): Promise<void> {
  await invoke("set_channel_topic", input);
}

export async function setChannelPurpose(
  input: SetChannelPurposeInput,
): Promise<void> {
  await invoke("set_channel_purpose", input);
}

export async function archiveChannel(channelId: string): Promise<void> {
  await invoke("archive_channel", { channelId });
}

export async function unarchiveChannel(channelId: string): Promise<void> {
  await invoke("unarchive_channel", { channelId });
}

export async function deleteChannel(channelId: string): Promise<void> {
  await invoke("delete_channel", { channelId });
}

export async function addChannelMembers(
  input: AddChannelMembersInput,
): Promise<AddChannelMembersResult> {
  return invoke<RawAddChannelMembersResult>("add_channel_members", input);
}

export async function removeChannelMember(
  channelId: string,
  pubkey: string,
): Promise<void> {
  await invoke("remove_channel_member", { channelId, pubkey });
}

export async function joinChannel(channelId: string): Promise<void> {
  await invoke("join_channel", { channelId });
}

export async function leaveChannel(channelId: string): Promise<void> {
  await invoke("leave_channel", { channelId });
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

export async function searchMessages(
  input: SearchMessagesInput,
): Promise<SearchMessagesResponse> {
  const response = await invoke<RawSearchResponse>("search_messages", input);

  return {
    hits: response.hits.map(fromRawSearchHit),
    found: response.found,
  };
}

export async function getEventById(eventId: string): Promise<RelayEvent> {
  const eventJson = await invoke<string>("get_event", { eventId });
  return JSON.parse(eventJson) as RelayEvent;
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
