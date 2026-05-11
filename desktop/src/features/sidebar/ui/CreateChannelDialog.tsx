import { Lock, Zap } from "lucide-react";
import * as React from "react";

import { useChannelTemplatesQuery } from "@/features/channel-templates/hooks";
import type { ChannelTemplate, ChannelVisibility } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { ChooserDialogContent } from "@/shared/ui/chooser-dialog-content";
import { Dialog } from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Switch } from "@/shared/ui/switch";
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
    templateId?: string;
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
  const [selectedTemplateId, setSelectedTemplateId] = React.useState<
    string | null
  >(null);
  const nameInputRef = React.useRef<HTMLInputElement>(null);

  const templatesQuery = useChannelTemplatesQuery();
  const templates = templatesQuery.data ?? [];

  const kindLabel = channelKind === "forum" ? "forum" : "channel";

  // Reset form state when dialog opens/closes or kind changes
  React.useEffect(() => {
    if (!open) return;

    setName("");
    setDescription("");
    setVisibility("open");
    setEphemeral(false);
    setErrorMessage(null);
    setSelectedTemplateId(null);

    // Small delay to let dialog animation start before focusing
    const timerId = globalThis.setTimeout(() => {
      nameInputRef.current?.focus();
    }, 50);
    return () => globalThis.clearTimeout(timerId);
  }, [open]);

  function handleTemplateChange(templateId: string) {
    if (!templateId) {
      setSelectedTemplateId(null);
      setDescription("");
      setVisibility("open");
      setErrorMessage(null);
      return;
    }

    const template = templates.find(
      (t: ChannelTemplate) => t.id === templateId,
    );
    if (!template) return;

    setSelectedTemplateId(templateId);

    // Pre-fill fields from template (always overwrite to avoid stale values)
    setDescription(template.description ?? "");
    setVisibility(template.visibility);

    // If the template's channel type differs from current dialog kind,
    // we still apply the visibility but don't change the kind
    // (kind is determined by how the dialog was opened)
    setErrorMessage(null);
  }

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
        templateId: selectedTemplateId ?? undefined,
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
      <ChooserDialogContent
        className="max-w-lg"
        data-testid="create-channel-dialog"
        title={`Create a new ${kindLabel}`}
        description={
          channelKind === "forum"
            ? "Forums organize threaded discussions around a topic."
            : "Channels are real-time streams for team conversation."
        }
        footer={
          <div className="flex w-full items-center justify-end gap-2">
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
              form="create-channel-form"
              type="submit"
            >
              {isCreating ? "Creating..." : `Create ${kindLabel}`}
            </Button>
          </div>
        }
      >
        <form
          className="space-y-4"
          id="create-channel-form"
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
            <div className="flex items-center justify-between gap-3">
              <label
                className="flex items-center gap-1.5 text-sm text-muted-foreground"
                htmlFor="create-channel-private"
              >
                <Lock className="h-3.5 w-3.5" />
                Private — only visible to invited members
              </label>
              <Switch
                checked={visibility === "private"}
                data-testid="create-channel-visibility"
                disabled={isCreating}
                id="create-channel-private"
                onCheckedChange={(checked) =>
                  setVisibility(checked ? "private" : "open")
                }
              />
            </div>
            <div className="flex items-center justify-between gap-3">
              <label
                className="flex items-center gap-1.5 text-sm text-muted-foreground"
                htmlFor="create-channel-ephemeral"
              >
                <Zap className="h-3.5 w-3.5" />
                Ephemeral — auto-archives after 1 day of inactivity
              </label>
              <Switch
                checked={ephemeral}
                disabled={isCreating}
                id="create-channel-ephemeral"
                onCheckedChange={setEphemeral}
              />
            </div>
          </div>

          {/* Template Selector */}
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium text-foreground"
              htmlFor="create-channel-template"
            >
              Template{" "}
              <span className="font-normal text-muted-foreground">
                (optional)
              </span>
            </label>
            <select
              className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              data-testid="create-channel-template"
              disabled={isCreating || templates.length === 0}
              id="create-channel-template"
              onChange={(event) => handleTemplateChange(event.target.value)}
              value={selectedTemplateId ?? ""}
            >
              <option value="">
                {templatesQuery.isLoading
                  ? "Loading..."
                  : templates.length === 0
                    ? "No templates created yet"
                    : "No template"}
              </option>
              {templates.map((template: ChannelTemplate) => (
                <option key={template.id} value={template.id}>
                  {template.name}
                </option>
              ))}
            </select>
          </div>

          {/* Error */}
          {errorMessage ? (
            <p className="text-sm text-destructive">{errorMessage}</p>
          ) : null}
        </form>
      </ChooserDialogContent>
    </Dialog>
  );
}
