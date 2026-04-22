import type * as React from "react";

import type {
  DesktopNotificationPermissionState,
  NotificationSettings,
} from "@/features/notifications/hooks";
import type { AcpProvider, Profile } from "@/shared/api/types";

export type OnboardingPage = "profile" | "setup";

export type OnboardingActions = {
  complete: () => void;
  skipForNow: () => void;
};

export type OnboardingProfileSeed = {
  profile?: Profile;
};

export type OnboardingProfileValues = {
  avatarUrl: string;
  displayName: string;
};

export type OnboardingNotifications = {
  errorMessage: string | null;
  isUpdatingDesktopEnabled: boolean;
  permission: DesktopNotificationPermissionState;
  setDesktopEnabled: (enabled: boolean) => Promise<boolean>;
  settings: NotificationSettings;
};

export type ProfileStepSaveRecovery = {
  canAdvanceWithoutSaving: boolean;
  canSkipForNow: boolean;
  errorMessage: string | null;
};

export type ProfileStepNameState = {
  draftValue: string;
  savedValue: string;
};

export type ProfileStepAvatarState = {
  draftUrl: string;
  errorMessage: string | null;
  inputRef: React.RefObject<HTMLInputElement | null>;
  isUploading: boolean;
  savedUrl: string;
};

export type ProfileStepState = {
  avatar: ProfileStepAvatarState;
  isSaving: boolean;
  name: ProfileStepNameState;
  saveRecovery: ProfileStepSaveRecovery;
};

export type ProfileStepActions = {
  advanceWithoutSaving: () => void;
  clearAvatarDraft: () => void;
  openAvatarPicker: () => void;
  skipForNow: () => void;
  submit: () => void;
  updateAvatarUrl: (value: string) => void;
  updateDisplayName: (value: string) => void;
  uploadAvatarFile: (event: React.ChangeEvent<HTMLInputElement>) => void;
};

export type SetupStepActions = {
  back: () => void;
  complete: () => void;
  enableDesktopNotifications: () => void;
};

export type SetupStepRuntimeState = {
  errorMessage: string | null;
  isChecking: boolean;
  items: AcpProvider[];
  showSetupLaterHint: boolean;
};

export type SetupStepState = {
  notifications: OnboardingNotifications;
  runtimeProviders: SetupStepRuntimeState;
};
