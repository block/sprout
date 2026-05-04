import * as React from "react";

import type { ManagedAgent } from "@/shared/api/types";

type AgentSessionContextValue = {
  onDetachAgentSession: ((agent: ManagedAgent) => void) | null;
  onOpenAgentSession: ((pubkey: string) => void) | null;
};

const AgentSessionContext = React.createContext<AgentSessionContextValue>({
  onDetachAgentSession: null,
  onOpenAgentSession: null,
});

export function AgentSessionProvider({
  children,
  onDetachAgentSession,
  onOpenAgentSession,
}: {
  children: React.ReactNode;
  onDetachAgentSession?: (agent: ManagedAgent) => void;
  onOpenAgentSession: (pubkey: string) => void;
}) {
  const value = React.useMemo(
    () => ({
      onDetachAgentSession: onDetachAgentSession ?? null,
      onOpenAgentSession,
    }),
    [onDetachAgentSession, onOpenAgentSession],
  );

  return (
    <AgentSessionContext.Provider value={value}>
      {children}
    </AgentSessionContext.Provider>
  );
}

export function useAgentSession() {
  return React.useContext(AgentSessionContext);
}
