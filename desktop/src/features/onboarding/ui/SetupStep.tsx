import { TerminalSquare } from "lucide-react";

import { useAvailableAcpProviders } from "@/features/agents/hooks";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import type { SetupStepActions, SetupStepState } from "./types";

type SetupStepProps = {
  actions: SetupStepActions;
};

type SetupStepContentProps = {
  actions: SetupStepActions;
  state: SetupStepState;
};

function useSetupStepState(): SetupStepState {
  const providersQuery = useAvailableAcpProviders();
  const items = providersQuery.data ?? [];
  const isChecking = providersQuery.isLoading;
  const errorMessage =
    providersQuery.error instanceof Error ? providersQuery.error.message : null;

  return {
    runtimeProviders: {
      errorMessage,
      isChecking,
      items,
      showSetupLaterHint:
        errorMessage === null && !isChecking && items.length === 0,
    },
  };
}

function RuntimeProvidersSection({
  runtimeProviders,
}: {
  runtimeProviders: SetupStepState["runtimeProviders"];
}) {
  const { errorMessage, isChecking, items, showSetupLaterHint } =
    runtimeProviders;

  return (
    <div className="space-y-4 rounded-[28px] border border-black/10 bg-white/70 p-5">
      <div className="space-y-1">
        <div className="flex items-center gap-2">
          <TerminalSquare className="h-4 w-4 text-primary" />
          <p className="text-sm font-medium text-black">Detected runtimes</p>
        </div>
        <p className="text-sm text-black/60">
          We only list runtimes the app can actually see on this machine.
        </p>
      </div>

      {items.length > 0 ? (
        <div className="grid gap-2">
          {items.map((provider) => (
            <div
              className="rounded-2xl border border-black/10 bg-white px-4 py-3"
              data-testid={`onboarding-provider-${provider.id}`}
              key={provider.id}
            >
              <div className="flex items-center justify-between gap-3">
                <p className="text-sm font-medium text-black">
                  {provider.label}
                </p>
                <Badge
                  className="border-black/10 bg-black/[0.04] tracking-normal text-black/65"
                  variant="outline"
                >
                  {provider.command}
                </Badge>
              </div>
            </div>
          ))}
        </div>
      ) : isChecking ? (
        <p className="text-sm text-black/60">
          Looking for compatible runtimes...
        </p>
      ) : errorMessage ? null : (
        <p className="text-sm text-black/60" data-testid="onboarding-acp-empty">
          No compatible ACP runtimes detected yet.
        </p>
      )}

      {showSetupLaterHint ? (
        <p className="rounded-2xl border border-black/10 bg-white/70 px-4 py-3 text-sm text-black/60">
          Nothing is installed yet. That&apos;s fine. You can finish setup now
          and come back later in Settings &gt; Doctor.
        </p>
      ) : null}

      {errorMessage ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {errorMessage}
        </p>
      ) : null}
    </div>
  );
}

function SetupStepContent({ actions, state }: SetupStepContentProps) {
  const { runtimeProviders } = state;

  return (
    <div
      className="font-cash-sans space-y-6 text-black"
      data-testid="onboarding-page-2"
    >
      <div className="space-y-3">
        <Badge
          className="border border-black/10 bg-black/[0.04] tracking-normal text-black/65"
          variant="outline"
        >
          First run
        </Badge>
        <div className="space-y-2">
          <h1 className="arcade-type-welcome-title text-black">ACP runtimes</h1>
          <p className="max-w-xl text-sm leading-6 text-black/60">
            ACP runtimes only matter when you want Sprout to launch local tools
            from this machine.
          </p>
        </div>
      </div>

      <RuntimeProvidersSection runtimeProviders={runtimeProviders} />

      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          className="border-black/12 bg-transparent text-black hover:bg-black/[0.06] hover:text-black"
          data-testid="onboarding-back"
          onClick={actions.back}
          type="button"
          variant="outline"
        >
          Back
        </Button>
        <Button
          className="bg-black text-white hover:bg-black/85"
          data-testid="onboarding-finish"
          onClick={actions.complete}
          type="button"
        >
          Finish
        </Button>
      </div>
    </div>
  );
}

export function SetupStep({ actions }: SetupStepProps) {
  const state = useSetupStepState();

  return <SetupStepContent actions={actions} state={state} />;
}
