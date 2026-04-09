import { AlertTriangle, ChevronDown } from "lucide-react";
import * as React from "react";

import {
  useCreateChannelManagedAgentsMutation,
  usePersonasQuery,
  useTeamsQuery,
  type CreateChannelManagedAgentResult,
} from "@/features/agents/hooks";
import { useInChannelPersonaCounts } from "@/features/channels/ui/useInChannelPersonaIds";
import { AddChannelBotGenericSection } from "@/features/channels/ui/AddChannelBotGenericSection";
import { AddChannelBotPersonasSection } from "@/features/channels/ui/AddChannelBotPersonasSection";
import { AddChannelBotTeamsSection } from "@/features/channels/ui/AddChannelBotTeamsSection";
import { probeBackendProvider } from "@/shared/api/tauri";
import type {
  AcpProvider,
  BackendProviderCandidate,
  BackendProviderProbeResult,
  ManagedAgentBackend,
} from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Dialog } from "@/shared/ui/dialog";
import { ChooserDialogContent } from "@/shared/ui/chooser-dialog-content";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import {
  coerceConfigValues,
  ProviderConfigFields,
} from "@/features/agents/ui/ProviderConfigFields";
import {
  collectProviderWarnings,
  resolvePersonaProvider,
} from "@/features/agents/lib/resolvePersonaProvider";
import { getActivePersonas } from "@/features/agents/lib/catalog";
import { getUsableTeams } from "@/features/agents/lib/teamPersonas";

type AddChannelBotDialogProps = {
  backendProviders?: BackendProviderCandidate[];
  backendProvidersLoading?: boolean;
  channelId: string | null;
  open: boolean;
  providers: AcpProvider[];
  providersErrorMessage?: string | null;
  providersLoading?: boolean;
  onAdded?: (result: CreateChannelManagedAgentResult) => void;
  onOpenChange: (open: boolean) => void;
};

function defaultBotName(provider: AcpProvider | null) {
  if (!provider) return "";
  const id = provider.id.trim().toLowerCase();
  return id || provider.label.trim().toLowerCase() || "agent";
}

function formatBatchFailureSummary(
  failures: ReadonlyArray<{ name: string; error: string }>,
) {
  if (failures.length === 1) {
    const [failure] = failures;
    return `Failed to add ${failure.name}: ${failure.error}`;
  }

  return failures
    .map((failure) => `${failure.name}: ${failure.error}`)
    .join("; ");
}

