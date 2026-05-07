import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  profileQueryKey,
  useUpdateProfileMutation,
} from "@/features/profile/hooks";
import { useWorkspaces } from "@/features/workspaces/useWorkspaces";
import {
  getIdentity,
  importIdentity as tauriImportIdentity,
  uploadMediaBytes,
} from "@/shared/api/tauri";
import { getMyRelayMembershipLookup } from "@/shared/api/relayMembers";
import { useIdentityQuery } from "@/shared/api/hooks";
import { pubkeyToNpub } from "@/shared/lib/nostrUtils";
import { relayClient } from "@/shared/api/relayClient";
import { MembershipDenied } from "./MembershipDenied";
import { ProfileStep } from "./ProfileStep";
import { SetupStep } from "./SetupStep";
import type {
  OnboardingActions,
  OnboardingNotifications,
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
  notifications: OnboardingNotifications;
};

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

const AVATAR_IMAGE_TYPES = [
  "image/gif",
  "image/jpeg",
  "image/png",
  "image/webp",
];

export function OnboardingFlow({
  actions,
  initialProfile,
  notifications,
}: OnboardingFlowProps) {
  const { complete, skipForNow } = actions;
  const { setDesktopEnabled } = notifications;
  const savedProfile = resolveSavedProfile(initialProfile);
  const avatarInputRef = React.useRef<HTMLInputElement | null>(null);
  const profileUpdateMutation = useUpdateProfileMutation();
  const { error: profileSaveError, isPending: isSavingProfile } =
    profileUpdateMutation;
  const [currentPage, setCurrentPage] =
    React.useState<OnboardingPage>("profile");
  const [profileDraft, setProfileDraft] =
    React.useState<OnboardingProfileValues>(savedProfile);
  const [avatarErrorMessage, setAvatarErrorMessage] = React.useState<
    string | null
  >(null);
  const [isUploadingAvatar, setIsUploadingAvatar] = React.useState(false);
  const [deniedPubkey, setDeniedPubkey] = React.useState<string>("");

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

  const openAvatarPicker = React.useCallback(() => {
    avatarInputRef.current?.click();
  }, []);

  const resetProfileSaveError = React.useCallback(() => {
    profileUpdateMutation.reset();
  }, [profileUpdateMutation]);

  const updateProfileDraft = React.useCallback(
    (
      patch: Partial<OnboardingProfileValues>,
      options?: { clearAvatarError?: boolean },
    ) => {
      resetProfileSaveError();
      if (options?.clearAvatarError) {
        setAvatarErrorMessage(null);
      }
      setProfileDraft((current) => ({
        ...current,
        ...patch,
      }));
    },
    [resetProfileSaveError],
  );

  const handleAvatarFileChange = React.useCallback(
    async (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = "";

      if (!file) {
        return;
      }

      if (!AVATAR_IMAGE_TYPES.includes(file.type)) {
        setAvatarErrorMessage("Choose a PNG, JPG, GIF, or WebP image.");
        return;
      }

      resetProfileSaveError();
      setIsUploadingAvatar(true);
      setAvatarErrorMessage(null);

      try {
        const buffer = await file.arrayBuffer();
        const uploaded = await uploadMediaBytes([...new Uint8Array(buffer)]);
        updateProfileDraft(
          { avatarUrl: uploaded.url },
          { clearAvatarError: true },
        );
      } catch (error) {
        setAvatarErrorMessage(
          error instanceof Error
            ? error.message
            : "Could not upload that avatar.",
        );
      } finally {
        setIsUploadingAvatar(false);
      }
    },
    [resetProfileSaveError, updateProfileDraft],
  );

  const showSetupPage = React.useCallback(() => {
    setCurrentPage("setup");
  }, []);

  const showProfilePage = React.useCallback(() => {
    setCurrentPage("profile");
  }, []);

  const saveProfileAndContinue = React.useCallback(async () => {
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

        // Error falls through to the error banner / recovery buttons.
        return;
      }
    }

    showSetupPage();
  }, [profileDraft, profileUpdateMutation, savedProfile, showSetupPage]);

  const updateDisplayNameDraft = React.useCallback(
    (value: string) => {
      updateProfileDraft({ displayName: value });
    },
    [updateProfileDraft],
  );

  const updateAvatarUrlDraft = React.useCallback(
    (value: string) => {
      updateProfileDraft({ avatarUrl: value }, { clearAvatarError: true });
    },
    [updateProfileDraft],
  );

  const resetAvatarDraft = React.useCallback(() => {
    updateProfileDraft(
      { avatarUrl: savedProfile.avatarUrl },
      { clearAvatarError: true },
    );
  }, [savedProfile.avatarUrl, updateProfileDraft]);

  const handleEnableDesktopNotifications = React.useCallback(() => {
    void setDesktopEnabled(true);
  }, [setDesktopEnabled]);
  const saveErrorMessage =
    profileSaveError instanceof Error ? profileSaveError.message : null;
  const profileStepState: ProfileStepState = {
    avatar: {
      draftUrl: profileDraft.avatarUrl,
      errorMessage: avatarErrorMessage,
      inputRef: avatarInputRef,
      isUploading: isUploadingAvatar,
      savedUrl: savedProfile.avatarUrl,
    },
    currentNpub,
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
          void saveProfileAndContinue();
        }}
        pubkey={deniedPubkey}
      />
    );
  }

  return (
    <div
      className="flex min-h-dvh items-center justify-center bg-[radial-gradient(circle_at_top,hsl(var(--primary)/0.16),transparent_44%),linear-gradient(180deg,hsl(var(--background)),hsl(var(--muted)/0.5))] px-4 py-8"
      data-testid="onboarding-gate"
    >
      <div className="w-full max-w-xl rounded-[32px] border border-border/70 bg-background/94 p-6 shadow-2xl backdrop-blur sm:p-8">
        {currentPage === "profile" ? (
          <ProfileStep
            actions={{
              advanceWithoutSaving: showSetupPage,
              clearAvatarDraft: resetAvatarDraft,
              importIdentity: handleImportIdentity,
              openAvatarPicker,
              skipForNow,
              submit: () => {
                void saveProfileAndContinue();
              },
              updateAvatarUrl: updateAvatarUrlDraft,
              updateDisplayName: updateDisplayNameDraft,
              uploadAvatarFile: (event) => {
                void handleAvatarFileChange(event);
              },
            }}
            state={profileStepState}
          />
        ) : (
          <SetupStep
            actions={{
              back: showProfilePage,
              complete,
              enableDesktopNotifications: handleEnableDesktopNotifications,
            }}
            notifications={notifications}
          />
        )}
      </div>
    </div>
  );
}
