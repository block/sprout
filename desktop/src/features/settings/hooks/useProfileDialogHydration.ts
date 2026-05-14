/**
 * State machine for AgentProviderProfileDialog form hydration.
 *
 * Purpose: hold `label` + `form` state alongside a `hydratedFor` marker
 * that records *which* profile (or the create sentinel) the current state
 * was seeded from. The dialog uses that marker to:
 *
 *   1. Disable Save until the form belongs to the profile currently
 *      targeted by the dialog. Otherwise a fast click during a
 *      profile-switch race could persist profile A's values under
 *      profile B's id.
 *   2. Aggressively clear secret/transient fields (`apiKey`, `revealKey`)
 *      on close and on profile switch, so plaintext keys and reveal
 *      state never linger across opens.
 *
 * Hydration sources of truth:
 *   - Create mode (`profileId === null`): zero-state defaults.
 *   - Edit mode: the backend `AgentProviderSettingsView` for `profileId`.
 *     Hydration waits for the query to resolve.
 */
import * as React from "react";

import {
  blankFormForProvider,
  type FormState,
  loadedFormFromView,
} from "@/features/settings/lib/agentProviderFormState.ts";
import type { AgentProviderSettingsView } from "@/features/settings/lib/agentProviderSettingsApi.ts";

const CREATE_SENTINEL = "__create__";

export type ProfileDialogHydration = {
  label: string;
  setLabel: (next: string) => void;
  form: FormState;
  setForm: React.Dispatch<React.SetStateAction<FormState>>;
  revealKey: boolean;
  setRevealKey: React.Dispatch<React.SetStateAction<boolean>>;
  showAdvanced: boolean;
  setShowAdvanced: React.Dispatch<React.SetStateAction<boolean>>;
  /**
   * True while the form state does NOT yet correspond to the profile
   * currently targeted by the dialog. Callers should use this to gate
   * Save.
   */
  hydrationStale: boolean;
  /**
   * Called by the parent dialog on a successful save so that closing +
   * reopening the same profile re-fetches from the (now-invalidated)
   * cache rather than reusing the in-memory hydration.
   */
  resetHydration: () => void;
};

export function useProfileDialogHydration(params: {
  open: boolean;
  profileId: string | null;
  loadedView: AgentProviderSettingsView | null;
  /**
   * True iff `loadedView` corresponds to data the query has freshly
   * resolved (not stale-but-served during an in-flight refetch after
   * `invalidateQueries`). When false, hydration waits even if
   * `loadedView` is non-null — otherwise a slow refetch racing a save
   * could re-seed the form with the pre-invalidation snapshot.
   */
  viewIsFresh: boolean;
}): ProfileDialogHydration {
  const { open, profileId, loadedView, viewIsFresh } = params;
  const isEdit = profileId !== null;
  const targetKey = isEdit ? (profileId as string) : CREATE_SENTINEL;

  const [label, setLabel] = React.useState<string>("");
  const [form, setForm] = React.useState<FormState>(() =>
    blankFormForProvider("anthropic"),
  );
  const [revealKey, setRevealKey] = React.useState(false);
  const [showAdvanced, setShowAdvanced] = React.useState(false);
  const [hydratedFor, setHydratedFor] = React.useState<string | null>(null);

  // Reset everything on close. Secrets (apiKey, revealKey) must not
  // linger; non-secret form fields are reset too so a future open starts
  // from neutral.
  React.useEffect(() => {
    if (!open) {
      setHydratedFor(null);
      setLabel("");
      setForm(blankFormForProvider("anthropic"));
      setRevealKey(false);
      setShowAdvanced(false);
    }
  }, [open]);

  // Profile switch while open: clear form to neutral and mark unhydrated
  // so the next-hydration effect re-runs against the new target.
  React.useEffect(() => {
    if (!open) return;
    if (hydratedFor !== null && hydratedFor !== targetKey) {
      setHydratedFor(null);
      setLabel("");
      setForm(blankFormForProvider("anthropic"));
      setRevealKey(false);
    }
  }, [open, hydratedFor, targetKey]);

  // Hydrate when the dialog is open and we don't yet match the target.
  React.useEffect(() => {
    if (!open) return;
    if (hydratedFor === targetKey) return;
    if (!isEdit) {
      setLabel("");
      setForm(blankFormForProvider("anthropic"));
      setRevealKey(false);
      setHydratedFor(CREATE_SENTINEL);
      return;
    }
    if (!loadedView) return; // wait for the backend query
    if (!viewIsFresh) return; // wait for in-flight refetch after invalidation
    setLabel(loadedView.label);
    setForm(loadedFormFromView(loadedView));
    setRevealKey(false);
    setHydratedFor(profileId as string);
  }, [
    open,
    isEdit,
    loadedView,
    viewIsFresh,
    hydratedFor,
    profileId,
    targetKey,
  ]);

  const resetHydration = React.useCallback(() => setHydratedFor(null), []);

  return {
    label,
    setLabel,
    form,
    setForm,
    revealKey,
    setRevealKey,
    showAdvanced,
    setShowAdvanced,
    hydrationStale: hydratedFor !== targetKey,
    resetHydration,
  };
}
