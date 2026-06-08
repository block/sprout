import { useAppNavigation } from "@/app/navigation/useAppNavigation";

import "./concierge.css";

/**
 * Home-screen entry point: a floating mini-orb that opens the Concierge.
 * Self-contained so the (dense) Home view stays untouched.
 */
export function ConciergeLauncher() {
  const { goConcierge } = useAppNavigation();
  return (
    <button
      aria-label="Open Concierge"
      className="concierge-launcher group fixed bottom-5 right-5 z-[45] flex items-center gap-2.5 rounded-full border border-border/60 bg-background/85 py-2 pl-2.5 pr-4 shadow-lg backdrop-blur-md transition-colors hover:border-primary/40 hover:bg-background"
      data-testid="concierge-launcher"
      onClick={() => {
        void goConcierge();
      }}
      type="button"
    >
      <span aria-hidden className="concierge-launcher__orb" />
      <span className="text-sm font-medium text-foreground/90">Concierge</span>
    </button>
  );
}
