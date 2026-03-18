import { ChevronDown, Loader2 } from "lucide-react";
import React from "react";

import type { AgentModelsResponse, ManagedAgent } from "@/shared/api/types";
import { getAgentModels, updateManagedAgent } from "@/shared/api/tauri";
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
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const [needsRestart, setNeedsRestart] = React.useState(false);

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

  const currentValue = agent.model ?? modelsData?.agentDefaultModel ?? "";
  const displayLabel =
    agent.model ??
    (modelsData?.agentDefaultModel
      ? `${modelsData.agentDefaultModel} (default)`
      : "Select model…");

  const handleModelChange = async (modelId: string) => {
    setSaving(true);
    try {
      await updateManagedAgent({
        pubkey: agent.pubkey,
        model: modelId === modelsData?.agentDefaultModel ? null : modelId,
      });
      if (agent.status === "running") {
        setNeedsRestart(true);
      }
      onModelChanged?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  if (!modelsData && !loading && !error) {
    return (
      <div className="rounded-2xl border border-border/60 bg-background/70 px-3 py-2">
        <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
          Model
        </p>
        <Button
          className="mt-1 h-7 px-2 text-sm"
          onClick={fetchModels}
          size="sm"
          type="button"
          variant="outline"
        >
          Discover models
        </Button>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="rounded-2xl border border-border/60 bg-background/70 px-3 py-2">
        <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
          Model
        </p>
        <div className="mt-1 flex items-center gap-1.5 text-sm text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          Discovering…
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-2xl border border-destructive/30 bg-destructive/10 px-3 py-2">
        <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-destructive">
          Model
        </p>
        <p className="mt-1 text-sm text-destructive">{error}</p>
        <Button
          className="mt-1 h-6 px-2 text-xs"
          onClick={fetchModels}
          size="sm"
          type="button"
          variant="outline"
        >
          Retry
        </Button>
      </div>
    );
  }

  if (!modelsData?.supportsSwitching) {
    return (
      <div className="rounded-2xl border border-border/60 bg-background/70 px-3 py-2">
        <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
          Model
        </p>
        <p className="mt-1 text-sm text-muted-foreground">Not configurable</p>
      </div>
    );
  }

  return (
    <div className="rounded-2xl border border-border/60 bg-background/70 px-3 py-2">
      <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
        Model
      </p>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger asChild>
          <Button
            className="mt-1 h-7 max-w-full justify-start gap-1.5 rounded-full border border-border/50 bg-muted/45 px-2.5 text-xs font-medium text-foreground shadow-none hover:bg-muted/70"
            disabled={saving}
            size="sm"
            type="button"
            variant="ghost"
          >
            <span className="truncate">{displayLabel}</span>
            <ChevronDown className="h-3 w-3 text-muted-foreground" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          className="max-h-64 min-w-48 overflow-y-auto"
          onCloseAutoFocus={(event) => event.preventDefault()}
        >
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
        </DropdownMenuContent>
      </DropdownMenu>
      {needsRestart ? (
        <p className="mt-1 text-[10px] text-amber-600 dark:text-amber-400">
          Restart agent to apply
        </p>
      ) : null}
    </div>
  );
}
