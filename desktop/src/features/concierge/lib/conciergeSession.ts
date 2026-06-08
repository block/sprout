import {
  createManagedAgent,
  listManagedAgents,
  openDm,
  startManagedAgent,
} from "@/shared/api/tauri";
import {
  meshAgentPreset,
  meshAvailability,
  type MeshServeTarget,
} from "@/shared/api/tauriMesh";
import { startRelayMeshClientForTarget } from "@/features/mesh-compute/startRelayMeshClientForTarget";
import { meshAgentPresetPatch } from "@/features/mesh-compute/applyMeshAgentPreset";
import type { Channel, ManagedAgent } from "@/shared/api/types";

import { CONCIERGE_AGENT_NAME, CONCIERGE_SYSTEM_PROMPT } from "./prompt";

export type ConciergeSession = {
  agent: ManagedAgent;
  /** The persistent DM channel — the Concierge's memory spine. */
  dm: Channel;
  createdAgent: boolean;
};

/**
 * Find an existing Concierge managed agent by name. Pure — exported for
 * tests. Prefers running agents, then most recently updated, mirroring
 * `pickPreferredManagedAgent` semantics without the channel-membership
 * exclusion (the Concierge SHOULD be in its own DM).
 */
export function findConciergeAgent(
  agents: ManagedAgent[],
): ManagedAgent | undefined {
  const candidates = agents.filter(
    (agent) =>
      agent.name.trim().toLowerCase() === CONCIERGE_AGENT_NAME.toLowerCase(),
  );
  return [...candidates].sort((left, right) => {
    const score = (agent: ManagedAgent) =>
      agent.status === "running" || agent.status === "deployed" ? 1 : 0;
    if (score(left) !== score(right)) return score(right) - score(left);
    return Date.parse(right.updatedAt) - Date.parse(left.updatedAt);
  })[0];
}

/** Pick the mesh serve target to run the Concierge brain on. Pure. */
export function pickMeshTarget(
  targets: MeshServeTarget[],
): MeshServeTarget | undefined {
  return targets[0];
}

/**
 * Derived-ownership session bootstrap (contract: RESEARCH/CONCIERGE_DESIGN.md).
 * Find/create the Concierge managed agent on the relay-mesh brain, open the
 * DM (idempotent by participant set), and make sure the agent is running.
 * No new persistence — ownership is derived from agent name + DM membership.
 */
export async function ensureConciergeSession(): Promise<ConciergeSession> {
  const agents = await listManagedAgents();
  let agent = findConciergeAgent(agents);
  let createdAgent = false;

  if (!agent) {
    const availability = await meshAvailability();
    const target = pickMeshTarget(availability.serveTargets);
    if (!availability.available || !target) {
      throw new Error(
        availability.reason ??
          "No relay-mesh model is being served. Start serving a model in Settings → Relay compute, then try again.",
      );
    }
    await startRelayMeshClientForTarget(target.modelId, target);
    const preset = meshAgentPresetPatch(await meshAgentPreset(target.modelId));
    const created = await createManagedAgent({
      name: CONCIERGE_AGENT_NAME,
      ...preset,
      systemPrompt: CONCIERGE_SYSTEM_PROMPT,
      spawnAfterCreate: true,
      // Mesh serve targets aren't persisted; start_managed_agent re-resolves
      // a live target on demand instead of auto-starting with the app.
      startOnAppLaunch: false,
      backend: { type: "local" },
      // Concierge answers its owner only — the DM is a 1:1 surface.
      respondTo: "owner-only",
    });
    agent = created.agent;
    createdAgent = true;
  } else if (agent.status !== "running" && agent.status !== "deployed") {
    // Rust-side preflight re-resolves a live mesh serve target before spawn.
    agent = await startManagedAgent(agent.pubkey);
  }

  const dm = await openDm({ pubkeys: [agent.pubkey] });
  return { agent, dm, createdAgent };
}
