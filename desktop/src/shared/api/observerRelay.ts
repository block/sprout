import { buildObserverControlEvent } from "@/shared/api/tauriObserver";
import type { RelayEvent } from "@/shared/api/types";
import { KIND_AGENT_OBSERVER_FRAME } from "@/shared/constants/kinds";
import { relayClient } from "./relayClient";

export function subscribeToAgentObserverFrames(
  ownerPubkey: string,
  onEvent: (event: RelayEvent) => void,
) {
  return relayClient.subscribeLive(
    {
      kinds: [KIND_AGENT_OBSERVER_FRAME],
      "#p": [ownerPubkey],
      limit: 0,
      since: Math.floor(Date.now() / 1_000),
    },
    onEvent,
  );
}

export async function sendAgentObserverControl(
  agentPubkey: string,
  payload: unknown,
) {
  await relayClient.preconnect();
  const event = await buildObserverControlEvent({ agentPubkey, payload });
  await relayClient.publishEvent(
    event,
    "Timed out while sending the agent control command.",
    "Failed to send the agent control command.",
  );
}
