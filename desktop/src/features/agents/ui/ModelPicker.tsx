import { ChevronDown } from "lucide-react";

import { Spinner } from "@/shared/ui/spinner";
import React from "react";

import type {
  AgentModelsResponse,
  ManagedAgent,
  RuntimeConfigSurface,
} from "@/shared/api/types";
import {
  getAgentConfigSurface,
  getAgentModels,
  updateManagedAgent,
} from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

export function ModelPicker({
  agent,
  onModelChanged,
}: {
  agent: ManagedAgent;
  onModelChanged?: () => void;
}) {
  const [modelsData, setModelsData] =
    React.useState<AgentModelsResponse | null>(null);
  const [configSurface, setConfigSurface] =
    React.useState<RuntimeConfigSurface | null>(null);
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const [needsRestart, setNeedsRestart] = React.useState(false);
  const [hasRequestedModels, setHasRequestedModels] = React.useState(false);

  const fetchModels = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getAgentModels(agent.pubkey);
      setModelsData(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [agent.pubkey]);

  const handleOpenChange = React.useCallback(
    (open: boolean) => {
      if (!open || loading || modelsData) {
        return;
      }

      setHasRequestedModels(true);
      // Fetch config surface for model provenance data alongside the model list.
      // The config surface call is best-effort — a failure doesn't block model
      // selection, it just means we won't show the origin badge.
      void getAgentConfigSurface(agent.pubkey)
        .then((surface) => {
          if (!surface.isPreSpawn) {
            setConfigSurface(surface);
          }
        })
        .catch(() => {
          // Intentionally swallowed — provenance badge is informational only.
        });
      void fetchModels();
    },
    [agent.pubkey, fetchModels, loading, modelsData],
  );

  const currentValue = agent.model ?? modelsData?.agentDefaultModel ?? "";
  const displayLabel =
    agent.model ??
    (modelsData?.agentDefaultModel
      ? `${modelsData.agentDefaultModel} (default)`
      : hasRequestedModels && loading
        ? "Loading..."
        : "Auto");

  // Provenance label shown only for post-spawn agents where the model origin
  // is known from the config surface and the source is not a user-explicit
  // Buzz setting (which is already self-evident from the picker state).
  const modelOriginLabel = React.useMemo(() => {
    const origin = configSurface?.normalized.model?.origin;
    if (!origin || origin === "buzzExplicit") return null;
    const labels: Record<string, string> = {
      acpNativeRead: "from ACP",
      acpConfigOption: "from ACP config",
      envVar: "from env",
      configFile: "from config file",
      personaDefault: "persona default",
    };
    return labels[origin] ?? null;
  }, [configSurface]);

  const handleModelChange = async (modelId: string) => {
    setSaving(true);
    try {
      await updateManagedAgent({
        pubkey: agent.pubkey,
        model: modelId === modelsData?.agentDefaultModel ? null : modelId,
      });
      if (agent.status === "running" || agent.status === "deployed") {
        setNeedsRestart(true);
      }
      onModelChanged?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <span className="inline-flex items-center gap-1.5">
      <DropdownMenu modal={false} onOpenChange={handleOpenChange}>
        <DropdownMenuTrigger asChild>
          <Button
            className="h-7 max-w-full justify-start gap-1.5 rounded-full border border-border/50 bg-muted/45 px-2.5 text-xs font-medium text-foreground shadow-none hover:bg-muted/70"
            disabled={saving}
            size="sm"
            type="button"
            variant="ghost"
          >
            <span className="truncate">{displayLabel}</span>
            {modelOriginLabel ? (
              <span className="shrink-0 text-[10px] text-muted-foreground/70">
                ({modelOriginLabel})
              </span>
            ) : null}
            <ChevronDown className="h-3 w-3 text-muted-foreground" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          className="max-h-64 min-w-48 overflow-y-auto"
          onCloseAutoFocus={(event) => event.preventDefault()}
        >
          {loading ? (
            <div className="flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground">
              <Spinner className="h-3.5 w-3.5" />
              Loading models...
            </div>
          ) : error ? (
            <div className="space-y-2 px-3 py-2 text-sm">
              <p className="text-destructive">Failed to load models.</p>
              <button
                className="text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
                onClick={() => {
                  setHasRequestedModels(true);
                  void fetchModels();
                }}
                type="button"
              >
                Retry
              </button>
            </div>
          ) : !modelsData ? (
            <div className="px-3 py-2 text-sm text-muted-foreground">
              Open to load available models.
            </div>
          ) : !modelsData.supportsSwitching ? (
            <div className="px-3 py-2 text-sm text-muted-foreground">
              {agent.model ? (
                <>
                  <p className="font-medium text-foreground">{agent.model}</p>
                  <p className="mt-0.5 text-xs">
                    This runtime does not support switching models.
                  </p>
                </>
              ) : (
                "This agent uses the runtime's default model."
              )}
            </div>
          ) : (
            <DropdownMenuRadioGroup
              onValueChange={handleModelChange}
              value={currentValue}
            >
              {modelsData.models.map((model) => (
                <DropdownMenuRadioItem key={model.id} value={model.id}>
                  {model.name ?? model.id}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {needsRestart ? (
        <span className="text-[10px] text-warning">restart to apply</span>
      ) : null}
    </span>
  );
}
