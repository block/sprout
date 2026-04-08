import {
  ChevronDown,
  ChevronRight,
  Clipboard,
  Ellipsis,
  FileText,
  KeyRound,
  Play,
  Power,
  Square,
  Trash2,
  UserPlus,
} from "lucide-react";

import { PresenceDot } from "@/features/presence/ui/PresenceBadge";
import type {
  ManagedAgent,
  PresenceLookup,
  PresenceStatus,
} from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { ManagedAgentLogPanel } from "./ManagedAgentLogPanel";
import { ModelPicker } from "./ModelPicker";
import { truncatePubkey } from "./agentUi";

export function ManagedAgentRow({
  agent,
  isActionPending,
  isLogSelected,
  logContent,
  logError,
  logLoading,
  personaLabelsById,
  presenceLookup,
  onAddToChannel,
  onDelete,
  onMintToken,
  onSelectLogAgent,
  onStart,
  onStop,
  onToggleStartOnAppLaunch,
}: {
  agent: ManagedAgent;
  isActionPending: boolean;
  isLogSelected: boolean;
  logContent: string | null;
  logError: Error | null;
  logLoading: boolean;
  personaLabelsById: Record<string, string>;
  presenceLookup: PresenceLookup;
  onAddToChannel: (agent: ManagedAgent) => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onSelectLogAgent: (pubkey: string | null) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
}) {
  const isActive = agent.status === "running" || agent.status === "deployed";
  const isLocal = agent.backend.type === "local";
  const runtimeSource =
    agent.backend.type === "local"
      ? `ACP ${agent.acpCommand}`
      : `Provider ${agent.backend.id}`;
  const personaLabel = agent.personaId
    ? (personaLabelsById[agent.personaId] ?? null)
    : null;
  const presenceStatus = presenceLookup[agent.pubkey.trim().toLowerCase()];
  const processDetail =
    agent.pid !== null
      ? `PID ${agent.pid}`
      : agent.lastExitCode !== null
        ? `Exit ${agent.lastExitCode}`
        : isLocal
          ? "Ready to launch"
          : "Managed remotely";

  return (
    <div
      className={cn(
        "overflow-hidden rounded-xl border bg-card/70 transition-colors",
        isLogSelected
          ? "border-primary/40 bg-primary/5 shadow-sm"
          : "border-border/70 hover:bg-muted/20",
      )}
      data-testid={`managed-agent-${agent.pubkey}`}
    >
      <div className="flex items-start gap-3 px-4 py-3">
        {isLocal ? (
          <button
            aria-expanded={isLogSelected}
            className="-m-1 min-w-0 flex-1 rounded-lg p-1 text-left transition-colors hover:bg-background/40"
            onClick={() =>
              onSelectLogAgent(isLogSelected ? null : agent.pubkey)
            }
            type="button"
          >
            <div className="grid gap-3 lg:grid-cols-[minmax(0,1.8fr)_minmax(120px,0.8fr)_minmax(0,1.1fr)] lg:gap-4">
              <AgentSummary
                agent={agent}
                isExpandable
                isLogSelected={isLogSelected}
                personaLabel={personaLabel}
                presenceStatus={presenceStatus}
              />
              <StatusBlock
                isActive={isActive}
                processDetail={processDetail}
                status={agent.status}
              />
              <RuntimeBlock agent={agent} runtimeSource={runtimeSource} />
            </div>
          </button>
        ) : (
          <div className="min-w-0 flex-1">
            <div className="grid gap-3 lg:grid-cols-[minmax(0,1.8fr)_minmax(120px,0.8fr)_minmax(0,1.1fr)] lg:gap-4">
              <AgentSummary
                agent={agent}
                isExpandable={false}
                isLogSelected={false}
                personaLabel={personaLabel}
                presenceStatus={presenceStatus}
              />
              <StatusBlock
                isActive={isActive}
                processDetail={processDetail}
                status={agent.status}
              />
              <RuntimeBlock agent={agent} runtimeSource={runtimeSource} />
            </div>
          </div>
        )}

        <div className="flex shrink-0 items-start gap-2 lg:pt-0.5">
          <ModelPicker agent={agent} />
          <AgentActionsMenu
            agent={agent}
            isActionPending={isActionPending}
            isActive={isActive}
            onAddToChannel={onAddToChannel}
            onDelete={onDelete}
            onMintToken={onMintToken}
            onOpenLogs={(pubkey) => onSelectLogAgent(pubkey)}
            onStart={onStart}
            onStop={onStop}
            onToggleStartOnAppLaunch={onToggleStartOnAppLaunch}
          />
        </div>
      </div>

      {isLocal && isLogSelected ? (
        <div
          className="border-t border-border/60 bg-background/60 px-4 py-4"
          data-testid="managed-agent-log-row"
        >
          <ManagedAgentLogPanel
            error={logError}
            isLoading={logLoading}
            logContent={logContent}
            selectedAgent={agent}
            variant="inline"
          />
        </div>
      ) : null}
    </div>
  );
}

