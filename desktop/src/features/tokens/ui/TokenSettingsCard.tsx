import { useQuery } from "@tanstack/react-query";
import {
  Check,
  Copy,
  KeyRound,
  Plus,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import * as React from "react";

import { useChannelsQuery } from "@/features/channels/hooks";
import {
  useMintTokenMutation,
  useRevokeAllTokensMutation,
  useRevokeTokenMutation,
  useTokensQuery,
} from "@/features/tokens/hooks";
import { TOKEN_SCOPE_OPTIONS } from "@/features/tokens/lib/scopeOptions";
import { getChannelMembers } from "@/shared/api/tauri";
import type { Channel, Token, TokenScope } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { formatRelativeTime } from "@/shared/lib/time";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";

const EXPIRY_OPTIONS = [
  { value: 7, label: "7 days" },
  { value: 30, label: "30 days" },
  { value: 90, label: "90 days" },
  { value: 365, label: "1 year" },
  { value: 0, label: "No expiry" },
] as const;

const MAX_ACTIVE_TOKENS = 10;

function tokenStatus(token: Token): "active" | "revoked" | "expired" {
  if (token.revokedAt) return "revoked";
  if (token.expiresAt && new Date(token.expiresAt) < new Date())
    return "expired";
  return "active";
}

function StatusBadge({ status }: { status: "active" | "revoked" | "expired" }) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
        status === "active" &&
          "bg-green-500/10 text-green-700 dark:text-green-400",
        status === "revoked" && "bg-muted text-muted-foreground",
        status === "expired" &&
          "bg-yellow-500/10 text-yellow-700 dark:text-yellow-400",
      )}
    >
      {status}
    </span>
  );
}

function ScopeBadge({ scope }: { scope: string }) {
  return (
    <span className="inline-flex items-center rounded-md bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
      {scope}
    </span>
  );
}

function formatRelativeDate(dateString: string): string {
  return (
    formatRelativeTime(dateString) ?? new Date(dateString).toLocaleDateString()
  );
}

