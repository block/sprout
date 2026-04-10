import type {
  AcpProvider,
  ManagedAgentPrereqs,
  TokenScope,
} from "@/shared/api/types";
import { MANAGED_AGENT_SCOPE_OPTIONS } from "@/features/tokens/lib/scopeOptions";
import { cn } from "@/shared/lib/cn";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import { describeResolvedCommand } from "./agentUi";

export function CreateAgentBasicsFields({
  name,
  onNameChange,
}: {
  name: string;
  onNameChange: (value: string) => void;
}) {
  return (
    <div className="space-y-1.5">
      <label className="text-sm font-medium" htmlFor="agent-name">
        Agent name
      </label>
      <Input
        aria-describedby="help-agent-name"
        autoCapitalize="none"
        autoCorrect="off"
        data-testid="agent-name-input"
        id="agent-name"
        onChange={(event) => onNameChange(event.target.value)}
        placeholder="Support bot"
        spellCheck={false}
        value={name}
      />
      <p className="text-xs text-muted-foreground" id="help-agent-name">
        Used as the local label and synced to the agent profile display name
        when the relay accepts the create-time auth.
      </p>
    </div>
  );
}

export function CreateAgentRuntimeProviderField({
  providers,
  providersLoading,
  selectedProvider,
  selectedProviderId,
  onProviderChange,
}: {
  providers: AcpProvider[];
  providersLoading: boolean;
  selectedProvider: AcpProvider | null;
  selectedProviderId: string;
  onProviderChange: (value: string) => void;
}) {
  return (
    <div className="space-y-1.5">
      <label className="text-sm font-medium" htmlFor="agent-provider">
        Agent runtime
      </label>
      <select
        className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
        id="agent-provider"
        onChange={(event) => onProviderChange(event.target.value)}
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
      ) : providersLoading ? (
        <p className="text-xs text-muted-foreground">
          Looking for installed ACP runtimes...
        </p>
      ) : (
        <p className="text-xs text-muted-foreground">
          No known ACP runtime was detected. You can still enter a custom
          command in Advanced setup.
        </p>
      )}
    </div>
  );
}

