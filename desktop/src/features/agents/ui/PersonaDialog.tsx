import * as React from "react";

import type {
  AcpProvider,
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
  providers: AcpProvider[];
  providersLoading?: boolean;
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
  providers,
  providersLoading = false,
  onOpenChange,
  onSubmit,
}: PersonaDialogProps) {
  const [displayName, setDisplayName] = React.useState("");
  const [avatarUrl, setAvatarUrl] = React.useState("");
  const [systemPrompt, setSystemPrompt] = React.useState("");
  const [provider, setProvider] = React.useState("");
  const [model, setModel] = React.useState("");

  React.useEffect(() => {
    if (!open || !initialValues) {
      return;
    }

    setDisplayName(initialValues.displayName);
    setAvatarUrl(initialValues.avatarUrl ?? "");
    setSystemPrompt(initialValues.systemPrompt);
    setProvider(initialValues.provider ?? "");
    setModel(initialValues.model ?? "");
  }, [initialValues, open]);

  function handleOpenChange(next: boolean) {
    if (!next) {
      setDisplayName("");
      setAvatarUrl("");
      setSystemPrompt("");
      setProvider("");
      setModel("");
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
      provider: provider.trim() || undefined,
      model: model.trim() || undefined,
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
          <DialogHeader className="shrink-0 border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{description}</DialogDescription>
          </DialogHeader>

          <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-6 py-5">
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

            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="persona-provider">
                Preferred runtime
              </label>
              <select
                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
                disabled={isPending || providersLoading}
                id="persona-provider"
                onChange={(event) => setProvider(event.target.value)}
                value={provider}
              >
                <option value="">
                  {providersLoading
                    ? "Loading runtimes..."
                    : "No preference (use default)"}
                </option>
                {providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </select>
              <p className="text-xs text-muted-foreground">
                Optional. When deploying this persona, the selected runtime will
                be pre-selected. Falls back to the default if unavailable.
              </p>
            </div>

            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="persona-model">
                Preferred model
              </label>
              <Input
                autoCapitalize="none"
                autoCorrect="off"
                disabled={isPending}
                id="persona-model"
                onChange={(event) => setModel(event.target.value)}
                placeholder="e.g. gpt-4o, claude-sonnet-4-20250514"
                spellCheck={false}
                value={model}
              />
              <p className="text-xs text-muted-foreground">
                Optional. Passed to the agent at creation time. Leave blank to
                use the runtime default.
              </p>
            </div>

            {error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {error.message}
              </p>
            ) : null}
          </div>

          <div className="flex shrink-0 justify-end gap-2 border-t border-border/60 px-6 py-4">
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
