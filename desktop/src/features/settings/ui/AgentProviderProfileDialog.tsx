/**
 * Create/edit dialog for one provider profile.
 *
 * Behavior split:
 * - `profileId === null` ⇒ Create mode. API key field is required (the
 *   backend rejects create with apiKey=null).
 * - `profileId === "<id>"` ⇒ Edit mode. API key field is optional —
 *   leaving it blank reuses the previously-stored key under the
 *   provider+detected_provider_id+origin-match contract.
 *
 * Provider auto-detection & all field reset rules are inherited from the
 * existing single-profile form (now reused per-profile).
 */
import * as React from "react";
import { Eye, EyeOff } from "lucide-react";
import { toast } from "sonner";

import {
  applyProviderSwitch,
  type FormState,
  parseOptionalInt,
} from "@/features/settings/lib/agentProviderFormState.ts";
import {
  ADMIN_ONLY_PROVIDER_ID,
  detectProvider,
} from "@/features/settings/lib/detectProvider.ts";
import {
  LOCAL_PLACEHOLDER_API_KEY,
  PROVIDER_CATALOG,
  PROVIDER_OPTIONS,
  type ProviderId,
} from "@/features/settings/lib/providerCatalog.ts";
import type {
  AgentProviderSettingsInput,
  ProviderDialect,
} from "@/features/settings/lib/agentProviderSettingsApi.ts";
import {
  useAgentProviderProfileQuery,
  useSaveAgentProviderProfileMutation,
} from "@/features/settings/hooks/useAgentProviderSettings.ts";
import { useProfileDialogHydration } from "@/features/settings/hooks/useProfileDialogHydration.ts";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { cn } from "@/shared/lib/cn";

import { AgentProviderAdvancedFields } from "./AgentProviderAdvancedFields";
import { AgentProviderProfileDialogFooter } from "./AgentProviderProfileDialogFooter";

function detectedProviderLabel(
  id: ProviderId | typeof ADMIN_ONLY_PROVIDER_ID,
): string {
  if (id === ADMIN_ONLY_PROVIDER_ID) {
    return "Anthropic admin key (rejected)";
  }
  return PROVIDER_CATALOG[id]?.label ?? "Custom";
}

export type AgentProviderProfileDialogProps = {
  /** `null` ⇒ create. `string` ⇒ edit this profile. */
  profileId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /**
   * Called after a successful save with the resulting profile id. Lets the
   * parent (the list view) re-select the row.
   */
  onSaved?: (profileId: string) => void;
};

