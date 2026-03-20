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
        autoCapitalize="none"
        autoCorrect="off"
        data-testid="agent-name-input"
        id="agent-name"
        onChange={(event) => onNameChange(event.target.value)}
        placeholder="Support bot"
        spellCheck={false}
        value={name}
      />
      <p className="text-xs text-muted-foreground">
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
            id="agent-relay-url"
            onChange={(event) => onRelayUrlChange(event.target.value)}
            placeholder="Leave blank to use the desktop relay"
            value={relayUrl}
          />
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-acp-command">
            ACP command
          </label>
          <Input
            id="agent-acp-command"
            onChange={(event) => onAcpCommandChange(event.target.value)}
            value={acpCommand}
          />
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
            onChange={(event) => onAgentCommandChange(event.target.value)}
            value={agentCommand}
          />
        </div>
      ) : null}

      <div className="grid gap-4 md:grid-cols-[1.5fr,1.2fr,0.8fr,0.8fr]">
        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-runtime-args">
            Agent runtime args
          </label>
          <Input
            id="agent-runtime-args"
            onChange={(event) => onAgentArgsChange(event.target.value)}
            placeholder="Comma-separated"
            value={agentArgs}
          />
          <p className="text-xs text-muted-foreground">
            `sprout-acp` splits args on commas, matching the testing guide.
          </p>
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-mcp-command">
            MCP command
          </label>
          <Input
            id="agent-mcp-command"
            onChange={(event) => onMcpCommandChange(event.target.value)}
            value={mcpCommand}
          />
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-timeout">
            Turn timeout
          </label>
          <Input
            id="agent-timeout"
            onChange={(event) => onTurnTimeoutChange(event.target.value)}
            placeholder="300"
            value={turnTimeoutSeconds}
          />
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium" htmlFor="agent-parallelism">
            Parallelism
          </label>
          <Input
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
          <p className="text-xs text-muted-foreground">
            Number of ACP worker subprocesses. `sprout-acp` allows 1-32.
          </p>
        </div>
      </div>

      <div className="space-y-1.5">
        <label className="text-sm font-medium" htmlFor="agent-system-prompt">
          System prompt override
        </label>
        <Textarea
          data-testid="agent-system-prompt-input"
          id="agent-system-prompt"
          onChange={(event) => onSystemPromptChange(event.target.value)}
          placeholder="Leave blank to send no ACP system prompt"
          value={systemPrompt}
        />
        <p className="text-xs text-muted-foreground">
          Blank means no override. `sprout-acp` will not add a `[System]`
          prompt.
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
            : "Use `sprout-admin` to create a bearer token for this agent."}
        </p>
      </button>

      <button
        aria-pressed={startOnAppLaunch}
        className={cn(
          "rounded-2xl border px-4 py-3 text-left transition-colors",
          startOnAppLaunch
            ? "border-primary bg-primary/10"
            : "border-border/70 bg-background/70",
        )}
        onClick={onToggleStartOnAppLaunch}
        type="button"
      >
        <p className="text-sm font-semibold tracking-tight">
          Start on app launch
        </p>
        <p className="mt-1 text-sm text-foreground/70">
          Reopen this local ACP harness automatically when the desktop app
          starts.
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
  onScopeToggle,
  onTokenNameChange,
}: {
  selectedScopes: Set<TokenScope>;
  tokenName: string;
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
            return (
              <button
                className={cn(
                  "rounded-lg border px-3 py-2 text-left text-sm transition-colors",
                  selected
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border/60 text-muted-foreground hover:bg-accent",
                )}
                key={scope.value}
                onClick={() => onScopeToggle(scope.value)}
                type="button"
              >
                {scope.label}
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
