import * as React from "react";

import type {
  CreatePersonaInput,
  UpdatePersonaInput,
} from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

type PersonaDialogProps = {
  open: boolean;
  title: string;
  description: string;
  submitLabel: string;
  initialValues: CreatePersonaInput | UpdatePersonaInput | null;
  error: Error | null;
  isPending: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (input: CreatePersonaInput | UpdatePersonaInput) => Promise<void>;
};

export function PersonaDialog({
  open,
  title,
  description,
  submitLabel,
  initialValues,
  error,
  isPending,
  onOpenChange,
  onSubmit,
}: PersonaDialogProps) {
  const [displayName, setDisplayName] = React.useState("");
  const [avatarUrl, setAvatarUrl] = React.useState("");
  const [systemPrompt, setSystemPrompt] = React.useState("");

  React.useEffect(() => {
    if (!open || !initialValues) {
      return;
    }

    setDisplayName(initialValues.displayName);
    setAvatarUrl(initialValues.avatarUrl ?? "");
    setSystemPrompt(initialValues.systemPrompt);
  }, [initialValues, open]);

  function handleOpenChange(next: boolean) {
    if (!next) {
      setDisplayName("");
      setAvatarUrl("");
      setSystemPrompt("");
    }

    onOpenChange(next);
  }

  async function handleSubmit() {
    if (!initialValues) {
      return;
    }

    const baseInput = {
      displayName,
      avatarUrl: avatarUrl.trim() || undefined,
      systemPrompt,
    };

    if ("id" in initialValues) {
      await onSubmit({
        id: initialValues.id,
        ...baseInput,
      });
      return;
    }

    await onSubmit(baseInput);
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-w-2xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{description}</DialogDescription>
          </DialogHeader>

          <div className="space-y-5 px-6 py-5">
            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="persona-display-name"
              >
                Display name
              </label>
              <Input
                autoCorrect="off"
                disabled={isPending}
                id="persona-display-name"
                onChange={(event) => setDisplayName(event.target.value)}
                placeholder="Researcher"
                value={displayName}
              />
            </div>

            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="persona-avatar-url"
              >
                Avatar URL
              </label>
              <Input
                autoCapitalize="none"
                autoCorrect="off"
                disabled={isPending}
                id="persona-avatar-url"
                onChange={(event) => setAvatarUrl(event.target.value)}
                placeholder="https://example.com/avatar.png"
                spellCheck={false}
                value={avatarUrl}
              />
              <p className="text-xs text-muted-foreground">
                Optional. Deployed agents fall back to the runtime avatar if
                this is blank.
              </p>
            </div>

            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="persona-system-prompt"
              >
                System prompt
              </label>
              <Textarea
                className="min-h-40"
                disabled={isPending}
                id="persona-system-prompt"
                onChange={(event) => setSystemPrompt(event.target.value)}
                placeholder="Describe what this persona should do."
                value={systemPrompt}
              />
            </div>

            {error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {error.message}
              </p>
            ) : null}
          </div>

          <div className="flex justify-end gap-2 border-t border-border/60 px-6 py-4">
            <Button
              onClick={() => handleOpenChange(false)}
              size="sm"
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            <Button
              disabled={
                displayName.trim().length === 0 ||
                systemPrompt.trim().length === 0 ||
                isPending
              }
              onClick={() => void handleSubmit()}
              size="sm"
              type="button"
            >
              {isPending ? "Saving..." : submitLabel}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