export function CreateAgentRuntimeFields({
  acpCommand,
  agentArgs,
  agentCommand,
  mcpCommand,
  parallelism,
  relayUrl,
  selectedProviderId,
  systemPrompt,
  turnTimeoutSeconds,
  onAcpCommandChange,
  onAgentArgsChange,
  onAgentCommandChange,
  onMcpCommandChange,
  onParallelismChange,
  onRelayUrlChange,
  onSystemPromptChange,
  onTurnTimeoutChange,
}: {
  acpCommand: string;
  agentArgs: string;
  agentCommand: string;
  mcpCommand: string;
  parallelism: string;
  relayUrl: string;
  selectedProviderId: string;
  systemPrompt: string;
  turnTimeoutSeconds: string;
  onAcpCommandChange: (value: string) => void;
  onAgentArgsChange: (value: string) => void;
  onAgentCommandChange: (value: string) => void;
  onMcpCommandChange: (value: string) => void;
  onParallelismChange: (value: string) => void;
  onRelayUrlChange: (value: string) => void;
  onSystemPromptChange: (value: string) => void;
  onTurnTimeoutChange: (value: string) => void;
}) {
  return (
    <>
      <div className="grid gap-4 md:grid-cols-2">
        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-relay-url">
            Relay URL
          </label>
          <Input
            aria-describedby="help-agent-relay-url"
            id="agent-relay-url"
            onChange={(event) => onRelayUrlChange(event.target.value)}
            placeholder="Leave blank to use the desktop relay"
            value={relayUrl}
          />
          <p
            className="text-xs text-muted-foreground"
            id="help-agent-relay-url"
          >
            WebSocket URL of the relay this agent connects to. Leave blank to
            use the built-in desktop relay.
          </p>
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-acp-command">
            ACP command
          </label>
          <Input
            aria-describedby="help-agent-acp-command"
            id="agent-acp-command"
            onChange={(event) => onAcpCommandChange(event.target.value)}
            value={acpCommand}
          />
          <p
            className="text-xs text-muted-foreground"
            id="help-agent-acp-command"
          >
            The sprout-acp binary path or alias used to launch the ACP harness
            process.
          </p>
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
            aria-describedby="help-agent-runtime-command"
            id="agent-runtime-command"
            onChange={(event) => onAgentCommandChange(event.target.value)}
            value={agentCommand}
          />
          <p
            className="text-xs text-muted-foreground"
            id="help-agent-runtime-command"
          >
            Full path or shell command for the agent binary when no known ACP
            runtime was detected.
          </p>
        </div>
      ) : null}

      <div className="grid gap-4 md:grid-cols-[1.5fr,1.2fr,0.8fr,0.8fr]">
        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-runtime-args">
            Agent runtime args
          </label>
          <Input
            aria-describedby="help-agent-runtime-args"
            id="agent-runtime-args"
            onChange={(event) => onAgentArgsChange(event.target.value)}
            placeholder="Comma-separated"
            value={agentArgs}
          />
          <p
            className="text-xs text-muted-foreground"
            id="help-agent-runtime-args"
          >
            sprout-acp splits args on commas, matching the testing guide.
          </p>
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-mcp-command">
            MCP command
          </label>
          <Input
            aria-describedby="help-agent-mcp-command"
            id="agent-mcp-command"
            onChange={(event) => onMcpCommandChange(event.target.value)}
            value={mcpCommand}
          />
          <p
            className="text-xs text-muted-foreground"
            id="help-agent-mcp-command"
          >
            Command the ACP harness uses to start the MCP tool server for this
            agent.
          </p>
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-timeout">
            Turn timeout
          </label>
          <Input
            aria-describedby="help-agent-timeout"
            id="agent-timeout"
            onChange={(event) => onTurnTimeoutChange(event.target.value)}
            placeholder="300"
            value={turnTimeoutSeconds}
          />
          <p className="text-xs text-muted-foreground" id="help-agent-timeout">
            Seconds before an agent turn is cancelled. Defaults to 300.
          </p>
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-parallelism">
            Parallelism
          </label>
          <Input
            aria-describedby="help-agent-parallelism"
            data-testid="agent-parallelism-input"
            id="agent-parallelism"
            inputMode="numeric"
            max="32"
            min="1"
            onChange={(event) => onParallelismChange(event.target.value)}
            placeholder="1"
            step="1"
            type="number"
            value={parallelism}
          />
          <p
            className="text-xs text-muted-foreground"
            id="help-agent-parallelism"
          >
            Number of ACP worker subprocesses. sprout-acp allows 1-32.
          </p>
        </div>
      </div>

      <div className="space-y-1.5">
        <label className="text-sm font-medium" htmlFor="agent-system-prompt">
          System prompt override
        </label>
        <Textarea
          aria-describedby="help-agent-system-prompt"
          data-testid="agent-system-prompt-input"
          id="agent-system-prompt"
          onChange={(event) => onSystemPromptChange(event.target.value)}
          placeholder="Leave blank to send no ACP system prompt"
          value={systemPrompt}
        />
        <p
          className="text-xs text-muted-foreground"
          id="help-agent-system-prompt"
        >
          Blank means no override. sprout-acp will not add a [System] prompt.
        </p>
      </div>
    </>
  );
}

