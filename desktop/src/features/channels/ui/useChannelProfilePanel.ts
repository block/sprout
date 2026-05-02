import * as React from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useOpenDmMutation } from "@/features/channels/hooks";

export function useChannelProfilePanel() {
  const { goChannel } = useAppNavigation();
  const openDmMutation = useOpenDmMutation();
  const [profilePanelPubkey, setProfilePanelPubkey] = React.useState<
    string | null
  >(null);

  const handleOpenProfilePanel = React.useCallback((pubkey: string) => {
    setProfilePanelPubkey(pubkey);
  }, []);

  const handleCloseProfilePanel = React.useCallback(() => {
    setProfilePanelPubkey(null);
  }, []);

  const handleOpenDm = React.useCallback(
    async (pubkeys: string[]) => {
      const dm = await openDmMutation.mutateAsync({ pubkeys });
      await goChannel(dm.id);
    },
    [goChannel, openDmMutation],
  );

  return {
    profilePanelPubkey,
    setProfilePanelPubkey,
    handleOpenProfilePanel,
    handleCloseProfilePanel,
    handleOpenDm,
  };
}
