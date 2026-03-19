import {
  Clipboard,
  Ellipsis,
  FileText,
  KeyRound,
  Play,
  Plus,
  Power,
  RefreshCcw,
  Square,
  Trash2,
  UserPlus,
} from "lucide-react";

import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Skeleton } from "@/shared/ui/skeleton";
import { ModelPicker } from "./ModelPicker";
import { truncatePubkey } from "./agentUi";

export function ManagedAgentsSection({
  actionErrorMessage,
  actionNoticeMessage,
  agents,
  error,
  isActionPending,
  isLoading,
  onAddToChannel,
  onCreate,
  onDelete,
  onMintToken,
  onRefresh,
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
  onAddToChannel: (agent: ManagedAgent) => void;
  onCreate: () => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onRefresh: () => void;
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
            Managed locally
          </h3>
          <p className="text-sm text-muted-foreground">
            Saved agent profiles and local ACP process state.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button onClick={onCreate} type="button">
            <Plus className="h-4 w-4" />
            Create agent
          </Button>
          <Button onClick={onRefresh} type="button" variant="outline">
            <RefreshCcw className="h-4 w-4" />
            Refresh
          </Button>
        </div>
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
                    onAddToChannel={onAddToChannel}
                    onDelete={onDelete}
                    onMintToken={onMintToken}
                    onModelChanged={onRefresh}
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
  onAddToChannel,
  onDelete,
  onMintToken,
  onModelChanged,
  onStart,
  onStop,
  onToggleStartOnAppLaunch,
  onViewLogs,
}: {
  agent: ManagedAgent;
  isActionPending: boolean;
  onAddToChannel: (agent: ManagedAgent) => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onModelChanged?: () => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
  onViewLogs: (pubkey: string) => void;
}) {
  const isRunning = agent.status === "running";

  return (
    <tr
      className="border-b border-border/60 last:border-b-0 hover:bg-muted/30"
      data-testid={`managed-agent-${agent.pubkey}`}
    >
      <td className="px-4 py-3">
        <p className="truncate font-medium text-foreground">{agent.name}</p>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {truncatePubkey(agent.pubkey)}
        </p>
      </td>
      <td className="px-4 py-3">
        <span
          className={cn(
            "inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.18em]",
            isRunning
              ? "bg-primary text-primary-foreground"
              : "bg-muted text-muted-foreground",
          )}
        >
          {agent.status}
        </span>
      </td>
      <td className="px-4 py-3">
        <ModelPicker agent={agent} onModelChanged={onModelChanged} />
      </td>
      <td className="px-4 py-3 text-muted-foreground">{agent.agentCommand}</td>
      <td className="px-3 py-3">
        <AgentActionsMenu
          agent={agent}
          isActionPending={isActionPending}
          isRunning={isRunning}
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
  isRunning,
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
  isRunning: boolean;
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
        {isRunning ? (
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

        <DropdownMenuItem onClick={() => onViewLogs(agent.pubkey)}>
          <FileText className="h-4 w-4" />
          View logs
        </DropdownMenuItem>

        <DropdownMenuItem
          disabled={isActionPending}
          onClick={() =>
            onToggleStartOnAppLaunch(agent.pubkey, !agent.startOnAppLaunch)
          }
        >
          <Power className="h-4 w-4" />
          {agent.startOnAppLaunch ? "Disable auto-start" : "Enable auto-start"}
        </DropdownMenuItem>

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
