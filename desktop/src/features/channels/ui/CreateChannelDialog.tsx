import { Lock, Search, UserPlus, X, Zap } from "lucide-react";
import * as React from "react";

import { usePersonasQuery } from "@/features/agents/hooks";
import { AddChannelBotPersonasSection } from "@/features/channels/ui/AddChannelBotPersonasSection";
import { formatPubkey } from "@/features/channels/lib/memberUtils";
import { useUserSearchQuery } from "@/features/profile/hooks";
import type {
  AgentPersona,
  ChannelType,
  ChannelVisibility,
  UserSearchResult,
} from "@/shared/api/types";
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

/** Default TTL for ephemeral channels: 1 day of inactivity. */
const EPHEMERAL_TTL_SECONDS = 86400;

// ---------------------------------------------------------------------------
// Invitee types — discriminated union for people vs personas
// ---------------------------------------------------------------------------

export type PersonInvitee = {
  kind: "person";
  user: UserSearchResult;
};

export type PersonaInvitee = {
  kind: "persona";
  persona: AgentPersona;
  count: number;
};

export type Invitee = PersonInvitee | PersonaInvitee;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatSearchUserName(user: UserSearchResult) {
  return (
    user.displayName?.trim() ||
    user.nip05Handle?.trim() ||
    formatPubkey(user.pubkey)
  );
}

function formatSearchUserSecondary(user: UserSearchResult) {
  const displayName = user.displayName?.trim();
  const nip05Handle = user.nip05Handle?.trim();

  if (displayName && nip05Handle) {
    return nip05Handle;
  }

  return formatPubkey(user.pubkey);
}

function channelTypeLabel(channelType: Exclude<ChannelType, "dm">) {
  return channelType === "stream" ? "channel" : "forum";
}

// ---------------------------------------------------------------------------
// PeopleSearch — search + chip selector for people only
// ---------------------------------------------------------------------------