function formatDate(dateString: string): string {
  return new Date(dateString).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function channelLabel(channelId: string, channelsById: Map<string, Channel>) {
  return (
    channelsById.get(channelId)?.name ?? `Channel ${channelId.slice(0, 8)}`
  );
}

function TokenRow({
  channelsById,
  token,
  onRevoke,
  isRevoking,
}: {
  channelsById: Map<string, Channel>;
  token: Token;
  onRevoke: (id: string) => void;
  isRevoking: boolean;
}) {
  const status = tokenStatus(token);
  const visibleChannelIds = token.channelIds.slice(0, 4);
  const hiddenChannelCount = token.channelIds.length - visibleChannelIds.length;

  return (
    <div
      className="flex items-start justify-between gap-3 rounded-lg border border-border/60 bg-background/60 px-3 py-2.5"
      data-testid={`token-row-${token.id}`}
    >
      <div className="min-w-0 space-y-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">{token.name}</span>
          <StatusBadge status={status} />
        </div>
        <div className="flex flex-wrap gap-1">
          {token.scopes.map((scope) => (
            <ScopeBadge key={scope} scope={scope} />
          ))}
        </div>
        <p className="text-xs text-muted-foreground">
          Created {formatRelativeDate(token.createdAt)}
          {token.lastUsedAt
            ? ` · Last used ${formatRelativeDate(token.lastUsedAt)}`
            : " · Never used"}
          {token.expiresAt ? ` · Expires ${formatDate(token.expiresAt)}` : ""}
        </p>
        <p className="text-xs text-muted-foreground">
          {token.channelIds.length === 0
            ? "All accessible channels"
            : `Scoped to ${token.channelIds.length} channel${token.channelIds.length === 1 ? "" : "s"}`}
        </p>
        {visibleChannelIds.length > 0 ? (
          <div className="flex flex-wrap gap-1">
            {visibleChannelIds.map((channelId) => (
              <ScopeBadge
                key={channelId}
                scope={channelLabel(channelId, channelsById)}
              />
            ))}
            {hiddenChannelCount > 0 ? (
              <ScopeBadge scope={`+${hiddenChannelCount} more`} />
            ) : null}
          </div>
        ) : null}
      </div>
      {status === "active" ? (
        <Button
          data-testid={`revoke-token-${token.id}`}
          disabled={isRevoking}
          onClick={() => onRevoke(token.id)}
          size="sm"
          variant="ghost"
        >
          <Trash2 className="h-3.5 w-3.5" />
          <span className="sr-only">Revoke</span>
        </Button>
      ) : null}
    </div>
  );
}

function CreateTokenDialog({
  activeTokenCount,
  currentPubkey,
  channels,
  hiddenChannelsCount,
  channelsError,
  isLoadingChannels,
  open,
  onOpenChange,
}: {
  activeTokenCount: number;
  currentPubkey?: string;
  channels: Channel[];
  hiddenChannelsCount: number;
  channelsError: Error | null;
  isLoadingChannels: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const mintMutation = useMintTokenMutation();
  const [name, setName] = React.useState("");
  const [selectedScopes, setSelectedScopes] = React.useState<Set<TokenScope>>(
    new Set(),
  );
  const [channelAccessMode, setChannelAccessMode] = React.useState<
    "all" | "selected"
  >("all");
  const [selectedChannelIds, setSelectedChannelIds] = React.useState<
    Set<string>
  >(new Set());
  const [expiryDays, setExpiryDays] = React.useState<number>(30);
  const [mintedToken, setMintedToken] = React.useState<string | null>(null);
  const [copied, setCopied] = React.useState(false);

  const canCreate =
    activeTokenCount < MAX_ACTIVE_TOKENS &&
    name.trim().length > 0 &&
    name.trim().length <= 100 &&
    selectedScopes.size > 0 &&
    (channelAccessMode === "all" || selectedChannelIds.size > 0) &&
    !mintMutation.isPending;

  function reset() {
    setName("");
    setSelectedScopes(new Set());
    setChannelAccessMode("all");
    setSelectedChannelIds(new Set());
    setExpiryDays(30);
    setMintedToken(null);
    setCopied(false);
    mintMutation.reset();
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      reset();
    }
    onOpenChange(next);
  }

  function toggleScope(scope: TokenScope) {
    setSelectedScopes((prev) => {
      const next = new Set(prev);
      if (next.has(scope)) {
        next.delete(scope);
      } else {
        next.add(scope);
      }
      return next;
    });
  }

  function toggleChannel(channelId: string) {
    setSelectedChannelIds((prev) => {
      const next = new Set(prev);
      if (next.has(channelId)) {
        next.delete(channelId);
      } else {
        next.add(channelId);
      }
      return next;
    });
  }

  async function handleCreate() {
    const result = await mintMutation.mutateAsync({
      name: name.trim(),
      scopes: [...selectedScopes],
      channelIds:
        channelAccessMode === "selected" ? [...selectedChannelIds] : undefined,
      expiresInDays: expiryDays === 0 ? undefined : expiryDays,
    });
    setMintedToken(result.token);
  }

  async function handleCopy() {
    if (!mintedToken) return;
    await navigator.clipboard.writeText(mintedToken);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  if (mintedToken) {
    return (
      <Dialog onOpenChange={handleOpenChange} open={open}>
        <DialogContent
          className="max-w-lg overflow-hidden p-0"
          data-testid="token-created-dialog"
        >
          <div className="flex max-h-[85vh] flex-col">
            <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
              <DialogTitle>Token created</DialogTitle>
              <DialogDescription>
                Copy this token now. You will not be able to see it again.
              </DialogDescription>
            </DialogHeader>

            <div className="flex-1 overflow-y-auto px-6 py-4">
              <div className="space-y-3">
                <div className="flex items-center gap-2">
                  <code className="min-w-0 flex-1 break-all rounded-lg border border-border bg-muted/50 px-3 py-2 text-sm">
                    {mintedToken}
                  </code>
                  <Button onClick={handleCopy} size="sm" variant="outline">
                    {copied ? (
                      <Check className="h-3.5 w-3.5" />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                  </Button>
                </div>
                <div className="flex items-start gap-2 rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-3 py-2 text-sm text-yellow-700 dark:text-yellow-400">
                  <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
                  <span>
                    This is the only time this token will be shown. Store it
                    securely.
                  </span>
                </div>
              </div>
            </div>

            <div className="flex justify-end border-t border-border/60 bg-background/95 px-6 py-4">
              <Button
                data-testid="token-created-done"
                onClick={() => handleOpenChange(false)}
                size="sm"
                variant="outline"
              >
                Done
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent
        className="max-w-lg overflow-hidden p-0"
        data-testid="create-token-dialog"
      >
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Create API token</DialogTitle>
            <DialogDescription>
              Tokens allow agents and scripts to authenticate with the relay on
              your behalf.
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 overflow-y-auto px-6 py-4">
            <div className="space-y-4">
              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="token-name">
                  Name
                </label>
                <Input
                  autoCapitalize="none"
                  autoCorrect="off"
                  data-testid="token-name-input"
                  id="token-name"
                  maxLength={100}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="e.g. my-agent-bot"
                  spellCheck={false}
                  value={name}
                />
              </div>

              <div className="space-y-1.5">
                <p className="text-sm font-medium">Scopes</p>
                <div className="grid grid-cols-2 gap-2">
                  {TOKEN_SCOPE_OPTIONS.map(({ value, label }) => {
                    const isSelected = selectedScopes.has(value);
                    return (
                      <button
                        className={cn(
                          "rounded-lg border px-3 py-2 text-left text-sm transition-colors",
                          isSelected
                            ? "border-primary bg-primary/10 text-foreground"
                            : "border-border/60 text-muted-foreground hover:bg-accent",
                        )}
                        data-testid={`token-scope-${value.replace(/:/g, "-")}`}
                        key={value}
                        onClick={() => toggleScope(value)}
                        type="button"
                      >
                        {label}
                      </button>
                    );
                  })}
                </div>
              </div>

              <div className="space-y-1.5">
                <div className="flex items-center justify-between gap-2">
                  <p className="text-sm font-medium">Channel access</p>
                  <span className="text-xs text-muted-foreground">
                    {channelAccessMode === "all"
                      ? "All accessible channels"
                      : `${selectedChannelIds.size} selected`}
                  </span>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  {[
                    {
                      value: "all" as const,
                      label: "All channels",
                      description:
                        "Unrestricted across the channels you can access.",
                    },
                    {
                      value: "selected" as const,
                      label: "Selected channels",
                      description: "Limit this token to specific channels.",
                    },
                  ].map((option) => {
                    const isSelected = channelAccessMode === option.value;
                    return (
                      <button
                        className={cn(
                          "rounded-lg border px-3 py-2 text-left text-sm transition-colors",
                          isSelected
                            ? "border-primary bg-primary/10 text-foreground"
                            : "border-border/60 text-muted-foreground hover:bg-accent",
                        )}
                        data-testid={`token-channel-access-${option.value}`}
                        key={option.value}
                        onClick={() => setChannelAccessMode(option.value)}
                        type="button"
                      >
                        <p className="font-medium">{option.label}</p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {option.description}
                        </p>
                      </button>
                    );
                  })}
                </div>

                {channelAccessMode === "selected" ? (
                  isLoadingChannels ? (
                    <p className="rounded-xl border border-border/60 bg-muted/20 px-3 py-2 text-sm text-muted-foreground">
                      Loading channels...
                    </p>
                  ) : channelsError ? (
                    <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                      {channelsError.message}
                    </p>
                  ) : channels.length > 0 ? (
                    <div className="max-h-52 space-y-2 overflow-y-auto rounded-xl border border-border/60 bg-muted/20 p-2">
                      {channels.map((channel) => {
                        const isSelected = selectedChannelIds.has(channel.id);
                        return (
                          <button
                            className={cn(
                              "flex w-full items-start justify-between gap-3 rounded-lg border px-3 py-2 text-left transition-colors",
                              isSelected
                                ? "border-primary bg-primary/10"
                                : "border-border/60 bg-background/70 hover:bg-accent",
                            )}
                            data-testid={`token-channel-${channel.id}`}
                            key={channel.id}
                            onClick={() => toggleChannel(channel.id)}
                            type="button"
                          >
                            <div className="min-w-0">
                              <p className="truncate text-sm font-medium text-foreground">
                                {channel.name}
                              </p>
                              <p className="text-xs text-muted-foreground">
                                {channel.visibility} {channel.channelType}
                              </p>
                            </div>
                            <span
                              className={cn(
                                "rounded-full px-2 py-0.5 text-xs font-medium",
                                isSelected
                                  ? "bg-primary text-primary-foreground"
                                  : "bg-muted text-muted-foreground",
                              )}
                            >
                              {isSelected ? "Selected" : "Select"}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  ) : (
                    <p className="rounded-xl border border-border/60 bg-muted/20 px-3 py-2 text-sm text-muted-foreground">
                      No accessible channels available for scoping yet.
                    </p>
                  )
                ) : null}

                <p className="text-xs text-muted-foreground">
                  Use channel-scoped tokens for guests and single-purpose
                  agents.
                </p>
                {currentPubkey ? (
                  <p className="text-xs text-muted-foreground">
                    Only channels where you are a member can be added to a
                    scoped token.
                    {hiddenChannelsCount > 0
                      ? ` ${hiddenChannelsCount} accessible channel${hiddenChannelsCount === 1 ? "" : "s"} hidden because you are not a member.`
                      : ""}
                  </p>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    Your identity is still loading, so channel membership cannot
                    be checked yet.
                  </p>
                )}
              </div>

              <div className="space-y-1.5">
                <p className="text-sm font-medium">Expiry</p>
                <div className="flex flex-wrap gap-2">
                  {EXPIRY_OPTIONS.map(({ value, label }) => (
                    <button
                      className={cn(
                        "rounded-lg border px-3 py-1.5 text-sm transition-colors",
                        expiryDays === value
                          ? "border-primary bg-primary/10 text-foreground"
                          : "border-border/60 text-muted-foreground hover:bg-accent",
                      )}
                      data-testid={`token-expiry-${value}`}
                      key={value}
                      onClick={() => setExpiryDays(value)}
                      type="button"
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              {activeTokenCount >= MAX_ACTIVE_TOKENS ? (
                <p className="rounded-xl border border-yellow-500/30 bg-yellow-500/10 px-3 py-2 text-sm text-yellow-700 dark:text-yellow-400">
                  You already have {MAX_ACTIVE_TOKENS} active tokens. Revoke one
                  before creating another.
                </p>
              ) : null}

              {mintMutation.error instanceof Error ? (
                <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                  {mintMutation.error.message}
                </p>
              ) : null}
            </div>
          </div>

          <div className="flex justify-end gap-2 border-t border-border/60 bg-background/95 px-6 py-4">
            <Button
              data-testid="cancel-create-token"
              onClick={() => handleOpenChange(false)}
              size="sm"
              variant="outline"
            >
              Cancel
            </Button>
            <Button
              data-testid="confirm-create-token"
              disabled={!canCreate}
              onClick={() => void handleCreate()}
              size="sm"
            >
              {mintMutation.isPending ? "Creating..." : "Create token"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function RevokeAllDialog({
  open,
  onOpenChange,
  onConfirm,
  isPending,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  isPending: boolean;
}) {
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Revoke all tokens?</DialogTitle>
          <DialogDescription>
            This will immediately revoke every active token. Agents using these
            tokens will lose access.
          </DialogDescription>
        </DialogHeader>
        <div className="flex justify-end gap-2">
          <Button
            onClick={() => onOpenChange(false)}
            size="sm"
            variant="outline"
          >
            Cancel
          </Button>
          <Button
            disabled={isPending}
            onClick={onConfirm}
            size="sm"
            variant="destructive"
          >
            {isPending ? "Revoking..." : "Revoke all"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function TokenSettingsCard({
  currentPubkey,
}: {
  currentPubkey?: string;
}) {
  const channelsQuery = useChannelsQuery();
  const tokensQuery = useTokensQuery();
  const revokeTokenMutation = useRevokeTokenMutation();
  const revokeAllMutation = useRevokeAllTokensMutation();

  const [createOpen, setCreateOpen] = React.useState(false);
  const [revokeAllOpen, setRevokeAllOpen] = React.useState(false);

  const allChannels = channelsQuery.data ?? [];
  const channels = allChannels.filter((channel) => channel.archivedAt === null);
  const scopeableChannelsQuery = useQuery({
    enabled:
      createOpen &&
      typeof currentPubkey === "string" &&
      currentPubkey.length > 0 &&
      channels.length > 0,
    queryKey: [
      "token-scopeable-channels",
      currentPubkey?.toLowerCase() ?? "",
      ...channels.map((channel) => channel.id),
    ],
    queryFn: async () => {
      if (!currentPubkey) {
        return [] as Channel[];
      }

      const memberships = await Promise.all(
        channels.map(async (channel) => {
          const members = await getChannelMembers(channel.id);
          return {
            channel,
            isMember: members.some(
              (member) =>
                member.pubkey.toLowerCase() === currentPubkey.toLowerCase(),
            ),
          };
        }),
      );

      return memberships
        .filter((entry) => entry.isMember)
        .map((entry) => entry.channel);
    },
    staleTime: 30_000,
  });
  const scopeableChannels = scopeableChannelsQuery.data ?? [];
  const hiddenChannelsCount = scopeableChannelsQuery.isSuccess
    ? Math.max(channels.length - scopeableChannels.length, 0)
    : 0;
  const channelsById = new Map(
    allChannels.map((channel) => [channel.id, channel]),
  );
  const tokens = tokensQuery.data ?? [];
  const activeTokens = tokens.filter((t) => tokenStatus(t) === "active");
  const hasReachedTokenLimit = activeTokens.length >= MAX_ACTIVE_TOKENS;

  return (
    <section
      className="rounded-xl border border-border/80 bg-card/80 p-4 shadow-sm"
      data-testid="settings-tokens"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <KeyRound className="h-4 w-4 text-muted-foreground" />
            <h2 className="text-sm font-semibold tracking-tight">API Tokens</h2>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            Create tokens for agents, guests, and integrations to access the
            relay. {activeTokens.length}/{MAX_ACTIVE_TOKENS} active.
          </p>
        </div>

        <div className="flex shrink-0 gap-2">
          {activeTokens.length > 0 ? (
            <Button
              onClick={() => setRevokeAllOpen(true)}
              size="sm"
              variant="outline"
            >
              Revoke all
            </Button>
          ) : null}
          <Button
            disabled={hasReachedTokenLimit}
            onClick={() => setCreateOpen(true)}
            size="sm"
          >
            <Plus className="h-3.5 w-3.5" />
            Create token
          </Button>
        </div>
      </div>

      {hasReachedTokenLimit ? (
        <p className="mt-3 rounded-xl border border-yellow-500/30 bg-yellow-500/10 px-3 py-2 text-sm text-yellow-700 dark:text-yellow-400">
          You've reached the active token limit. Revoke an existing token to
          mint another.
        </p>
      ) : null}

      {tokensQuery.error instanceof Error ? (
        <p className="mt-3 rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {tokensQuery.error.message}
        </p>
      ) : null}

      {tokens.length > 0 ? (
        <div className="mt-4 space-y-2">
          {tokens.map((token) => (
            <TokenRow
              channelsById={channelsById}
              isRevoking={revokeTokenMutation.isPending}
              key={token.id}
              onRevoke={(id) => revokeTokenMutation.mutate(id)}
              token={token}
            />
          ))}
        </div>
      ) : tokensQuery.isSuccess ? (
        <p className="mt-4 text-sm text-muted-foreground">
          No tokens yet. Create one to get started.
        </p>
      ) : null}

      <CreateTokenDialog
        activeTokenCount={activeTokens.length}
        channels={scopeableChannels}
        channelsError={
          scopeableChannelsQuery.error instanceof Error
            ? scopeableChannelsQuery.error
            : channelsQuery.error instanceof Error
              ? channelsQuery.error
              : null
        }
        currentPubkey={currentPubkey}
        hiddenChannelsCount={hiddenChannelsCount}
        isLoadingChannels={
          channelsQuery.isLoading || scopeableChannelsQuery.isLoading
        }
        onOpenChange={setCreateOpen}
        open={createOpen}
      />
      <RevokeAllDialog
        isPending={revokeAllMutation.isPending}
        onConfirm={() => {
          revokeAllMutation.mutate(undefined, {
            onSuccess: () => setRevokeAllOpen(false),
          });
        }}
        onOpenChange={setRevokeAllOpen}
        open={revokeAllOpen}
      />
    </section>
  );
}
