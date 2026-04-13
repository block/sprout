import { Bot, Lock, Search, Zap } from "lucide-react";
import * as React from "react";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import type { ChannelVisibility, ManagedAgent } from "@/shared/api/types";
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
    agentPubkeys?: string[];
  }) => Promise<void>;
};

// ---------------------------------------------------------------------------
// Agent picker — collapsible section for selecting agents to invite
// ---------------------------------------------------------------------------

function AgentPicker({
  disabled,
  selectedAgents,
  onToggleAgent,
}: {
  disabled: boolean;
  selectedAgents: Set<string>;
  onToggleAgent: (agent: ManagedAgent) => void;
}) {
  const [searchQuery, setSearchQuery] = React.useState("");
  const managedAgentsQuery = useManagedAgentsQuery();
  const agents = managedAgentsQuery.data ?? [];
  const deferredQuery = React.useDeferredValue(
    searchQuery.trim().toLowerCase(),
  );

  const filteredAgents = React.useMemo(() => {
    if (deferredQuery.length === 0) return agents;
    return agents.filter((agent) =>
      agent.name.toLowerCase().includes(deferredQuery),
    );
  }, [agents, deferredQuery]);

  if (agents.length === 0) {
    return null;
  }

  return (
    <div className="space-y-2">
      <p className="text-sm font-medium text-foreground">
        Invite agents
        {selectedAgents.size > 0 ? (
          <span className="ml-1.5 text-muted-foreground">
            ({selectedAgents.size} selected)
          </span>
        ) : null}
      </p>
      <div className="overflow-hidden rounded-xl border border-border/80 bg-muted/20">
        {agents.length > 4 ? (
          <div className="flex items-center gap-2 px-3 py-2">
            <Search className="h-4 w-4 shrink-0 text-muted-foreground" />
            <Input
              className="h-auto border-0 bg-transparent px-0 py-0 shadow-none focus-visible:ring-0"
              disabled={disabled}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="Search agents..."
              value={searchQuery}
            />
          </div>
        ) : null}
        <div
          className={`max-h-40 overflow-y-auto ${agents.length > 4 ? "border-t border-border/70" : ""} px-1 py-1`}
        >
          {filteredAgents.length === 0 ? (
            <p className="px-2 py-1.5 text-sm text-muted-foreground">
              No matching agents.
            </p>
          ) : (
            filteredAgents.map((agent) => {
              const isSelected = selectedAgents.has(agent.pubkey);
              return (
                <button
                  className="flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left transition-colors hover:bg-accent hover:text-accent-foreground"
                  disabled={disabled}
                  key={agent.pubkey}
                  onClick={() => onToggleAgent(agent)}
                  type="button"
                >
                  <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-muted text-muted-foreground">
                    <Bot className="h-4 w-4" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{agent.name}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {agent.status === "running"
                        ? "Running"
                        : agent.status === "deployed"
                          ? "Deployed"
                          : "Stopped"}
                    </p>
                  </div>
                  <div
                    className={`flex h-5 w-5 shrink-0 items-center justify-center rounded-md border text-xs transition-colors ${
                      isSelected
                        ? "border-primary bg-primary text-primary-foreground"
                        : "border-border"
                    }`}
                  >
                    {isSelected ? "✓" : null}
                  </div>
                </button>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}

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
  const [selectedAgentPubkeys, setSelectedAgentPubkeys] = React.useState<
    Set<string>
  >(new Set());
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
    setSelectedAgentPubkeys(new Set());
    setErrorMessage(null);

    // Small delay to let dialog animation start before focusing
    const timerId = globalThis.setTimeout(() => {
      nameInputRef.current?.focus();
    }, 50);
    return () => globalThis.clearTimeout(timerId);
  }, [open]);

  function handleToggleAgent(agent: ManagedAgent) {
    setSelectedAgentPubkeys((current) => {
      const next = new Set(current);
      if (next.has(agent.pubkey)) {
        next.delete(agent.pubkey);
      } else {
        next.add(agent.pubkey);
      }
      return next;
    });
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
        agentPubkeys:
          selectedAgentPubkeys.size > 0
            ? Array.from(selectedAgentPubkeys)
            : undefined,
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

          {/* Agent picker */}
          <AgentPicker
            disabled={isCreating}
            selectedAgents={selectedAgentPubkeys}
            onToggleAgent={handleToggleAgent}
          />

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
