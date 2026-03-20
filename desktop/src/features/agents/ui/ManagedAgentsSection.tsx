import {
  Clipboard,
  Ellipsis,
  FileText,
  KeyRound,
  Play,
  Plus,
  Power,
  Square,
  Trash2,
  UserPlus,
} from "lucide-react";

import type { ManagedAgent, PresenceLookup } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { PresenceDot } from "@/features/presence/ui/PresenceBadge";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Skeleton } from "@/shared/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import { ModelPicker } from "./ModelPicker";
import { truncatePubkey } from "./agentUi";

export function ManagedAgentsSection({
  actionErrorMessage,
  actionNoticeMessage,
  agents,
  error,
  isActionPending,
  isLoading,
  personaLabelsById,
  presenceLookup,
  onAddToChannel,
  onCreate,
  onDelete,
  onMintToken,
  onStart,
  onStop,
  onToggleStartOnAppLaunch,
  onViewLogs,
}: {
  actionErrorMessage: string | null;
  actionNoticeMessage: string | null;
  agents: ManagedAgent[];
  error: Error | null;
  isActionPending: boolean;
  isLoading: boolean;
  personaLabelsById: Record<string, string>;
  presenceLookup: PresenceLookup;
  onAddToChannel: (agent: ManagedAgent) => void;
  onCreate: () => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
  onViewLogs: (pubkey: string) => void;
}) {
  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold tracking-tight">
            Managed agents
          </h3>
          <p className="text-sm text-muted-foreground">
            Agent profiles and process state — local and remote.
          </p>
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              aria-label="Create agent"
              onClick={onCreate}
              type="button"
              variant="ghost"
              size="icon"
            >
              <Plus className="h-4 w-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Create agent</TooltipContent>
        </Tooltip>
      </div>

      {isLoading ? (
        <div className="overflow-hidden rounded-xl border border-border/70 bg-card/80 shadow-sm">
          {["first", "second"].map((key) => (
            <div
              className="flex items-center gap-4 border-b border-border/60 px-4 py-3 last:border-b-0"
              key={key}
            >
              <Skeleton className="h-4 w-28" />
              <Skeleton className="h-5 w-16 rounded-full" />
              <Skeleton className="h-4 w-24" />
              <Skeleton className="h-4 w-20" />
            </div>
          ))}
        </div>
      ) : null}

      {!isLoading && agents.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center">
          <p className="text-sm font-semibold tracking-tight">
            No local agents yet
          </p>
          <p className="mt-2 text-sm text-muted-foreground">
            Create one to generate a keypair, mint a token, and launch the ACP
            harness from the desktop app.
          </p>
        </div>
      ) : null}

      {!isLoading && agents.length > 0 ? (
        <div className="overflow-hidden rounded-xl border border-border/70 bg-card/80 shadow-sm">
          <div className="overflow-x-auto">
            <table
              className="w-full border-collapse text-left text-sm"
              data-testid="managed-agents-table"
            >
              <thead className="bg-muted/35 text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                <tr>
                  <th className="px-4 py-3">Agent</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Model</th>
                  <th className="px-4 py-3">Runtime</th>
                  <th className="w-10 px-3 py-3" />
                </tr>
              </thead>
              <tbody>
                {agents.map((agent) => (
                  <ManagedAgentRow
                    agent={agent}
                    isActionPending={isActionPending}
                    key={agent.pubkey}
                    personaLabelsById={personaLabelsById}
                    presenceLookup={presenceLookup}
                    onAddToChannel={onAddToChannel}
                    onDelete={onDelete}
                    onMintToken={onMintToken}
                    onStart={onStart}
                    onStop={onStop}
                    onToggleStartOnAppLaunch={onToggleStartOnAppLaunch}
                    onViewLogs={onViewLogs}
                  />
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}

      {error ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error.message}
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
  );
}

function ManagedAgentRow({
  agent,
  isActionPending,
  personaLabelsById,
  presenceLookup,
  onAddToChannel,
  onDelete,
  onMintToken,
  onStart,
  onStop,
  onToggleStartOnAppLaunch,
  onViewLogs,
}: {
  agent: ManagedAgent;
  isActionPending: boolean;
  personaLabelsById: Record<string, string>;
  presenceLookup: PresenceLookup;
  onAddToChannel: (agent: ManagedAgent) => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
  onViewLogs: (pubkey: string) => void;
}) {
  const isActive = agent.status === "running" || agent.status === "deployed";
  const personaLabel = agent.personaId
    ? (personaLabelsById[agent.personaId] ?? null)
    : null;
  const presenceStatus = presenceLookup[agent.pubkey.trim().toLowerCase()];

  return (
    <tr
      className={cn(
        "border-b border-border/60 last:border-b-0 hover:bg-muted/30",
        agent.backend.type === "local" && "cursor-pointer",
      )}
      data-testid={`managed-agent-${agent.pubkey}`}
      onClick={() => {
        if (agent.backend.type === "local") {
          onViewLogs(agent.pubkey);
        }
      }}
    >
      <td className="px-4 py-3">
        <div className="flex items-center gap-1.5">
          {presenceStatus ? (
            <PresenceDot className="shrink-0" status={presenceStatus} />
          ) : null}
          <p className="truncate font-medium text-foreground">{agent.name}</p>
        </div>
        {personaLabel ? (
          <p className="mt-1">
            <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              {personaLabel}
            </span>
          </p>
        ) : null}
        <p className="mt-0.5 text-xs text-muted-foreground">
          {truncatePubkey(agent.pubkey)}
        </p>
      </td>
      <td className="px-4 py-3">
        <span
          className={cn(
            "inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.18em]",
            isActive
              ? "bg-primary text-primary-foreground"
              : "bg-muted text-muted-foreground",
          )}
        >
          {agent.status}
        </span>
      </td>
      <td
        className="px-4 py-3"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <ModelPicker agent={agent} />
      </td>
      <td className="px-4 py-3 text-muted-foreground">{agent.agentCommand}</td>
      <td
        className="px-3 py-3"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <AgentActionsMenu
          agent={agent}
          isActionPending={isActionPending}
          isActive={isActive}
          onAddToChannel={onAddToChannel}
          onDelete={onDelete}
          onMintToken={onMintToken}
          onStart={onStart}
          onStop={onStop}
          onToggleStartOnAppLaunch={onToggleStartOnAppLaunch}
          onViewLogs={onViewLogs}
        />
      </td>
    </tr>
  );
}

function AgentActionsMenu({
  agent,
  isActionPending,
  isActive,
  onAddToChannel,
  onDelete,
  onMintToken,
  onStart,
  onStop,
  onToggleStartOnAppLaunch,
  onViewLogs,
}: {
  agent: ManagedAgent;
  isActionPending: boolean;
  isActive: boolean;
  onAddToChannel: (agent: ManagedAgent) => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
  onViewLogs: (pubkey: string) => void;
}) {
  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <button
          className="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          type="button"
        >
          <Ellipsis className="h-4 w-4" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        {agent.backend.type === "provider" ? (
          <>
            <DropdownMenuItem
              disabled={isActionPending}
              onClick={() => onStart(agent.pubkey)}
            >
              <Play className="h-4 w-4" />
              {isActive ? "Redeploy" : "Deploy"}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={isActionPending}
              onClick={() => onStop(agent.pubkey)}
            >
              <Square className="h-4 w-4" />
              Shutdown
            </DropdownMenuItem>
          </>
        ) : isActive ? (
          <DropdownMenuItem
            disabled={isActionPending}
            onClick={() => onStop(agent.pubkey)}
          >
            <Square className="h-4 w-4" />
            Stop
          </DropdownMenuItem>
        ) : (
          <DropdownMenuItem
            disabled={isActionPending}
            onClick={() => onStart(agent.pubkey)}
          >
            <Play className="h-4 w-4" />
            Spawn
          </DropdownMenuItem>
        )}

        <DropdownMenuItem
          disabled={isActionPending}
          onClick={() => onAddToChannel(agent)}
        >
          <UserPlus className="h-4 w-4" />
          Add to channel
        </DropdownMenuItem>

        <DropdownMenuItem
          disabled={isActionPending}
          onClick={() => onMintToken(agent.pubkey, agent.name)}
        >
          <KeyRound className="h-4 w-4" />
          Mint token
        </DropdownMenuItem>

        <DropdownMenuItem
          onClick={() => navigator.clipboard.writeText(agent.pubkey)}
        >
          <Clipboard className="h-4 w-4" />
          Copy pubkey
        </DropdownMenuItem>

        {agent.backend.type === "local" ? (
          <DropdownMenuItem onClick={() => onViewLogs(agent.pubkey)}>
            <FileText className="h-4 w-4" />
            View logs
          </DropdownMenuItem>
        ) : null}

        {agent.backend.type === "local" ? (
          <DropdownMenuItem
            disabled={isActionPending}
            onClick={() =>
              onToggleStartOnAppLaunch(agent.pubkey, !agent.startOnAppLaunch)
            }
          >
            <Power className="h-4 w-4" />
            {agent.startOnAppLaunch
              ? "Disable auto-start"
              : "Enable auto-start"}
          </DropdownMenuItem>
        ) : null}

        <DropdownMenuSeparator />

        <DropdownMenuItem
          className="text-destructive focus:text-destructive"
          disabled={isActionPending}
          onClick={() => onDelete(agent.pubkey)}
        >
          <Trash2 className="h-4 w-4" />
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
