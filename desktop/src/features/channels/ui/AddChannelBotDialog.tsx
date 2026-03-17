import { ChevronDown } from "lucide-react";
import * as React from "react";

import {
  useCreateChannelManagedAgentMutation,
  type CreateChannelManagedAgentResult,
} from "@/features/agents/hooks";
import type { AcpProvider } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

type AddChannelBotDialogProps = {
  channelId: string | null;
  open: boolean;
  providers: AcpProvider[];
  providersErrorMessage?: string | null;
  providersLoading?: boolean;
  onAdded?: (result: CreateChannelManagedAgentResult) => void;
  onOpenChange: (open: boolean) => void;
};

function defaultBotName(provider: AcpProvider | null) {
  if (!provider) {
    return "";
  }

  const normalizedId = provider.id.trim().toLowerCase();
  if (normalizedId.length > 0) {
    return normalizedId;
  }

  return provider.label.trim().toLowerCase() || "agent";
}

export function AddChannelBotDialog({
  channelId,
  open,
  providers,
  providersErrorMessage,
  providersLoading = false,
  onAdded,
  onOpenChange,
}: AddChannelBotDialogProps) {
  const createBotMutation = useCreateChannelManagedAgentMutation(channelId);
  const [selectedProviderId, setSelectedProviderId] = React.useState("");
  const [name, setName] = React.useState("");
  const [prompt, setPrompt] = React.useState("");
  const [hasEditedName, setHasEditedName] = React.useState(false);

  const selectedProvider = React.useMemo(
    () =>
      providers.find((provider) => provider.id === selectedProviderId) ??
      providers[0] ??
      null,
    [providers, selectedProviderId],
  );

  React.useEffect(() => {
    if (!open) {
      return;
    }

    if (!selectedProviderId && providers[0]) {
      setSelectedProviderId(providers[0].id);
    }
  }, [open, providers, selectedProviderId]);

  React.useEffect(() => {
    if (!selectedProvider || hasEditedName) {
      return;
    }

    setName(defaultBotName(selectedProvider));
  }, [hasEditedName, selectedProvider]);

  function reset() {
    setSelectedProviderId(providers[0]?.id ?? "");
    setName(providers[0] ? defaultBotName(providers[0]) : "");
    setPrompt("");
    setHasEditedName(false);
    createBotMutation.reset();
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      reset();
    }

    onOpenChange(next);
  }

  async function handleSubmit() {
    if (!selectedProvider) {
      return;
    }

    try {
      const result = await createBotMutation.mutateAsync({
        provider: selectedProvider,
        name,
        systemPrompt: prompt,
        role: "bot",
      });
      onAdded?.(result);
      handleOpenChange(false);
    } catch {
      // The mutation error is rendered inline.
    }
  }

  const canSubmit =
    selectedProvider !== null &&
    name.trim().length > 0 &&
    !providersLoading &&
    !createBotMutation.isPending;
  const canChooseProvider =
    providers.length > 0 && !providersLoading && !createBotMutation.isPending;
  const providerTriggerLabel = providersLoading
    ? "Loading runtimes..."
    : (selectedProvider?.label ?? "No runtimes found");

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Add bot</DialogTitle>
          <DialogDescription>
            Pick a runtime, adjust the default name if needed, and describe what
            this bot should do in the channel.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3.5">
          <div className="grid gap-3 sm:grid-cols-[max-content,minmax(0,1fr)] sm:items-end">
            <div className="space-y-1.5">
              <div className="text-sm font-medium">Agent</div>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    className="h-9 max-w-full justify-start gap-1.5 rounded-full border border-border/50 bg-muted/45 px-3 text-sm font-medium text-foreground shadow-none hover:bg-muted/70"
                    disabled={!canChooseProvider}
                    size="default"
                    type="button"
                    variant="ghost"
                  >
                    <span className="truncate">{providerTriggerLabel}</span>
                    <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="min-w-40">
                  <DropdownMenuRadioGroup
                    onValueChange={(value) => {
                      setSelectedProviderId(value);
                      setHasEditedName(false);
                    }}
                    value={selectedProvider?.id ?? ""}
                  >
                    {providers.map((provider) => (
                      <DropdownMenuRadioItem
                        key={provider.id}
                        value={provider.id}
                      >
                        {provider.label}
                      </DropdownMenuRadioItem>
                    ))}
                  </DropdownMenuRadioGroup>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>

            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="channel-bot-name">
                Name
              </label>
              <Input
                autoCapitalize="none"
                autoCorrect="off"
                disabled={createBotMutation.isPending}
                id="channel-bot-name"
                onChange={(event) => {
                  setHasEditedName(true);
                  setName(event.target.value);
                }}
                spellCheck={false}
                value={name}
              />
            </div>
          </div>

          <p className="text-xs text-muted-foreground">
            The name defaults to the runtime, but you can edit it before adding
            the bot.
          </p>

          <div className="space-y-1.5">
            <label className="text-sm font-medium" htmlFor="channel-bot-prompt">
              Prompt
            </label>
            <Textarea
              className="min-h-24"
              disabled={createBotMutation.isPending}
              id="channel-bot-prompt"
              onChange={(event) => setPrompt(event.target.value)}
              placeholder="What should this bot help with in the channel?"
              value={prompt}
            />
            <p className="text-xs text-muted-foreground">
              Saved as the bot&apos;s system prompt override.
            </p>
          </div>

          {providersErrorMessage ? (
            <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
              {providersErrorMessage}
            </p>
          ) : null}

          {createBotMutation.error instanceof Error ? (
            <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
              {createBotMutation.error.message}
            </p>
          ) : null}
        </div>

        <div className="flex justify-end gap-2">
          <Button
            onClick={() => handleOpenChange(false)}
            size="sm"
            type="button"
            variant="outline"
          >
            Cancel
          </Button>
          <Button
            disabled={!canSubmit}
            onClick={() => void handleSubmit()}
            size="sm"
            type="button"
          >
            {createBotMutation.isPending ? "Adding..." : "Add"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
