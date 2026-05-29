// Typed wrappers around Max's `mesh_*` Tauri commands.
//
// PLACEHOLDER: these call `mesh_*` commands that Max owns and has not yet
// landed. Each function below will resolve via the real Tauri bridge once
// Max commits; until then, the UI uses `useMockMeshAvailability` /
// `useMockMeshNodeStatus` hooks to render the card structure against fixed
// data. When Max's first command commit lands, swap the hooks to call these
// wrappers and delete the mock module.
//
// Frozen contract source: Max's freeze post 2026-05-29T19:02 + 19:14 in
// the sprout-and-mesh-llm thread.

import { invokeTauri } from "@/shared/api/tauri";
import type {
  MeshAgentPreset,
  MeshAvailability,
  MeshModelOption,
  MeshNodeStatus,
  StartMeshNodeRequest,
} from "./types";

/** Single source of truth for "can I use the relay mesh right now?" */
export async function meshAvailability(): Promise<MeshAvailability> {
  return invokeTauri<MeshAvailability>("mesh_availability");
}

/** Lifecycle status of the local in-process mesh node. */
export async function meshNodeStatus(): Promise<MeshNodeStatus> {
  return invokeTauri<MeshNodeStatus>("mesh_node_status");
}

/** Start the local mesh node in serve or client mode. */
export async function meshStartNode(
  request: StartMeshNodeRequest,
): Promise<MeshNodeStatus> {
  return invokeTauri<MeshNodeStatus>("mesh_start_node", { request });
}

export async function meshStopNode(): Promise<MeshNodeStatus> {
  return invokeTauri<MeshNodeStatus>("mesh_stop_node");
}

/**
 * Local models on disk, ready to serve without download. Source for the
 * "Already installed on this machine" picklist in the Share-compute card.
 */
export async function meshInstalledModels(): Promise<MeshModelOption[]> {
  return invokeTauri<MeshModelOption[]>("mesh_installed_models");
}

/**
 * Build a managed-agent preset for the given model. Returns flat fields that
 * overwrite the corresponding `ManagedAgent` fields on creation (acpCommand,
 * agentCommand, mcpCommand, agentArgs, model, envVars).
 */
export async function meshAgentPreset(
  modelId: string,
): Promise<MeshAgentPreset> {
  return invokeTauri<MeshAgentPreset>("mesh_agent_preset", {
    request: { modelId },
  });
}

/**
 * RESERVED for v2 — signature only. Do NOT call from v1 UI; the command is
 * not implemented and will error. Tracked so the v2 search UX lands as an
 * additive PR against an already-agreed shape.
 */
export async function meshSearchModels(
  _query: string,
): Promise<MeshModelOption[]> {
  throw new Error(
    "mesh_search_models is reserved for v2 and not callable in v1",
  );
}
