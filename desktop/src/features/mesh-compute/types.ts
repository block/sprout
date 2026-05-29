// Type mirrors of Max's frozen Tauri contract for mesh-llm desktop integration.
//
// Source of truth: Max's freeze post, 2026-05-29T19:02 + 19:14 + 19:14:51 in the
// sprout-and-mesh-llm channel thread rooted at 49667c31… (mesh-llm one-PR).
// These types are what Max's `mesh_*` Tauri commands return; they are NOT
// derived from mesh-llm's own types — they are Sprout's UI-shaped projection,
// stripped of raw runtime/local-path/secret material.
//
// When Max's Rust commands land, regenerate (or hand-align) these against the
// real `RawMeshAvailability` etc. surfaced in `desktop/src/shared/api/tauri.ts`.

/** Lifecycle state of the local in-process mesh node. */
export type MeshNodeState =
  | "off"
  | "starting"
  | "running"
  | "stopping"
  | "failed";

/** What mode the local mesh node is running in. */
export type MeshNodeMode = "serve" | "client";

/**
 * Operational health of the running node. `degraded` and `failed` carry a
 * human-readable reason for the card to render verbatim (e.g. "downloading
 * weights", "model load failed: …"). Granularity matches whatever the mesh
 * runtime emits — coarse is fine, no fake percents.
 */
export type MeshHealth =
  | { status: "ok" }
  | { status: "degraded" | "failed"; reason: string };

/**
 * A model option surfaced through `mesh_availability` (mesh-wide, from kind:30621)
 * or `mesh_installed_models` (local-only, what's on disk and ready to serve).
 *
 * `id` is the routable model ref — the string mesh accepts and the value that
 * lands in `OPENAI_COMPAT_MODEL` env var. UI displays `name ?? id`.
 */
export type MeshModelOption = {
  id: string;
  name: string | null;
};

/** A reachable serve node in the relay's mesh, projected from kind:30621. */
export type MeshServeTarget = {
  modelId: string;
  modelName: string | null;
  /** Mesh's invite-token: base64(json(EndpointAddr)). Not a secret — a dial pointer. */
  endpointAddr: string;
  nodeName: string | null;
  capacity: { vramGb: number | null } | null;
};

/**
 * Single source of truth for both:
 *   1. Is the relay mesh-capable AND am I a member? (Settings client-mode tile)
 *   2. Is there something to consume? (Create-agent "Relay mesh" flow)
 *
 * Caller asks "can I use this?" — that's `available`, the only field UI logic
 * branches on. The other booleans + `reason` are for honest disabled-state copy.
 */
export type MeshAvailability = {
  capable: boolean;
  admitted: boolean;
  /** capable && admitted && serveTargets.length > 0 */
  available: boolean;
  /** UI-safe disabled reason. Null when `available`. */
  reason: string | null;
  /** Union of models advertised across all serveTargets, deduped by id. */
  models: MeshModelOption[];
  serveTargets: MeshServeTarget[];
};

/** Status of the local in-process mesh node. */
export type MeshNodeStatus = {
  state: MeshNodeState;
  mode: MeshNodeMode | null;
  health: MeshHealth;
  /** http://127.0.0.1:9337/v1 when client mode is running. */
  apiBaseUrl: string | null;
  /** Debug/Advanced-only. */
  consoleUrl: string | null;
  modelId: string | null;
  modelName: string | null;
};

/** Request payload for `mesh_start_node`. */
export type StartMeshNodeRequest = {
  mode: MeshNodeMode;
  /** Serve-only. Catalog name / hf:// ref / local GGUF path. */
  modelId?: string;
  /** Serve-only, Advanced. */
  maxVramGb?: number;
};

/**
 * Provider preset returned by `mesh_agent_preset(modelId)`. Fields flatten
 * directly onto a `ManagedAgent` — see `desktop/src/shared/api/types.ts:271`.
 *
 * Apply: `Object.assign(newAgentDraft, preset)` (or explicit destructure;
 * `Object.assign` works because field names are an exact superset).
 */
export type MeshAgentPreset = {
  providerId: "relay-mesh";
  label: "Relay mesh";
  acpCommand: "sprout-acp";
  agentCommand: "sprout-agent";
  agentArgs: [];
  mcpCommand: "sprout-dev-mcp";
  /** The same string as `modelId` passed in. Lands in `ManagedAgent.model`. */
  model: string;
  /**
   * Exactly:
   *   SPROUT_AGENT_PROVIDER=openai
   *   OPENAI_COMPAT_BASE_URL=http://127.0.0.1:9337/v1
   *   OPENAI_COMPAT_MODEL=<modelId>
   *   OPENAI_COMPAT_API_KEY=sprout-mesh-local  (placeholder; iroh admission is the gate)
   *   OPENAI_COMPAT_API=chat
   */
  envVars: Record<string, string>;
};

/**
 * Classification of a free-text model ref entered into the serve card.
 * UI shows a hint inline ("Looks like a catalog name") for trust feedback.
 * Mirrors mesh's own resolve logic at `runtime/mod.rs:3390`.
 */
export type ModelRefKind =
  | { kind: "catalog"; name: string }
  | { kind: "huggingface"; ref: string }
  | { kind: "local-path"; path: string }
  | { kind: "unknown" };
