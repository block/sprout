import * as React from "react";
import { ArrowLeft } from "lucide-react";

import {
  playOnboardingSound,
  playOnboardingTypingSoundForKey,
  preloadOnboardingSounds,
} from "@/features/onboarding/ui/onboardingSounds";
import { getIdentity } from "@/shared/api/tauri";
import { cn } from "@/shared/lib/cn";
import { useSmoothCornerClipPath } from "@/shared/lib/useSmoothCornerClipPath";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

import { initFirstWorkspace } from "../workspaceStorage";
import type { Workspace } from "../types";

type WelcomeSetupProps = {
  defaultRelayUrl: string;
  onComplete: (workspace: Workspace) => void;
};

const WELCOME_PANEL_RADIUS = 12;
const WELCOME_PANEL_SMOOTHING = 0.6;
const WELCOME_PANEL_TRANSITION_MS = 700;
const WELCOME_PANEL_CONTENT_REVEAL_BUFFER_MS = 120;
const WELCOME_PANEL_CONTENT_REVEAL_DELAY_MS =
  WELCOME_PANEL_TRANSITION_MS + WELCOME_PANEL_CONTENT_REVEAL_BUFFER_MS;
const WELCOME_PANEL_CONTENT_REVEAL_DURATION_MS = 360;
const WELCOME_PANEL_HANDOFF_MS =
  WELCOME_PANEL_CONTENT_REVEAL_DELAY_MS +
  WELCOME_PANEL_CONTENT_REVEAL_DURATION_MS;

