import * as React from "react";

import { useUpdateManagedAgentMutation } from "@/features/agents/hooks";
import type { ManagedAgent, UpdateManagedAgentInput } from "@/shared/api/types";
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
  CreateAgentRuntimeFields,
} from "./CreateAgentDialogSections";

export function EditAgentDialog({
  agent,
  open,
  onOpenChange,
  onUpdated,
}: {
  agent: ManagedAgent;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onUpdated?: (agent: ManagedAgent) => void;
}) {
  const updateMutation = useUpdateManagedAgentMutation();

  const [name, setName] = React.useState(agent.name);
  const [relayUrl, setRelayUrl] = React.useState(agent.relayUrl);
  const [acpCommand, setAcpCommand] = React.useState(agent.acpCommand);
  const [agentCommand, setAgentCommand] = React.useState(agent.agentCommand);
  const [agentArgs, setAgentArgs] = React.useState(agent.agentArgs.join(","));
  const [mcpCommand, setMcpCommand] = React.useState(agent.mcpCommand);
  const [turnTimeoutSeconds, setTurnTimeoutSeconds] = React.useState(
    String(agent.turnTimeoutSeconds),
  );
  const [parallelism, setParallelism] = React.useState(
    String(agent.parallelism),
  );
  const [systemPrompt, setSystemPrompt] = React.useState(
    agent.systemPrompt ?? "",
  );

  // Reset form state when the dialog opens with a (possibly different) agent.
  React.useEffect(() => {
    if (open) {
      setName(agent.name);
      setRelayUrl(agent.relayUrl);
      setAcpCommand(agent.acpCommand);
      setAgentCommand(agent.agentCommand);
      setAgentArgs(agent.agentArgs.join(","));
      setMcpCommand(agent.mcpCommand);
      setTurnTimeoutSeconds(String(agent.turnTimeoutSeconds));
      setParallelism(String(agent.parallelism));
      setSystemPrompt(agent.systemPrompt ?? "");
      updateMutation.reset();
    }
  }, [
    open,
    agent,
    agent.name,
    agent.relayUrl,
    agent.acpCommand,
    agent.agentCommand,
    agent.agentArgs,
    agent.mcpCommand,
    agent.turnTimeoutSeconds,
    agent.parallelism,
    agent.systemPrompt,
    updateMutation.reset,
  ]);

  function handleOpenChange(next: boolean) {
    onOpenChange(next);
  }

  const canSubmit = name.trim().length > 0 && !updateMutation.isPending;

  async function handleSubmit() {
    try {
      const parsedParallelism = Number.parseInt(parallelism, 10);
      const parsedTimeout = Number.parseInt(turnTimeoutSeconds, 10);
      const parsedArgs = agentArgs
        .split(",")
        .map((v) => v.trim())
        .filter((v) => v.length > 0);

      const input: UpdateManagedAgentInput = {
        pubkey: agent.pubkey,
        name: name.trim() !== agent.name ? name.trim() : undefined,
        relayUrl: relayUrl !== agent.relayUrl ? relayUrl : undefined,
        acpCommand:
          acpCommand.trim() !== agent.acpCommand
            ? acpCommand.trim()
            : undefined,
        agentCommand:
          agentCommand.trim() !== agent.agentCommand
            ? agentCommand.trim()
            : undefined,
        agentArgs:
          parsedArgs.join(",") !== agent.agentArgs.join(",")
            ? parsedArgs
            : undefined,
        mcpCommand:
          mcpCommand.trim() !== agent.mcpCommand
            ? mcpCommand.trim()
            : undefined,
        turnTimeoutSeconds:
          parsedTimeout > 0 && parsedTimeout !== agent.turnTimeoutSeconds
            ? parsedTimeout
            : undefined,
        parallelism:
          parsedParallelism > 0 && parsedParallelism !== agent.parallelism
            ? parsedParallelism
            : undefined,
        // Use tri-state: send null to clear, value to set, omit if unchanged.
        systemPrompt:
          (systemPrompt.trim() || null) !== agent.systemPrompt
            ? systemPrompt.trim() || null
            : undefined,
      };

      const updated = await updateMutation.mutateAsync(input);
      handleOpenChange(false);
      onUpdated?.(updated);
    } catch {
      // React Query stores the error; keep dialog open and render it inline.
    }
  }

  return (
    <Dialog onOpenChange={handleOpenChange} open={open}>
      <DialogContent className="max-w-3xl overflow-hidden p-0">
        <div className="flex max-h-[85vh] flex-col">
          <DialogHeader className="shrink-0 border-b border-border/60 px-6 py-5 pr-14">
            <DialogTitle>Edit agent</DialogTitle>
            <DialogDescription>
              Update configuration for{" "}
              <span className="font-medium">{agent.name}</span>. Changes take
              effect on the next start.
            </DialogDescription>
          </DialogHeader>

          <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-6 py-5">
            <CreateAgentBasicsFields name={name} onNameChange={setName} />

            <CreateAgentRuntimeFields
              acpCommand={acpCommand}
              agentArgs={agentArgs}
              agentCommand={agentCommand}
              mcpCommand={mcpCommand}
              onAcpCommandChange={setAcpCommand}
              onAgentArgsChange={setAgentArgs}
              onAgentCommandChange={setAgentCommand}
              onMcpCommandChange={setMcpCommand}
              onParallelismChange={setParallelism}
              onRelayUrlChange={setRelayUrl}
              onSystemPromptChange={setSystemPrompt}
              onTurnTimeoutChange={setTurnTimeoutSeconds}
              parallelism={parallelism}
              relayUrl={relayUrl}
              selectedProviderId="custom"
              systemPrompt={systemPrompt}
              turnTimeoutSeconds={turnTimeoutSeconds}
            />

            {updateMutation.error instanceof Error ? (
              <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {updateMutation.error.message}
              </p>
            ) : null}
          </div>

          <div className="flex shrink-0 justify-end gap-2 border-t border-border/60 px-6 py-4">
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
              {updateMutation.isPending ? "Saving..." : "Save changes"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
