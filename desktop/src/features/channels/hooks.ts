import * as React from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

import {
  addChannelMembers,
  archiveChannel,
  createChannel,
  deleteChannel,
  getChannelDetails,
  getChannelMembers,
  getChannels,
  joinChannel,
  leaveChannel,
  openDm,
  removeChannelMember,
  setChannelPurpose,
  setChannelTopic,
  unarchiveChannel,
  updateChannel,
} from "@/shared/api/tauri";
import type {
  AddChannelMembersInput,
  Channel,
  ChannelDetail,
  CreateChannelInput,
  OpenDmInput,
  SetChannelPurposeInput,
  SetChannelTopicInput,
  UpdateChannelInput,
} from "@/shared/api/types";

export const channelsQueryKey = ["channels"] as const;
const channelDetailQueryKey = (channelId: string) =>
  ["channels", channelId, "detail"] as const;
const channelMembersQueryKey = (channelId: string) =>
  ["channels", channelId, "members"] as const;
const channelTypeOrder = {
  stream: 0,
  forum: 1,
  dm: 2,
} as const;

function sortChannels(channels: Channel[]) {
  const uniqueChannels = new Map<string, Channel>();

  for (const channel of channels) {
    uniqueChannels.set(channel.id, channel);
  }

  return [...uniqueChannels.values()].sort((left, right) => {
    const typeOrder =
      channelTypeOrder[left.channelType] - channelTypeOrder[right.channelType];

    if (typeOrder !== 0) {
      return typeOrder;
    }

    return left.name.localeCompare(right.name);
  });
}

function parseTimestamp(value: string | null | undefined) {
  if (!value) {
    return null;
  }

  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? null : timestamp;
}

function isNewerTimestamp(
  candidate: string | null | undefined,
  current: string | null | undefined,
) {
  const candidateTimestamp = parseTimestamp(candidate);
  if (candidateTimestamp === null) {
    return false;
  }

  const currentTimestamp = parseTimestamp(current);
  return currentTimestamp === null || candidateTimestamp > currentTimestamp;
}

export function updateChannelLastMessageAt(
  queryClient: QueryClient,
  channelId: string,
  lastMessageAt: string | null | undefined,
) {
  const lastMessageTimestamp = parseTimestamp(lastMessageAt);
  const normalizedLastMessageAt =
    lastMessageTimestamp === null
      ? null
      : new Date(lastMessageTimestamp).toISOString();

  if (!normalizedLastMessageAt) {
    return;
  }

  queryClient.setQueryData<Channel[]>(channelsQueryKey, (current) => {
    if (!current) {
      return current;
    }

    let didUpdate = false;
    const nextChannels = current.map((channel) => {
      if (
        channel.id !== channelId ||
        !isNewerTimestamp(normalizedLastMessageAt, channel.lastMessageAt)
      ) {
        return channel;
      }

      didUpdate = true;
      return {
        ...channel,
        lastMessageAt: normalizedLastMessageAt,
      };
    });

    return didUpdate ? nextChannels : current;
  });
}

async function invalidateChannelState(
  queryClient: ReturnType<typeof useQueryClient>,
  channelId: string | null | undefined,
) {
  await queryClient.invalidateQueries({ queryKey: channelsQueryKey });

  if (!channelId) {
    return;
  }

  await Promise.all([
    queryClient.invalidateQueries({
      queryKey: channelDetailQueryKey(channelId),
    }),
    queryClient.invalidateQueries({
      queryKey: channelMembersQueryKey(channelId),
    }),
  ]);
}

export function useChannelsQuery() {
  return useQuery({
    queryKey: channelsQueryKey,
    queryFn: async () => sortChannels(await getChannels()),
    staleTime: 60_000,
    refetchInterval: 60_000,
    refetchIntervalInBackground: false,
  });
}

export function useCreateChannelMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: CreateChannelInput) => createChannel(input),
    onSuccess: (createdChannel) => {
      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        sortChannels([
          ...current.filter((channel) => channel.id !== createdChannel.id),
          createdChannel,
        ]),
      );
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    },
  });
}

export function useOpenDmMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: OpenDmInput) => openDm(input),
    onSuccess: (openedChannel) => {
      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        sortChannels([
          ...current.filter((channel) => channel.id !== openedChannel.id),
          openedChannel,
        ]),
      );
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    },
  });
}

export function useChannelDetailsQuery(
  channelId: string | null,
  enabled = true,
) {
  return useQuery({
    enabled: enabled && channelId !== null,
    queryKey: ["channels", channelId ?? "none", "detail"],
    queryFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return getChannelDetails(channelId);
    },
    staleTime: 30_000,
  });
}

export function useChannelMembersQuery(
  channelId: string | null,
  enabled = true,
) {
  return useQuery({
    enabled: enabled && channelId !== null,
    queryKey: ["channels", channelId ?? "none", "members"],
    queryFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return getChannelMembers(channelId);
    },
    staleTime: 30_000,
  });
}

export function useUpdateChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: Omit<UpdateChannelInput, "channelId">) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return updateChannel({ ...input, channelId });
    },
    onSuccess: (updatedChannel) => {
      if (!channelId) {
        return;
      }

      queryClient.setQueryData<ChannelDetail>(
        channelDetailQueryKey(channelId),
        updatedChannel,
      );
      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        sortChannels(
          current.map((channel) =>
            channel.id === updatedChannel.id ? updatedChannel : channel,
          ),
        ),
      );
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useSetChannelTopicMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: Omit<SetChannelTopicInput, "channelId">) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return setChannelTopic({ ...input, channelId });
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useSetChannelPurposeMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: Omit<SetChannelPurposeInput, "channelId">) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return setChannelPurpose({ ...input, channelId });
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useArchiveChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await archiveChannel(channelId);
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useUnarchiveChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await unarchiveChannel(channelId);
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useDeleteChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await deleteChannel(channelId);
    },
    onSuccess: () => {
      if (!channelId) {
        return;
      }

      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        current.filter((channel) => channel.id !== channelId),
      );
      queryClient.removeQueries({
        queryKey: channelDetailQueryKey(channelId),
      });
      queryClient.removeQueries({
        queryKey: channelMembersQueryKey(channelId),
      });
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    },
  });
}

export function useAddChannelMembersMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: Omit<AddChannelMembersInput, "channelId">) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return addChannelMembers({ ...input, channelId });
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useRemoveChannelMemberMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (pubkey: string) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await removeChannelMember(channelId, pubkey);
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useJoinChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await joinChannel(channelId);
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useLeaveChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await leaveChannel(channelId);
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useSelectedChannel(
  channels: Channel[],
  preferredChannelId: string | null,
) {
  const [selectedChannelId, setSelectedChannelId] = React.useState<
    string | null
  >(preferredChannelId);

  const selectedChannel = React.useMemo(
    () =>
      channels.find((channel) => channel.id === selectedChannelId) ??
      channels.find((channel) => channel.channelType !== "forum") ??
      channels[0] ??
      null,
    [channels, selectedChannelId],
  );

  React.useEffect(() => {
    if (!selectedChannel && channels.length === 0) {
      return;
    }

    if (!selectedChannelId && selectedChannel) {
      setSelectedChannelId(selectedChannel.id);
      return;
    }

    if (
      selectedChannelId &&
      !channels.some((channel) => channel.id === selectedChannelId) &&
      selectedChannel
    ) {
      setSelectedChannelId(selectedChannel.id);
    }
  }, [channels, selectedChannel, selectedChannelId]);

  return {
    selectedChannel,
    selectedChannelId,
    setSelectedChannelId,
  };
}
