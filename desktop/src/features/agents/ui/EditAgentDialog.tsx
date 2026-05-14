import * as React from "react";
import { toast } from "sonner";

import { useUpdateManagedAgentMutation } from "@/features/agents/hooks";
import { useAgentProviderSettingsStateQuery } from "@/features/settings/hooks/useAgentProviderSettings";
import type {
  ManagedAgent,
  RespondToMode,
  UpdateManagedAgentInput,
} from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { resolveAcpProviderId } from "@/features/agents/lib/resolveAcpProviderId";
import {
  CreateAgentBasicsFields,
  CreateAgentRuntimeFields,
} from "./CreateAgentDialogSections";
import { CreateAgentRespondToField } from "./RespondToField";
import { SproutAgentProfilePicker } from "./SproutAgentProfilePicker";

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
  const [mcpToolsets, setMcpToolsets] = React.useState(agent.mcpToolsets ?? "");
  const [turnTimeoutSeconds, setTurnTimeoutSeconds] = React.useState(
    String(agent.turnTimeoutSeconds),
  );
  const [parallelism, setParallelism] = React.useState(
    String(agent.parallelism),
  );
  const [systemPrompt, setSystemPrompt] = React.useState(
    agent.systemPrompt ?? "",
  );
  const [respondTo, setRespondTo] = React.useState<RespondToMode>(
    agent.respondTo,
  );
  const [respondToAllowlist, setRespondToAllowlist] = React.useState<string[]>(
    agent.respondToAllowlist,
  );
  const [providerProfileId, setProviderProfileId] = React.useState<
    string | null
  >(agent.providerProfileId);

  // Reset form state only when the dialog opens or when switching to a different
  // agent. Omitting the full agent object and its array fields from deps prevents
  // the effect from firing on every 5s background poll (arrays are never
  // reference-equal across renders), which would wipe in-progress user edits.
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional — including agent fields would re-fire on every 5s poll and wipe edits
  React.useEffect(() => {
    if (open) {
      setName(agent.name);
      setRelayUrl(agent.relayUrl);
      setAcpCommand(agent.acpCommand);
      setAgentCommand(agent.agentCommand);
      setAgentArgs(agent.agentArgs.join(","));
      setMcpCommand(agent.mcpCommand);
      setMcpToolsets(agent.mcpToolsets ?? "");
      setTurnTimeoutSeconds(String(agent.turnTimeoutSeconds));
      setParallelism(String(agent.parallelism));
      setSystemPrompt(agent.systemPrompt ?? "");
      setRespondTo(agent.respondTo);
      setRespondToAllowlist(agent.respondToAllowlist);
      setProviderProfileId(agent.providerProfileId);
      updateMutation.reset();
    }
  }, [open, agent.pubkey]);

  function handleOpenChange(next: boolean) {
    onOpenChange(next);
  }

  // Resolve the ACP provider id from the live agent record's command so the
  // shared `CreateAgentRuntimeFields` knows whether to hide the per-agent
  // System prompt / Model fields (those are global for sprout-agent and live
  // in Settings &rsaquo; Agent Provider).
  const resolvedProviderId = React.useMemo(
    () => resolveAcpProviderId(agentCommand) ?? "custom",
    [agentCommand],
  );
  const isSproutAgent = resolvedProviderId === "sprout-agent";

  // If the agent is pinned to a provider profile that no longer exists in
  // the saved settings AND there's a valid default profile to fall back
  // to, treat any save as an implicit normalization to null (use default).
  //
  // We *don't* auto-clear when there is no valid default: that would
  // silently turn "missing profile (spawn will fail)" into "use default
  // (spawn will also fail, but with a less honest UI)". In that case the
  // user has to explicitly pick a profile in the picker, and the missing
  // warning stays visible.
  //
  // We also only act when the settings query is `ok` — a transient load
  // error / identity mismatch must not clobber a pin.
  const settingsStateQuery = useAgentProviderSettingsStateQuery();
  const pinnedProfileIsMissing = React.useMemo(() => {
    if (!isSproutAgent) return false;
    if (providerProfileId === null) return false;
    const data = settingsStateQuery.data;
    if (!data || data.status !== "ok") return false;
    return !data.profiles.some((p) => p.id === providerProfileId);
  }, [isSproutAgent, providerProfileId, settingsStateQuery.data]);
  const validDefaultExists = React.useMemo(() => {
    const data = settingsStateQuery.data;
    if (!data || data.status !== "ok") return false;
    const id = data.defaultProfileId;
    if (id === null) return false;
    return data.profiles.some((p) => p.id === id);
  }, [settingsStateQuery.data]);
  const autoClearPinOnSave = pinnedProfileIsMissing && validDefaultExists;

  const parallelismValid =
    parallelism.trim() === "" ||
    !Number.isNaN(Number.parseInt(parallelism, 10));
  const timeoutValid =
    turnTimeoutSeconds.trim() === "" ||
    !Number.isNaN(Number.parseInt(turnTimeoutSeconds, 10));
  // Block clearing a previously-set command to empty — the backend has no
  // "clear to None" path for Option<String> fields, so an empty string would
  // cause a runtime failure.
  const acpCommandValid = !(agent.acpCommand && acpCommand.trim() === "");
  const mcpCommandValid = !(agent.mcpCommand && mcpCommand.trim() === "");
  // Allowlist mode requires at least one entry — mirrors the harness's own
  // validation. The backend would reject the request anyway; we block early
  // so the user sees the disabled button instead of a round-tripped error.
  const respondToValid =
    respondTo !== "allowlist" || respondToAllowlist.length > 0;

  // Block save when the row would be a known-unstartable sprout-agent: any
  // case where we can't resolve a profile at spawn time. That's both the
  // "pin missing AND no valid default" branch and the "no pin AND no valid
  // default" branch (the latter mirrors CreateAgentDialog's gate). Edit
  // would otherwise silently persist a config that can never spawn.
  //
  // Only fires when settings are `ok` — load/error/identity-mismatch are
  // transient and shouldn't wedge unrelated edits.
  const providerPinValid = React.useMemo(() => {
    if (!isSproutAgent) return true;
    const data = settingsStateQuery.data;
    if (!data || data.status !== "ok") return true;
    if (providerProfileId === null) return validDefaultExists;
    return !pinnedProfileIsMissing || validDefaultExists;
  }, [
    isSproutAgent,
    pinnedProfileIsMissing,
    providerProfileId,
    settingsStateQuery.data,
    validDefaultExists,
  ]);

  const canSubmit =
    name.trim().length > 0 &&
    parallelismValid &&
    timeoutValid &&
    acpCommandValid &&
    mcpCommandValid &&
    respondToValid &&
    providerPinValid &&
    !updateMutation.isPending;

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
        relayUrl:
          relayUrl.trim() !== agent.relayUrl ? relayUrl.trim() : undefined,
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
        mcpToolsets:
          (mcpToolsets.trim() || null) !== agent.mcpToolsets
            ? mcpToolsets.trim() || null
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
        // For sprout-agent, the System prompt is managed globally in
        // Settings > Agent Provider — never carry per-agent values into the
        // mutation even if stale form state still has them.
        systemPrompt: isSproutAgent
          ? agent.systemPrompt !== null
            ? null
            : undefined
          : (systemPrompt.trim() || null) !== agent.systemPrompt
            ? systemPrompt.trim() || null
            : undefined,
        respondTo: respondTo !== agent.respondTo ? respondTo : undefined,
        // The allowlist is preserved across mode toggles in local UI state
        // (so a user can flip away from allowlist and back without losing
        // their entries), but we only send it on the wire when (a) it
        // actually changed, AND (b) the saved mode will need it. Sending
        // an allowlist while switching to a non-allowlist mode would be
        // harmless server-side, but it's noise in the persisted record.
        respondToAllowlist:
          respondTo === "allowlist" &&
          respondToAllowlist.join(",") !== agent.respondToAllowlist.join(",")
            ? respondToAllowlist
            : undefined,
        // Tri-state: only meaningful for sprout-agent rows. For non-sprout
        // agents we never send it (the backend ignores it but it's noise).
        // Normalize a dangling pin (referenced profile was deleted) to
        // `null` on any save, so editing an unrelated field doesn't leave
        // the agent permanently broken at spawn.
        providerProfileId: isSproutAgent
          ? autoClearPinOnSave
            ? null
            : providerProfileId !== agent.providerProfileId
              ? providerProfileId
              : undefined
          : undefined,
      };

      const result = await updateMutation.mutateAsync(input);
      if (result.profileSyncError) {
        console.warn("Relay profile sync failed:", result.profileSyncError);
      }
      if (autoClearPinOnSave) {
        toast.warning(
          "Cleared a pinned profile that no longer exists. This agent now uses the default.",
        );
      }
      handleOpenChange(false);
      onUpdated?.(result.agent);
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

            <CreateAgentRespondToField
              allowlist={respondToAllowlist}
              mode={respondTo}
              onAllowlistChange={setRespondToAllowlist}
              onModeChange={setRespondTo}
            />

            <CreateAgentRuntimeFields
              acpCommand={acpCommand}
              agentArgs={agentArgs}
              agentCommand={agentCommand}
              mcpCommand={mcpCommand}
              mcpToolsets={mcpToolsets}
              onAcpCommandChange={setAcpCommand}
              onAgentArgsChange={setAgentArgs}
              onAgentCommandChange={setAgentCommand}
              onMcpCommandChange={setMcpCommand}
              onMcpToolsetsChange={setMcpToolsets}
              onParallelismChange={setParallelism}
              onRelayUrlChange={setRelayUrl}
              onSystemPromptChange={setSystemPrompt}
              onTurnTimeoutChange={setTurnTimeoutSeconds}
              parallelism={parallelism}
              relayUrl={relayUrl}
              // Always treat Edit as the "custom" path so the agent command
              // input stays visible — existing Goose/Codex/Claude/sprout-agent
              // rows need to be editable (e.g. to correct a stale binary
              // path). The sprout-agent system-prompt hide still works because
              // `isSproutAgentPath` also matches on the agentCommand value
              // itself, not only on selectedProviderId.
              selectedProviderId="custom"
              systemPrompt={systemPrompt}
              turnTimeoutSeconds={turnTimeoutSeconds}
            />

            {isSproutAgent ? (
              <SproutAgentProfilePicker
                onChange={setProviderProfileId}
                value={providerProfileId}
              />
            ) : null}

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
