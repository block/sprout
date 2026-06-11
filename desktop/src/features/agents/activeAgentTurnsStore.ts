import * as React from "react";

import {
  subscribeAgentObserverStore,
  getAgentObserverSnapshot,
} from "@/features/agents/observerRelayStore";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { ObserverEvent } from "./ui/agentSessionTypes";

/** How long before a turn is considered stale if no completion event arrives. */
const STALENESS_TIMEOUT_MS = 5 * 60 * 1000;

type ActiveTurn = {
  turnId: string;
  channelId: string;
  startedAt: number;
};

// Module-level state: agentPubkey → channelId → ActiveTurn
const activeTurnsByAgent = new Map<string, Map<string, ActiveTurn>>();
const listeners = new Set<() => void>();

// Track which observer events we've already processed (by seq per agent)
const lastProcessedSeq = new Map<string, number>();

let cleanupInterval: ReturnType<typeof setInterval> | null = null;

function notifyListeners() {
  for (const listener of listeners) {
    listener();
  }
}

function startTurn(
  agentPubkey: string,
  channelId: string,
  turnId: string,
  timestamp: string,
) {
  const key = normalizePubkey(agentPubkey);
  let agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns) {
    agentTurns = new Map();
    activeTurnsByAgent.set(key, agentTurns);
  }
  agentTurns.set(channelId, {
    turnId,
    channelId,
    startedAt: Date.parse(timestamp) || Date.now(),
  });
}

function endTurn(agentPubkey: string, channelId: string | null) {
  const key = normalizePubkey(agentPubkey);
  const agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns) return;

  if (channelId) {
    agentTurns.delete(channelId);
  }
  if (agentTurns.size === 0) {
    activeTurnsByAgent.delete(key);
  }
}

function pruneStale() {
  const now = Date.now();
  let changed = false;
  for (const [agentKey, agentTurns] of activeTurnsByAgent) {
    for (const [channelId, turn] of agentTurns) {
      if (now - turn.startedAt > STALENESS_TIMEOUT_MS) {
        agentTurns.delete(channelId);
        changed = true;
      }
    }
    if (agentTurns.size === 0) {
      activeTurnsByAgent.delete(agentKey);
    }
  }
  if (changed) {
    notifyListeners();
  }
}

function processEvent(agentPubkey: string, event: ObserverEvent) {
  const key = normalizePubkey(agentPubkey);
  const lastSeq = lastProcessedSeq.get(key) ?? 0;
  if (event.seq <= lastSeq) return;
  lastProcessedSeq.set(key, event.seq);

  if (event.kind === "turn_started" && event.channelId) {
    startTurn(
      agentPubkey,
      event.channelId,
      event.turnId ?? `seq-${event.seq}`,
      event.timestamp,
    );
    notifyListeners();
  } else if (
    event.kind === "turn_completed" ||
    event.kind === "turn_error" ||
    event.kind === "agent_panic"
  ) {
    endTurn(agentPubkey, event.channelId);
    notifyListeners();
  }
}

function ensureCleanupInterval() {
  if (cleanupInterval) return;
  cleanupInterval = setInterval(pruneStale, 30_000);
}

function stopCleanupInterval() {
  if (cleanupInterval) {
    clearInterval(cleanupInterval);
    cleanupInterval = null;
  }
}

// ─── Public API ──────────────────────────────────────────────────────────────

export function subscribeActiveAgentTurns(listener: () => void) {
  listeners.add(listener);
  if (listeners.size === 1) {
    ensureCleanupInterval();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      stopCleanupInterval();
    }
  };
}

export function getActiveChannelsForAgent(
  agentPubkey: string | null | undefined,
): Set<string> {
  if (!agentPubkey) return EMPTY_SET;
  const key = normalizePubkey(agentPubkey);
  const agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns || agentTurns.size === 0) return EMPTY_SET;
  return new Set(agentTurns.keys());
}

const EMPTY_SET: Set<string> = new Set();

/**
 * Synchronize the active-turns store with the latest observer events for a
 * given agent. Call this from within a React component that has access to the
 * observer snapshot.
 */
export function syncAgentTurnsFromEvents(
  agentPubkey: string,
  events: ObserverEvent[],
) {
  for (const event of events) {
    processEvent(agentPubkey, event);
  }
}

/**
 * Hook: returns the set of channel IDs where the given agent is currently working.
 * Re-renders when the set changes.
 */
export function useActiveAgentTurns(
  agentPubkey: string | null | undefined,
): Set<string> {
  const getSnapshot = React.useCallback(
    () => getActiveChannelsForAgent(agentPubkey),
    [agentPubkey],
  );

  return React.useSyncExternalStore(subscribeActiveAgentTurns, getSnapshot);
}

/**
 * Bridge hook: processes observer events into the active-turns store.
 * Should be called by a parent component that has access to the observer events.
 */
export function useActiveAgentTurnsBridge(
  agents: readonly { pubkey: string; status: string }[],
) {
  // Subscribe to observer store changes and sync active turns
  React.useEffect(() => {
    function syncAll() {
      for (const agent of agents) {
        if (agent.status !== "running" && agent.status !== "deployed") continue;
        const snapshot = getAgentObserverSnapshot(agent.pubkey, true);
        syncAgentTurnsFromEvents(agent.pubkey, snapshot.events);
      }
    }

    syncAll();
    return subscribeAgentObserverStore(syncAll);
  }, [agents]);
}

export function resetActiveAgentTurnsStore() {
  activeTurnsByAgent.clear();
  lastProcessedSeq.clear();
  notifyListeners();
}
