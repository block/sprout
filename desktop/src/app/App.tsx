import { getCurrentWindow } from "@tauri-apps/api/window";
import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { ArrowLeft } from "lucide-react";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { router } from "@/app/router";
import { useReloadShortcut } from "@/app/useReloadShortcut";
import { useAppOnboardingState } from "@/features/onboarding/hooks";
import { OnboardingFlow } from "@/features/onboarding/ui/OnboardingFlow";
import type { Workspace } from "@/features/workspaces/types";
import { useWorkspaceInit } from "@/features/workspaces/useWorkspaceInit";
import { useWorkspaces } from "@/features/workspaces/useWorkspaces";
import { WelcomeSetup } from "@/features/workspaces/ui/WelcomeSetup";
import { createSproutQueryClient } from "@/shared/api/queryClient";
import { isSharedIdentity as isSharedIdentityCmd } from "@/shared/api/tauri";
import { listenForDeepLinks } from "@/shared/deep-link";
import { cn } from "@/shared/lib/cn";
import { useSmoothCornerClipPath } from "@/shared/lib/useSmoothCornerClipPath";

const FIRST_RUN_PREVIEW_PANEL_RADIUS = 12;
const FIRST_RUN_PREVIEW_PANEL_SMOOTHING = 0.6;
const FIRST_RUN_PREVIEW_RETURN_TO_WELCOME_MS = 700;

function AppLoadingGate() {
  return <div className="min-h-dvh bg-background" />;
}

