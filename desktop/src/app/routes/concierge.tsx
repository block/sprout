import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import type {
  ConciergePhase,
  DispatchIntent,
} from "@/features/concierge/types";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const ConciergeLiveScreen = React.lazy(async () => {
  const module = await import("@/features/concierge/ui/ConciergeLiveScreen");
  return { default: module.ConciergeLiveScreen };
});

const ConciergeDemoScreen = React.lazy(async () => {
  const module = await import("@/features/concierge/ui/ConciergeScreen");
  return { default: module.ConciergeScreen };
});

type ConciergeSearch = {
  /** Screenshot/aesthetics loop — forces the static demo screen. */
  demo?: boolean;
  phase?: ConciergePhase;
  dispatch?: DispatchIntent["status"];
};

const PHASES: ConciergePhase[] = ["idle", "listening", "thinking", "speaking"];
const DISPATCH_STATES: DispatchIntent["status"][] = [
  "pending",
  "approved",
  "dismissed",
];

export const Route = createFileRoute("/concierge")({
  // The screenshot/aesthetics loop drives the static demo screen via search
  // params; without them the route renders the live session.
  validateSearch: (search: Record<string, unknown>): ConciergeSearch => ({
    demo: search.demo === true || search.demo === "true" ? true : undefined,
    phase: PHASES.includes(search.phase as ConciergePhase)
      ? (search.phase as ConciergePhase)
      : undefined,
    dispatch: DISPATCH_STATES.includes(
      search.dispatch as DispatchIntent["status"],
    )
      ? (search.dispatch as DispatchIntent["status"])
      : undefined,
  }),
  component: ConciergeRouteComponent,
});

function ConciergeRouteComponent() {
  const { demo, phase, dispatch } = Route.useSearch();
  const useDemo = demo || phase !== undefined || dispatch !== undefined;
  return (
    <React.Suspense
      fallback={<ViewLoadingFallback includeHeader kind="pulse" />}
    >
      {useDemo ? (
        <ConciergeDemoScreen
          initialDispatchStatus={dispatch}
          initialPhase={phase}
        />
      ) : (
        <ConciergeLiveScreen />
      )}
    </React.Suspense>
  );
}
