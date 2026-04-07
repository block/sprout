import * as React from "react";

import type { Channel } from "@/shared/api/types";

type ChannelNavigationContextValue = {
  channels: Channel[];
};

const ChannelNavigationContext =
  React.createContext<ChannelNavigationContextValue>({
    channels: [],
  });

export function ChannelNavigationProvider({
  channels,
  children,
}: {
  channels: Channel[];
  children: React.ReactNode;
}) {
  const value = React.useMemo(() => ({ channels }), [channels]);

  return (
    <ChannelNavigationContext.Provider value={value}>
      {children}
    </ChannelNavigationContext.Provider>
  );
}

export function useChannelNavigation() {
  return React.useContext(ChannelNavigationContext);
}
