import {
  Check,
  CircleAlert,
  Copy,
  KeyRound,
  Play,
  Plus,
  RefreshCcw,
  Square,
  TerminalSquare,
  Trash2,
  UserPlus,
} from "lucide-react";
import * as React from "react";

import {
  useAcpProvidersQuery,
  useCreateManagedAgentMutation,
  useDeleteManagedAgentMutation,
  useManagedAgentLogQuery,
  useManagedAgentPrereqsQuery,
  useManagedAgentsQuery,
  useMintManagedAgentTokenMutation,
  useRelayAgentsQuery,
  useStartManagedAgentMutation,
  useStopManagedAgentMutation,
} from "@/features/agents/hooks";
import {
  useChannelsQuery,
  useAddChannelMembersMutation,
} from "@/features/channels/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import type {
  Channel,
  ChannelRole,
  CreateManagedAgentInput,
  CreateManagedAgentResponse,
  ManagedAgent,
  RelayAgent,
  TokenScope,
} from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Skeleton } from "@/shared/ui/skeleton";

const AGENT_SCOPE_OPTIONS: Array<{ value: TokenScope; label: string }> = [
  { value: "messages:read", label: "Messages read" },
  { value: "messages:write", label: "Messages write" },
  { value: "channels:read", label: "Channels read" },
  { value: "channels:write", label: "Channels write" },
  { value: "users:read", label: "Users read" },
  { value: "files:read", label: "Files read" },
  { value: "files:write", label: "Files write" },
];

function formatTimestamp(value: string | null) {
  if (!value) {
    return "Never";
  }

  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(value));
}

function truncatePubkey(pubkey: string) {
  return `${pubkey.slice(0, 8)}…${pubkey.slice(-6)}`;
}

function commandLooksLikePath(command: string) {
  const trimmed = command.trim();
  return (
    trimmed.startsWith(".") ||
    trimmed.startsWith("~") ||
    trimmed.includes("/") ||
    trimmed.includes("\\")
  );
}

function describeResolvedCommand(command: string, resolvedPath: string) {
  const normalized = resolvedPath.replace(/\\/g, "/");

  if (normalized.includes("/target/release/")) {
    return "workspace release build";
  }
  if (normalized.includes("/target/debug/")) {
    return "workspace debug build";
  }

  if (commandLooksLikePath(command)) {
    return "custom command";
  }

  return "installed on PATH";
}

function describeLogFile(path: string) {
  const normalized = path.replace(/\\/g, "/");
  const basename = normalized.split("/").pop() ?? path;

  if (!basename.endsWith(".log")) {
    return "local harness log";
  }

  const stem = basename.slice(0, -4);
  if (stem.length <= 18) {
    return basename;
  }

  return `${stem.slice(0, 8)}…${stem.slice(-6)}.log`;
}

function CopyButton({ value, label }: { value: string; label?: string }) {
  const [copied, setCopied] = React.useState(false);

  return (
    <Button
      onClick={async () => {
        await navigator.clipboard.writeText(value);
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1_500);
      }}
      size="sm"
      type="button"
      variant="outline"
    >
      {copied ? (
        <Check className="h-3.5 w-3.5" />
      ) : (
        <Copy className="h-3.5 w-3.5" />
      )}
      <span>{copied ? "Copied" : (label ?? "Copy")}</span>
    </Button>
  );
}