export function AgentProviderProfileDialog({
  profileId,
  open,
  onOpenChange,
  onSaved,
}: AgentProviderProfileDialogProps) {
  const isEdit = profileId !== null;
  const profileQuery = useAgentProviderProfileQuery(profileId);
  const saveMutation = useSaveAgentProviderProfileMutation();
  // Two-click delete confirmation, mirroring the row pattern. Disarm
  // whenever the dialog closes OR the target profile changes — otherwise
  // an arm-then-flip-target race (uncommon in current UI, defended for
  // depth) could have the second click delete the wrong profile. Using
  // a per-scope marker lets us write the effect dep list as just the
  // scope key, which biome's exhaustive-deps accepts without complaint
  // and which avoids re-firing on unrelated re-renders.
  const armScope = `${profileId ?? "create"}:${open ? "open" : "closed"}`;
  const [deleteArmed, setDeleteArmed] = React.useState(false);
  React.useEffect(() => {
    // armScope changes on (profileId, open) transitions → disarm.
    void armScope;
    setDeleteArmed(false);
  }, [armScope]);

  const loadedView =
    profileQuery.data && profileQuery.data.status === "ok"
      ? profileQuery.data.view
      : null;

  // "Fresh" = the query is settled and not currently refetching after
  // invalidation. React Query keeps `data` around as stale-but-served
  // during a refetch, so without this gate the hook could hydrate from a
  // ghost view (e.g. cross-window mutation invalidated us while a slow
  // get_profile is in flight). We only treat loadedView as authoritative
  // for hydration when the query is fully settled.
  const viewIsFresh = !profileQuery.isFetching && profileQuery.isFetched;

  // Hydration state machine — owns label/form/revealKey/showAdvanced and
  // tracks `hydrationStale` so Save can block during a profile-switch
  // race. See useProfileDialogHydration for the contract.
  const {
    label,
    setLabel,
    form,
    setForm,
    revealKey,
    setRevealKey,
    showAdvanced,
    setShowAdvanced,
    hydrationStale,
    resetHydration,
  } = useProfileDialogHydration({
    open,
    profileId,
    loadedView,
    viewIsFresh,
  });

  const update = React.useCallback(
    <K extends keyof FormState>(key: K, value: FormState[K]) => {
      setForm((prev) => ({ ...prev, [key]: value }));
    },
    [setForm],
  );

  const setProviderManually = React.useCallback(
    (providerId: ProviderId) => {
      setForm((prev) =>
        applyProviderSwitch(prev, providerId, { manual: true }),
      );
    },
    [setForm],
  );

  const detection = React.useMemo(
    () => detectProvider(form.apiKey, form.baseUrl),
    [form.apiKey, form.baseUrl],
  );

  React.useEffect(() => {
    if (form.detectionOverridden) return;
    if (detection.providerId === ADMIN_ONLY_PROVIDER_ID) return;
    if (detection.confidence === "none") return;
    const detectedId: ProviderId = detection.providerId;
    setForm((prev) => {
      if (prev.providerId === detectedId) return prev;
      return applyProviderSwitch(prev, detectedId, { manual: false });
    });
  }, [
    detection.providerId,
    detection.confidence,
    form.detectionOverridden,
    setForm,
  ]);

  const providerEntry = PROVIDER_CATALOG[form.providerId];
  const dialect: ProviderDialect = providerEntry.dialect;
  const isLocal = providerEntry.isLocal;
  const adminKeyDetected = detection.providerId === ADMIN_ONLY_PROVIDER_ID;
  const apiKeyPresent = loadedView?.apiKeyPresent ?? false;
  const apiKeyPreview = loadedView?.apiKeyPreview ?? null;

  const savedProviderId = loadedView?.detectedProviderId;
  const providerChangedWithoutKey =
    isEdit &&
    apiKeyPresent &&
    !form.apiKey &&
    !isLocal &&
    savedProviderId !== undefined &&
    savedProviderId !== form.providerId;

  // Create mode requires an API key (backend rejects null on create).
  // Edit mode allows null (reuse). Local providers use a fixed placeholder.
  const apiKeyRequired = !isEdit && !isLocal;

  const labelInvalid = label.trim().length === 0 || label.trim().length > 64;

  // `hydrationStale` (from useProfileDialogHydration) is true whenever the
  // form state does not yet correspond to the dialog's current target —
  // i.e. the in-flight profile-switch race window. Blocking Save here
  // prevents persisting profile A's values under profile B's id.
  const saveDisabled =
    saveMutation.isPending ||
    hydrationStale ||
    adminKeyDetected ||
    labelInvalid ||
    !form.model.trim() ||
    !form.baseUrl.trim() ||
    (isLocal ? false : apiKeyRequired && !form.apiKey) ||
    (isEdit && providerChangedWithoutKey);

  const onSave = async () => {
    let parsed: AgentProviderSettingsInput;
    try {
      parsed = {
        profileId,
        label: label.trim(),
        provider: dialect,
        apiKey: isLocal
          ? LOCAL_PLACEHOLDER_API_KEY
          : form.apiKey
            ? form.apiKey
            : null,
        model: form.model.trim(),
        baseUrl: form.baseUrl.trim().replace(/\/+$/, ""),
        anthropicApiVersion:
          dialect === "anthropic" && form.anthropicApiVersion.trim()
            ? form.anthropicApiVersion.trim()
            : null,
        systemPrompt: form.systemPrompt.trim() || null,
        maxRounds: parseOptionalInt(form.maxRounds),
        maxOutputTokens: parseOptionalInt(form.maxOutputTokens),
        llmTimeoutSecs: parseOptionalInt(form.llmTimeoutSecs),
        toolTimeoutSecs: parseOptionalInt(form.toolTimeoutSecs),
        maxHistoryBytes: parseOptionalInt(form.maxHistoryBytes),
        detectedProviderId: form.providerId,
        detectionOverridden: form.detectionOverridden,
      };
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Invalid input: ${msg}`);
      return;
    }
    try {
      const resp = await saveMutation.mutateAsync(parsed);
      toast.success(
        isEdit
          ? "Profile saved"
          : resp.setAsDefault
            ? "Profile saved (set as default)"
            : "Profile saved",
      );
      setForm((prev) => ({ ...prev, apiKey: "" }));
      setRevealKey(false);
      resetHydration();
      onSaved?.(resp.profileId);
      onOpenChange(false);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Save failed: ${msg}`);
    }
  };

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent
        className="max-w-[640px]"
        data-testid="agent-provider-profile-dialog"
      >
        <DialogHeader>
          <DialogTitle>{isEdit ? "Edit profile" : "Add profile"}</DialogTitle>
        </DialogHeader>

        <form
          className="flex flex-col gap-4"
          data-testid="agent-provider-profile-form"
          onSubmit={(e) => {
            e.preventDefault();
            void onSave();
          }}
        >
          {/* Label */}
          <div className="flex flex-col gap-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="agent-provider-profile-label"
            >
              Label
            </label>
            <Input
              data-testid="agent-provider-profile-label"
              id="agent-provider-profile-label"
              maxLength={64}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="e.g. Anthropic (work)"
              value={label}
            />
            {labelInvalid && label.length > 0 ? (
              <span className="text-xs text-red-600 dark:text-red-400">
                Label must be 1–64 characters.
              </span>
            ) : null}
          </div>

          {/* Provider */}
          <div className="flex flex-col gap-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="agent-provider-provider-select"
            >
              Provider
            </label>
            <select
              className="rounded-md border border-input bg-background px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              data-testid="agent-provider-provider-select"
              id="agent-provider-provider-select"
              onChange={(e) =>
                setProviderManually(e.target.value as ProviderId)
              }
              value={form.providerId}
            >
              {PROVIDER_OPTIONS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
            {detection.confidence !== "none" && !form.detectionOverridden ? (
              <span
                className={cn(
                  "text-xs",
                  adminKeyDetected
                    ? "text-red-600 dark:text-red-400"
                    : detection.confidence === "medium"
                      ? "text-yellow-600 dark:text-yellow-400"
                      : "text-emerald-600 dark:text-emerald-400",
                )}
                data-testid="agent-provider-detected-badge"
              >
                Detected: {detectedProviderLabel(detection.providerId)}
                {detection.confidence === "medium"
                  ? " (medium confidence)"
                  : ""}
              </span>
            ) : null}
            {providerEntry.notes ? (
              <p className="text-xs text-muted-foreground">
                {providerEntry.notes}
              </p>
            ) : null}
          </div>

          {/* API key */}
          <div className="flex flex-col gap-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="agent-provider-api-key"
            >
              API key
            </label>
            <div className="relative">
              <Input
                autoComplete="off"
                data-testid="agent-provider-api-key"
                disabled={isLocal}
                id="agent-provider-api-key"
                onChange={(e) => update("apiKey", e.target.value)}
                placeholder={
                  isLocal
                    ? "(no auth required — using sk-local placeholder)"
                    : apiKeyPresent
                      ? `(saved — type to replace${
                          apiKeyPreview ? ` — ends in ${apiKeyPreview}` : ""
                        })`
                      : "Paste your API key"
                }
                spellCheck={false}
                type={revealKey ? "text" : "password"}
                value={form.apiKey}
              />
              {!isLocal ? (
                <button
                  aria-label={revealKey ? "Hide API key" : "Reveal API key"}
                  className="absolute right-2 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted-foreground hover:text-foreground"
                  data-testid="agent-provider-api-key-reveal"
                  onClick={() => setRevealKey((v) => !v)}
                  type="button"
                >
                  {revealKey ? (
                    <EyeOff className="h-4 w-4" />
                  ) : (
                    <Eye className="h-4 w-4" />
                  )}
                </button>
              ) : null}
            </div>
            {adminKeyDetected ? (
              <p
                className="text-xs text-red-600 dark:text-red-400"
                data-testid="agent-provider-admin-key-error"
              >
                Anthropic admin keys (sk-ant-admin01-…) are dashboard-only and
                cannot be used for agent inference. Use a regular API key
                (sk-ant-api03-…) instead.
              </p>
            ) : null}
            {providerChangedWithoutKey ? (
              <p
                className="text-xs text-yellow-700 dark:text-yellow-400"
                data-testid="agent-provider-provider-change-warning"
              >
                Provider changed — enter a new API key for the new provider.
              </p>
            ) : null}
          </div>

          {/* Model */}
          <div className="flex flex-col gap-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="agent-provider-model"
            >
              Model
            </label>
            <Input
              data-testid="agent-provider-model"
              id="agent-provider-model"
              list="agent-provider-model-suggestions"
              onChange={(e) => update("model", e.target.value)}
              placeholder="claude-sonnet-4-5"
              value={form.model}
            />
            {providerEntry.modelSuggestions.length > 0 ? (
              <datalist id="agent-provider-model-suggestions">
                {providerEntry.modelSuggestions.map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
            ) : null}
          </div>

          {/* Base URL */}
          <div className="flex flex-col gap-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="agent-provider-base-url"
            >
              Base URL
            </label>
            <Input
              data-testid="agent-provider-base-url"
              id="agent-provider-base-url"
              onChange={(e) => update("baseUrl", e.target.value)}
              placeholder="https://api.anthropic.com"
              value={form.baseUrl}
            />
          </div>

          {/* Anthropic version pin */}
          {dialect === "anthropic" ? (
            <div className="flex flex-col gap-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="agent-provider-anthropic-version"
              >
                Anthropic API version
              </label>
              <Input
                data-testid="agent-provider-anthropic-version"
                id="agent-provider-anthropic-version"
                onChange={(e) => update("anthropicApiVersion", e.target.value)}
                placeholder="2023-06-01"
                value={form.anthropicApiVersion}
              />
            </div>
          ) : null}

          <AgentProviderAdvancedFields
            form={form}
            onChange={update}
            onToggle={setShowAdvanced}
            open={showAdvanced}
          />

          <AgentProviderProfileDialogFooter
            deleteArmed={deleteArmed}
            isEdit={isEdit}
            isSaving={saveMutation.isPending}
            onCancel={() => onOpenChange(false)}
            onDeleted={() => onOpenChange(false)}
            onSetDeleteArmed={setDeleteArmed}
            profileId={profileId}
            saveDisabled={saveDisabled}
          />
        </form>
      </DialogContent>
    </Dialog>
  );
}