function AgentSummary({
  agent,
  isExpandable,
  isLogSelected,
  personaLabel,
  presenceStatus,
}: {
  agent: ManagedAgent;
  isExpandable: boolean;
  isLogSelected: boolean;
  personaLabel: string | null;
  presenceStatus: PresenceStatus | undefined;
}) {
  return (
    <div className="min-w-0">
      <div className="flex items-start gap-3">
        {isExpandable ? (
          isLogSelected ? (
            <ChevronDown className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
          ) : (
            <ChevronRight className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
          )
        ) : (
          <span className="mt-0.5 h-4 w-4 shrink-0" />
        )}
        {presenceStatus ? (
          <PresenceDot className="mt-1 shrink-0" status={presenceStatus} />
        ) : (
          <span className="mt-1 h-2 w-2 shrink-0" />
        )}
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="truncate font-medium text-foreground">{agent.name}</p>
            {personaLabel ? (
              <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                {personaLabel}
              </span>
            ) : null}
            <AgentOriginBadge agent={agent} />
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            <span className="font-mono">{truncatePubkey(agent.pubkey)}</span>
            {agent.backend.type === "local" ? (
              <span>
                {agent.startOnAppLaunch ? "Auto-start" : "Manual start"}
              </span>
            ) : (
              <span>Remote deployment</span>
            )}
            {isExpandable ? (
              <span>
                {isLogSelected ? "Logs visible inline" : "Click to view logs"}
              </span>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}

function StatusBlock({
  isActive,
  processDetail,
  status,
}: {
  isActive: boolean;
  processDetail: string;
  status: ManagedAgent["status"];
}) {
  return (
    <div className="space-y-1 lg:pt-0.5">
      <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground lg:hidden">
        Status
      </p>
      <AgentStatusBadge isActive={isActive} status={status} />
      <p className="text-xs text-muted-foreground">{processDetail}</p>
    </div>
  );
}

function RuntimeBlock({
  agent,
  runtimeSource,
}: {
  agent: ManagedAgent;
  runtimeSource: string;
}) {
  return (
    <div className="space-y-1 lg:pt-0.5">
      <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground lg:hidden">
        Runtime
      </p>
      <p className="truncate font-mono text-xs text-foreground">
        {agent.agentCommand}
      </p>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
        <span>{runtimeSource}</span>
        {agent.model ? <span>{agent.model}</span> : null}
      </div>
    </div>
  );
}

function AgentActionsMenu({
  agent,
  isActionPending,
  isActive,
  onAddToChannel,
  onDelete,
  onMintToken,
  onOpenLogs,
  onStart,
  onStop,
  onToggleStartOnAppLaunch,
}: {
  agent: ManagedAgent;
  isActionPending: boolean;
  isActive: boolean;
  onAddToChannel: (agent: ManagedAgent) => void;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onOpenLogs: (pubkey: string) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
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
              title={
                isActive
                  ? "Push a new deployment to the provider"
                  : "Deploy this agent to the provider"
              }
            >
              <Play className="h-4 w-4" />
              {isActive ? "Redeploy" : "Deploy"}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={isActionPending}
              onClick={() => onStop(agent.pubkey)}
              title="Stop the provider deployment and free its resources"
            >
              <Square className="h-4 w-4" />
              Shutdown
            </DropdownMenuItem>
          </>
        ) : isActive ? (
          <DropdownMenuItem
            disabled={isActionPending}
            onClick={() => onStop(agent.pubkey)}
            title="Stop the running ACP harness process"
          >
            <Square className="h-4 w-4" />
            Stop
          </DropdownMenuItem>
        ) : (
          <DropdownMenuItem
            disabled={isActionPending}
            onClick={() => onStart(agent.pubkey)}
            title="Launch the local ACP harness process for this agent"
          >
            <Play className="h-4 w-4" />
            Spawn
          </DropdownMenuItem>
        )}

        <DropdownMenuItem
          disabled={isActionPending}
          onClick={() => onAddToChannel(agent)}
          title="Invite this agent to a channel so it can participate in conversations"
        >
          <UserPlus className="h-4 w-4" />
          Add to channel
        </DropdownMenuItem>

        <DropdownMenuItem
          disabled={isActionPending}
          onClick={() => onMintToken(agent.pubkey, agent.name)}
          title="Generate a bearer token this agent uses to authenticate with the relay"
        >
          <KeyRound className="h-4 w-4" />
          Mint token
        </DropdownMenuItem>

        <DropdownMenuItem
          onClick={() => navigator.clipboard.writeText(agent.pubkey)}
          title="Copy the agent's public key to the clipboard"
        >
          <Clipboard className="h-4 w-4" />
          Copy pubkey
        </DropdownMenuItem>

        {agent.backend.type === "local" ? (
          <DropdownMenuItem
            onClick={() => onOpenLogs(agent.pubkey)}
            title="Show the ACP harness stdout/stderr log inline"
          >
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
            title={
              agent.startOnAppLaunch
                ? "Stop launching this agent automatically when the desktop app starts"
                : "Launch this agent automatically every time the desktop app starts"
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
          title="Permanently remove this agent profile from the desktop app"
        >
          <Trash2 className="h-4 w-4" />
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function AgentOriginBadge({ agent }: { agent: ManagedAgent }) {
  return (
    <span className="rounded-full border border-border/70 bg-background/80 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
      {agent.backend.type === "local" ? "Local" : "Remote"}
    </span>
  );
}

function AgentStatusBadge({
  isActive,
  status,
}: {
  isActive: boolean;
  status: ManagedAgent["status"];
}) {
  return (
    <span
      className={cn(
        "inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.18em]",
        isActive
          ? "bg-primary text-primary-foreground"
          : "bg-muted text-muted-foreground",
      )}
    >
      {status.replace(/_/g, " ")}
    </span>
  );
}
