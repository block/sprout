import * as React from "react";

import { subscribeToAgentObserverFrames } from "@/shared/api/observerRelay";
import type { RelayEvent, ManagedAgent } from "@/shared/api/types";
import { getIdentity } from "@/shared/api/tauri";
import { decryptObserverEvent } from "@/shared/api/tauriObserver";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { ConnectionState, ObserverEvent } from "./ui/agentSessionTypes";

const MAX_OBSERVER_EVENTS = 800;

type ObserverSnapshot = {
  connectionState: ConnectionState;
  errorMessage: string | null;
  events: ObserverEvent[];
};

const listeners = new Set<() => void>();
const eventsByAgent = new Map<string, ObserverEvent[]>();

// Normalized pubkeys of agents we are actively managing. Only events whose
// "agent" tag matches an entry here will be decrypted (defense-in-depth).
const knownAgentPubkeys = new Set<string>();

let connectionState: ConnectionState = "idle";
let errorMessage: string | null = null;
let unsubscribeRelay: (() => Promise<void>) | null = null;
let startPromise: Promise<void> | null = null;
let eventProcessingQueue: Promise<void> = Promise.resolve();
let generation = 0;

function notifyListeners() {
  for (const listener of listeners) {
    listener();
  }
}

function setConnectionState(
  nextState: ConnectionState,
  nextErrorMessage: string | null = errorMessage,
) {
  connectionState = nextState;
  errorMessage = nextErrorMessage;
  notifyListeners();
}

function observerTag(event: RelayEvent, tagName: string) {
  return event.tags.find((tag) => tag[0] === tagName)?.[1] ?? null;
}

function appendAgentEvent(agentPubkey: string, event: ObserverEvent) {
  const key = normalizePubkey(agentPubkey);
  const current = eventsByAgent.get(key) ?? [];
  if (
    current.some(
      (existing) =>
        existing.seq === event.seq && existing.timestamp === event.timestamp,
    )
  ) {
    return;
  }

  const next = [...current, event].sort(compareObserverEvents);
  eventsByAgent.set(
    key,
    next.length > MAX_OBSERVER_EVENTS
      ? next.slice(next.length - MAX_OBSERVER_EVENTS)
      : next,
  );
  notifyListeners();
}

function compareObserverEvents(left: ObserverEvent, right: ObserverEvent) {
  const leftTime = Date.parse(left.timestamp);
  const rightTime = Date.parse(right.timestamp);
  if (Number.isFinite(leftTime) && Number.isFinite(rightTime)) {
    const timeDiff = leftTime - rightTime;
    if (timeDiff !== 0) {
      return timeDiff;
    }
  }

  return left.seq - right.seq;
}

async function handleRelayObserverEvent(
  event: RelayEvent,
  activeGeneration: number,
) {
  const agentPubkey = observerTag(event, "agent");
  const frame = observerTag(event, "frame");
  if (!agentPubkey || frame !== "telemetry") {
    return;
  }

  // Verify agent is known/trusted before decrypting.
  // Silently drop events from agents we are not managing.
  if (!knownAgentPubkeys.has(normalizePubkey(agentPubkey))) {
    return;
  }

  try {
    const parsed = (await decryptObserverEvent(event)) as ObserverEvent;
    if (activeGeneration !== generation) {
      return;
    }
    appendAgentEvent(agentPubkey, parsed);
  } catch (error) {
    if (activeGeneration !== generation) {
      return;
    }
    setConnectionState(
      "error",
      error instanceof Error
        ? `Observer event decrypt failed: ${error.message}`
        : "Observer event decrypt failed.",
    );
  }
}

export function ensureRelayObserverSubscription() {
  if (unsubscribeRelay) {
    return Promise.resolve();
  }
  if (startPromise) {
    return startPromise;
  }

  const activeGeneration = generation;
  setConnectionState("connecting", null);
  startPromise = (async () => {
    const identity = await getIdentity();
    const unsubscribe = await subscribeToAgentObserverFrames(
      identity.pubkey,
      (event) => {
        eventProcessingQueue = eventProcessingQueue
          .then(() => handleRelayObserverEvent(event, activeGeneration))
          .catch((error) => {
            if (activeGeneration !== generation) {
              return;
            }
            setConnectionState(
              "error",
              error instanceof Error
                ? `Observer event handling failed: ${error.message}`
                : "Observer event handling failed.",
            );
          });
      },
    );
    if (activeGeneration !== generation) {
      await unsubscribe();
      return;
    }
    unsubscribeRelay = unsubscribe;
    setConnectionState("open", null);
  })()
    .catch((error) => {
      if (activeGeneration === generation) {
        setConnectionState(
          "error",
          error instanceof Error
            ? error.message
            : "Observer relay subscription failed.",
        );
      }
    })
    .finally(() => {
      if (activeGeneration === generation) {
        startPromise = null;
      }
    });

  return startPromise;
}

export function subscribeAgentObserverStore(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getAgentObserverSnapshot(
  agentPubkey: string,
): ObserverSnapshot {
  return {
    connectionState,
    errorMessage,
    events: eventsByAgent.get(normalizePubkey(agentPubkey)) ?? [],
  };
}

export function useManagedAgentObserverBridge(agents: readonly ManagedAgent[]) {
  const hasActiveAgent = React.useMemo(
    () =>
      agents.some(
        (agent) => agent.status === "running" || agent.status === "deployed",
      ),
    [agents],
  );

  // Keep the trusted-pubkey set in sync with the current managed agent list.
  React.useEffect(() => {
    knownAgentPubkeys.clear();
    for (const agent of agents) {
      knownAgentPubkeys.add(normalizePubkey(agent.pubkey));
    }
  }, [agents]);

  React.useEffect(() => {
    if (!hasActiveAgent) {
      return;
    }
    void ensureRelayObserverSubscription();
  }, [hasActiveAgent]);
}

export function resetAgentObserverStore() {
  generation += 1;
  const unsubscribe = unsubscribeRelay;
  unsubscribeRelay = null;
  startPromise = null;
  eventProcessingQueue = Promise.resolve();
  eventsByAgent.clear();
  connectionState = "idle";
  errorMessage = null;
  notifyListeners();
  void unsubscribe?.();
}
