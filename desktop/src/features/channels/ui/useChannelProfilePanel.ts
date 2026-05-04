import * as React from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useOpenDmMutation } from "@/features/channels/hooks";

type UseChannelProfilePanelOptions = {
  closeAgentSession: () => void;
  setExpandedThreadReplyIds: (value: Set<string>) => void;
  setOpenThreadHeadId: (value: string | null) => void;
  setProfilePanelPubkey: (value: string | null) => void;
  setThreadReplyTargetId: (value: string | null) => void;
  setThreadScrollTargetId: (value: string | null) => void;
};

export function useChannelProfilePanel({
  closeAgentSession,
  setExpandedThreadReplyIds,
  setOpenThreadHeadId,
  setProfilePanelPubkey,
  setThreadReplyTargetId,
  setThreadScrollTargetId,
}: UseChannelProfilePanelOptions) {
  const { goChannel } = useAppNavigation();
  const openDmMutation = useOpenDmMutation();

  const handleOpenProfilePanel = React.useCallback(
    (pubkey: string) => {
      setOpenThreadHeadId(null);
      setExpandedThreadReplyIds(new Set());
      setThreadScrollTargetId(null);
      setThreadReplyTargetId(null);
      closeAgentSession();
      setProfilePanelPubkey(pubkey);
    },
    [
      closeAgentSession,
      setExpandedThreadReplyIds,
      setOpenThreadHeadId,
      setProfilePanelPubkey,
      setThreadReplyTargetId,
      setThreadScrollTargetId,
    ],
  );

  const handleCloseProfilePanel = React.useCallback(() => {
    setProfilePanelPubkey(null);
  }, [setProfilePanelPubkey]);

  const handleOpenDm = React.useCallback(
    async (pubkeys: string[]) => {
      const dm = await openDmMutation.mutateAsync({ pubkeys });
      await goChannel(dm.id);
    },
    [goChannel, openDmMutation],
  );

  return {
    handleOpenProfilePanel,
    handleCloseProfilePanel,
    handleOpenDm,
  };
}
