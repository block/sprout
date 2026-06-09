import * as React from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useManagedAgentsQuery } from "@/features/agents/hooks";
import {
  readConciergeSelection,
  SELECTION_CHANGED_EVENT,
} from "@/features/concierge/lib/conciergeSelection";
import { useIdentityQuery } from "@/shared/api/hooks";

import "./concierge.css";

/** The selected Concierge agent's display name, live across selection
 *  changes (same-tab custom event + cross-tab storage event). Falls back to
 *  "Concierge" when nothing is selected yet. */
function useConciergeName(): string {
  const selfPubkey = useIdentityQuery().data?.pubkey;
  const agentsQuery = useManagedAgentsQuery();
  const [selectedPubkey, setSelectedPubkey] = React.useState<string | null>(
    null,
  );

  React.useEffect(() => {
    if (!selfPubkey) return;
    const read = () =>
      setSelectedPubkey(
        readConciergeSelection(selfPubkey)?.agentPubkey ?? null,
      );
    read();
    window.addEventListener(SELECTION_CHANGED_EVENT, read);
    window.addEventListener("storage", read);
    return () => {
      window.removeEventListener(SELECTION_CHANGED_EVENT, read);
      window.removeEventListener("storage", read);
    };
  }, [selfPubkey]);

  const selected = agentsQuery.data?.find(
    (agent) => agent.pubkey === selectedPubkey,
  );
  return selected?.name ?? "Concierge";
}

/**
 * Home-screen entry point: a floating mini-orb that opens the Concierge.
 * Shows the user's selected agent name. Self-contained so the (dense) Home
 * view stays untouched.
 */
export function ConciergeLauncher() {
  const { goConcierge } = useAppNavigation();
  const name = useConciergeName();
  return (
    <button
      aria-label={`Open ${name}`}
      className="concierge-launcher group fixed bottom-5 right-5 z-[45] flex items-center gap-2.5 rounded-full border border-border/60 bg-background/85 py-2 pl-2.5 pr-4 shadow-lg backdrop-blur-md transition-colors hover:border-primary/40 hover:bg-background"
      data-testid="concierge-launcher"
      onClick={() => {
        void goConcierge();
      }}
      type="button"
    >
      <span aria-hidden className="concierge-launcher__orb" />
      <span className="text-sm font-medium text-foreground/90">{name}</span>
    </button>
  );
}
