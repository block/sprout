import * as React from "react";

import type { Channel } from "@/shared/api/types";

type ChannelNavigationContextValue = {
  channels: Channel[];
  onOpenChannel: (channelId: string) => void;
};

const ChannelNavigationContext =
  React.createContext<ChannelNavigationContextValue>({
    channels: [],
    onOpenChannel: () => {},
  });

export function ChannelNavigationProvider({
  channels,
  children,
  onOpenChannel,
}: {
  channels: Channel[];
  children: React.ReactNode;
  onOpenChannel: (channelId: string) => void;
}) {
  const value = React.useMemo(
    () => ({ channels, onOpenChannel }),
    [channels, onOpenChannel],
  );

  return (
    <ChannelNavigationContext.Provider value={value}>
      {children}
    </ChannelNavigationContext.Provider>
  );
}

export function useChannelNavigation() {
  return React.useContext(ChannelNavigationContext);
}
