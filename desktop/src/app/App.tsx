import { getCurrentWindow } from "@tauri-apps/api/window";
import { RouterProvider } from "@tanstack/react-router";
import { useLayoutEffect } from "react";

import { router } from "@/app/router";
import { useAppOnboardingState } from "@/features/onboarding/hooks";
import { OnboardingFlow } from "@/features/onboarding/ui/OnboardingFlow";

function AppLoadingGate() {
  return (
    <div className="flex min-h-dvh items-center justify-center bg-[radial-gradient(circle_at_top,hsl(var(--primary)/0.14),transparent_48%),linear-gradient(180deg,hsl(var(--background)),hsl(var(--muted)/0.55))] px-4 py-8">
      <div className="w-full max-w-sm rounded-[28px] border border-border/70 bg-background/92 p-8 shadow-2xl backdrop-blur">
        <p className="text-xs font-medium uppercase tracking-[0.2em] text-muted-foreground">
          Sprout
        </p>
        <h1 className="mt-3 text-2xl font-semibold tracking-tight text-foreground">
          Checking your setup
        </h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          One sec while we load your profile.
        </p>
      </div>
    </div>
  );
}

export function App() {
  useLayoutEffect(() => {
    void getCurrentWindow().show();
  }, []);

  const onboarding = useAppOnboardingState();

  if (onboarding.stage === "onboarding") {
    return (
      <OnboardingFlow
        actions={onboarding.flow.actions}
        initialProfile={onboarding.flow.initialProfile}
        key={onboarding.currentPubkey ?? "anonymous"}
        notifications={onboarding.flow.notifications}
      />
    );
  }

  if (onboarding.stage === "blocking") {
    return <AppLoadingGate />;
  }

  return <RouterProvider router={router} />;
}
