/**
 * Settings → Agent Provider panel: list of saved provider profiles.
 *
 * The single-profile form became `AgentProviderProfileDialog`; this card is
 * a thin orchestrator over the list + dialog state. Other agents
 * (goose, codex, …) bring their own provider config; profiles here only
 * configure sprout-agent.
 */
import * as React from "react";
import { Check, KeyRound, Pencil, Plus, Star, Trash2 } from "lucide-react";
import { toast } from "sonner";

import {
  useAgentProviderEnvPresenceQuery,
  useAgentProviderSettingsStateQuery,
  useDeleteAgentProviderProfileMutation,
  useDeleteAgentProviderSettingsMutation,
  useSetDefaultAgentProviderProfileMutation,
} from "@/features/settings/hooks/useAgentProviderSettings.ts";
import type { ProfileSummary } from "@/features/settings/lib/agentProviderSettingsApi.ts";
import { PROVIDER_CATALOG } from "@/features/settings/lib/providerCatalog.ts";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

import { AgentProviderProfileDialog } from "./AgentProviderProfileDialog";
import {
  AgentProviderLoadErrorBanner,
  AgentProviderRotationBanner,
  AgentProviderShellEnvHint,
} from "./AgentProviderBanners";
import { ClearSettingsDialog } from "./AgentProviderClearDialog";

function providerLabel(id: string): string {
  return PROVIDER_CATALOG[id as keyof typeof PROVIDER_CATALOG]?.label ?? id;
}

type DialogMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; profileId: string };

