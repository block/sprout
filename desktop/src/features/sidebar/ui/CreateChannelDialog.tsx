import { Lock, Zap } from "lucide-react";
import * as React from "react";

import type { ChannelVisibility } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

/** Default TTL for ephemeral channels: 1 day of inactivity. */
const EPHEMERAL_TTL_SECONDS = 86400;

type ChannelKind = "stream" | "forum";

type CreateChannelDialogProps = {
  /** Which kind of channel to create, or null when closed. */
  channelKind: ChannelKind | null;
  isCreating: boolean;
  onOpenChange: (open: boolean) => void;
  onCreate: (input: {
    name: string;
    description?: string;
    visibility: ChannelVisibility;
    ttlSeconds?: number;
  }) => Promise<void>;
};

// ---------------------------------------------------------------------------
// CreateChannelDialog
// ---------------------------------------------------------------------------

export function CreateChannelDialog({
  channelKind,
  isCreating,
  onOpenChange,
  onCreate,
}: CreateChannelDialogProps) {
  const open = channelKind !== null;
  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [visibility, setVisibility] = React.useState<ChannelVisibility>("open");
  const [ephemeral, setEphemeral] = React.useState(false);
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);
  const nameInputRef = React.useRef<HTMLInputElement>(null);

  const kindLabel = channelKind === "forum" ? "forum" : "channel";

  // Reset form state when dialog opens/closes or kind changes
  React.useEffect(() => {
    if (!open) return;

    setName("");
    setDescription("");
    setVisibility("open");
    setEphemeral(false);
    setErrorMessage(null);

    // Small delay to let dialog animation start before focusing
    const timerId = globalThis.setTimeout(() => {
      nameInputRef.current?.focus();
    }, 50);
    return () => globalThis.clearTimeout(timerId);
  }, [open]);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const trimmedName = name.trim();
    if (!trimmedName) return;

    setErrorMessage(null);

    try {
      await onCreate({
        name: trimmedName,
        description: description.trim() || undefined,
        visibility,
        ttlSeconds: ephemeral ? EPHEMERAL_TTL_SECONDS : undefined,
      });

      onOpenChange(false);
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : `Failed to create ${kindLabel}.`,
      );
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && isCreating) return;
        onOpenChange(nextOpen);
      }}
    >
      <DialogContent className="max-w-lg" data-testid="create-channel-dialog">
        <DialogHeader>
          <DialogTitle>Create a new {kindLabel}</DialogTitle>
          <DialogDescription>
            {channelKind === "forum"
              ? "Forums organize threaded discussions around a topic."
              : "Channels are real-time streams for team conversation."}
          </DialogDescription>
        </DialogHeader>

        <form
          className="space-y-4"
          onSubmit={(event) => {
            void handleSubmit(event);
          }}
        >
          {/* Name */}
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium text-foreground"
              htmlFor="create-channel-name"
            >
              Name
            </label>
            <Input
              autoCapitalize="none"
              autoComplete="off"
              autoCorrect="off"
              data-testid="create-channel-name"
              disabled={isCreating}
              id="create-channel-name"
              onChange={(event) => {
                setName(event.target.value);
                setErrorMessage(null);
              }}
              placeholder={
                channelKind === "forum" ? "design-discussions" : "release-notes"
              }
              ref={nameInputRef}
              spellCheck={false}
              value={name}
            />
          </div>

          {/* Description */}
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium text-foreground"
              htmlFor="create-channel-description"
            >
              Description{" "}
              <span className="font-normal text-muted-foreground">
                (optional)
              </span>
            </label>
            <Textarea
              className="min-h-16 resize-none"
              data-testid="create-channel-description"
              disabled={isCreating}
              id="create-channel-description"
              onChange={(event) => {
                setDescription(event.target.value);
                setErrorMessage(null);
              }}
              placeholder={`What this ${kindLabel} is for`}
              rows={2}
              value={description}
            />
          </div>

          {/* Options */}
          <div className="space-y-3">
            <PrivateCheckbox
              disabled={isCreating}
              isPrivate={visibility === "private"}
              onChange={(isPrivate) =>
                setVisibility(isPrivate ? "private" : "open")
              }
            />
            <EphemeralCheckbox
              disabled={isCreating}
              isEphemeral={ephemeral}
              onChange={setEphemeral}
            />
          </div>

          {/* Error */}
          {errorMessage ? (
            <p className="text-sm text-destructive">{errorMessage}</p>
          ) : null}

          {/* Footer */}
          <div className="flex items-center justify-end gap-2 pt-2">
            <Button
              disabled={isCreating}
              onClick={() => onOpenChange(false)}
              type="button"
              variant="ghost"
            >
              Cancel
            </Button>
            <Button
              data-testid="create-channel-submit"
              disabled={isCreating || name.trim().length === 0}
              type="submit"
            >
              {isCreating ? "Creating..." : `Create ${kindLabel}`}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Checkbox helpers (moved from AppSidebar)
// ---------------------------------------------------------------------------

function PrivateCheckbox({
  disabled,
  isPrivate,
  onChange,
}: {
  disabled: boolean;
  isPrivate: boolean;
  onChange: (isPrivate: boolean) => void;
}) {
  const id = React.useId();

  return (
    <div className="flex items-center gap-2">
      <Checkbox
        checked={isPrivate}
        data-testid="create-channel-visibility"
        disabled={disabled}
        id={id}
        onCheckedChange={(checked) => onChange(checked === true)}
      />
      <label
        className="flex cursor-pointer items-center gap-1.5 text-sm text-muted-foreground select-none peer-disabled:cursor-not-allowed peer-disabled:opacity-50"
        htmlFor={id}
      >
        <Lock className="h-3.5 w-3.5" />
        Private — only visible to invited members
      </label>
    </div>
  );
}

function EphemeralCheckbox({
  disabled,
  isEphemeral,
  onChange,
}: {
  disabled: boolean;
  isEphemeral: boolean;
  onChange: (isEphemeral: boolean) => void;
}) {
  const id = React.useId();

  return (
    <div className="flex items-center gap-2">
      <Checkbox
        checked={isEphemeral}
        disabled={disabled}
        id={id}
        onCheckedChange={(checked) => onChange(checked === true)}
      />
      <label
        className="flex cursor-pointer items-center gap-1.5 text-sm text-muted-foreground select-none peer-disabled:cursor-not-allowed peer-disabled:opacity-50"
        htmlFor={id}
      >
        <Zap className="h-3.5 w-3.5" />
        Ephemeral — auto-archives after 1 day of inactivity
      </label>
    </div>
  );
}
