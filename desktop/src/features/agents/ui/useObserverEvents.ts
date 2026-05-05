import * as React from "react";

import {
  ensureRelayObserverSubscription,
  getAgentObserverSnapshot,
  subscribeAgentObserverStore,
} from "@/features/agents/observerRelayStore";
import type { ConnectionState, ObserverEvent } from "./agentSessionTypes";

export function useObserverEvents(
  enabled: boolean,
  agentPubkey?: string | null,
) {
  const [events, setEvents] = React.useState<ObserverEvent[]>([]);
  const [connectionState, setConnectionState] =
    React.useState<ConnectionState>("idle");
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);

  React.useEffect(() => {
    setEvents([]);
    setErrorMessage(null);

    if (!enabled) {
      setConnectionState("idle");
      return;
    }

    if (!agentPubkey) {
      setConnectionState("idle");
      return;
    }

    const syncSnapshot = () => {
      const snapshot = getAgentObserverSnapshot(agentPubkey);
      setConnectionState(snapshot.connectionState);
      setErrorMessage(snapshot.errorMessage);
      setEvents(snapshot.events);
    };

    syncSnapshot();
    const unsubscribe = subscribeAgentObserverStore(syncSnapshot);
    void ensureRelayObserverSubscription();

    return () => {
      unsubscribe();
    };
  }, [agentPubkey, enabled]);

  return { connectionState, errorMessage, events };
}
