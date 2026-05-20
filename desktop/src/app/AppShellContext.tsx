import * as React from "react";

type AppShellContextValue = {
  markChannelRead: (
    channelId: string,
    readAt: string | null | undefined,
  ) => void;
  markChannelUnread: (
    channelId: string,
    lastMessageAt: string | null | undefined,
  ) => void;
  openChannelManagement: () => void;
  // NIP-RS read marker for a channel as a unix-seconds timestamp, or null
  // when unknown. Backed by the single AppShell-mounted ReadStateManager so
  // every surface (sidebar, home, badges) projects from the same source.
  getChannelReadAt: (channelId: string) => number | null;
  // Bump-counter that invalidates whenever the read marker changes. Include
  // in memo deps that consume getChannelReadAt.
  readStateVersion: number;
};

const AppShellContext = React.createContext<AppShellContextValue>({
  markChannelRead: () => {},
  markChannelUnread: () => {},
  openChannelManagement: () => {},
  getChannelReadAt: () => null,
  readStateVersion: 0,
});

export function AppShellProvider({
  children,
  value,
}: {
  children: React.ReactNode;
  value: AppShellContextValue;
}) {
  return (
    <AppShellContext.Provider value={value}>
      {children}
    </AppShellContext.Provider>
  );
}

export function useAppShell() {
  return React.useContext(AppShellContext);
}
