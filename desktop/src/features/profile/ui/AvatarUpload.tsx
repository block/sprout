import * as React from "react";
import { Camera, Link2, Loader2 } from "lucide-react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { useAvatarUpload } from "@/features/profile/useAvatarUpload";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

type AvatarUploadProps = {
  avatarUrl: string;
  previewName: string;
  onUrlChange: (url: string) => void;
  onClear?: () => void;
  showClear?: boolean;
  disabled?: boolean;
  testIdPrefix?: string;
};

export function AvatarUpload({
  avatarUrl,
  previewName,
  onUrlChange,
  onClear,
  showClear,
  disabled,
  testIdPrefix = "avatar",
}: AvatarUploadProps) {
  const onUploadSuccess = React.useCallback(
    (url: string) => {
      onUrlChange(url);
    },
    [onUrlChange],
  );

  const {
    inputRef,
    isUploading,
    errorMessage,
    clearError,
    openPicker,
    handleFileChange,
  } = useAvatarUpload({ onUploadSuccess });

  const isInputDisabled = disabled || isUploading;

  return (
    <div className="rounded-[28px] border border-border/70 bg-muted/20 p-5">
      <div className="flex flex-col gap-5 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-4">
          <div className="relative h-20 w-20 shrink-0">
            <ProfileAvatar
              avatarUrl={avatarUrl || null}
              className="h-full w-full rounded-3xl text-xl"
              iconClassName="h-6 w-6"
              label={previewName}
              testId={`${testIdPrefix}-preview`}
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
            data-testid={`${testIdPrefix}-upload`}
            disabled={isInputDisabled}
            onClick={openPicker}
            size="lg"
            type="button"
          >
            {isUploading ? <Loader2 className="animate-spin" /> : <Camera />}
            {isUploading ? "Uploading..." : "Upload photo"}
          </Button>
          {showClear && onClear ? (
            <Button
              data-testid={`${testIdPrefix}-clear`}
              onClick={onClear}
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
            onChange={(event) => {
              void handleFileChange(event);
            }}
            ref={inputRef}
            type="file"
          />
        </div>
      </div>

      <div className="mt-5 space-y-1.5">
        <label className="text-sm font-medium" htmlFor={`${testIdPrefix}-url`}>
          Avatar URL
        </label>
        <div className="relative min-w-0">
          <Link2 className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-9"
            data-testid={`${testIdPrefix}-url`}
            disabled={isInputDisabled}
            id={`${testIdPrefix}-url`}
            onChange={(event) => {
              clearError();
              onUrlChange(event.target.value);
            }}
            placeholder="https://example.com/avatar.png"
            value={avatarUrl}
          />
        </div>
        <p className="text-xs text-muted-foreground">
          Prefer a link instead? Paste it here and we&apos;ll save that instead.
        </p>
      </div>

      {errorMessage ? (
        <p className="mt-3 rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {errorMessage}
        </p>
      ) : null}
    </div>
  );
}
