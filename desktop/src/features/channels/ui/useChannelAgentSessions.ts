import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";
import type { Channel, ChannelMember, ManagedAgent } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";

type UseChannelAgentSessionsOptions = {
  activeChannel: Channel | null;
  activeChannelId: string | null;
  channelMembers?: ChannelMember[];
  handleOpenThread: (message: TimelineMessage) => void;
  managedAgents: ManagedAgent[];
  setExpandedThreadReplyIds: (value: Set<string>) => void;
  setOpenThreadHeadId: (value: string | null) => void;
  setThreadReplyTargetId: (value: string | null) => void;
  setThreadScrollTargetId: (value: string | null) => void;
  targetMessageId: string | null;
  timelineMessages: TimelineMessage[];
};

export function useChannelAgentSessions({
  activeChannel,
  activeChannelId,
  channelMembers,
  handleOpenThread,
  managedAgents,
  setExpandedThreadReplyIds,
  setOpenThreadHeadId,
  setThreadReplyTargetId,
  setThreadScrollTargetId,
  targetMessageId,
  timelineMessages,
}: UseChannelAgentSessionsOptions) {
  const [openAgentSessionPubkey, setOpenAgentSessionPubkey] = React.useState<
    string | null
  >(null);
  const handledThreadTargetIdRef = React.useRef<string | null>(null);

  const channelAgentSessionAgents = React.useMemo<ManagedAgent[]>(() => {
    if (!channelMembers) {
      return [];
    }

    const memberPubkeys = new Set(
      channelMembers.map((member) => normalizePubkey(member.pubkey)),
    );

    return managedAgents.filter(
      (agent) =>
        agent.backend.type === "local" &&
        memberPubkeys.has(normalizePubkey(agent.pubkey)),
    );
  }, [channelMembers, managedAgents]);

  const closeAgentSession = React.useCallback(() => {
    setOpenAgentSessionPubkey(null);
  }, []);

  const openAgentSession = React.useCallback(
    (pubkey: string) => {
      setOpenThreadHeadId(null);
      setExpandedThreadReplyIds(new Set());
      setThreadScrollTargetId(null);
      setThreadReplyTargetId(null);
      setOpenAgentSessionPubkey(pubkey);
    },
    [
      setExpandedThreadReplyIds,
      setOpenThreadHeadId,
      setThreadReplyTargetId,
      setThreadScrollTargetId,
    ],
  );

  const selectAgentSession = React.useCallback((pubkey: string) => {
    setOpenAgentSessionPubkey(pubkey);
  }, []);

  const openThreadAndCloseAgentSession = React.useCallback(
    (message: TimelineMessage) => {
      setOpenAgentSessionPubkey(null);
      handleOpenThread(message);
    },
    [handleOpenThread],
  );

  React.useEffect(() => {
    if (!targetMessageId) {
      handledThreadTargetIdRef.current = null;
      return;
    }

    const targetKey = `${activeChannelId ?? "none"}:${targetMessageId}`;
    if (
      handledThreadTargetIdRef.current !== null &&
      handledThreadTargetIdRef.current !== targetKey
    ) {
      handledThreadTargetIdRef.current = null;
    }

    if (
      handledThreadTargetIdRef.current === targetKey ||
      !activeChannel ||
      activeChannel.channelType === "forum"
    ) {
      return;
    }

    const targetMessage =
      timelineMessages.find((message) => message.id === targetMessageId) ??
      null;

    if (!targetMessage?.parentId) {
      return;
    }

    const threadHeadId = targetMessage.rootId ?? targetMessage.parentId;
    const messageById = new Map(
      timelineMessages.map((message) => [message.id, message]),
    );

    if (!messageById.has(threadHeadId)) {
      return;
    }

    const expandedReplyIds = new Set<string>();
    let ancestorId: string | null = targetMessage.parentId;
    let guard = 0;

    while (
      ancestorId &&
      ancestorId !== threadHeadId &&
      guard < timelineMessages.length
    ) {
      expandedReplyIds.add(ancestorId);
      ancestorId = messageById.get(ancestorId)?.parentId ?? null;
      guard += 1;
    }

    setOpenAgentSessionPubkey(null);
    setOpenThreadHeadId(threadHeadId);
    setThreadReplyTargetId(threadHeadId);
    setThreadScrollTargetId(targetMessageId);
    setExpandedThreadReplyIds(expandedReplyIds);
    handledThreadTargetIdRef.current = targetKey;
  }, [
    activeChannel,
    activeChannelId,
    setExpandedThreadReplyIds,
    setOpenThreadHeadId,
    setThreadReplyTargetId,
    setThreadScrollTargetId,
    targetMessageId,
    timelineMessages,
  ]);

  React.useEffect(() => {
    if (
      openAgentSessionPubkey &&
      !channelAgentSessionAgents.some(
        (agent) => agent.pubkey === openAgentSessionPubkey,
      )
    ) {
      setOpenAgentSessionPubkey(null);
    }
  }, [channelAgentSessionAgents, openAgentSessionPubkey]);

  return {
    channelAgentSessionAgents,
    closeAgentSession,
    openAgentSession,
    openAgentSessionPubkey,
    openThreadAndCloseAgentSession,
    selectAgentSession,
  };
}