function WorkspaceQueryProvider({ children }: { children: ReactNode }) {
  const [queryClient] = useState(createSproutQueryClient);

  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

function FirstRunNamePreview({ onBack }: { onBack?: () => void }) {
  const [isReturningToWelcome, setIsReturningToWelcome] = useState(false);
  const returnToWelcomeTimerRef = useRef<number | null>(null);
  const leftPanelClip = useSmoothCornerClipPath<HTMLElement>({
    cornerRadius: FIRST_RUN_PREVIEW_PANEL_RADIUS,
    cornerSmoothing: FIRST_RUN_PREVIEW_PANEL_SMOOTHING,
  });
  const contentPanelClip = useSmoothCornerClipPath<HTMLElement>({
    cornerRadius: FIRST_RUN_PREVIEW_PANEL_RADIUS,
    cornerSmoothing: FIRST_RUN_PREVIEW_PANEL_SMOOTHING,
  });

  useEffect(() => {
    return () => {
      if (returnToWelcomeTimerRef.current !== null) {
        window.clearTimeout(returnToWelcomeTimerRef.current);
      }
    };
  }, []);

  const handleBack = useCallback(() => {
    if (!onBack || isReturningToWelcome) {
      return;
    }

    if (returnToWelcomeTimerRef.current !== null) {
      window.clearTimeout(returnToWelcomeTimerRef.current);
    }

    setIsReturningToWelcome(true);
    returnToWelcomeTimerRef.current = window.setTimeout(() => {
      returnToWelcomeTimerRef.current = null;
      onBack();
    }, FIRST_RUN_PREVIEW_RETURN_TO_WELCOME_MS);
  }, [isReturningToWelcome, onBack]);

  return (
    <div className="font-cash-sans min-h-dvh bg-[#F2F2F2] p-2">
      <div
        aria-hidden="true"
        className="fixed inset-x-0 top-0 z-20 h-10 cursor-default select-none"
        data-tauri-drag-region
      />

      <div
        className={cn(
          "grid min-h-[calc(100dvh-16px)] overflow-hidden transition-[gap,grid-template-columns] duration-[700ms] ease-[cubic-bezier(0.19,1,0.22,1)]",
          isReturningToWelcome ? "gap-0" : "gap-2",
        )}
        style={{
          gridTemplateColumns: isReturningToWelcome
            ? "minmax(0, 1fr) minmax(0, 0fr)"
            : "minmax(280px, 1fr) minmax(0, 2fr)",
        }}
      >
        <section
          className="relative min-h-[calc(100dvh-16px)] min-w-0 overflow-hidden bg-black text-white"
          ref={leftPanelClip.ref}
          style={leftPanelClip.style}
        >
          <div
            aria-hidden={!isReturningToWelcome}
            className={cn(
              "absolute inset-0 flex items-center justify-center px-6 py-12 text-center transition-[opacity,transform] duration-[500ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
              isReturningToWelcome
                ? "opacity-100 delay-75"
                : "pointer-events-none opacity-0",
            )}
          >
            <div
              className={cn(
                "flex w-full max-w-[560px] flex-col items-center transition-transform duration-[500ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
                isReturningToWelcome ? "scale-100" : "scale-[0.98]",
              )}
            >
              <img
                alt="Sprout"
                className="h-16 w-16 object-contain"
                src="/sprout-welcome.png"
              />

              <div className="arcade-type-welcome-kicker mt-8 rounded-full bg-white px-4 py-1.5 text-black">
                BETA PREVIEW
              </div>

              <div className="mt-20 space-y-3 sm:mt-16">
                <h1 className="arcade-type-welcome-title text-white">
                  Welcome to Sprout
                </h1>
                <p className="arcade-type-welcome-subcopy text-white/45">
                  Where people and AI agents work together
                </p>
              </div>

              <div className="mt-20 grid w-full max-w-[560px] grid-cols-1 gap-5 justify-self-center sm:mt-16 sm:grid-cols-2">
                <div className="arcade-type-body-medium h-auto min-h-0 rounded-full bg-white px-6 py-4 text-black">
                  Continue with default settings
                </div>

                <div className="arcade-type-body-medium h-auto min-h-0 rounded-full bg-[#262626] px-6 py-4 text-white">
                  Custom Relay
                </div>
              </div>
            </div>
          </div>

          <div
            aria-hidden={isReturningToWelcome}
            className={cn(
              "relative flex min-h-[calc(100dvh-16px)] w-full flex-col p-12 transition-[opacity,transform,min-height] duration-[400ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
              isReturningToWelcome
                ? "pointer-events-none translate-x-8 opacity-0"
                : "translate-x-0 opacity-100",
            )}
          >
            <div className="max-w-[420px]">
              <h1 className="arcade-type-display-headline-small text-white">
                First, let's start with your name
              </h1>
              <p className="arcade-type-body-medium mt-4 text-white/45">
                Enter a nickname or whatever you want people to call you
              </p>
            </div>

            <div className="mt-auto flex w-full items-center gap-10">
              <button
                aria-label="Back"
                className="flex h-14 w-14 shrink-0 items-center justify-center rounded-full bg-[#262626] p-0 text-white shadow-none transition-colors hover:bg-[#303030]"
                disabled={isReturningToWelcome}
                onClick={handleBack}
                tabIndex={onBack ? undefined : -1}
                type="button"
              >
                <ArrowLeft className="h-6 w-6" strokeWidth={2.25} />
              </button>

              <button
                className="arcade-type-body-medium h-14 min-h-0 flex-1 rounded-full bg-[#5A5A5A] px-6 py-0 text-black/45 shadow-none"
                disabled
                type="button"
              >
                Next
              </button>
            </div>
          </div>
        </section>

        <section
          className={cn(
            "flex min-h-[calc(100dvh-16px)] min-w-0 items-center justify-center overflow-hidden bg-[#F2F2F2] px-6 py-12 text-black transition-opacity duration-300 sm:px-10",
            isReturningToWelcome
              ? "pointer-events-none opacity-0"
              : "opacity-100",
          )}
          ref={contentPanelClip.ref}
          style={contentPanelClip.style}
        >
          <div className="h-20 w-full max-w-[576px]" />
        </section>
      </div>
    </div>
  );
}

function AppReady({
  fallback,
  isSharedIdentity,
  onGateResolved,
  onReturnToWelcome,
}: {
  fallback?: ReactNode;
  isSharedIdentity: boolean;
  onGateResolved?: () => void;
  onReturnToWelcome: () => void;
}) {
  const onboarding = useAppOnboardingState(isSharedIdentity);

  useEffect(() => {
    if (onboarding.stage !== "blocking") {
      onGateResolved?.();
    }
  }, [onGateResolved, onboarding.stage]);

  if (onboarding.stage === "onboarding") {
    return (
      <OnboardingFlow
        actions={{
          ...onboarding.flow.actions,
          returnToWelcome: onReturnToWelcome,
        }}
        initialProfile={onboarding.flow.initialProfile}
        key={onboarding.currentPubkey ?? "anonymous"}
      />
    );
  }

  if (onboarding.stage === "blocking") {
    return fallback ?? <AppLoadingGate />;
  }

  return <RouterProvider router={router} />;
}

function DevFirstRunResetButton() {
  if (!import.meta.env.DEV) {
    return null;
  }

  const resetFirstRun = () => {
    window.localStorage.removeItem("sprout-workspaces");
    window.localStorage.removeItem("sprout-active-workspace-id");

    for (let index = window.localStorage.length - 1; index >= 0; index -= 1) {
      const key = window.localStorage.key(index);
      if (key?.startsWith("sprout-onboarding-complete.v1:")) {
        window.localStorage.removeItem(key);
      }
    }

    window.location.reload();
  };

  return (
    <button
      className="arcade-type-body-medium fixed bottom-4 right-4 z-[1000] h-auto min-h-0 rounded-full border border-white/15 bg-[#262626] px-4 py-2 text-white shadow-[0_8px_32px_rgba(0,0,0,0.28)] transition-colors hover:bg-[#303030]"
      onClick={resetFirstRun}
      type="button"
    >
      Reset
    </button>
  );
}

export function App() {
  // Mounted at the root so Cmd/Ctrl+R reloads in every app state,
  // including the loading and first-run setup screens below.
  useReloadShortcut();

  useLayoutEffect(() => {
    void getCurrentWindow().show();
  }, []);

  const [sharedIdentity, setSharedIdentity] = useState<boolean | null>(null);
  useEffect(() => {
    isSharedIdentityCmd()
      .then(setSharedIdentity)
      .catch((err) => {
        console.warn("is_shared_identity command failed:", err);
        setSharedIdentity(false);
      });
  }, []);

  const {
    activeWorkspace,
    reinitKey,
    addWorkspace,
    clearWorkspaces,
    switchWorkspace,
    reconnectWorkspace,
  } = useWorkspaces();

  useEffect(() => {
    const unlisten = listenForDeepLinks({
      addWorkspace,
      switchWorkspace,
      reconnectWorkspace,
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [addWorkspace, switchWorkspace, reconnectWorkspace]);
  // Composite key: changes when workspace ID changes OR when
  // the active workspace's config is updated (relayUrl/token).
  const workspaceKey = `${activeWorkspace?.id ?? "none"}-${reinitKey}`;
  const workspace = useWorkspaceInit(
    activeWorkspace,
    workspaceKey,
    sharedIdentity ?? false,
  );
  const [isFirstRunPreview, setIsFirstRunPreview] = useState(false);
  const [forcedWelcomeRelayUrl, setForcedWelcomeRelayUrl] = useState<
    string | null
  >(null);

  const handleSetupComplete = useCallback(
    (nextWorkspace: Workspace) => {
      setForcedWelcomeRelayUrl(null);
      setIsFirstRunPreview(true);
      const nextWorkspaceId = addWorkspace(nextWorkspace);
      switchWorkspace(nextWorkspaceId);
    },
    [addWorkspace, switchWorkspace],
  );

  const handleGateResolved = useCallback(() => {
    setIsFirstRunPreview(false);
  }, []);

  const handleReturnToWelcome = useCallback(() => {
    setForcedWelcomeRelayUrl(
      activeWorkspace?.relayUrl ?? "ws://localhost:3000",
    );
    setIsFirstRunPreview(false);
    clearWorkspaces();
  }, [activeWorkspace?.relayUrl, clearWorkspaces]);

  let content: ReactNode;

  // Wait for the shared-identity IPC call to resolve before rendering
  // anything that depends on it. Without this gate, children briefly see
  // isSharedIdentity=false and may flash WelcomeSetup or the onboarding flow.
  if (sharedIdentity === null) {
    content = <AppLoadingGate />;
  } else if (forcedWelcomeRelayUrl !== null) {
    content = (
      <WelcomeSetup
        defaultRelayUrl={forcedWelcomeRelayUrl}
        onComplete={handleSetupComplete}
      />
    );
  } else if (workspace.needsSetup) {
    // Show welcome setup for first-run users with no workspaces
    content = (
      <WelcomeSetup
        defaultRelayUrl={workspace.defaultRelayUrl}
        onComplete={handleSetupComplete}
      />
    );
  } else if (!workspace.isReady || workspace.appliedKey !== workspaceKey) {
    // Wait for this exact workspace config to be applied to the backend before
    // rendering anything that connects to the relay. The appliedKey check avoids
    // a one-render race where React sees the new active workspace while the Tauri
    // backend is still configured for the previous one.
    content = isFirstRunPreview ? (
      <FirstRunNamePreview onBack={handleReturnToWelcome} />
    ) : (
      <AppLoadingGate />
    );
  } else {
    content = (
      <WorkspaceQueryProvider key={workspaceKey}>
        <AppReady
          fallback={
            isFirstRunPreview ? (
              <FirstRunNamePreview onBack={handleReturnToWelcome} />
            ) : null
          }
          isSharedIdentity={sharedIdentity}
          key={workspaceKey}
          onGateResolved={handleGateResolved}
          onReturnToWelcome={handleReturnToWelcome}
        />
      </WorkspaceQueryProvider>
    );
  }

  return (
    <>
      {content}
      <DevFirstRunResetButton />
    </>
  );
}
