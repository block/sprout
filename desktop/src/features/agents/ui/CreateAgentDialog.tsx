import { ChevronDown } from "lucide-react";
import * as React from "react";

import {
  useAcpProvidersQuery,
  useCreateManagedAgentMutation,
  useManagedAgentPrereqsQuery,
} from "@/features/agents/hooks";
import { DEFAULT_MANAGED_AGENT_SCOPES } from "@/features/tokens/lib/scopeOptions";
import type {
  CreateManagedAgentInput,
  CreateManagedAgentResponse,
  TokenScope,
} from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import {
  CreateAgentBasicsFields,
  CreateAgentOptionToggles,
  CreateAgentRuntimeProviderField,
  CreateAgentRuntimeFields,
  CreateAgentTokenSection,
} from "./CreateAgentDialogSections";

export function CreateAgentDialog({
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
    () => new Set<TokenScope>(DEFAULT_MANAGED_AGENT_SCOPES),
  );
  const [turnTimeoutSeconds, setTurnTimeoutSeconds] = React.useState("300");
  const [parallelism, setParallelism] = React.useState("1");
  const [systemPrompt, setSystemPrompt] = React.useState("");
  const [selectedProviderId, setSelectedProviderId] =
    React.useState<string>("custom");
  const [hasSyncedProviderSelection, setHasSyncedProviderSelection] =
    React.useState(false);
  const [showAdvanced, setShowAdvanced] = React.useState(false);
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

  React.useEffect(() => {
    if (
      providersQuery.error instanceof Error ||
      prereqsQuery.error instanceof Error
    ) {
      setShowAdvanced(true);
    }
  }, [prereqsQuery.error, providersQuery.error]);

  function reset() {
    setName("");
    setRelayUrl("");
    setMintToken(true);
    setSpawnAfterCreate(true);
    setTokenName("");
    setSelectedScopes(new Set<TokenScope>(DEFAULT_MANAGED_AGENT_SCOPES));
    setAcpCommand("sprout-acp");
    setAgentCommand("goose");
    setAgentArgs("acp");
    setMcpCommand("sprout-mcp-server");
    setTurnTimeoutSeconds("300");
    setParallelism("1");
    setSystemPrompt("");
    setSelectedProviderId("custom");
    setHasSyncedProviderSelection(false);
    setShowAdvanced(false);
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
      setShowAdvanced(true);
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
        parallelism:
          Number.parseInt(parallelism, 10) > 0
            ? Number.parseInt(parallelism, 10)
            : undefined,
        systemPrompt: systemPrompt.trim() || undefined,
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
              This creates a local agent identity, syncs its display name when
              possible, optionally mints a relay token, and can spawn
              `sprout-acp` immediately.
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 space-y-5 overflow-y-auto px-6 py-5">
            <CreateAgentBasicsFields name={name} onNameChange={setName} />

            <CreateAgentRuntimeProviderField
              onProviderChange={handleProviderChange}
              providers={providers}
              providersLoading={providersQuery.isLoading}
              selectedProvider={selectedProvider}
              selectedProviderId={selectedProviderId}
            />

            <CreateAgentOptionToggles
              isMintSupported={isMintSupported}
              isSpawnSupported={isSpawnSupported}
              mintToken={mintToken}
              mintToggleDisabled={mintToggleDisabled}
              onToggleMintToken={() => {
                if (!mintToggleDisabled) {
                  setMintToken((current) => !current);
                }
              }}
              onToggleSpawnAfterCreate={() => {
                if (!spawnToggleDisabled) {
                  setSpawnAfterCreate((current) => !current);
                }
              }}
              prereqs={prereqs}
              spawnAfterCreate={spawnAfterCreate}
              spawnToggleDisabled={spawnToggleDisabled}
            />

            {mintToken ? (
              <CreateAgentTokenSection
                onScopeToggle={toggleScope}
                onTokenNameChange={setTokenName}
                selectedScopes={selectedScopes}
                tokenName={tokenName}
              />
            ) : null}

            <div className="rounded-2xl border border-border/70 bg-muted/20">
              <button
                aria-expanded={showAdvanced}
                className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left"
                onClick={() => setShowAdvanced((current) => !current)}
                type="button"
              >
                <div>
                  <p className="text-sm font-semibold tracking-tight">
                    Advanced setup
                  </p>
                  <p className="text-sm text-muted-foreground">
                    Relay overrides, raw commands, timeout, parallelism, prompt
                    override, and doctor guidance.
                  </p>
                </div>
                <span className="shrink-0 self-center text-muted-foreground">
                  <ChevronDown
                    className={`h-4 w-4 transition-transform ${showAdvanced ? "rotate-180" : ""}`}
                  />
                </span>
              </button>

              {showAdvanced ? (
                <div className="overflow-hidden">
                  <div className="space-y-5 border-t border-border/60 px-4 py-4">
                    <CreateAgentRuntimeFields
                      acpCommand={acpCommand}
                      agentArgs={agentArgs}
                      agentCommand={agentCommand}
                      mcpCommand={mcpCommand}
                      onParallelismChange={setParallelism}
                      onAcpCommandChange={setAcpCommand}
                      onAgentArgsChange={setAgentArgs}
                      onAgentCommandChange={setAgentCommand}
                      onMcpCommandChange={setMcpCommand}
                      onRelayUrlChange={setRelayUrl}
                      onSystemPromptChange={setSystemPrompt}
                      onTurnTimeoutChange={setTurnTimeoutSeconds}
                      parallelism={parallelism}
                      relayUrl={relayUrl}
                      selectedProviderId={selectedProviderId}
                      systemPrompt={systemPrompt}
                      turnTimeoutSeconds={turnTimeoutSeconds}
                    />

                    <p className="rounded-2xl border border-border/70 bg-background/70 px-4 py-3 text-sm text-muted-foreground">
                      Local Sprout binary checks and ACP runtime discovery now
                      live in Settings &gt; Doctor.
                    </p>

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
                  </div>
                </div>
              ) : null}
            </div>

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