function SecretRevealDialog({
  created,
  onOpenChange,
}: {
  created: CreateManagedAgentResponse | null;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog onOpenChange={onOpenChange} open={created !== null}>
      <DialogContent className="max-w-2xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Agent created</DialogTitle>
            <DialogDescription>
              Save the private key and token now. The app can keep running the
              harness locally, but these secrets are only revealed here.
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 space-y-4 overflow-y-auto px-6 py-5">
            {created ? (
              <>
                <div className="rounded-2xl border border-border/70 bg-muted/20 p-4">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <p className="text-sm font-semibold tracking-tight">
                        Private key (nsec)
                      </p>
                      <p className="text-sm text-muted-foreground">
                        This is the agent identity used by `sprout-acp`.
                      </p>
                    </div>
                    <CopyButton
                      label="Copy key"
                      value={created.privateKeyNsec}
                    />
                  </div>
                  <code className="mt-3 block break-all rounded-xl border border-border/70 bg-background/80 px-3 py-2 text-xs">
                    {created.privateKeyNsec}
                  </code>
                </div>

                {created.apiToken ? (
                  <div className="rounded-2xl border border-border/70 bg-muted/20 p-4">
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <p className="text-sm font-semibold tracking-tight">
                          API token
                        </p>
                        <p className="text-sm text-muted-foreground">
                          Optional for local dev, required when the relay
                          enforces bearer auth.
                        </p>
                      </div>
                      <CopyButton label="Copy token" value={created.apiToken} />
                    </div>
                    <code className="mt-3 block break-all rounded-xl border border-border/70 bg-background/80 px-3 py-2 text-xs">
                      {created.apiToken}
                    </code>
                  </div>
                ) : null}

                {created.spawnError ? (
                  <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                    {created.spawnError}
                  </p>
                ) : (
                  <p className="rounded-2xl border border-primary/20 bg-primary/10 px-4 py-3 text-sm text-primary">
                    {created.agent.name} is ready
                    {created.agent.status === "running" ? " and running." : "."}
                  </p>
                )}
              </>
            ) : null}
          </div>

          <div className="flex justify-end border-t border-border/60 px-6 py-4">
            <Button
              onClick={() => onOpenChange(false)}
              size="sm"
              type="button"
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

function TokenRevealDialog({
  name,
  token,
  onOpenChange,
}: {
  name: string | null;
  token: string | null;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog onOpenChange={onOpenChange} open={token !== null}>
      <DialogContent className="max-w-xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Agent token minted</DialogTitle>
            <DialogDescription>
              Save this token now. Restart the harness if you want the running
              agent to pick it up immediately.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 px-6 py-5">
            <div className="rounded-2xl border border-border/70 bg-muted/20 p-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold tracking-tight">{name}</p>
                  <p className="text-sm text-muted-foreground">
                    Token shown once only.
                  </p>
                </div>
                {token ? <CopyButton label="Copy token" value={token} /> : null}
              </div>
              {token ? (
                <code className="mt-3 block break-all rounded-xl border border-border/70 bg-background/80 px-3 py-2 text-xs">
                  {token}
                </code>
              ) : null}
            </div>
          </div>

          <div className="flex justify-end border-t border-border/60 px-6 py-4">
            <Button
              onClick={() => onOpenChange(false)}
              size="sm"
              type="button"
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

function CreateAgentDialog({
  open,
  onCreated,
  onOpenChange,
}: {
  open: boolean;
  onCreated: (result: CreateManagedAgentResponse) => void;
  onOpenChange: (open: boolean) => void;
}) {
  const createMutation = useCreateManagedAgentMutation();
  const providersQuery = useAcpProvidersQuery();
  const [acpCommand, setAcpCommand] = React.useState("sprout-acp");
  const [agentCommand, setAgentCommand] = React.useState("goose");
  const [agentArgs, setAgentArgs] = React.useState("acp");
  const [mcpCommand, setMcpCommand] = React.useState("sprout-mcp-server");
  const prereqsQuery = useManagedAgentPrereqsQuery(acpCommand, mcpCommand);
  const [name, setName] = React.useState("");
  const [relayUrl, setRelayUrl] = React.useState("");
  const [mintToken, setMintToken] = React.useState(true);
  const [spawnAfterCreate, setSpawnAfterCreate] = React.useState(true);
  const [tokenName, setTokenName] = React.useState("");
  const [selectedScopes, setSelectedScopes] = React.useState<Set<TokenScope>>(
    () =>
      new Set<TokenScope>(["messages:read", "messages:write", "channels:read"]),
  );
  const [turnTimeoutSeconds, setTurnTimeoutSeconds] = React.useState("300");
  const [selectedProviderId, setSelectedProviderId] =
    React.useState<string>("custom");
  const [hasSyncedProviderSelection, setHasSyncedProviderSelection] =
    React.useState(false);
  const providers = providersQuery.data ?? [];
  const prereqs = prereqsQuery.data ?? null;
  const selectedProvider = React.useMemo(
    () =>
      providers.find((provider) => provider.id === selectedProviderId) ?? null,
    [providers, selectedProviderId],
  );
  const isMintSupported = prereqs?.admin.available ?? false;
  const isSpawnSupported =
    prereqs?.acp.available === true && prereqs?.mcp.available === true;
  const mintToggleDisabled =
    prereqsQuery.isLoading || (prereqs !== null && !isMintSupported);
  const spawnToggleDisabled =
    prereqsQuery.isLoading || (prereqs !== null && !isSpawnSupported);
  const isDiscoveryPending = providersQuery.isLoading || prereqsQuery.isLoading;
  const prerequisiteCards = [
    {
      id: "admin",
      label: "Token minting",
      info: prereqs?.admin ?? null,
      command: prereqs?.admin.command ?? "sprout-admin",
    },
    {
      id: "acp",
      label: "ACP harness",
      info: prereqs?.acp ?? null,
      command: prereqs?.acp.command ?? (acpCommand.trim() || "sprout-acp"),
    },
    {
      id: "mcp",
      label: "MCP server",
      info: prereqs?.mcp ?? null,
      command:
        prereqs?.mcp.command ?? (mcpCommand.trim() || "sprout-mcp-server"),
    },
  ];

  React.useEffect(() => {
    if (hasSyncedProviderSelection || providersQuery.isLoading) {
      return;
    }

    const matchingProvider =
      providers.find((provider) => provider.command === agentCommand) ?? null;
    if (matchingProvider) {
      setSelectedProviderId(matchingProvider.id);
    }
    setHasSyncedProviderSelection(true);
  }, [
    agentCommand,
    hasSyncedProviderSelection,
    providers,
    providersQuery.isLoading,
  ]);

  React.useEffect(() => {
    if (!prereqs || prereqs.admin.available || !mintToken) {
      return;
    }

    setMintToken(false);
  }, [mintToken, prereqs]);

  React.useEffect(() => {
    if (
      !prereqs ||
      (prereqs.acp.available && prereqs.mcp.available) ||
      !spawnAfterCreate
    ) {
      return;
    }

    setSpawnAfterCreate(false);
  }, [prereqs, spawnAfterCreate]);

  function reset() {
    setName("");
    setRelayUrl("");
    setMintToken(true);
    setSpawnAfterCreate(true);
    setTokenName("");
    setSelectedScopes(
      new Set<TokenScope>(["messages:read", "messages:write", "channels:read"]),
    );
    setAcpCommand("sprout-acp");
    setAgentCommand("goose");
    setAgentArgs("acp");
    setMcpCommand("sprout-mcp-server");
    setTurnTimeoutSeconds("300");
    setSelectedProviderId("custom");
    setHasSyncedProviderSelection(false);
    createMutation.reset();
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      reset();
    }

    onOpenChange(next);
  }

  function toggleScope(scope: TokenScope) {
    setSelectedScopes((previous) => {
      const next = new Set(previous);
      if (next.has(scope)) {
        next.delete(scope);
      } else {
        next.add(scope);
      }
      return next;
    });
  }

  function handleProviderChange(nextProviderId: string) {
    setSelectedProviderId(nextProviderId);

    if (nextProviderId === "custom") {
      return;
    }

    const provider = providers.find(
      (candidate) => candidate.id === nextProviderId,
    );
    if (!provider) {
      return;
    }

    setAgentCommand(provider.command);
    setAgentArgs(provider.defaultArgs.join(","));
  }

  const canSubmit =
    name.trim().length > 0 &&
    (!mintToken || selectedScopes.size > 0) &&
    !isDiscoveryPending &&
    !(mintToken && prereqs !== null && !isMintSupported) &&
    !(spawnAfterCreate && prereqs !== null && !isSpawnSupported) &&
    !createMutation.isPending;

  async function handleSubmit() {
    try {
      const input: CreateManagedAgentInput = {
        name: name.trim(),
        relayUrl: relayUrl.trim() || undefined,
        acpCommand: acpCommand.trim() || undefined,
        agentCommand: agentCommand.trim() || undefined,
        agentArgs: agentArgs
          .split(",")
          .map((value) => value.trim())
          .filter((value) => value.length > 0),
        mcpCommand: mcpCommand.trim() || undefined,
        turnTimeoutSeconds:
          Number.parseInt(turnTimeoutSeconds, 10) > 0
            ? Number.parseInt(turnTimeoutSeconds, 10)
            : undefined,
        mintToken,
        tokenName: tokenName.trim() || undefined,
        tokenScopes: [...selectedScopes],
        spawnAfterCreate,
      };
      const created = await createMutation.mutateAsync(input);
      handleOpenChange(false);
      onCreated(created);
    } catch {
      // React Query stores the error; keep the dialog open and render it inline.
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-w-3xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Create agent</DialogTitle>
            <DialogDescription>
              This creates a local agent identity, optionally mints a relay
              token, and can spawn `sprout-acp` immediately.
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 space-y-5 overflow-y-auto px-6 py-5">
            <div className="grid gap-4 md:grid-cols-2">
              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="agent-name">
                  Name
                </label>
                <Input
                  data-testid="agent-name-input"
                  id="agent-name"
                  onChange={(event) => setName(event.target.value)}
                  placeholder="alice"
                  value={name}
                />
              </div>

              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="agent-relay-url"
                >
                  Relay URL
                </label>
                <Input
                  id="agent-relay-url"
                  onChange={(event) => setRelayUrl(event.target.value)}
                  placeholder="Leave blank to use the desktop relay"
                  value={relayUrl}
                />
              </div>
            </div>

            <div className="grid gap-4 md:grid-cols-2">
              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="agent-acp-command"
                >
                  ACP command
                </label>
                <Input
                  id="agent-acp-command"
                  onChange={(event) => setAcpCommand(event.target.value)}
                  value={acpCommand}
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="agent-provider">
                  Agent runtime
                </label>
                <select
                  className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
                  id="agent-provider"
                  onChange={(event) => handleProviderChange(event.target.value)}
                  value={selectedProviderId}
                >
                  {providers.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.label}
                    </option>
                  ))}
                  <option value="custom">Custom command</option>
                </select>
                {selectedProvider ? (
                  <p className="text-xs text-muted-foreground">
                    Detected via{" "}
                    <span className="font-medium">
                      {describeResolvedCommand(
                        selectedProvider.command,
                        selectedProvider.binaryPath,
                      )}
                    </span>
                  </p>
                ) : providersQuery.isLoading ? (
                  <p className="text-xs text-muted-foreground">
                    Looking for installed ACP runtimes...
                  </p>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    No known ACP runtime was detected. You can still enter a
                    custom command below.
                  </p>
                )}
              </div>
            </div>

            {selectedProviderId === "custom" ? (
              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="agent-runtime-command"
                >
                  Custom agent runtime command
                </label>
                <Input
                  id="agent-runtime-command"
                  onChange={(event) => setAgentCommand(event.target.value)}
                  value={agentCommand}
                />
              </div>
            ) : null}

            <div className="grid gap-4 md:grid-cols-[1.5fr,1.2fr,0.8fr]">
              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="agent-runtime-args"
                >
                  Agent runtime args
                </label>
                <Input
                  id="agent-runtime-args"
                  onChange={(event) => setAgentArgs(event.target.value)}
                  placeholder="Comma-separated"
                  value={agentArgs}
                />
                <p className="text-xs text-muted-foreground">
                  `sprout-acp` splits args on commas, matching the testing
                  guide.
                </p>
              </div>

              <div className="space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="agent-mcp-command"
                >
                  MCP command
                </label>
                <Input
                  id="agent-mcp-command"
                  onChange={(event) => setMcpCommand(event.target.value)}
                  value={mcpCommand}
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="agent-timeout">
                  Turn timeout
                </label>
                <Input
                  id="agent-timeout"
                  onChange={(event) =>
                    setTurnTimeoutSeconds(event.target.value)
                  }
                  value={turnTimeoutSeconds}
                />
              </div>
            </div>

            <div className="rounded-2xl border border-border/70 bg-muted/20 p-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold tracking-tight">
                    Local Sprout binaries
                  </p>
                  <p className="text-sm text-muted-foreground">
                    The desktop app uses these commands to mint tokens and spawn
                    harnesses.
                  </p>
                </div>
                {prereqsQuery.isLoading ? (
                  <span className="text-xs text-muted-foreground">
                    Checking...
                  </span>
                ) : null}
              </div>

              <div className="mt-4 grid gap-3 md:grid-cols-3">
                {prerequisiteCards.map((card) => (
                  <div
                    className="rounded-2xl border border-border/70 bg-background/80 px-3 py-3"
                    key={card.id}
                  >
                    <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                      {card.label}
                    </p>
                    <p className="mt-2 text-sm font-medium">{card.command}</p>
                    <p
                      className={cn(
                        "mt-1 text-xs",
                        card.info?.available
                          ? "text-muted-foreground"
                          : "text-destructive",
                      )}
                    >
                      {card.info?.resolvedPath
                        ? `Available via ${describeResolvedCommand(card.command, card.info.resolvedPath)}`
                        : prereqsQuery.isLoading
                          ? "Looking for a matching binary..."
                          : "Not currently available."}
                    </p>
                  </div>
                ))}
              </div>

              {prereqs &&
              (!prereqs.admin.available ||
                !prereqs.acp.available ||
                !prereqs.mcp.available) ? (
                <p className="mt-4 rounded-2xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300">
                  Build the workspace binaries with `cargo build --release
                  --workspace` or point the command fields at installed binaries
                  before enabling token minting or spawn.
                </p>
              ) : null}
            </div>

            {providersQuery.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {providersQuery.error.message}
              </p>
            ) : null}

            {prereqsQuery.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {prereqsQuery.error.message}
              </p>
            ) : null}

            <div className="grid gap-3 md:grid-cols-2">
              <button
                aria-pressed={mintToken}
                className={cn(
                  "rounded-2xl border px-4 py-3 text-left transition-colors",
                  mintToggleDisabled && "cursor-not-allowed opacity-60",
                  mintToken
                    ? "border-primary bg-primary/10"
                    : "border-border/70 bg-background/70",
                )}
                disabled={mintToggleDisabled}
                onClick={() => {
                  if (!mintToggleDisabled) {
                    setMintToken((current) => !current);
                  }
                }}
                type="button"
              >
                <p className="text-sm font-semibold tracking-tight">
                  Mint token
                </p>
                <p className="mt-1 text-sm text-muted-foreground">
                  {prereqs !== null && !isMintSupported
                    ? `Unavailable until ${prereqs.admin.command} is installed.`
                    : "Use `sprout-admin` to create a bearer token for this agent."}
                </p>
              </button>

              <button
                aria-pressed={spawnAfterCreate}
                className={cn(
                  "rounded-2xl border px-4 py-3 text-left transition-colors",
                  spawnToggleDisabled && "cursor-not-allowed opacity-60",
                  spawnAfterCreate
                    ? "border-primary bg-primary/10"
                    : "border-border/70 bg-background/70",
                )}
                disabled={spawnToggleDisabled}
                onClick={() => {
                  if (!spawnToggleDisabled) {
                    setSpawnAfterCreate((current) => !current);
                  }
                }}
                type="button"
              >
                <p className="text-sm font-semibold tracking-tight">
                  Spawn after create
                </p>
                <p className="mt-1 text-sm text-muted-foreground">
                  {prereqs !== null && !isSpawnSupported
                    ? "Requires both the ACP harness and MCP server binaries."
                    : "Start the local ACP harness immediately after the profile is saved."}
                </p>
              </button>
            </div>

            {mintToken ? (
              <div className="space-y-4 rounded-2xl border border-border/70 bg-muted/20 p-4">
                <div className="space-y-1.5">
                  <label
                    className="text-sm font-medium"
                    htmlFor="agent-token-name"
                  >
                    Token name
                  </label>
                  <Input
                    id="agent-token-name"
                    onChange={(event) => setTokenName(event.target.value)}
                    placeholder="Leave blank to reuse the agent name"
                    value={tokenName}
                  />
                </div>

                <div className="space-y-2">
                  <p className="text-sm font-medium">Token scopes</p>
                  <div className="grid gap-2 md:grid-cols-3">
                    {AGENT_SCOPE_OPTIONS.map((scope) => {
                      const selected = selectedScopes.has(scope.value);
                      return (
                        <button
                          className={cn(
                            "rounded-xl border px-3 py-2 text-left text-sm transition-colors",
                            selected
                              ? "border-primary bg-primary/10"
                              : "border-border/70 bg-background/80 hover:bg-accent",
                          )}
                          key={scope.value}
                          onClick={() => toggleScope(scope.value)}
                          type="button"
                        >
                          {scope.label}
                        </button>
                      );
                    })}
                  </div>
                </div>
              </div>
            ) : null}

            {createMutation.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {createMutation.error.message}
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
              data-testid="create-agent-submit"
              disabled={!canSubmit}
              onClick={() => void handleSubmit()}
              size="sm"
              type="button"
            >
              {createMutation.isPending ? "Creating..." : "Create agent"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function AddAgentToChannelDialog({
  agent,
  open,
  onAdded,
  onOpenChange,
}: {
  agent: ManagedAgent | null;
  open: boolean;
  onAdded: (channel: Channel, agent: ManagedAgent) => void;
  onOpenChange: (open: boolean) => void;
}) {
  const channelsQuery = useChannelsQuery();
  const [channelId, setChannelId] = React.useState("");
  const [role, setRole] = React.useState<Exclude<ChannelRole, "owner">>("bot");
  const addMembersMutation = useAddChannelMembersMutation(channelId || null);
  const channels = React.useMemo(
    () =>
      (channelsQuery.data ?? []).filter(
        (channel) => channel.channelType !== "dm" && !channel.archivedAt,
      ),
    [channelsQuery.data],
  );

  function reset() {
    setChannelId("");
    setRole("bot");
    addMembersMutation.reset();
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      reset();
    }

    onOpenChange(next);
  }

  React.useEffect(() => {
    if (!open) {
      return;
    }

    if (!channelId && channels.length > 0) {
      setChannelId(channels[0].id);
    }
  }, [channelId, channels, open]);

  const selectedChannel =
    channels.find((channel) => channel.id === channelId) ?? null;

  async function handleSubmit() {
    if (!agent || !selectedChannel) {
      return;
    }

    try {
      const result = await addMembersMutation.mutateAsync({
        pubkeys: [agent.pubkey],
        role,
      });
      const membershipError = result.errors.find(
        (error) => error.pubkey === agent.pubkey,
      );

      if (membershipError) {
        throw new Error(membershipError.error);
      }

      onAdded(selectedChannel, agent);
      handleOpenChange(false);
    } catch {
      // React Query stores the error; keep the dialog open and render it inline.
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-w-xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Add agent to channel</DialogTitle>
            <DialogDescription>
              Add {agent?.name ?? "this agent"} to a channel so desktop chat can
              `@mention` it. If the harness is already running, restart it after
              adding membership so it subscribes to the new channel.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-5 px-6 py-5">
            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="agent-channel-id">
                Channel
              </label>
              <select
                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
                disabled={channels.length === 0 || addMembersMutation.isPending}
                id="agent-channel-id"
                onChange={(event) => setChannelId(event.target.value)}
                value={channelId}
              >
                {channels.length === 0 ? (
                  <option value="">No channels available</option>
                ) : null}
                {channels.map((channel) => (
                  <option key={channel.id} value={channel.id}>
                    {channel.name} · {channel.visibility}
                  </option>
                ))}
              </select>
              <p className="text-xs text-muted-foreground">
                Only channels accessible to the current desktop user are shown
                here.
              </p>
            </div>

            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="agent-channel-role"
              >
                Role
              </label>
              <select
                className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
                disabled={addMembersMutation.isPending}
                id="agent-channel-role"
                onChange={(event) =>
                  setRole(event.target.value as Exclude<ChannelRole, "owner">)
                }
                value={role}
              >
                <option value="bot">bot</option>
                <option value="member">member</option>
                <option value="guest">guest</option>
                <option value="admin">admin</option>
              </select>
            </div>

            <div className="rounded-2xl border border-border/70 bg-muted/20 p-4">
              <p className="text-sm font-semibold tracking-tight">
                Agent pubkey
              </p>
              <div className="mt-3 flex items-center justify-between gap-3">
                <code className="min-w-0 flex-1 break-all rounded-xl border border-border/70 bg-background/80 px-3 py-2 text-xs">
                  {agent?.pubkey ?? "No agent selected"}
                </code>
                {agent ? (
                  <CopyButton label="Copy pubkey" value={agent.pubkey} />
                ) : null}
              </div>
            </div>

            {channelsQuery.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {channelsQuery.error.message}
              </p>
            ) : null}

            {addMembersMutation.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {addMembersMutation.error.message}
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
                !agent ||
                !selectedChannel ||
                channelsQuery.isLoading ||
                addMembersMutation.isPending
              }
              onClick={() => void handleSubmit()}
              size="sm"
              type="button"
            >
              {addMembersMutation.isPending ? "Adding..." : "Add to channel"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function ManagedAgentCard({
  agent,
  onAddToChannel,
  isSelected,
  onDelete,
  onMintToken,
  onSelect,
  onStart,
  onStop,
}: {
  agent: ManagedAgent;
  onAddToChannel: (agent: ManagedAgent) => void;
  isSelected: boolean;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onSelect: (pubkey: string) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
}) {
  const statusBadgeClass =
    agent.status === "running"
      ? "bg-primary text-primary-foreground"
      : "bg-muted text-muted-foreground";

  return (
    <article
      className={cn(
        "rounded-3xl border p-4 shadow-sm transition-colors",
        isSelected
          ? "border-primary/60 bg-primary/5"
          : "border-border/70 bg-card/80 hover:bg-card",
      )}
      data-testid={`managed-agent-${agent.pubkey}`}
    >
      <button
        className="w-full text-left"
        onClick={() => onSelect(agent.pubkey)}
        type="button"
      >
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-sm font-semibold tracking-tight">
                {agent.name}
              </h3>
              <span
                className={cn(
                  "rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.18em]",
                  statusBadgeClass,
                )}
              >
                {agent.status}
              </span>
            </div>
            <p className="mt-1 text-xs text-muted-foreground">
              {truncatePubkey(agent.pubkey)}
              {agent.pid ? ` · pid ${agent.pid}` : ""}
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
            <TerminalSquare className="h-4 w-4" />
            <span>{agent.agentCommand}</span>
          </div>
        </div>
      </button>

      <div className="mt-4 grid gap-2 sm:grid-cols-2">
        <div className="rounded-2xl border border-border/60 bg-background/70 px-3 py-2">
          <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Relay
          </p>
          <p className="mt-1 truncate text-sm">{agent.relayUrl}</p>
        </div>
        <div className="rounded-2xl border border-border/60 bg-background/70 px-3 py-2">
          <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Auth
          </p>
          <p className="mt-1 text-sm">
            {agent.hasApiToken ? "Bearer token saved" : "Key-only dev mode"}
          </p>
        </div>
      </div>

      <div className="mt-4 flex flex-wrap gap-2">
        <CopyButton label="Copy pubkey" value={agent.pubkey} />

        {agent.status === "running" ? (
          <Button
            onClick={() => onStop(agent.pubkey)}
            size="sm"
            type="button"
            variant="outline"
          >
            <Square className="h-3.5 w-3.5" />
            Stop
          </Button>
        ) : (
          <Button onClick={() => onStart(agent.pubkey)} size="sm" type="button">
            <Play className="h-3.5 w-3.5" />
            Spawn
          </Button>
        )}

        <Button
          onClick={() => onAddToChannel(agent)}
          size="sm"
          type="button"
          variant="outline"
        >
          <UserPlus className="h-3.5 w-3.5" />
          Add to channel
        </Button>

        <Button
          onClick={() => onMintToken(agent.pubkey, agent.name)}
          size="sm"
          type="button"
          variant="outline"
        >
          <KeyRound className="h-3.5 w-3.5" />
          Mint token
        </Button>

        <Button
          onClick={() => onDelete(agent.pubkey)}
          size="sm"
          type="button"
          variant="ghost"
        >
          <Trash2 className="h-3.5 w-3.5" />
          Delete
        </Button>
      </div>

      <div className="mt-4 grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
        <p>Started {formatTimestamp(agent.lastStartedAt)}</p>
        <p>Stopped {formatTimestamp(agent.lastStoppedAt)}</p>
      </div>

      {agent.lastError ? (
        <p className="mt-3 rounded-2xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {agent.lastError}
        </p>
      ) : null}
    </article>
  );
}

function RelayAgentCard({
  agent,
  isManagedLocally,
}: {
  agent: RelayAgent;
  isManagedLocally: boolean;
}) {
  const visibleCapabilities = agent.capabilities.slice(0, 4);
  const hiddenCapabilityCount =
    agent.capabilities.length - visibleCapabilities.length;

  return (
    <article className="rounded-3xl border border-border/70 bg-card/80 p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-sm font-semibold tracking-tight">
              {agent.name}
            </h3>
            {isManagedLocally ? (
              <span className="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.18em] text-primary-foreground">
                Local
              </span>
            ) : null}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {truncatePubkey(agent.pubkey)}
            {agent.agentType ? ` · ${agent.agentType}` : ""}
          </p>
        </div>
        <PresenceBadge status={agent.status} />
      </div>

      <div className="mt-4 flex flex-wrap gap-2">
        {visibleCapabilities.map((capability) => (
          <span
            className="rounded-full border border-border/70 bg-background/70 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground"
            key={capability}
          >
            {capability}
          </span>
        ))}
        {hiddenCapabilityCount > 0 ? (
          <span className="rounded-full border border-border/70 bg-background/70 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
            +{hiddenCapabilityCount}
          </span>
        ) : null}
      </div>

      <p className="mt-4 text-xs text-muted-foreground">
        {agent.channels.length > 0
          ? `Visible in ${agent.channels.join(", ")}`
          : "No visible channel memberships yet."}
      </p>
    </article>
  );
}

export function AgentsView() {
  const relayAgentsQuery = useRelayAgentsQuery();
  const managedAgentsQuery = useManagedAgentsQuery();
  const startMutation = useStartManagedAgentMutation();
  const stopMutation = useStopManagedAgentMutation();
  const deleteMutation = useDeleteManagedAgentMutation();
  const mintTokenMutation = useMintManagedAgentTokenMutation();
  const [isCreateOpen, setIsCreateOpen] = React.useState(false);
  const [agentToAddToChannel, setAgentToAddToChannel] =
    React.useState<ManagedAgent | null>(null);
  const [createdAgent, setCreatedAgent] =
    React.useState<CreateManagedAgentResponse | null>(null);
  const [revealedToken, setRevealedToken] = React.useState<{
    name: string;
    token: string;
  } | null>(null);
  const [actionNoticeMessage, setActionNoticeMessage] = React.useState<
    string | null
  >(null);
  const [actionErrorMessage, setActionErrorMessage] = React.useState<
    string | null
  >(null);
  const managedAgents = React.useMemo(
    () =>
      [...(managedAgentsQuery.data ?? [])].sort((left, right) => {
        if (left.status !== right.status) {
          return left.status === "running" ? -1 : 1;
        }

        return left.name.localeCompare(right.name);
      }),
    [managedAgentsQuery.data],
  );
  const [selectedAgentPubkey, setSelectedAgentPubkey] = React.useState<
    string | null
  >(null);
  const selectedAgent =
    managedAgents.find((agent) => agent.pubkey === selectedAgentPubkey) ??
    managedAgents[0] ??
    null;
  const managedAgentLogQuery = useManagedAgentLogQuery(
    selectedAgent?.pubkey ?? null,
  );
  const managedPubkeys = React.useMemo(
    () => new Set(managedAgents.map((agent) => agent.pubkey)),
    [managedAgents],
  );

  React.useEffect(() => {
    if (
      selectedAgentPubkey &&
      managedAgents.some((agent) => agent.pubkey === selectedAgentPubkey)
    ) {
      return;
    }

    setSelectedAgentPubkey(managedAgents[0]?.pubkey ?? null);
  }, [managedAgents, selectedAgentPubkey]);

  async function handleStart(pubkey: string) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      await startMutation.mutateAsync(pubkey);
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to start agent.",
      );
    }
  }

  async function handleStop(pubkey: string) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      await stopMutation.mutateAsync(pubkey);
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to stop agent.",
      );
    }
  }

  async function handleDelete(pubkey: string) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      await deleteMutation.mutateAsync(pubkey);
      if (selectedAgentPubkey === pubkey) {
        setSelectedAgentPubkey(null);
      }
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to delete agent.",
      );
    }
  }

  async function handleMintToken(pubkey: string, name: string) {
    setActionNoticeMessage(null);
    setActionErrorMessage(null);

    try {
      const result = await mintTokenMutation.mutateAsync({
        pubkey,
        tokenName: `${name}-token`,
      });
      setRevealedToken({
        name,
        token: result.token,
      });
    } catch (error) {
      setActionErrorMessage(
        error instanceof Error ? error.message : "Failed to mint token.",
      );
    }
  }

  function handleAddedToChannel(channel: Channel, agent: ManagedAgent) {
    setActionErrorMessage(null);
    setActionNoticeMessage(
      agent.status === "running"
        ? `Added ${agent.name} to ${channel.name}. Restart the agent to subscribe to the new channel.`
        : `Added ${agent.name} to ${channel.name}.`,
    );
    void managedAgentsQuery.refetch();
    void relayAgentsQuery.refetch();
  }

  const isActionPending =
    startMutation.isPending ||
    stopMutation.isPending ||
    deleteMutation.isPending ||
    mintTokenMutation.isPending;

  return (
    <>
      <div className="flex-1 overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 sm:px-6">
        <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
          <div className="grid gap-6 xl:grid-cols-[1.2fr,0.9fr]">
            <section className="space-y-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <h3 className="text-sm font-semibold tracking-tight">
                    Managed locally
                  </h3>
                  <p className="text-sm text-muted-foreground">
                    Saved agent profiles and local ACP process state.
                  </p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button
                    onClick={() => {
                      setIsCreateOpen(true);
                    }}
                    type="button"
                  >
                    <Plus className="h-4 w-4" />
                    Create agent
                  </Button>
                  <Button
                    onClick={() => {
                      void managedAgentsQuery.refetch();
                      void relayAgentsQuery.refetch();
                      void managedAgentLogQuery.refetch();
                    }}
                    type="button"
                    variant="outline"
                  >
                    <RefreshCcw className="h-4 w-4" />
                    Refresh
                  </Button>
                </div>
              </div>

              {managedAgentsQuery.isLoading ? (
                <div className="grid gap-3">
                  {["first", "second"].map((key) => (
                    <div
                      className="rounded-3xl border border-border/70 bg-card/80 p-4"
                      key={key}
                    >
                      <Skeleton className="h-5 w-32" />
                      <Skeleton className="mt-3 h-4 w-48" />
                      <Skeleton className="mt-4 h-16 w-full" />
                    </div>
                  ))}
                </div>
              ) : null}

              {!managedAgentsQuery.isLoading && managedAgents.length === 0 ? (
                <div className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center">
                  <p className="text-sm font-semibold tracking-tight">
                    No local agents yet
                  </p>
                  <p className="mt-2 text-sm text-muted-foreground">
                    Create one to generate a keypair, mint a token, and launch
                    the ACP harness from the desktop app.
                  </p>
                </div>
              ) : null}

              {managedAgents.map((agent) => (
                <ManagedAgentCard
                  agent={agent}
                  isSelected={selectedAgent?.pubkey === agent.pubkey}
                  key={agent.pubkey}
                  onAddToChannel={(managedAgent) => {
                    if (!isActionPending) {
                      setActionNoticeMessage(null);
                      setActionErrorMessage(null);
                      setAgentToAddToChannel(managedAgent);
                    }
                  }}
                  onDelete={(pubkey) => {
                    if (!isActionPending) {
                      void handleDelete(pubkey);
                    }
                  }}
                  onMintToken={(pubkey, name) => {
                    if (!isActionPending) {
                      void handleMintToken(pubkey, name);
                    }
                  }}
                  onSelect={setSelectedAgentPubkey}
                  onStart={(pubkey) => {
                    if (!isActionPending) {
                      void handleStart(pubkey);
                    }
                  }}
                  onStop={(pubkey) => {
                    if (!isActionPending) {
                      void handleStop(pubkey);
                    }
                  }}
                />
              ))}

              {managedAgentsQuery.error instanceof Error ? (
                <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                  {managedAgentsQuery.error.message}
                </p>
              ) : null}

              {actionNoticeMessage ? (
                <p className="rounded-2xl border border-primary/20 bg-primary/10 px-4 py-3 text-sm text-primary">
                  {actionNoticeMessage}
                </p>
              ) : null}

              {actionErrorMessage ? (
                <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                  {actionErrorMessage}
                </p>
              ) : null}
            </section>

            <section className="space-y-4">
              <div>
                <h3 className="text-sm font-semibold tracking-tight">
                  Relay directory
                </h3>
                <p className="text-sm text-muted-foreground">
                  Bot and agent identities visible to the current desktop user.
                </p>
              </div>

              {relayAgentsQuery.isLoading ? (
                <div className="grid gap-3">
                  {["directory-1", "directory-2"].map((key) => (
                    <div
                      className="rounded-3xl border border-border/70 bg-card/80 p-4"
                      key={key}
                    >
                      <Skeleton className="h-5 w-36" />
                      <Skeleton className="mt-3 h-4 w-44" />
                      <Skeleton className="mt-4 h-12 w-full" />
                    </div>
                  ))}
                </div>
              ) : null}

              {!relayAgentsQuery.isLoading &&
              (relayAgentsQuery.data?.length ?? 0) === 0 ? (
                <div className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center">
                  <p className="text-sm font-semibold tracking-tight">
                    No relay-visible agents yet
                  </p>
                  <p className="mt-2 text-sm text-muted-foreground">
                    Start one of your local harnesses or join an existing bot to
                    a channel and it will appear here.
                  </p>
                </div>
              ) : null}

              {relayAgentsQuery.data?.map((agent) => (
                <RelayAgentCard
                  agent={agent}
                  isManagedLocally={managedPubkeys.has(agent.pubkey)}
                  key={agent.pubkey}
                />
              ))}

              {relayAgentsQuery.error instanceof Error ? (
                <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                  {relayAgentsQuery.error.message}
                </p>
              ) : null}
            </section>
          </div>

          <section className="rounded-[28px] border border-border/70 bg-card/90 p-5 shadow-sm">
            <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <h3 className="text-sm font-semibold tracking-tight">
                  Harness log
                </h3>
                <p className="text-sm text-muted-foreground">
                  {selectedAgent
                    ? `${selectedAgent.name} · ${describeLogFile(selectedAgent.logPath)}`
                    : "Select a local agent to inspect recent output."}
                </p>
              </div>
              {selectedAgent ? (
                <CopyButton
                  label="Copy log"
                  value={managedAgentLogQuery.data?.content ?? ""}
                />
              ) : null}
            </div>

            {!selectedAgent ? (
              <div className="mt-4 rounded-3xl border border-dashed border-border/80 bg-background/70 px-6 py-10 text-center">
                <p className="text-sm font-semibold tracking-tight">
                  No local agent selected
                </p>
                <p className="mt-2 text-sm text-muted-foreground">
                  Pick a managed agent to view the latest ACP log output.
                </p>
              </div>
            ) : managedAgentLogQuery.isLoading ? (
              <div className="mt-4 rounded-3xl border border-border/70 bg-background/80 p-4">
                <Skeleton className="h-4 w-48" />
                <Skeleton className="mt-3 h-4 w-full" />
                <Skeleton className="mt-2 h-4 w-full" />
                <Skeleton className="mt-2 h-4 w-3/4" />
              </div>
            ) : (
              <div className="mt-4 overflow-hidden rounded-3xl border border-border/70 bg-[#17171d] text-[12px] text-zinc-100">
                <div className="flex items-center justify-between border-b border-white/10 px-4 py-2 text-[11px] uppercase tracking-[0.18em] text-zinc-400">
                  <span>{selectedAgent.name}</span>
                  <span>{selectedAgent.status}</span>
                </div>
                <pre className="max-h-[22rem] overflow-auto px-4 py-4 whitespace-pre-wrap">
                  {managedAgentLogQuery.data?.content?.trim()
                    ? managedAgentLogQuery.data.content
                    : "No log output yet."}
                </pre>
              </div>
            )}

            {managedAgentLogQuery.error instanceof Error ? (
              <p className="mt-4 inline-flex items-center gap-2 rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                <CircleAlert className="h-4 w-4" />
                {managedAgentLogQuery.error.message}
              </p>
            ) : null}
          </section>
        </div>
      </div>

      <CreateAgentDialog
        onCreated={(result) => {
          setCreatedAgent(result);
        }}
        onOpenChange={setIsCreateOpen}
        open={isCreateOpen}
      />
      <AddAgentToChannelDialog
        agent={agentToAddToChannel}
        onAdded={handleAddedToChannel}
        onOpenChange={(open) => {
          if (!open) {
            setAgentToAddToChannel(null);
          }
        }}
        open={agentToAddToChannel !== null}
      />
      <SecretRevealDialog
        created={createdAgent}
        onOpenChange={(open) => {
          if (!open) {
            setCreatedAgent(null);
          }
        }}
      />
      <TokenRevealDialog
        name={revealedToken?.name ?? null}
        onOpenChange={(open) => {
          if (!open) {
            setRevealedToken(null);
          }
        }}
        token={revealedToken?.token ?? null}
      />
    </>
  );
}
