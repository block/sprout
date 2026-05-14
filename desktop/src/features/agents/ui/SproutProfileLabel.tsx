/**
 * Resolved profile label for a sprout-agent row.
 *
 * Lives in its own component so the underlying settings query (which decrypts
 * the envelope on every read) only fires for sprout-agent rows. Non-sprout
 * rows never mount this component → zero queries.
 *
 * "Missing" detection is gated on `state.status === "ok"` so a transient load
 * error or identity-mismatch doesn't paint a valid pin red.
 */
import { useAgentProviderSettingsStateQuery } from "@/features/settings/hooks/useAgentProviderSettings";
import { cn } from "@/shared/lib/cn";

export function SproutProfileLabel({
  pinnedProfileId,
  pubkey,
}: {
  pinnedProfileId: string | null;
  pubkey: string;
}) {
  const state = useAgentProviderSettingsStateQuery().data;
  const isOk = state?.status === "ok";
  const profiles = isOk ? state.profiles : [];
  const defaultId = isOk ? state.defaultProfileId : null;

  let kind: "default" | "named" | "missing" = "default";
  let text = "Profile: default";
  if (pinnedProfileId !== null) {
    const found = profiles.find((p) => p.id === pinnedProfileId);
    if (found) {
      kind = "named";
      text = `Profile: ${found.label}`;
    } else if (isOk) {
      kind = "missing";
      text = "Profile: (missing — edit to fix)";
    } else {
      // Loading / error / identity-mismatch — surface the raw id; the
      // separate state-error UI surfaces the actual problem.
      kind = "named";
      text = `Profile: ${pinnedProfileId}`;
    }
  } else if (isOk) {
    // Unpinned — agent will fall back to the panel's default at spawn
    // time. If there's no valid default the agent can't start; show the
    // same broken state as the settings banner.
    const def = profiles.find((p) => p.id === defaultId);
    if (def) {
      text = `Profile: default (${def.label})`;
    } else {
      kind = "missing";
      text = "Profile: default (none set — edit to fix)";
    }
  }

  return (
    <span
      className={cn(
        "text-xs",
        kind === "missing"
          ? "text-yellow-700 dark:text-yellow-400"
          : "text-muted-foreground",
      )}
      data-testid={`managed-agent-row-profile-${pubkey}`}
    >
      {text}
    </span>
  );
}
