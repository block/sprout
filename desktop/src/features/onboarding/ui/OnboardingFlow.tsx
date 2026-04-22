import * as React from "react";

import { useUpdateProfileMutation } from "@/features/profile/hooks";
import { uploadMediaBytes } from "@/shared/api/tauri";
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

    const updatePayload = createProfileUpdatePayload({
      draftProfile: profileDraft,
      savedProfile,
    });

    if (Object.keys(updatePayload).length > 0) {
      try {
        await profileUpdateMutation.mutateAsync(updatePayload);
      } catch {
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