export function CreateAgentOptionToggles({
  isMintSupported,
  isSpawnSupported,
  mintToken,
  mintToggleDisabled,
  prereqs,
  startOnAppLaunch,
  startOnAppLaunchDisabled,
  spawnAfterCreate,
  spawnToggleDisabled,
  onToggleMintToken,
  onToggleStartOnAppLaunch,
  onToggleSpawnAfterCreate,
}: {
  isMintSupported: boolean;
  isSpawnSupported: boolean;
  mintToken: boolean;
  mintToggleDisabled: boolean;
  prereqs: ManagedAgentPrereqs | null;
  startOnAppLaunch: boolean;
  /** When true, the toggle is disabled (e.g. remote agents don't support auto-start). */
  startOnAppLaunchDisabled?: boolean;
  spawnAfterCreate: boolean;
  spawnToggleDisabled: boolean;
  onToggleMintToken: () => void;
  onToggleStartOnAppLaunch: () => void;
  onToggleSpawnAfterCreate: () => void;
}) {
  return (
    <div className="grid gap-3 md:grid-cols-3">
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
        onClick={onToggleMintToken}
        type="button"
      >
        <p className="text-sm font-semibold tracking-tight">Mint token</p>
        <p className="mt-1 text-sm text-foreground/70">
          {prereqs !== null && !isMintSupported
            ? `Unavailable until ${prereqs.admin.command} is installed.`
            : "Use sprout-admin to create a bearer token for this agent."}
        </p>
      </button>

      <button
        aria-pressed={startOnAppLaunch}
        className={cn(
          "rounded-2xl border px-4 py-3 text-left transition-colors",
          startOnAppLaunchDisabled && "cursor-not-allowed opacity-60",
          startOnAppLaunch
            ? "border-primary bg-primary/10"
            : "border-border/70 bg-background/70",
        )}
        disabled={startOnAppLaunchDisabled}
        onClick={onToggleStartOnAppLaunch}
        type="button"
      >
        <p className="text-sm font-semibold tracking-tight">
          Start on app launch
        </p>
        <p className="mt-1 text-sm text-foreground/70">
          {startOnAppLaunchDisabled
            ? "Remote agents are managed by their provider and don\u2019t auto-start with the desktop app."
            : "Reopen this local ACP harness automatically when the desktop app starts."}
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
        onClick={onToggleSpawnAfterCreate}
        type="button"
      >
        <p className="text-sm font-semibold tracking-tight">
          Spawn after create
        </p>
        <p className="mt-1 text-sm text-foreground/70">
          {prereqs !== null && !isSpawnSupported
            ? "Requires both the ACP harness and MCP server binaries."
            : "Start the local ACP harness immediately after the profile is saved."}
        </p>
      </button>
    </div>
  );
}

export function CreateAgentTokenSection({
  selectedScopes,
  tokenName,
  lockedScopes,
  onScopeToggle,
  onTokenNameChange,
}: {
  selectedScopes: Set<TokenScope>;
  tokenName: string;
  /** Scopes that cannot be removed (e.g. required for remote agent controllability). */
  lockedScopes?: Set<TokenScope>;
  onScopeToggle: (scope: TokenScope) => void;
  onTokenNameChange: (value: string) => void;
}) {
  return (
    <div className="space-y-4 rounded-2xl border border-border/70 bg-muted/20 p-4">
      <div className="space-y-1.5">
        <label className="text-sm font-medium" htmlFor="agent-token-name">
          Token name
        </label>
        <Input
          id="agent-token-name"
          onChange={(event) => onTokenNameChange(event.target.value)}
          placeholder="Leave blank to reuse the agent name"
          value={tokenName}
        />
      </div>

      <div className="space-y-2">
        <p className="text-sm font-medium">Scopes</p>
        <div className="grid grid-cols-2 gap-2">
          {MANAGED_AGENT_SCOPE_OPTIONS.map((scope) => {
            const selected = selectedScopes.has(scope.value);
            const locked = lockedScopes?.has(scope.value) && selected;
            return (
              <button
                className={cn(
                  "rounded-lg border px-3 py-2 text-left text-sm transition-colors",
                  selected
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border/60 text-muted-foreground hover:bg-accent",
                  locked && "cursor-not-allowed opacity-70",
                )}
                disabled={locked}
                key={scope.value}
                onClick={() => onScopeToggle(scope.value)}
                title={
                  locked
                    ? "Required for remote agent controllability"
                    : undefined
                }
                type="button"
              >
                {scope.label}
                {locked ? (
                  <span className="ml-1 text-xs text-muted-foreground">
                    (required)
                  </span>
                ) : null}
                <span className="block text-xs text-foreground/60">
                  {scope.description}
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
