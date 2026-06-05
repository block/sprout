import * as React from "react";
import { createFileRoute, Navigate } from "@tanstack/react-router";

import { FeatureGate } from "@/shared/features";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const PulseScreen = React.lazy(async () => {
  const module = await import("@/features/pulse/ui/PulseScreen");
  return { default: module.PulseScreen };
});

export const Route = createFileRoute("/pulse")({
  component: PulseRouteComponent,
});

function PulseRouteComponent() {
  return (
    <FeatureGate feature="pulse" fallback={<Navigate to="/" />}>
      <React.Suspense
        fallback={<ViewLoadingFallback includeHeader kind="pulse" />}
      >
        <PulseScreen />
      </React.Suspense>
    </FeatureGate>
  );
}
