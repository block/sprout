/**
 * Per-sprout-agent provider profile picker.
 *
 * Rendered only when the agent's path is `sprout-agent` (the caller is
 * responsible for the gate — this component assumes that). The dropdown
 * lets the user pin a specific profile from the Settings → Agent Provider
 * panel, or fall back to the panel's default.
 *
 * "Missing profile" handling: if `value` references an id that no longer
 * exists in the saved profile list, the dropdown surfaces a disabled
 * sentinel entry so the user can see the staleness and pick another
 * profile to save. We never silently re-default.
 */
import { useAgentProviderSettingsStateQuery } from "@/features/settings/hooks/useAgentProviderSettings.ts";

const MISSING_VALUE = "__missing__";
const DEFAULT_VALUE = "__default__";
const PENDING_VALUE = "__pending__";

export type SproutAgentProfilePickerProps = {
  /** `null` = "use panel default". Otherwise a profile id. */
  value: string | null;
  onChange: (next: string | null) => void;
};

export function SproutAgentProfilePicker({
  value,
  onChange,
}: SproutAgentProfilePickerProps) {
  const stateQuery = useAgentProviderSettingsStateQuery();
  const state = stateQuery.data;

  // Only `ok` state tells us anything about profile membership. Loading,
  // error, identity-mismatch, and `none` all leave profiles unknown — we
  // must NOT compute "missing" from them or a perfectly valid pin renders
  // red during a transient load (or after an identity rotation that has
  // nothing to do with this pin yet).
  const isOk = state?.status === "ok";
  const profiles = isOk ? state.profiles : [];
  const defaultId = isOk ? state.defaultProfileId : null;
  const isMissing =
    isOk && value !== null && !profiles.some((p) => p.id === value);

  // When `value` is non-null but settings aren't `ok` yet, we can't render
  // it as a real option (profiles list is empty during load/error) — fall
  // back to a disabled PENDING sentinel so the <select> stays controlled.
  const selectValue =
    value === null
      ? DEFAULT_VALUE
      : isMissing
        ? MISSING_VALUE
        : isOk
          ? value
          : PENDING_VALUE;

  const handle = (raw: string) => {
    if (raw === DEFAULT_VALUE) onChange(null);
    else if (raw === MISSING_VALUE || raw === PENDING_VALUE) {
      // Disabled sentinels — should never fire, but guard anyway.
      return;
    } else {
      onChange(raw);
    }
  };

  const noSettings = state?.status === "none";

  return (
    <div className="flex flex-col gap-1.5">
      <label
        className="text-sm font-medium"
        htmlFor="sprout-agent-profile-picker"
      >
        Provider profile
      </label>
      <select
        className="rounded-md border border-input bg-background px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        data-testid="sprout-agent-profile-picker"
        id="sprout-agent-profile-picker"
        onChange={(e) => handle(e.target.value)}
        value={selectValue}
      >
        <option value={DEFAULT_VALUE}>
          {defaultId !== null
            ? `Use default (${profiles.find((p) => p.id === defaultId)?.label ?? defaultId})`
            : "Use default (no default set)"}
        </option>
        {profiles.map((p) => (
          <option key={p.id} value={p.id}>
            {p.label}
          </option>
        ))}
        {isMissing ? (
          <option disabled value={MISSING_VALUE}>
            (missing: {value})
          </option>
        ) : null}
        {selectValue === PENDING_VALUE ? (
          <option disabled value={PENDING_VALUE}>
            {value} (loading…)
          </option>
        ) : null}
      </select>
      {isMissing ? (
        <span
          className="text-xs text-yellow-700 dark:text-yellow-400"
          data-testid="sprout-agent-profile-missing-warning"
        >
          The pinned profile no longer exists. Pick another and save.
        </span>
      ) : null}
      {noSettings || (isOk && defaultId === null && value === null) ? (
        <span className="text-xs text-muted-foreground">
          {noSettings
            ? "No profiles configured. Add one in Sprout Settings → Agent Provider."
            : "No default profile is set; this agent won't be able to start until you pick one or set a default in Settings → Agent Provider."}
        </span>
      ) : null}
      {state?.status === "error" ? (
        <span className="text-xs text-red-600 dark:text-red-400">
          Settings unreadable: {state.message}
        </span>
      ) : null}
      {state?.status === "identity_mismatch" ? (
        <span className="text-xs text-yellow-700 dark:text-yellow-400">
          Settings were saved by a different identity. Open Settings → Agent
          Provider to clear or migrate.
        </span>
      ) : null}
    </div>
  );
}