export function AddChannelBotDialog({
  backendProviders,
  backendProvidersLoading,
  channelId,
  open,
  providers,
  providersErrorMessage,
  providersLoading = false,
  onAdded,
  onOpenChange,
}: AddChannelBotDialogProps) {
  const personasQuery = usePersonasQuery();
  const teamsQuery = useTeamsQuery();
  const inChannelPersonaCounts = useInChannelPersonaCounts(
    channelId,
    open && channelId !== null,
  );
  const createBotsMutation = useCreateChannelManagedAgentsMutation(channelId);
  const personas = React.useMemo(
    () => getActivePersonas(personasQuery.data ?? []),
    [personasQuery.data],
  );
  const teams = React.useMemo(
    () => getUsableTeams(teamsQuery.data ?? [], personas),
    [personas, teamsQuery.data],
  );
  const [selectedProviderId, setSelectedProviderId] = React.useState("");
  const [selectedPersonaCounts, setSelectedPersonaCounts] = React.useState<
    Map<string, number>
  >(() => new Map());
  const [includeGeneric, setIncludeGeneric] = React.useState(false);
  const [customName, setCustomName] = React.useState("");
  const [customPrompt, setCustomPrompt] = React.useState("");
  const [hasEditedCustomName, setHasEditedCustomName] = React.useState(false);
  const [submissionNotice, setSubmissionNotice] = React.useState<string | null>(
    null,
  );
  const [submissionError, setSubmissionError] = React.useState<string | null>(
    null,
  );

  const resolvedBackendProviders = backendProviders ?? [];
  const resolvedBackendProvidersLoading = backendProvidersLoading ?? false;

  const [runOn, setRunOn] = React.useState<"local" | string>("local");
  const [providerConfig, setProviderConfig] = React.useState<
    Record<string, string>
  >({});
  const [probedProvider, setProbedProvider] =
    React.useState<BackendProviderProbeResult | null>(null);
  const [probeError, setProbeError] = React.useState<string | null>(null);

  const selectedProvider = React.useMemo(
    () =>
      providers.find((provider) => provider.id === selectedProviderId) ??
      providers[0] ??
      null,
    [providers, selectedProviderId],
  );
  const selectedPersonas = React.useMemo(
    () =>
      personas.filter(
        (persona) => (selectedPersonaCounts.get(persona.id) ?? 0) > 0,
      ),
    [personas, selectedPersonaCounts],
  );
  const totalPersonaInstances = React.useMemo(() => {
    let total = 0;
    for (const count of selectedPersonaCounts.values()) {
      total += count;
    }
    return total;
  }, [selectedPersonaCounts]);
  const selectedCount = totalPersonaInstances + (includeGeneric ? 1 : 0);

  // Surface warnings when a persona's preferred provider differs from the
  // user-selected provider. In this dialog the user explicitly picks a
  // runtime via the dropdown, so the fallback is `selectedProvider` (their
  // choice), NOT `providers[0]`. This differs intentionally from
  // AddTeamToChannelDialog which has no provider selector and falls back
  // to the first available runtime.
  const providerWarnings = React.useMemo(
    () =>
      collectProviderWarnings(selectedPersonas, providers, selectedProvider),
    [selectedPersonas, providers, selectedProvider],
  );

  const isProviderMode = runOn !== "local";
  const selectedBackendProvider = React.useMemo(
    () => resolvedBackendProviders.find((p) => p.id === runOn) ?? null,
    [resolvedBackendProviders, runOn],
  );
  const providerConfigComplete = React.useMemo(() => {
    if (!isProviderMode || !probedProvider?.config_schema) return true;
    const schema = probedProvider.config_schema as Record<string, unknown>;
    const required: string[] = (schema?.required as string[] | undefined) ?? [];
    return required.every(
      (key) => (providerConfig[key] ?? "").trim().length > 0,
    );
  }, [isProviderMode, probedProvider, providerConfig]);

  React.useEffect(() => {
    if (!open) {
      return;
    }

    if (!selectedProviderId && providers[0]) {
      setSelectedProviderId(providers[0].id);
    }
  }, [open, providers, selectedProviderId]);

  React.useEffect(() => {
    if (!selectedProvider || hasEditedCustomName) {
      return;
    }

    setCustomName(defaultBotName(selectedProvider));
  }, [hasEditedCustomName, selectedProvider]);

  React.useEffect(() => {
    const validIds = new Set(personas.map((p) => p.id));
    setSelectedPersonaCounts((current) => {
      const next = new Map<string, number>();
      for (const [id, count] of current) {
        if (validIds.has(id)) {
          next.set(id, count);
        }
      }
      return next.size === current.size ? current : next;
    });
  }, [personas]);

  React.useEffect(() => {
    if (!isProviderMode || !selectedBackendProvider) {
      setProbedProvider(null);
      setProbeError(null);
      return;
    }

    let cancelled = false;
    setProbeError(null);
    setProbedProvider(null);

    probeBackendProvider(selectedBackendProvider.binaryPath)
      .then((result) => {
        if (!cancelled) {
          setProbedProvider(result);
          if (result.config_schema) {
            const props =
              (result.config_schema as Record<string, unknown>)?.properties ??
              {};

            const defaults: Record<string, string> = {};
            for (const [key, prop] of Object.entries(props) as [
              string,
              Record<string, unknown>,
            ][]) {
              if (prop.default != null) {
                defaults[key] = String(prop.default);
              }
            }
            setProviderConfig(defaults);
          }
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setProbeError(err instanceof Error ? err.message : String(err));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [isProviderMode, selectedBackendProvider]);

  function reset() {
    setSelectedProviderId(providers[0]?.id ?? "");
    setSelectedPersonaCounts(new Map());
    setIncludeGeneric(false);
    setCustomName(providers[0] ? defaultBotName(providers[0]) : "");
    setCustomPrompt("");
    setHasEditedCustomName(false);
    setSubmissionNotice(null);
    setSubmissionError(null);
    setRunOn("local");
    setProviderConfig({});
    setProbedProvider(null);
    setProbeError(null);
    createBotsMutation.reset();
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      reset();
    }

    onOpenChange(next);
  }

  function handleToggleTeam(personaIds: string[]) {
    setSelectedPersonaCounts((current) => {
      const allSelected = personaIds.every((id) => (current.get(id) ?? 0) > 0);
      const next = new Map(current);
      if (allSelected) {
        for (const id of personaIds) {
          next.delete(id);
        }
      } else {
        for (const id of personaIds) {
          if (!next.has(id) || next.get(id) === 0) {
            next.set(id, 1);
          }
        }
      }
      return next;
    });
    setSubmissionNotice(null);
    setSubmissionError(null);
  }

  function handleRunOnChange(value: string) {
    setRunOn(value);
    setProviderConfig({});
    setProbedProvider(null);
    setProbeError(null);
    setSubmissionNotice(null);
    setSubmissionError(null);
  }

  async function handleSubmit() {
    if (!selectedProvider || selectedCount === 0) {
      return;
    }

    const backend: ManagedAgentBackend = isProviderMode
      ? {
          type: "provider",
          id: runOn,
          config: coerceConfigValues(
            providerConfig,
            probedProvider?.config_schema,
          ),
        }
      : { type: "local" };

    const inputs = [
      ...(includeGeneric
        ? [
            {
              provider: selectedProvider,
              name: customName,
              systemPrompt: customPrompt,
              role: "bot" as const,
              backend,
            },
          ]
        : []),
      ...selectedPersonas.flatMap((persona) => {
        const count = selectedPersonaCounts.get(persona.id) ?? 1;
        const resolved = resolvePersonaProvider(
          persona.provider,
          providers,
          selectedProvider,
        );
        return Array.from({ length: count }, (_, i) => ({
          provider: resolved.provider ?? selectedProvider,
          name:
            count > 1
              ? `${persona.displayName}::${String(i + 1).padStart(2, "0")}`
              : persona.displayName,
          personaId: persona.id,
          systemPrompt: persona.systemPrompt,
          avatarUrl: persona.avatarUrl ?? undefined,
          model: persona.model ?? undefined,
          role: "bot" as const,
          backend,
        }));
      }),
    ];

    setSubmissionNotice(null);
    setSubmissionError(null);

    try {
      const result = await createBotsMutation.mutateAsync(inputs);

      if (result.failures.length === 0) {
        if (result.successes[0]) {
          onAdded?.(result.successes[0]);
        }
        handleOpenChange(false);
        return;
      }

      const failedPersonaIds = new Set(
        result.failures
          .map((failure) => failure.personaId)
          .filter((personaId): personaId is string => Boolean(personaId)),
      );
      setSelectedPersonaCounts((current) => {
        const next = new Map<string, number>();
        for (const [id, count] of current) {
          if (failedPersonaIds.has(id)) {
            next.set(id, count);
          }
        }
        return next;
      });
      setIncludeGeneric(
        result.failures.some((failure) => failure.kind === "generic"),
      );

      if (result.successes.length > 0) {
        const n = result.successes.length;
        setSubmissionNotice(`Added ${n} ${n === 1 ? "agent" : "agents"}.`);
      }

      setSubmissionError(formatBatchFailureSummary(result.failures));
    } catch {
      // The mutation error is rendered inline.
    }
  }

  const canSubmit =
    selectedProvider !== null &&
    selectedCount > 0 &&
    (!includeGeneric || customName.trim().length > 0) &&
    !(isProviderMode && !probedProvider) &&
    providerConfigComplete &&
    !providersLoading &&
    !(isProviderMode && resolvedBackendProvidersLoading) &&
    !createBotsMutation.isPending;
  const canChooseProvider =
    providers.length > 0 && !providersLoading && !createBotsMutation.isPending;
  const canToggleSelections = !createBotsMutation.isPending;
  const providerTriggerLabel = providersLoading
    ? "Loading runtimes..."
    : (selectedProvider?.label ?? "No runtimes found");
  const addButtonLabel = createBotsMutation.isPending
    ? selectedCount > 1
      ? `Adding ${selectedCount}...`
      : "Adding..."
    : selectedCount > 1
      ? `Add ${selectedCount} agents`
      : "Add agent";

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <ChooserDialogContent
        className="max-w-3xl"
        data-testid="add-channel-bot-dialog"
        description="Select any combination of saved personas, or turn on Generic for a one-off custom agent."
        footer={
          <>
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
              {addButtonLabel}
            </Button>
          </>
        }
        footerClassName="justify-end gap-2"
        footerTestId="add-channel-bot-dialog-footer"
        headerTestId="add-channel-bot-dialog-header"
        scrollAreaClassName="space-y-5"
        scrollAreaTestId="add-channel-bot-dialog-scroll-area"
        title="Add agents"
      >
        {resolvedBackendProviders.length > 0 ? (
          <div className="space-y-1.5">
            <div className="text-sm font-medium">Run on</div>
            <select
              className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm"
              disabled={createBotsMutation.isPending}
              onChange={(e) => handleRunOnChange(e.target.value)}
              value={runOn}
            >
              <option value="local">This computer</option>
              {resolvedBackendProviders.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.id}
                </option>
              ))}
            </select>
          </div>
        ) : null}

        {isProviderMode && selectedBackendProvider ? (
          <div className="flex gap-3 rounded-2xl border border-warning/30 bg-warning-bg px-4 py-3">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
            <p className="text-sm text-warning">
              This provider at{" "}
              <span className="font-mono font-medium">
                {selectedBackendProvider.binaryPath}
              </span>{" "}
              will receive your agent&apos;s private key. Only use providers
              from trusted sources.
            </p>
          </div>
        ) : null}

        {probeError ? (
          <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            Could not probe provider: {probeError}
          </p>
        ) : null}

        {isProviderMode && probedProvider?.config_schema ? (
          <ProviderConfigFields
            config={providerConfig}
            onChange={setProviderConfig}
            schema={probedProvider.config_schema}
          />
        ) : null}

        <div className="space-y-1.5">
          <div className="text-sm font-medium">Runtime</div>
          <DropdownMenu modal={false}>
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
            <DropdownMenuContent
              align="start"
              className="min-w-40"
              onCloseAutoFocus={(event) => event.preventDefault()}
            >
              <DropdownMenuRadioGroup
                onValueChange={setSelectedProviderId}
                value={selectedProvider?.id ?? ""}
              >
                {providers.map((provider) => (
                  <DropdownMenuRadioItem key={provider.id} value={provider.id}>
                    {provider.label}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        {teams.length > 0 ? (
          <AddChannelBotTeamsSection
            canToggleSelections={canToggleSelections}
            inChannelPersonaCounts={inChannelPersonaCounts}
            isLoading={teamsQuery.isLoading}
            onToggleTeam={handleToggleTeam}
            personas={personas}
            selectedPersonaCounts={selectedPersonaCounts}
            teams={teams}
          />
        ) : null}

        <AddChannelBotPersonasSection
          canToggleSelections={canToggleSelections}
          inChannelPersonaCounts={inChannelPersonaCounts}
          includeGeneric={includeGeneric}
          isLoading={personasQuery.isLoading}
          onToggleGeneric={() => {
            setIncludeGeneric((current) => !current);
            setSubmissionNotice(null);
            setSubmissionError(null);
          }}
          onSetPersonaCount={(personaId, count) => {
            setSelectedPersonaCounts((current) => {
              const next = new Map(current);
              if (count <= 0) {
                next.delete(personaId);
              } else {
                next.set(personaId, count);
              }
              return next;
            });
            setSubmissionNotice(null);
            setSubmissionError(null);
          }}
          personas={personas}
          selectedPersonaCounts={selectedPersonaCounts}
        />

        {includeGeneric ? (
          <AddChannelBotGenericSection
            disabled={createBotsMutation.isPending}
            name={customName}
            onNameChange={(value) => {
              setHasEditedCustomName(true);
              setCustomName(value);
            }}
            onPromptChange={setCustomPrompt}
            prompt={customPrompt}
          />
        ) : null}

        {selectedCount === 0 ? (
          <div className="rounded-2xl border border-dashed border-border/70 bg-muted/15 px-4 py-4 text-sm text-muted-foreground">
            Pick one or more personas, or enable Generic to add a custom agent.
          </div>
        ) : null}

        {providersErrorMessage ? (
          <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {providersErrorMessage}
          </p>
        ) : null}

        {providerWarnings.length > 0
          ? providerWarnings.map((warning) => (
              <div
                className="flex gap-3 rounded-2xl border border-warning/30 bg-warning-bg px-4 py-3"
                key={warning}
              >
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
                <p className="text-sm text-warning">{warning}</p>
              </div>
            ))
          : null}

        {personasQuery.error instanceof Error ? (
          <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {personasQuery.error.message}
          </p>
        ) : null}

        {submissionNotice ? (
          <p className="rounded-2xl border border-border/70 bg-muted/25 px-4 py-3 text-sm text-foreground">
            {submissionNotice}
          </p>
        ) : null}

        {submissionError ? (
          <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {submissionError}
          </p>
        ) : null}

        {createBotsMutation.error instanceof Error ? (
          <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {createBotsMutation.error.message}
          </p>
        ) : null}
      </ChooserDialogContent>
    </Dialog>
  );
}
