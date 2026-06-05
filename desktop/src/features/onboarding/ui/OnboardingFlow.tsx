import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";

import {
  profileQueryKey,
  useUpdateProfileMutation,
} from "@/features/profile/hooks";
import { useWorkspaces } from "@/features/workspaces/useWorkspaces";
import {
  getIdentity,
  importIdentity as tauriImportIdentity,
} from "@/shared/api/tauri";
import { getMyRelayMembershipLookup } from "@/shared/api/relayMembers";
import { useIdentityQuery } from "@/shared/api/hooks";
import { pubkeyToNpub } from "@/shared/lib/nostrUtils";
import { relayClient } from "@/shared/api/relayClient";
import { cn } from "@/shared/lib/cn";
import { useSmoothCornerClipPath } from "@/shared/lib/useSmoothCornerClipPath";
import { Button } from "@/shared/ui/button";
import type { Profile } from "@/shared/api/types";
import { AgentsStep } from "./AgentsStep";
import { AvatarStep } from "./AvatarStep";
import { CompleteStep } from "./CompleteStep";
import { MembershipDenied } from "./MembershipDenied";
import { ProfileStep } from "./ProfileStep";
import { SetupStep } from "./SetupStep";
import { TeamTasksStep } from "./TeamTasksStep";
import {
  playOnboardingSound,
  preloadOnboardingSounds,
} from "./onboardingSounds";
import type {
  OnboardingActions,
  OnboardingPage,
  OnboardingProfileSeed,
  OnboardingProfileValues,
  ProfileStepState,
} from "./types";

/**
 * Check whether the relay denies access due to membership gating.
 *
 * Uses the standard relay message path to read the NIP-43 membership snapshot.
 *
 * Returns `true` if denied, `false` if the user is a member (or if the
 * relay doesn't enforce membership / isn't reachable).
 */
function isRelayMembershipDeniedError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }

  return (
    error.message.includes("You must be a relay member") ||
    error.message.includes("relay_membership_required") ||
    error.message.includes("restricted: not a relay member") ||
    error.message.includes("invalid: you are not a relay member")
  );
}

async function checkMembershipDenied(): Promise<boolean> {
  try {
    const { membership, snapshotFound } = await getMyRelayMembershipLookup();
    return snapshotFound && membership === null;
  } catch (error) {
    if (isRelayMembershipDeniedError(error)) {
      return true;
    }
    // Network errors, 401s, 500s — not membership denials.
    return false;
  }
}

type OnboardingFlowProps = {
  actions: OnboardingActions;
  initialProfile: OnboardingProfileSeed;
};

const ONBOARDING_PANEL_RADIUS = 12;
const ONBOARDING_PANEL_SMOOTHING = 0.6;
const ONBOARDING_RETURN_TO_WELCOME_MS = 700;

function isFallbackDisplayName(value?: string | null) {
  const normalizedValue = value?.trim().toLowerCase() ?? "";
  return (
    normalizedValue.startsWith("npub1") ||
    normalizedValue.startsWith("nostr:npub1")
  );
}

function sanitizeDisplayName(value?: string | null) {
  const trimmedValue = value?.trim() ?? "";
  return isFallbackDisplayName(trimmedValue) ? "" : trimmedValue;
}

function resolveSavedProfile({
  profile,
}: OnboardingProfileSeed): OnboardingProfileValues {
  return {
    avatarUrl: profile?.avatarUrl ?? "",
    displayName: sanitizeDisplayName(profile?.displayName),
  };
}

function createProfileUpdatePayload({
  draftProfile,
  savedProfile,
}: {
  draftProfile: OnboardingProfileValues;
  savedProfile: OnboardingProfileValues;
}) {
  const nextDisplayName = draftProfile.displayName.trim();
  const nextAvatarUrl = draftProfile.avatarUrl.trim();
  const updatePayload: {
    avatarUrl?: string;
    displayName?: string;
  } = {};

  if (
    nextDisplayName.length > 0 &&
    nextDisplayName !== savedProfile.displayName
  ) {
    updatePayload.displayName = nextDisplayName;
  }

  if (nextAvatarUrl.length > 0 && nextAvatarUrl !== savedProfile.avatarUrl) {
    updatePayload.avatarUrl = nextAvatarUrl;
  }

  return updatePayload;
}

