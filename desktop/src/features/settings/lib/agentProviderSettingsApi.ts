/**
 * IPC helpers for the encrypted Agent Provider settings panel.
 *
 * The Tauri commands behind these wrappers live in
 * `desktop/src-tauri/src/commands/agent_provider_settings/`. The plaintext
 * API key crosses the IPC boundary in exactly one direction (writer-only)
 * via `saveAgentProviderProfile`. Loaders return metadata + an
 * `apiKeyPreview` (last 4 chars) but never the full key.
 *
 * Wire shapes match the Rust structs under `#[serde(rename_all = "camelCase")]`.
 *
 * ## Multi-profile model
 *
 * The encrypted envelope contains a wrapper with N named profiles. One
 * profile is optionally marked as the default. Per-agent records hold
 * `providerProfileId` to pin a specific profile; `null` falls back to the
 * default at spawn.
 */

import { invokeTauri } from "@/shared/api/tauri";

export type ProviderDialect = "anthropic" | "openai";

// ── State (list view) ──────────────────────────────────────────────────────

/** Discriminated by `status`. */
export type AgentProviderSettingsState =
  | { status: "none" }
  | {
      status: "ok";
      defaultProfileId: string | null;
      profiles: ProfileSummary[];
    }
  | { status: "identity_mismatch"; storedPubkey: string }
  | { status: "error"; message: string };

/** Non-secret metadata used by the profile list & per-agent picker. */
export type ProfileSummary = {
  id: string;
  label: string;
  createdAt: number;
  updatedAt: number;
  provider: ProviderDialect;
  model: string;
  baseUrl: string;
  detectedProviderId: string;
  apiKeyPresent: boolean;
  apiKeyPreview: string | null;
};

// ── Single profile (edit view) ─────────────────────────────────────────────

export type AgentProviderProfileLoadStatus =
  | { status: "none" }
  | { status: "ok"; view: AgentProviderSettingsView }
  | { status: "identity_mismatch"; storedPubkey: string }
  | { status: "error"; message: string };

/** Full editable view of one profile — same shape as before, no `apiKey`. */
export type AgentProviderSettingsView = {
  /** Profile label. Lets the edit dialog hydrate label+form in one effect. */
  label: string;
  provider: ProviderDialect;
  model: string;
  baseUrl: string;
  anthropicApiVersion: string | null;
  systemPrompt: string | null;
  maxRounds: number | null;
  maxOutputTokens: number | null;
  llmTimeoutSecs: number | null;
  toolTimeoutSecs: number | null;
  maxHistoryBytes: number | null;
  detectedProviderId: string;
  detectionOverridden: boolean;
  apiKeyPresent: boolean;
  apiKeyPreview: string | null;
};

// ── Save input ─────────────────────────────────────────────────────────────

export type AgentProviderSettingsInput = {
  /**
   * `null` ⇒ create. `string` ⇒ update the profile with this id. Unknown
   * id is rejected by the backend.
   */
  profileId: string | null;
  /** Human-readable label, 1..=64 chars after trim. */
  label: string;
  provider: ProviderDialect;
  /**
   * `null` ⇒ preserve the previously stored key (only valid on UPDATE when
   * provider, detected_provider_id, and base-URL origin match the existing
   * slot). On CREATE, `null` is rejected. `""` is always rejected.
   */
  apiKey: string | null;
  model: string;
  baseUrl: string;
  anthropicApiVersion: string | null;
  systemPrompt: string | null;
  maxRounds: number | null;
  maxOutputTokens: number | null;
  llmTimeoutSecs: number | null;
  toolTimeoutSecs: number | null;
  maxHistoryBytes: number | null;
  detectedProviderId: string;
  detectionOverridden: boolean;
};

export type SaveProfileResponse = {
  profileId: string;
  /** True when this save also became the default (first-profile rule). */
  setAsDefault: boolean;
};

// ── Shell-env presence ─────────────────────────────────────────────────────

export type AgentProviderEnvPresence = {
  sproutAgentProvider: boolean;
  anthropicApiKey: boolean;
  openaiCompatApiKey: boolean;
};

// ── Invocations ────────────────────────────────────────────────────────────

export async function getAgentProviderSettingsState(): Promise<AgentProviderSettingsState> {
  return invokeTauri<AgentProviderSettingsState>(
    "get_agent_provider_settings_state",
  );
}

export async function getAgentProviderProfile(
  profileId: string,
): Promise<AgentProviderProfileLoadStatus> {
  return invokeTauri<AgentProviderProfileLoadStatus>(
    "get_agent_provider_profile",
    { profileId },
  );
}

export async function saveAgentProviderProfile(
  input: AgentProviderSettingsInput,
): Promise<SaveProfileResponse> {
  return invokeTauri<SaveProfileResponse>("save_agent_provider_profile", {
    input,
  });
}

export async function setDefaultAgentProviderProfile(
  profileId: string | null,
): Promise<void> {
  await invokeTauri<void>("set_default_agent_provider_profile", { profileId });
}

export async function deleteAgentProviderProfile(
  profileId: string,
): Promise<void> {
  await invokeTauri<void>("delete_agent_provider_profile", { profileId });
}

export async function deleteAgentProviderSettings(): Promise<void> {
  await invokeTauri<void>("delete_agent_provider_settings");
}

export async function getAgentProviderEnvPresence(): Promise<AgentProviderEnvPresence> {
  return invokeTauri<AgentProviderEnvPresence>(
    "get_agent_provider_env_presence",
  );
}