function PeopleSearch({
  disabled,
  selectedPeople,
  onSelect,
  onRemove,
}: {
  disabled: boolean;
  selectedPeople: UserSearchResult[];
  onSelect: (user: UserSearchResult) => void;
  onRemove: (pubkey: string) => void;
}) {
  const [query, setQuery] = React.useState("");
  const deferredQuery = React.useDeferredValue(query.trim());

  const selectedPubkeys = React.useMemo(
    () => new Set(selectedPeople.map((u) => u.pubkey.toLowerCase())),
    [selectedPeople],
  );

  const userSearchQuery = useUserSearchQuery(deferredQuery, {
    enabled: deferredQuery.length > 0,
    limit: 8,
  });

  const filteredResults = React.useMemo(
    () =>
      (userSearchQuery.data ?? []).filter(
        (user) => !selectedPubkeys.has(user.pubkey.toLowerCase()),
      ),
    [selectedPubkeys, userSearchQuery.data],
  );

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
        <UserPlus className="h-4 w-4" />
        <span>Add people</span>
      </div>
      <div className="rounded-lg border border-border/80 bg-background">
        <div className="flex items-center gap-2 px-2.5 py-2">
          <Search className="h-4 w-4 text-muted-foreground" />
          <Input
            className="h-auto border-0 px-0 py-0 shadow-none focus-visible:ring-0"
            data-testid="create-channel-invite-search"
            disabled={disabled}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search by name or NIP-05"
            value={query}
          />
        </div>

        {selectedPeople.length > 0 ? (
          <div className="flex flex-wrap gap-1.5 border-t border-border/70 px-2.5 py-2">
            {selectedPeople.map((person) => (
              <div
                className="inline-flex items-center gap-1.5 rounded-full border border-border/80 bg-muted/60 px-2.5 py-1 text-[11px] leading-none"
                data-testid={`create-channel-invitee-${person.pubkey}`}
                key={person.pubkey}
              >
                <span className="font-medium">
                  {formatSearchUserName(person)}
                </span>
                <button
                  aria-label={`Remove ${formatSearchUserName(person)}`}
                  className="text-muted-foreground transition-colors hover:text-foreground"
                  onClick={() => onRemove(person.pubkey)}
                  type="button"
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            ))}
          </div>
        ) : null}

        {deferredQuery.length > 0 ? (
          <div className="border-t border-border/70 px-2 py-2">
            {userSearchQuery.isLoading ? (
              <p className="px-2 py-1 text-sm text-muted-foreground">
                Searching…
              </p>
            ) : filteredResults.length > 0 ? (
              <div className="max-h-44 space-y-1 overflow-y-auto">
                {filteredResults.map((result) => (
                  <button
                    className="flex w-full items-center justify-between rounded-md px-2.5 py-1.5 text-left transition-colors hover:bg-accent hover:text-accent-foreground"
                    data-testid={`create-channel-search-result-${result.pubkey}`}
                    key={result.pubkey}
                    onClick={() => {
                      onSelect(result);
                      setQuery("");
                    }}
                    type="button"
                  >
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium leading-5">
                        {formatSearchUserName(result)}
                      </p>
                      <p className="truncate text-xs text-muted-foreground">
                        {formatSearchUserSecondary(result)}
                      </p>
                    </div>
                    <span className="text-xs text-muted-foreground">Add</span>
                  </button>
                ))}
              </div>
            ) : (
              <p className="px-2 py-1 text-sm text-muted-foreground">
                No matching people.
              </p>
            )}
          </div>
        ) : null}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// CreateChannelDialog
// ---------------------------------------------------------------------------

export function CreateChannelDialog({
  channelType,
  isPending,
  onCreate,
  onOpenChange,
  open,
}: {
  channelType: Exclude<ChannelType, "dm">;
  isPending: boolean;
  onCreate: (input: {
    name: string;
    description?: string;
    visibility: ChannelVisibility;
    ttlSeconds?: number;
    invitees: Invitee[];
  }) => Promise<void>;
  onOpenChange: (open: boolean) => void;
  open: boolean;
}) {
  const label = channelTypeLabel(channelType);
  const nameInputRef = React.useRef<HTMLInputElement>(null);

  const [name, setName] = React.useState("");
  const [description, setDescription] = React.useState("");
  const [visibility, setVisibility] = React.useState<ChannelVisibility>("open");
  const [ephemeral, setEphemeral] = React.useState(false);
  const [selectedPeople, setSelectedPeople] = React.useState<
    UserSearchResult[]
  >([]);
  const [personaCounts, setPersonaCounts] = React.useState<Map<string, number>>(
    () => new Map(),
  );
  const [errorMessage, setErrorMessage] = React.useState<string | undefined>();

  const personasQuery = usePersonasQuery();
  const personas = personasQuery.data ?? [];

  const visibilityId = React.useId();
  const ephemeralId = React.useId();

  // Reset state when dialog closes.
  React.useEffect(() => {
    if (!open) {
      setName("");
      setDescription("");
      setVisibility("open");
      setEphemeral(false);
      setSelectedPeople([]);
      setPersonaCounts(new Map());
      setErrorMessage(undefined);
    }
  }, [open]);

  // Auto-focus the name input when the dialog opens.
  React.useEffect(() => {
    if (open) {
      const raf = requestAnimationFrame(() => {
        nameInputRef.current?.focus();
      });
      return () => cancelAnimationFrame(raf);
    }
  }, [open]);

  // Build the invitees list for submission.
  function buildInvitees(): Invitee[] {
    const invitees: Invitee[] = selectedPeople.map((user) => ({
      kind: "person",
      user,
    }));

    for (const [personaId, count] of personaCounts) {
      if (count <= 0) continue;
      const persona = personas.find((p) => p.id === personaId);
      if (persona) {
        invitees.push({ kind: "persona", persona, count });
      }
    }

    return invitees;
  }

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const trimmedName = name.trim();
    if (!trimmedName) {
      return;
    }

    setErrorMessage(undefined);

    try {
      await onCreate({
        name: trimmedName,
        description: description.trim() || undefined,
        visibility,
        ttlSeconds: ephemeral ? EPHEMERAL_TTL_SECONDS : undefined,
        invitees: buildInvitees(),
      });
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : `Failed to create ${label}.`,
      );
    }
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Create a {label}</DialogTitle>
          <DialogDescription>
            {channelType === "stream"
              ? "Streams are for real-time conversation."
              : "Forums are for threaded discussion."}
          </DialogDescription>
        </DialogHeader>

        <form
          className="space-y-4"
          data-testid={`create-${channelType}-form`}
          onSubmit={(event) => {
            void handleSubmit(event);
          }}
        >
          {/* Name */}
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="create-channel-name"
            >
              Name
            </label>
            <Input
              autoCapitalize="none"
              autoComplete="off"
              autoCorrect="off"
              data-testid={`create-${channelType}-name`}
              disabled={isPending}
              id="create-channel-name"
              onChange={(event) => {
                setErrorMessage(undefined);
                setName(event.target.value);
              }}
              placeholder={
                channelType === "stream"
                  ? "release-notes"
                  : "design-discussions"
              }
              ref={nameInputRef}
              spellCheck={false}
              value={name}
            />
          </div>

          {/* Description */}
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="create-channel-description"
            >
              Description{" "}
              <span className="font-normal text-muted-foreground">
                (optional)
              </span>
            </label>
            <Input
              autoComplete="off"
              data-testid={`create-${channelType}-description`}
              disabled={isPending}
              id="create-channel-description"
              onChange={(event) => {
                setErrorMessage(undefined);
                setDescription(event.target.value);
              }}
              placeholder={
                channelType === "stream"
                  ? "What this channel is for"
                  : "What this forum is for"
              }
              value={description}
            />
          </div>

          {/* Visibility + Ephemeral toggles */}
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Checkbox
                checked={visibility === "private"}
                data-testid={`create-${channelType}-visibility`}
                disabled={isPending}
                id={visibilityId}
                onCheckedChange={(checked) =>
                  setVisibility(checked === true ? "private" : "open")
                }
              />
              <label
                className="flex cursor-pointer items-center gap-1.5 text-sm select-none peer-disabled:cursor-not-allowed peer-disabled:opacity-50"
                htmlFor={visibilityId}
              >
                <Lock className="h-3.5 w-3.5" />
                Private {label}
              </label>
            </div>
            <div className="flex items-center gap-2">
              <Checkbox
                checked={ephemeral}
                disabled={isPending}
                id={ephemeralId}
                onCheckedChange={(checked) => setEphemeral(checked === true)}
              />
              <label
                className="flex cursor-pointer items-center gap-1.5 text-sm select-none peer-disabled:cursor-not-allowed peer-disabled:opacity-50"
                htmlFor={ephemeralId}
              >
                <Zap className="h-3.5 w-3.5" />
                Ephemeral — auto-archives after 1 day of inactivity
              </label>
            </div>
          </div>

          {/* Add people */}
          <PeopleSearch
            disabled={isPending}
            onRemove={(pubkey) =>
              setSelectedPeople((current) =>
                current.filter((u) => u.pubkey !== pubkey),
              )
            }
            onSelect={(user) =>
              setSelectedPeople((current) => [...current, user])
            }
            selectedPeople={selectedPeople}
          />

          {/* Add bots — persona pills with stepper */}
          <AddChannelBotPersonasSection
            canToggleSelections={!isPending}
            includeGeneric={false}
            isLoading={personasQuery.isLoading}
            onToggleGeneric={() => {}}
            onSetPersonaCount={(personaId, count) => {
              setPersonaCounts((current) => {
                const next = new Map(current);
                if (count <= 0) {
                  next.delete(personaId);
                } else {
                  next.set(personaId, count);
                }
                return next;
              });
            }}
            personas={personas}
            selectedPersonaCounts={personaCounts}
          />

          {/* Error message */}
          {errorMessage ? (
            <p className="text-sm text-destructive">{errorMessage}</p>
          ) : null}

          {/* Actions */}
          <div className="flex items-center justify-end gap-2 pt-2">
            <Button
              disabled={isPending}
              onClick={() => onOpenChange(false)}
              size="sm"
              type="button"
              variant="ghost"
            >
              Cancel
            </Button>
            <Button
              data-testid={`create-${channelType}-submit`}
              disabled={isPending || name.trim().length === 0}
              size="sm"
              type="submit"
            >
              {isPending ? "Creating…" : `Create ${label}`}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