function resolveProfileSaveRecovery(
  errorMessage: string | null,
  savedDisplayName: string,
): ProfileStepState["saveRecovery"] {
  return {
    canAdvanceWithoutSaving:
      errorMessage !== null && savedDisplayName.length > 0,
    canSkipForNow: errorMessage !== null && savedDisplayName.length === 0,
    errorMessage,
  };
}

export function OnboardingFlow({
  actions,
  initialProfile,
}: OnboardingFlowProps) {
  const leftPanelClip = useSmoothCornerClipPath<HTMLElement>({
    cornerRadius: ONBOARDING_PANEL_RADIUS,
    cornerSmoothing: ONBOARDING_PANEL_SMOOTHING,
  });
  const contentPanelClip = useSmoothCornerClipPath<HTMLElement>({
    cornerRadius: ONBOARDING_PANEL_RADIUS,
    cornerSmoothing: ONBOARDING_PANEL_SMOOTHING,
  });
  const { complete, returnToWelcome, skipForNow } = actions;
  const savedProfile = resolveSavedProfile(initialProfile);
  const profileUpdateMutation = useUpdateProfileMutation();
  const { error: profileSaveError, isPending: isSavingProfile } =
    profileUpdateMutation;
  const [currentPage, setCurrentPage] =
    React.useState<OnboardingPage>("profile");
  const [profileDraft, setProfileDraft] =
    React.useState<OnboardingProfileValues>(savedProfile);
  const [deniedPubkey, setDeniedPubkey] = React.useState<string>("");
  const [isUploadingAvatar, setIsUploadingAvatar] = React.useState(false);
  const [isReturningToWelcome, setIsReturningToWelcome] = React.useState(false);
  const returnToWelcomeTimerRef = React.useRef<number | null>(null);

  React.useEffect(() => {
    preloadOnboardingSounds();

    return () => {
      if (returnToWelcomeTimerRef.current !== null) {
        window.clearTimeout(returnToWelcomeTimerRef.current);
      }
    };
  }, []);

  // For displaying the current identity at the top of the profile step and
  // for refreshing the UI in place after `import_identity` completes — the
  // `key={currentPubkey}` on this component in App.tsx remounts the whole
  // tree once the cache update lands, giving us a clean reset of all
  // form/import state without a `window.location.reload()`.
  const queryClient = useQueryClient();
  const identityQuery = useIdentityQuery();
  const currentNpub = React.useMemo(() => {
    const pubkey = identityQuery.data?.pubkey;
    if (!pubkey) {
      return null;
    }
    try {
      return pubkeyToNpub(pubkey);
    } catch {
      return null;
    }
  }, [identityQuery.data?.pubkey]);

  // Used by the import action to update the active workspace's display
  // pubkey. Workspaces never store the nsec — `identity.key` on disk is the
  // single source of truth — but we keep `pubkey` accurate so switcher
  // labels and similar UI reflect the active identity.
  const { activeWorkspace, updateWorkspace } = useWorkspaces();

  const resetProfileSaveError = React.useCallback(() => {
    profileUpdateMutation.reset();
  }, [profileUpdateMutation]);

  const updateProfileDraft = React.useCallback(
    (patch: Partial<OnboardingProfileValues>) => {
      resetProfileSaveError();
      setProfileDraft((current) => ({
        ...current,
        ...patch,
      }));
    },
    [resetProfileSaveError],
  );

  const showAvatarPage = React.useCallback(() => {
    setCurrentPage("avatar");
  }, []);

  const showAgentsPage = React.useCallback(() => {
    setCurrentPage("agents");
  }, []);

  const showTeamPage = React.useCallback(() => {
    setCurrentPage("team");
  }, []);

  const showProfilePage = React.useCallback(() => {
    setCurrentPage("profile");
  }, []);

  const showWelcomeSetup = React.useCallback(() => {
    if (returnToWelcomeTimerRef.current !== null) {
      window.clearTimeout(returnToWelcomeTimerRef.current);
    }

    setIsReturningToWelcome(true);
    returnToWelcomeTimerRef.current = window.setTimeout(() => {
      returnToWelcomeTimerRef.current = null;
      returnToWelcome();
    }, ONBOARDING_RETURN_TO_WELCOME_MS);
  }, [returnToWelcome]);

  const seedLocalProfileDraft = React.useCallback(() => {
    const pubkey = identityQuery.data?.pubkey;
    if (!pubkey) {
      return;
    }

    const displayName = profileDraft.displayName.trim();
    const avatarUrl = profileDraft.avatarUrl.trim();
    queryClient.setQueryData<Profile | undefined>(
      profileQueryKey,
      (existing) => ({
        pubkey,
        displayName: displayName || (existing?.displayName ?? null),
        avatarUrl: avatarUrl || (existing?.avatarUrl ?? null),
        about: existing?.about ?? null,
        nip05Handle: existing?.nip05Handle ?? null,
      }),
    );
  }, [
    identityQuery.data?.pubkey,
    profileDraft.avatarUrl,
    profileDraft.displayName,
    queryClient,
  ]);

  const saveProfileAndContinue = React.useCallback(
    async (nextPage: OnboardingPage) => {
      if (profileDraft.displayName.trim().length === 0) {
        return;
      }

      // Check membership before attempting the profile save. On open relays
      // this passes instantly. On gated relays it prevents a 403 during save.
      const denied = await checkMembershipDenied();
      if (denied) {
        try {
          const identity = await getIdentity();
          setDeniedPubkey(identity.pubkey);
        } catch {
          setDeniedPubkey("");
        }
        setCurrentPage("membership-denied");
        return;
      }

      const updatePayload = createProfileUpdatePayload({
        draftProfile: profileDraft,
        savedProfile,
      });

      if (Object.keys(updatePayload).length > 0) {
        try {
          await profileUpdateMutation.mutateAsync(updatePayload);
        } catch (error) {
          if (isRelayMembershipDeniedError(error)) {
            try {
              const identity = await getIdentity();
              setDeniedPubkey(identity.pubkey);
            } catch {
              setDeniedPubkey("");
            }
            setCurrentPage("membership-denied");
            return;
          }

          // Temporary first-run bypass: local relay builds may not be able to
          // persist profile metadata yet, but the design flow should continue.
          console.warn(
            "Profile save failed during onboarding; continuing with local draft.",
            error,
          );
          seedLocalProfileDraft();
          resetProfileSaveError();
          setCurrentPage(nextPage);
          return;
        }
      }

      setCurrentPage(nextPage);
    },
    [
      profileDraft,
      profileUpdateMutation,
      resetProfileSaveError,
      savedProfile,
      seedLocalProfileDraft,
    ],
  );

  const updateDisplayNameDraft = React.useCallback(
    (value: string) => {
      updateProfileDraft({ displayName: value });
    },
    [updateProfileDraft],
  );

  const updateAvatarUrlDraft = React.useCallback(
    (value: string) => {
      updateProfileDraft({ avatarUrl: value });
    },
    [updateProfileDraft],
  );

  const resetAvatarDraft = React.useCallback(() => {
    updateProfileDraft({ avatarUrl: savedProfile.avatarUrl });
  }, [savedProfile.avatarUrl, updateProfileDraft]);

  const saveErrorMessage =
    profileSaveError instanceof Error ? profileSaveError.message : null;
  const profileStepState: ProfileStepState = {
    avatar: {
      draftUrl: profileDraft.avatarUrl,
      savedUrl: savedProfile.avatarUrl,
    },
    currentNpub,
    isUploadingAvatar,
    isSaving: isSavingProfile,
    name: {
      draftValue: profileDraft.displayName,
      savedValue: savedProfile.displayName,
    },
    saveRecovery: resolveProfileSaveRecovery(
      saveErrorMessage,
      savedProfile.displayName,
    ),
  };
  const canSubmitProfile =
    profileStepState.name.draftValue.trim().length > 0 &&
    !profileStepState.isSaving &&
    !profileStepState.isUploadingAvatar;
  const canSubmitAvatar =
    profileStepState.avatar.draftUrl.trim().length > 0 &&
    !profileStepState.isSaving &&
    !profileStepState.isUploadingAvatar;
  const canSubmitAgents = currentPage === "agents";
  const canSubmitTeam = currentPage === "team";
  const canSubmitComplete = currentPage === "complete";
  const canSubmitCurrentStep =
    currentPage === "complete"
      ? canSubmitComplete
      : currentPage === "team"
        ? canSubmitTeam
        : currentPage === "agents"
          ? canSubmitAgents
          : currentPage === "avatar"
            ? canSubmitAvatar
            : canSubmitProfile;
  const activeIntroIndex =
    currentPage === "profile"
      ? 0
      : currentPage === "avatar"
        ? 1
        : currentPage === "agents"
          ? 2
          : currentPage === "team"
            ? 3
            : currentPage === "complete"
              ? 4
              : 5;

  const handleBack = React.useCallback(() => {
    if (isReturningToWelcome) {
      return;
    }

    if (currentPage === "team") {
      showAgentsPage();
      return;
    }

    if (currentPage === "agents") {
      showAvatarPage();
      return;
    }

    if (currentPage === "avatar") {
      showProfilePage();
      return;
    }

    showWelcomeSetup();
  }, [
    currentPage,
    isReturningToWelcome,
    showAgentsPage,
    showAvatarPage,
    showProfilePage,
    showWelcomeSetup,
  ]);

  const handleNext = React.useCallback(() => {
    if (isReturningToWelcome) {
      return;
    }

    if (currentPage === "profile") {
      void saveProfileAndContinue("avatar");
      return;
    }

    if (currentPage === "avatar") {
      void saveProfileAndContinue("agents");
      return;
    }

    if (currentPage === "agents") {
      setCurrentPage("team");
      return;
    }

    if (currentPage === "team") {
      setCurrentPage("complete");
      return;
    }

    if (currentPage === "complete") {
      complete();
    }
  }, [complete, currentPage, isReturningToWelcome, saveProfileAndContinue]);

  const playButtonSoundOnPress = React.useCallback(
    (soundName: "toggleA" | "toggleB") => {
      if (isReturningToWelcome) {
        return;
      }

      playOnboardingSound(soundName);
    },
    [isReturningToWelcome],
  );

  const playBackSoundOnPress = React.useCallback(() => {
    playButtonSoundOnPress("toggleB");
  }, [playButtonSoundOnPress]);

  const playNextSoundOnPress = React.useCallback(() => {
    playButtonSoundOnPress("toggleA");
  }, [playButtonSoundOnPress]);

  const playButtonSoundOnKeyboardPress = React.useCallback(
    (
      event: React.KeyboardEvent<HTMLButtonElement>,
      soundName: "toggleA" | "toggleB",
    ) => {
      if (event.repeat || (event.key !== "Enter" && event.key !== " ")) {
        return;
      }

      playButtonSoundOnPress(soundName);
    },
    [playButtonSoundOnPress],
  );

  const handleImportIdentity = React.useCallback(
    async (nsec: string) => {
      // Backend writes the nsec to `identity.key`, swaps `state.keys`, and
      // clears any session token. After this returns, every Rust command
      // reads the new key fresh on the next call.
      const next = await tauriImportIdentity(nsec);

      // Drop the WebSocket so it re-AUTHs as the new pubkey on next use.
      // Stale subscriptions bound to the old pubkey would otherwise leak
      // through and cause confusing membership/permission errors until the
      // user navigated away.
      try {
        relayClient.disconnect();
      } catch (error) {
        console.warn("relayClient.disconnect() during import failed", error);
      }

      // Update the active workspace's display pubkey. The workspace never
      // stores nsec — this is purely cosmetic for the workspace switcher.
      if (activeWorkspace && activeWorkspace.pubkey !== next.pubkey) {
        updateWorkspace(activeWorkspace.id, { pubkey: next.pubkey });
      }

      // Drop any membership-denied banner from a previous identity.
      setDeniedPubkey("");

      // Refresh identity + profile caches. The identity query lives at
      // staleTime: Infinity so an explicit invalidation is required.
      // Once `["identity"]` updates, App.tsx's `key={currentPubkey}` will
      // remount this entire component, giving us a clean form state for
      // the new identity without a page reload.
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["identity"] }),
        queryClient.invalidateQueries({ queryKey: profileQueryKey }),
      ]);
    },
    [activeWorkspace, queryClient, updateWorkspace],
  );

  if (currentPage === "membership-denied") {
    return (
      <MembershipDenied
        onChangeKey={showProfilePage}
        onRetry={() => {
          void saveProfileAndContinue("avatar");
        }}
        pubkey={deniedPubkey}
      />
    );
  }

  return (
    <div
      className={cn(
        "font-cash-sans min-h-dvh p-2 transition-[background-color,padding] duration-[700ms] ease-[cubic-bezier(0.19,1,0.22,1)]",
        "bg-[#F2F2F2]",
      )}
      data-testid="onboarding-gate"
      style={
        {
          "--onboarding-panel-height": "calc(100dvh - 16px)",
        } as React.CSSProperties
      }
    >
      <div
        aria-hidden="true"
        className="fixed inset-x-0 top-0 z-20 h-10 cursor-default select-none"
        data-tauri-drag-region
      />

      <div
        className={cn(
          "grid min-h-[var(--onboarding-panel-height)] overflow-hidden transition-[gap,grid-template-columns,min-height] duration-[700ms] ease-[cubic-bezier(0.19,1,0.22,1)]",
          isReturningToWelcome ? "gap-0" : "gap-2",
        )}
        style={{
          gridTemplateColumns: isReturningToWelcome
            ? "minmax(0, 1fr) minmax(0, 0fr)"
            : "minmax(280px, 1fr) minmax(0, 2fr)",
        }}
      >
        <section
          className="relative min-h-[var(--onboarding-panel-height)] min-w-0 overflow-hidden bg-black text-white transition-[min-height] duration-[700ms] ease-[cubic-bezier(0.19,1,0.22,1)]"
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
              "relative flex min-h-[var(--onboarding-panel-height)] w-full flex-col p-12 transition-[opacity,transform,min-height] duration-[400ms] ease-[cubic-bezier(0.16,1,0.3,1)]",
              isReturningToWelcome
                ? "pointer-events-none translate-x-8 opacity-0"
                : "translate-x-0 opacity-100",
            )}
          >
            <div className="relative min-h-[268px] max-w-[460px] overflow-hidden">
              {[
                {
                  title: "First, let's start with your name",
                  body: "Enter a nickname or whatever you want people to call you",
                },
                {
                  title: "Next, add a display image",
                  body: "Choose an image or emoji as your avatar",
                },
                {
                  title: "Using agents to get work done.",
                  body: "In sprout, you can create agents to help you look for files, create and submit PRs, or anything else you might be looking to do",
                },
                {
                  title: "Use a single agent or a team for more intense tasks",
                  body: "Pair builders, reviewers, researchers, and specialists to handle work that needs more than one perspective.",
                },
                {
                  title: "That's it! — welcome to Sprout",
                  body: "You're now ready to collaborate with teammates and agents to get work done.",
                },
                {
                  title: "Next, let's check your setup",
                  body: "Sprout can launch local tools when compatible runtimes are available.",
                },
              ].map((intro, index) => (
                <div
                  aria-hidden={activeIntroIndex !== index}
                  className="absolute inset-x-0 top-0 transition-[opacity,transform] duration-500 ease-[cubic-bezier(0.22,1,0.36,1)]"
                  key={intro.title}
                  style={{
                    opacity: activeIntroIndex === index ? 1 : 0,
                    transform: `translateX(${(index - activeIntroIndex) * 115}%)`,
                  }}
                >
                  <h1 className="arcade-type-display-headline-small text-white">
                    {intro.title}
                  </h1>
                  <p className="arcade-type-body-medium mt-4 text-white/45">
                    {intro.body}
                  </p>
                </div>
              ))}
            </div>

            {currentPage === "profile" ||
            currentPage === "avatar" ||
            currentPage === "agents" ||
            currentPage === "team" ||
            currentPage === "complete" ? (
              <div className="mt-auto flex w-full items-center gap-10">
                {currentPage === "complete" ? null : (
                  <Button
                    aria-label="Back"
                    className="h-14 w-14 shrink-0 rounded-full bg-[#262626] p-0 text-white shadow-none hover:bg-[#303030]"
                    data-testid="onboarding-back-to-welcome"
                    disabled={isReturningToWelcome}
                    onClick={handleBack}
                    onKeyDown={(event) =>
                      playButtonSoundOnKeyboardPress(event, "toggleB")
                    }
                    onPointerDown={playBackSoundOnPress}
                    type="button"
                    variant="ghost"
                  >
                    <ArrowLeft className="!h-6 !w-6" strokeWidth={2.25} />
                  </Button>
                )}

                <Button
                  className={cn(
                    "arcade-type-body-medium h-14 min-h-0 flex-1 rounded-full px-6 py-0 shadow-none",
                    "bg-white text-black hover:bg-white/90",
                    "disabled:bg-[#5A5A5A] disabled:text-black/45 disabled:opacity-100",
                  )}
                  data-testid={
                    currentPage === "complete"
                      ? "onboarding-finish"
                      : "onboarding-next"
                  }
                  disabled={!canSubmitCurrentStep || isReturningToWelcome}
                  onClick={handleNext}
                  onKeyDown={(event) =>
                    playButtonSoundOnKeyboardPress(event, "toggleA")
                  }
                  onPointerDown={playNextSoundOnPress}
                  type="button"
                >
                  {currentPage === "complete"
                    ? "Get started"
                    : profileStepState.isSaving
                      ? "Saving..."
                      : "Next"}
                </Button>
              </div>
            ) : null}
          </div>
        </section>

        <section
          aria-hidden={isReturningToWelcome}
          className={cn(
            "flex min-h-[var(--onboarding-panel-height)] min-w-0 items-center justify-center overflow-hidden bg-[#F2F2F2] px-6 py-12 text-black transition-[opacity,transform,min-height] duration-[420ms] [--background:0_0%_100%] [--border:0_0%_82%] [--foreground:0_0%_0%] [--input:0_0%_70%] [--muted-foreground:0_0%_38%] [--primary-foreground:0_0%_100%] [--primary:0_0%_0%] [--ring:0_0%_0%] ease-[cubic-bezier(0.16,1,0.3,1)] sm:px-10",
            isReturningToWelcome
              ? "pointer-events-none opacity-0"
              : "opacity-100",
          )}
          ref={contentPanelClip.ref}
          style={contentPanelClip.style}
        >
          {currentPage === "setup" ? (
            <div className="w-full max-w-xl">
              <SetupStep
                actions={{
                  back: showTeamPage,
                  complete,
                }}
              />
            </div>
          ) : (
            <div className="relative flex min-h-[704px] w-full max-w-[1120px] items-center justify-center overflow-visible">
              <div
                inert={currentPage !== "profile"}
                className="absolute inset-0 flex items-center justify-center transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
                style={{
                  opacity: currentPage === "profile" ? 1 : 0,
                  transform:
                    currentPage === "profile" ? "scale(1)" : "scale(0.95)",
                  pointerEvents: currentPage === "profile" ? "auto" : "none",
                }}
              >
                <ProfileStep
                  actions={{
                    advanceWithoutSaving: showAvatarPage,
                    clearAvatarDraft: resetAvatarDraft,
                    importIdentity: handleImportIdentity,
                    onUploadingChange: setIsUploadingAvatar,
                    skipForNow,
                    submit: () => {
                      void saveProfileAndContinue("avatar");
                    },
                    updateAvatarUrl: updateAvatarUrlDraft,
                    updateDisplayName: updateDisplayNameDraft,
                  }}
                  state={profileStepState}
                />
              </div>

              <div
                inert={currentPage !== "avatar"}
                className="absolute inset-0 flex items-center justify-center transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
                style={{
                  opacity: currentPage === "avatar" ? 1 : 0,
                  transform:
                    currentPage === "avatar" ? "scale(1)" : "scale(0.95)",
                  pointerEvents: currentPage === "avatar" ? "auto" : "none",
                }}
              >
                <AvatarStep
                  actions={{
                    updateAvatarUrl: updateAvatarUrlDraft,
                  }}
                  state={profileStepState}
                />
              </div>

              <div
                inert={currentPage !== "agents"}
                className="absolute inset-0 flex items-center justify-center transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
                style={{
                  opacity: currentPage === "agents" ? 1 : 0,
                  transform:
                    currentPage === "agents" ? "scale(1)" : "scale(0.95)",
                  pointerEvents: currentPage === "agents" ? "auto" : "none",
                }}
              >
                <AgentsStep />
              </div>

              <div
                inert={currentPage !== "team"}
                className="absolute inset-0 flex items-center justify-center transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
                style={{
                  opacity: currentPage === "team" ? 1 : 0,
                  transform:
                    currentPage === "team" ? "scale(1)" : "scale(0.95)",
                  pointerEvents: currentPage === "team" ? "auto" : "none",
                }}
              >
                {currentPage === "team" ? (
                  <TeamTasksStep profile={profileDraft} />
                ) : null}
              </div>

              <div
                inert={currentPage !== "complete"}
                className="absolute inset-0 flex items-center justify-center transition-[opacity,transform] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
                style={{
                  opacity: currentPage === "complete" ? 1 : 0,
                  transform:
                    currentPage === "complete" ? "scale(1)" : "scale(0.95)",
                  pointerEvents: currentPage === "complete" ? "auto" : "none",
                }}
              >
                {currentPage === "complete" ? <CompleteStep /> : null}
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
