import { Camera, Link2, Loader2, UserRound } from "lucide-react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import type { ProfileStepActions, ProfileStepState } from "./types";

type ProfileStepProps = {
  actions: ProfileStepActions;
  state: ProfileStepState;
};

function ErrorBanner({ message }: { message: string | null }) {
  if (!message) {
    return null;
  }

  return (
    <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
      {message}
    </p>
  );
}

type AvatarSectionProps = {
  actions: Pick<
    ProfileStepActions,
    | "clearAvatarDraft"
    | "openAvatarPicker"
    | "updateAvatarUrl"
    | "uploadAvatarFile"
  >;
  avatar: ProfileStepState["avatar"];
  isSaving: boolean;
  previewName: string;
};

function AvatarSection({
  actions,
  avatar,
  isSaving,
  previewName,
}: AvatarSectionProps) {
  const {
    clearAvatarDraft,
    openAvatarPicker,
    updateAvatarUrl,
    uploadAvatarFile,
  } = actions;
  const { draftUrl, inputRef, isUploading, savedUrl } = avatar;
  const hasAvatarDraftChanges = draftUrl.length > 0 && draftUrl !== savedUrl;
  const isAvatarInputDisabled = isSaving || isUploading;

  return (
    <div className="rounded-[28px] border border-border/70 bg-muted/20 p-5">
      <div className="flex flex-col gap-5 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-4">
          <div className="relative h-20 w-20 shrink-0">
            <ProfileAvatar
              avatarUrl={draftUrl || null}
              className="h-full w-full rounded-3xl text-xl"
              iconClassName="h-6 w-6"
              label={previewName}
              testId="onboarding-avatar-preview"
            />
            <div className="absolute -bottom-1 -right-1 flex h-8 w-8 items-center justify-center rounded-full border border-background bg-primary text-primary-foreground shadow-sm">
              <Camera className="h-4 w-4" />
            </div>
          </div>
          <div className="space-y-2">
            <p className="text-sm font-medium">Add a profile photo</p>
            <p className="max-w-sm text-sm text-muted-foreground">
              Optional, but it makes conversations easier to scan.
            </p>
          </div>
        </div>

        <div className="flex flex-col items-stretch gap-2 sm:min-w-[220px]">
          <Button
            className="w-full justify-center"
            data-testid="onboarding-avatar-upload"
            disabled={isAvatarInputDisabled}
            onClick={openAvatarPicker}
            size="lg"
            type="button"
          >
            {isUploading ? <Loader2 className="animate-spin" /> : <Camera />}
            {isUploading ? "Uploading..." : "Upload photo"}
          </Button>
          {hasAvatarDraftChanges ? (
            <Button
              data-testid="onboarding-avatar-clear"
              onClick={clearAvatarDraft}
              size="sm"
              type="button"
              variant="ghost"
            >
              Undo
            </Button>
          ) : (
            <p className="text-xs text-muted-foreground">
              You can always add one later.
            </p>
          )}
          <input
            accept="image/gif,image/jpeg,image/png,image/webp"
            className="hidden"
            onChange={uploadAvatarFile}
            ref={inputRef}
            type="file"
          />
        </div>
      </div>

      <div className="mt-5 space-y-1.5">
        <label className="text-sm font-medium" htmlFor="onboarding-avatar-url">
          Avatar URL
        </label>
        <div className="relative min-w-0">
          <Link2 className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-9"
            data-testid="onboarding-avatar-url"
            disabled={isAvatarInputDisabled}
            id="onboarding-avatar-url"
            onChange={(event) => updateAvatarUrl(event.target.value)}
            placeholder="https://example.com/avatar.png"
            value={draftUrl}
          />
        </div>
        <p className="text-xs text-muted-foreground">
          Prefer a link instead? Paste it here and we&apos;ll save that instead.
        </p>
      </div>
    </div>
  );
}

export function ProfileStep({ actions, state }: ProfileStepProps) {
  const {
    advanceWithoutSaving,
    clearAvatarDraft,
    openAvatarPicker,
    skipForNow,
    submit,
    updateAvatarUrl,
    updateDisplayName,
    uploadAvatarFile,
  } = actions;
  const { avatar, isSaving, name, saveRecovery } = state;
  const { errorMessage: avatarErrorMessage } = avatar;
  const { draftValue: displayNameDraft, savedValue: savedDisplayName } = name;
  const isSubmittingDisabled = isSaving || avatar.isUploading;
  const canSubmit = displayNameDraft.trim().length > 0 && !isSubmittingDisabled;
  const avatarPreviewLabel =
    displayNameDraft.trim() || savedDisplayName || "You";

  return (
    <div className="space-y-6" data-testid="onboarding-page-1">
      <div className="space-y-3">
        <Badge variant="info">First run</Badge>
        <div className="space-y-2">
          <h1 className="text-3xl font-semibold tracking-tight text-foreground">
            Set up your profile
          </h1>
          <p className="max-w-xl text-sm leading-6 text-muted-foreground">
            Add the name people will see in Sprout. A photo is optional, but it
            helps people spot you faster.
          </p>
        </div>
      </div>

      <div className="space-y-2">
        <label
          className="text-sm font-medium"
          htmlFor="onboarding-display-name"
        >
          Display name
        </label>
        <div className="relative">
          <UserRound className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoFocus
            className="pl-9"
            data-testid="onboarding-display-name"
            disabled={isSubmittingDisabled}
            id="onboarding-display-name"
            onChange={(event) => updateDisplayName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && canSubmit) {
                event.preventDefault();
                submit();
              }
            }}
            placeholder="How people should see you"
            value={displayNameDraft}
          />
        </div>
        <p className="text-xs text-muted-foreground">
          You can change this later in Profile settings.
        </p>
      </div>

      <AvatarSection
        actions={{
          clearAvatarDraft,
          openAvatarPicker,
          updateAvatarUrl,
          uploadAvatarFile,
        }}
        avatar={avatar}
        isSaving={isSaving}
        previewName={avatarPreviewLabel}
      />

      <ErrorBanner message={avatarErrorMessage} />
      <ErrorBanner message={saveRecovery.errorMessage} />

      <div className="flex flex-wrap items-center justify-end gap-2">
        {saveRecovery.canSkipForNow ? (
          <Button
            data-testid="onboarding-skip"
            onClick={skipForNow}
            type="button"
            variant="outline"
          >
            Skip for now
          </Button>
        ) : null}
        {saveRecovery.canAdvanceWithoutSaving ? (
          <Button
            data-testid="onboarding-next-without-saving"
            onClick={advanceWithoutSaving}
            type="button"
            variant="outline"
          >
            Continue without saving
          </Button>
        ) : null}
        <Button
          data-testid="onboarding-next"
          disabled={!canSubmit}
          onClick={submit}
          type="button"
        >
          {isSaving ? "Saving..." : "Next"}
        </Button>
      </div>
    </div>
  );
}
