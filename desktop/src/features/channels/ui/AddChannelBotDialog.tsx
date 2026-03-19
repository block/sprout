import { ChevronDown } from "lucide-react";
import * as React from "react";

import {
  useCreateChannelManagedAgentsMutation,
  usePersonasQuery,
  type CreateChannelManagedAgentResult,
} from "@/features/agents/hooks";
import { AddChannelBotGenericSection } from "@/features/channels/ui/AddChannelBotGenericSection";
import { AddChannelBotPersonasSection } from "@/features/channels/ui/AddChannelBotPersonasSection";
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

function toggleValue(values: readonly string[], value: string) {
  return values.includes(value)
    ? values.filter((candidate) => candidate !== value)
    : [...values, value];
}

function formatAgentCountLabel(count: number) {
  return count === 1 ? "agent" : "agents";
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
  channelId,
  open,
  providers,
  providersErrorMessage,
  providersLoading = false,
  onAdded,
  onOpenChange,
}: AddChannelBotDialogProps) {
  const personasQuery = usePersonasQuery();
  const createBotsMutation = useCreateChannelManagedAgentsMutation(channelId);
  const personas = personasQuery.data ?? [];
  const [selectedProviderId, setSelectedProviderId] = React.useState("");
  const [selectedPersonaIds, setSelectedPersonaIds] = React.useState<string[]>(
    [],
  );
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

  const selectedProvider = React.useMemo(
    () =>
      providers.find((provider) => provider.id === selectedProviderId) ??
      providers[0] ??
      null,
    [providers, selectedProviderId],
  );
  const selectedPersonas = React.useMemo(
    () => personas.filter((persona) => selectedPersonaIds.includes(persona.id)),
    [personas, selectedPersonaIds],
  );
  const selectedCount = selectedPersonas.length + (includeGeneric ? 1 : 0);

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
    setSelectedPersonaIds((current) =>
      current.filter((id) => personas.some((persona) => persona.id === id)),
    );
  }, [personas]);

  function reset() {
    setSelectedProviderId(providers[0]?.id ?? "");
    setSelectedPersonaIds([]);
    setIncludeGeneric(false);
    setCustomName(providers[0] ? defaultBotName(providers[0]) : "");
    setCustomPrompt("");
    setHasEditedCustomName(false);
    setSubmissionNotice(null);
    setSubmissionError(null);
    createBotsMutation.reset();
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      reset();
    }

    onOpenChange(next);
  }

  async function handleSubmit() {
    if (!selectedProvider || selectedCount === 0) {
      return;
    }

    const inputs = [
      ...(includeGeneric
        ? [
            {
              provider: selectedProvider,
              name: customName,
              systemPrompt: customPrompt,
              role: "bot" as const,
            },
          ]
        : []),
      ...selectedPersonas.map((persona) => ({
        provider: selectedProvider,
        name: persona.displayName,
        personaId: persona.id,
        systemPrompt: persona.systemPrompt,
        avatarUrl: persona.avatarUrl ?? undefined,
        role: "bot" as const,
      })),
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
      setSelectedPersonaIds((current) =>
        current.filter((personaId) => failedPersonaIds.has(personaId)),
      );
      setIncludeGeneric(
        result.failures.some((failure) => failure.kind === "generic"),
      );

      if (result.successes.length > 0) {
        setSubmissionNotice(
          `Added ${result.successes.length} ${formatAgentCountLabel(
            result.successes.length,
          )}.`,
        );
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
    !providersLoading &&
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
      <DialogContent className="max-w-3xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Add agents</DialogTitle>
            <DialogDescription>
              Select any combination of saved personas, or turn on Generic for a
              one-off custom agent.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-5 px-6 py-5">
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

            <AddChannelBotPersonasSection
              canToggleSelections={canToggleSelections}
              includeGeneric={includeGeneric}
              isLoading={personasQuery.isLoading}
              onToggleGeneric={() => {
                setIncludeGeneric((current) => !current);
                setSubmissionNotice(null);
                setSubmissionError(null);
              }}
              onTogglePersona={(personaId) => {
                setSelectedPersonaIds((current) =>
                  toggleValue(current, personaId),
                );
                setSubmissionNotice(null);
                setSubmissionError(null);
              }}
              personas={personas}
              selectedPersonaIds={selectedPersonaIds}
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
                Pick one or more personas, or enable Generic to add a custom
                agent.
              </div>
            ) : null}

            {providersErrorMessage ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {providersErrorMessage}
              </p>
            ) : null}

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
              disabled={!canSubmit}
              onClick={() => void handleSubmit()}
              size="sm"
              type="button"
            >
              {addButtonLabel}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