export function WelcomeSetup({
  defaultRelayUrl,
  onComplete,
}: WelcomeSetupProps) {
  const welcomePanelClip = useSmoothCornerClipPath<HTMLElement>({
    cornerRadius: WELCOME_PANEL_RADIUS,
    cornerSmoothing: WELCOME_PANEL_SMOOTHING,
  });
  const nextPanelClip = useSmoothCornerClipPath<HTMLElement>({
    cornerRadius: WELCOME_PANEL_RADIUS,
    cornerSmoothing: WELCOME_PANEL_SMOOTHING,
  });
  const [relayUrl, setRelayUrl] = React.useState(defaultRelayUrl);
  const [showCustomRelay, setShowCustomRelay] = React.useState(false);
  const [isConnecting, setIsConnecting] = React.useState(false);
  const [isAdvancing, setIsAdvancing] = React.useState(false);
  const [isNamePreviewCopyMounted, setIsNamePreviewCopyMounted] =
    React.useState(false);
  const [isNamePreviewCopyVisible, setIsNamePreviewCopyVisible] =
    React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const advanceTimerRef = React.useRef<number | null>(null);
  const namePreviewCopyTimerRef = React.useRef<number | null>(null);
  const namePreviewCopyFrameRef = React.useRef<number | null>(null);
  const connectRunRef = React.useRef(0);
  const transitionRunRef = React.useRef(0);

  const clearWelcomeTransitionTimers = React.useCallback(() => {
    if (advanceTimerRef.current !== null) {
      window.clearTimeout(advanceTimerRef.current);
      advanceTimerRef.current = null;
    }
    if (namePreviewCopyTimerRef.current !== null) {
      window.clearTimeout(namePreviewCopyTimerRef.current);
      namePreviewCopyTimerRef.current = null;
    }
    if (namePreviewCopyFrameRef.current !== null) {
      window.cancelAnimationFrame(namePreviewCopyFrameRef.current);
      namePreviewCopyFrameRef.current = null;
    }
  }, []);

  React.useEffect(() => {
    preloadOnboardingSounds();

    return clearWelcomeTransitionTimers;
  }, [clearWelcomeTransitionTimers]);

  const handleConnect = React.useCallback(
    async (nextRelayUrl?: string) => {
      const trimmedUrl = (nextRelayUrl ?? relayUrl).trim();
      if (!trimmedUrl) {
        setError("Please enter a relay URL.");
        return;
      }

      setIsConnecting(true);
      setError(null);
      const connectRun = connectRunRef.current + 1;
      connectRunRef.current = connectRun;

      try {
        // We snapshot only the pubkey for display purposes (workspace switcher
        // labels, etc.). The private key lives on disk in `identity.key` and
        // is the single source of truth — never copied into localStorage.
        const identity = await getIdentity();
        const workspace = initFirstWorkspace(trimmedUrl, identity.pubkey);
        if (connectRunRef.current !== connectRun) {
          return;
        }

        // App.tsx takes this workspace and applies it through useWorkspaceInit.
        // No reload here; keeping the handoff in React avoids a visual flash.
        onComplete(workspace);
      } catch (err) {
        if (connectRunRef.current !== connectRun) {
          return;
        }
        setError(
          err instanceof Error ? err.message : "Failed to connect. Try again.",
        );
        setIsConnecting(false);
        setIsAdvancing(false);
        setIsNamePreviewCopyMounted(false);
        setIsNamePreviewCopyVisible(false);
      }
    },
    [relayUrl, onComplete],
  );

  const handleDefaultContinue = React.useCallback(() => {
    transitionRunRef.current += 1;
    const transitionRun = transitionRunRef.current;
    clearWelcomeTransitionTimers();
    setIsAdvancing(true);
    setIsNamePreviewCopyMounted(false);
    setIsNamePreviewCopyVisible(false);
    setError(null);
    namePreviewCopyTimerRef.current = window.setTimeout(() => {
      if (transitionRunRef.current !== transitionRun) {
        return;
      }

      namePreviewCopyTimerRef.current = null;
      setIsNamePreviewCopyMounted(true);
      namePreviewCopyFrameRef.current = window.requestAnimationFrame(() => {
        if (transitionRunRef.current !== transitionRun) {
          namePreviewCopyFrameRef.current = null;
          return;
        }

        namePreviewCopyFrameRef.current = window.requestAnimationFrame(() => {
          namePreviewCopyFrameRef.current = null;
          if (transitionRunRef.current !== transitionRun) {
            return;
          }

          setIsNamePreviewCopyVisible(true);
        });
      });
    }, WELCOME_PANEL_CONTENT_REVEAL_DELAY_MS);
    advanceTimerRef.current = window.setTimeout(() => {
      if (transitionRunRef.current !== transitionRun) {
        return;
      }

      advanceTimerRef.current = null;
      void handleConnect(defaultRelayUrl);
    }, WELCOME_PANEL_HANDOFF_MS);
  }, [clearWelcomeTransitionTimers, defaultRelayUrl, handleConnect]);

  const playForwardSoundOnPress = React.useCallback(() => {
    if (isConnecting || isAdvancing) {
      return;
    }

    playOnboardingSound("toggleA");
  }, [isAdvancing, isConnecting]);

  const playBackSoundOnPress = React.useCallback(() => {
    playOnboardingSound("toggleB");
  }, []);

  const playButtonSoundOnKeyboardPress = React.useCallback(
    (
      event: React.KeyboardEvent<HTMLButtonElement>,
      soundName: "toggleA" | "toggleB",
    ) => {
      if (event.repeat || (event.key !== "Enter" && event.key !== " ")) {
        return;
      }

      if (soundName === "toggleA") {
        playForwardSoundOnPress();
        return;
      }

      playBackSoundOnPress();
    },
    [playBackSoundOnPress, playForwardSoundOnPress],
  );

  const handleBackFromNamePreview = React.useCallback(() => {
    transitionRunRef.current += 1;
    connectRunRef.current += 1;
    clearWelcomeTransitionTimers();

    setIsConnecting(false);
    setIsAdvancing(false);
    setIsNamePreviewCopyMounted(false);
    setIsNamePreviewCopyVisible(false);
    setError(null);
  }, [clearWelcomeTransitionTimers]);

  return (
    <div className="font-cash-sans min-h-dvh bg-[#F2F2F2] p-2 text-white">
      <div
        aria-hidden="true"
        className="fixed inset-x-0 top-0 z-20 h-10 cursor-default select-none"
        data-tauri-drag-region
      />

      <div
        className={cn(
          "grid min-h-[calc(100dvh-16px)] overflow-hidden transition-[gap,grid-template-columns] duration-[700ms] ease-[cubic-bezier(0.19,1,0.22,1)]",
          isAdvancing ? "gap-2" : "gap-0",
        )}
        style={{
          gridTemplateColumns: isAdvancing
            ? "minmax(280px, 1fr) minmax(0, 2fr)"
            : "minmax(0, 1fr) minmax(0, 0fr)",
        }}
      >
        <section
          className={cn(
            "flex min-h-[calc(100dvh-16px)] bg-black transition-[padding] duration-[700ms] ease-[cubic-bezier(0.19,1,0.22,1)]",
            isAdvancing
              ? "flex-col p-12"
              : "items-center justify-center px-6 py-12",
          )}
          ref={welcomePanelClip.ref}
          style={welcomePanelClip.style}
        >
          {isAdvancing ? (
            isNamePreviewCopyMounted ? (
              <>
                <div
                  className={cn(
                    "max-w-[420px] transition-[opacity,transform] duration-[360ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
                    isNamePreviewCopyVisible
                      ? "translate-y-0 opacity-100"
                      : "translate-y-3 opacity-0",
                  )}
                >
                  <h1 className="arcade-type-display-headline-small text-white">
                    First, let's start with your name
                  </h1>
                  <p className="arcade-type-body-medium mt-4 text-white/45">
                    Enter a nickname or whatever you want people to call you
                  </p>
                </div>

                <div
                  className={cn(
                    "mt-auto flex w-full items-center gap-10 transition-[opacity,transform] duration-[360ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
                    isNamePreviewCopyVisible
                      ? "translate-y-0 opacity-100"
                      : "translate-y-3 opacity-0",
                  )}
                >
                  <Button
                    aria-label="Back"
                    className="h-14 w-14 shrink-0 rounded-full bg-[#262626] p-0 text-white shadow-none hover:bg-[#303030]"
                    disabled={!isNamePreviewCopyVisible}
                    onClick={handleBackFromNamePreview}
                    onKeyDown={(event) =>
                      playButtonSoundOnKeyboardPress(event, "toggleB")
                    }
                    onPointerDown={playBackSoundOnPress}
                    type="button"
                    variant="ghost"
                  >
                    <ArrowLeft className="!h-6 !w-6" strokeWidth={2.25} />
                  </Button>

                  <Button
                    className="arcade-type-body-medium h-14 min-h-0 flex-1 rounded-full bg-[#5A5A5A] px-6 py-0 text-black/45 shadow-none opacity-100"
                    disabled
                    type="button"
                  >
                    Next
                  </Button>
                </div>
              </>
            ) : null
          ) : (
            <div className="flex w-full max-w-[560px] flex-col items-center text-center">
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

              <div className="mt-20 flex w-full flex-col items-center gap-5 sm:mt-16">
                {showCustomRelay ? (
                  <div className="flex w-full flex-col items-center gap-5">
                    <div className="w-full space-y-2 text-left">
                      <label
                        className="text-sm font-medium text-white/55"
                        htmlFor="relay-url"
                      >
                        Relay URL
                      </label>
                      <Input
                        autoFocus
                        className="h-14 rounded-full border-white/14 bg-white/[0.06] px-6 text-lg text-white placeholder:text-white/35 focus-visible:ring-white/30"
                        id="relay-url"
                        onChange={(e) => {
                          setRelayUrl(e.target.value);
                          setError(null);
                        }}
                        onKeyDown={playOnboardingTypingSoundForKey}
                        placeholder="wss://relay.example.com"
                        type="url"
                        value={relayUrl}
                      />
                    </div>

                    <div className="inline-grid gap-5 justify-self-center">
                      <Button
                        className="arcade-type-body-medium h-auto min-h-0 w-full rounded-full bg-white px-6 py-4 text-black hover:bg-white/90"
                        disabled={isConnecting || !relayUrl.trim()}
                        onClick={() => {
                          void handleConnect();
                        }}
                        onKeyDown={(event) =>
                          playButtonSoundOnKeyboardPress(event, "toggleA")
                        }
                        onPointerDown={playForwardSoundOnPress}
                        type="button"
                      >
                        {isConnecting
                          ? "Connecting..."
                          : "Connect to custom relay"}
                      </Button>

                      <Button
                        className="arcade-type-body-medium h-auto min-h-0 w-full rounded-full bg-[#262626] px-6 py-4 text-white hover:bg-[#303030]"
                        disabled={isConnecting}
                        onClick={() => {
                          setShowCustomRelay(false);
                          setRelayUrl(defaultRelayUrl);
                          setError(null);
                        }}
                        type="button"
                      >
                        Use default settings
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="grid w-full max-w-[560px] grid-cols-1 gap-5 justify-self-center sm:grid-cols-2">
                    <Button
                      className="arcade-type-body-medium h-auto min-h-0 w-full rounded-full bg-white px-6 py-4 text-black hover:bg-white/90"
                      disabled={isConnecting || isAdvancing}
                      onClick={handleDefaultContinue}
                      onKeyDown={(event) =>
                        playButtonSoundOnKeyboardPress(event, "toggleA")
                      }
                      onPointerDown={playForwardSoundOnPress}
                      type="button"
                    >
                      {isConnecting || isAdvancing
                        ? "Continuing..."
                        : "Continue with default settings"}
                    </Button>

                    <Button
                      className="arcade-type-body-medium h-auto min-h-0 w-full rounded-full bg-[#262626] px-6 py-4 text-white hover:bg-[#303030]"
                      disabled={isConnecting || isAdvancing}
                      onClick={() => {
                        setRelayUrl(defaultRelayUrl);
                        setShowCustomRelay(true);
                        setError(null);
                      }}
                      onKeyDown={(event) =>
                        playButtonSoundOnKeyboardPress(event, "toggleA")
                      }
                      onPointerDown={playForwardSoundOnPress}
                      type="button"
                    >
                      Custom Relay
                    </Button>
                  </div>
                )}

                {error ? (
                  <p className="text-sm text-destructive">{error}</p>
                ) : null}
              </div>
            </div>
          )}
        </section>

        <section
          aria-hidden="true"
          className={cn(
            "flex min-h-[calc(100dvh-16px)] items-center justify-center overflow-hidden bg-[#F2F2F2] px-6 transition-[opacity,transform] duration-300 sm:px-10",
            isAdvancing
              ? "scale-100 opacity-100 delay-100"
              : "scale-[0.98] opacity-0",
          )}
          ref={nextPanelClip.ref}
          style={nextPanelClip.style}
        >
          <div className="h-20 w-full max-w-[576px]" />
        </section>
      </div>
    </div>
  );
}
