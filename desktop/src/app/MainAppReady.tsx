import { RouterProvider } from "@tanstack/react-router";

import { AppLoadingGate } from "@/app/AppLoadingGate";
import { router } from "@/app/router";
import { useAppOnboardingState } from "@/features/onboarding/hooks";
import { OnboardingFlow } from "@/features/onboarding/ui/OnboardingFlow";

export function MainAppReady() {
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
