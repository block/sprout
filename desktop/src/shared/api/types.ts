export type ChannelType = "stream" | "forum" | "dm";
export type ChannelVisibility = "open" | "private";

export type Channel = {
  id: string;
  name: string;
  channelType: ChannelType;
  description: string;
  participants: string[];
  participantPubkeys: string[];
};

export type CreateChannelInput = {
  name: string;
  channelType: Exclude<ChannelType, "dm">;
  visibility: ChannelVisibility;
  description?: string;
};

export type Identity = {
  pubkey: string;
  displayName: string;
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