export function AgentProviderSettingsCard() {
  const stateQuery = useAgentProviderSettingsStateQuery();
  const envPresenceQuery = useAgentProviderEnvPresenceQuery();
  const setDefaultMutation = useSetDefaultAgentProviderProfileMutation();
  const deleteProfileMutation = useDeleteAgentProviderProfileMutation();
  const deleteAllMutation = useDeleteAgentProviderSettingsMutation();

  const [dialog, setDialog] = React.useState<DialogMode>({ kind: "closed" });
  const [showConfirmClear, setShowConfirmClear] = React.useState(false);
  const [pendingDeleteId, setPendingDeleteId] = React.useState<string | null>(
    null,
  );

  const state = stateQuery.data;
  const profiles: ProfileSummary[] =
    state && state.status === "ok" ? state.profiles : [];
  const defaultId =
    state && state.status === "ok" ? state.defaultProfileId : null;
  const identityMismatch =
    state && state.status === "identity_mismatch" ? state.storedPubkey : null;
  const noSettings = state?.status === "none";
  const stateError = state?.status === "error" ? state.message : null;
  const queryError = stateQuery.error
    ? stateQuery.error instanceof Error
      ? stateQuery.error.message
      : String(stateQuery.error)
    : null;
  const loadError = stateError ?? queryError;

  // Hint state: shell env var is set but no saved settings.
  const env = envPresenceQuery.data;
  const showShellEnvHint = Boolean(
    noSettings && env && (env.anthropicApiKey || env.openaiCompatApiKey),
  );

  const onSetDefault = async (profileId: string) => {
    try {
      await setDefaultMutation.mutateAsync(profileId);
      toast.success("Default profile updated");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Failed to set default: ${msg}`);
    }
  };

  const onDeleteProfile = async (profileId: string) => {
    try {
      await deleteProfileMutation.mutateAsync(profileId);
      toast.success("Profile deleted");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Delete failed: ${msg}`);
    } finally {
      setPendingDeleteId(null);
    }
  };

  const onClearAll = async () => {
    setShowConfirmClear(false);
    try {
      await deleteAllMutation.mutateAsync();
      toast.success("Agent provider settings cleared");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Clear failed: ${msg}`);
    }
  };

  return (
    <section className="min-w-0" data-testid="settings-agent-provider">
      <div className="mb-3 flex items-start justify-between gap-2 min-w-0">
        <div className="min-w-0">
          <h2 className="flex items-center gap-2 text-sm font-semibold tracking-tight">
            <KeyRound className="h-4 w-4" /> Agent Provider
          </h2>
          <p className="text-sm text-muted-foreground">
            Configure the language models your Sprout agents can use. Saved on
            this device, encrypted with your nostr key. Each agent can pin a
            specific profile; agents without a pin use the default below.
          </p>
        </div>
        <Button
          data-testid="agent-provider-add"
          disabled={Boolean(identityMismatch)}
          onClick={() => setDialog({ kind: "create" })}
          size="sm"
          type="button"
        >
          <Plus className="mr-1 h-4 w-4" /> Add profile
        </Button>
      </div>

      <AgentProviderLoadErrorBanner message={loadError} />
      <AgentProviderRotationBanner visible={Boolean(identityMismatch)} />
      <AgentProviderShellEnvHint visible={showShellEnvHint} />

      {/* No-default warning when ≥1 profile exists but default is unset. */}
      {state?.status === "ok" && profiles.length > 0 && defaultId === null ? (
        <div
          className="mb-3 rounded-md border border-yellow-300 bg-yellow-50 px-3 py-2 text-xs text-yellow-900 dark:border-yellow-700 dark:bg-yellow-950 dark:text-yellow-200"
          data-testid="agent-provider-no-default-banner"
        >
          No default profile is set. Either choose one below, or pin a specific
          profile to each sprout-agent in the Agents view.
        </div>
      ) : null}

      {/* Empty state when ok-but-empty (last-profile-deleted). */}
      {state?.status === "ok" && profiles.length === 0 ? (
        <p
          className="text-sm text-muted-foreground"
          data-testid="agent-provider-empty-list"
        >
          No profiles yet. Click "Add profile" to create one.
        </p>
      ) : null}

      {/* Profile list */}
      {profiles.length > 0 ? (
        <ul
          className="flex flex-col divide-y divide-border rounded-md border border-border"
          data-testid="agent-provider-profile-list"
        >
          {profiles.map((profile) => (
            <li
              className="flex flex-wrap items-center justify-between gap-2 px-3 py-2"
              data-testid={`agent-provider-profile-row-${profile.id}`}
              key={profile.id}
            >
              <div className="flex min-w-0 flex-col">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-sm">{profile.label}</span>
                  {defaultId === profile.id ? (
                    <span
                      className="inline-flex items-center gap-0.5 rounded-full bg-emerald-100 px-1.5 py-0.5 text-[10px] font-medium text-emerald-900 dark:bg-emerald-900 dark:text-emerald-100"
                      data-testid={`agent-provider-profile-default-pill-${profile.id}`}
                    >
                      <Check className="h-2.5 w-2.5" /> default
                    </span>
                  ) : null}
                </div>
                <span className="text-xs text-muted-foreground">
                  {providerLabel(profile.detectedProviderId)} • {profile.model}
                  {profile.apiKeyPreview
                    ? ` • ••••${profile.apiKeyPreview}`
                    : ""}
                </span>
              </div>
              <div className="flex shrink-0 items-center gap-1">
                {defaultId !== profile.id ? (
                  <Button
                    aria-label="Set as default"
                    data-testid={`agent-provider-profile-set-default-${profile.id}`}
                    disabled={setDefaultMutation.isPending}
                    onClick={() => void onSetDefault(profile.id)}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    <Star className="h-3.5 w-3.5" />
                  </Button>
                ) : null}
                <Button
                  aria-label="Edit profile"
                  data-testid={`agent-provider-profile-edit-${profile.id}`}
                  onClick={() =>
                    setDialog({ kind: "edit", profileId: profile.id })
                  }
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  <Pencil className="h-3.5 w-3.5" />
                </Button>
                <Button
                  aria-label="Delete profile"
                  className={cn(
                    pendingDeleteId === profile.id &&
                      "text-red-600 dark:text-red-400",
                  )}
                  data-testid={`agent-provider-profile-delete-${profile.id}`}
                  disabled={deleteProfileMutation.isPending}
                  onClick={() => {
                    if (pendingDeleteId === profile.id) {
                      void onDeleteProfile(profile.id);
                    } else {
                      setPendingDeleteId(profile.id);
                    }
                  }}
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            </li>
          ))}
        </ul>
      ) : null}

      {/*
        Clear-everything escape hatch. Reachable whenever the envelope
        exists in any form worth removing: a normal `ok` state (with or
        without profiles — an empty wrapper still occupies a file on
        disk), an identity mismatch (rotated key), or a load error
        (corrupt / partial envelope). Hidden only on `status === "none"`
        i.e. genuinely nothing to clear.
       */}
      {state?.status === "ok" || identityMismatch || loadError !== null ? (
        <div className="mt-3 flex justify-end">
          <Button
            data-testid="agent-provider-clear"
            disabled={deleteAllMutation.isPending}
            onClick={() => setShowConfirmClear(true)}
            size="sm"
            type="button"
            variant="ghost"
          >
            Clear all settings
          </Button>
        </div>
      ) : null}

      <AgentProviderProfileDialog
        onOpenChange={(open) => {
          if (!open) {
            setDialog({ kind: "closed" });
          }
        }}
        onSaved={() => setDialog({ kind: "closed" })}
        open={dialog.kind !== "closed"}
        profileId={dialog.kind === "edit" ? dialog.profileId : null}
      />

      <ClearSettingsDialog
        onConfirm={() => void onClearAll()}
        onOpenChange={setShowConfirmClear}
        open={showConfirmClear}
      />
    </section>
  );
}
