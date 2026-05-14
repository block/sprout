import * as React from "react";

import {
  ChevronDown,
  ChevronRight,
  Clipboard,
  Ellipsis,
  FileText,
  Pencil,
  Play,
  Power,
  Square,
  Trash2,
  UserPlus,
} from "lucide-react";
import { toast } from "sonner";

import { useAppShell } from "@/app/AppShellContext";
import { isSproutAgent } from "@/features/agents/lib/resolveAcpProviderId";
import { SproutProfileLabel } from "./SproutProfileLabel";
import { PresenceDot } from "@/features/presence/ui/PresenceBadge";
import { Badge } from "@/shared/ui/badge";
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
import { EditAgentDialog } from "./EditAgentDialog";
import { ManagedAgentLogPanel } from "./ManagedAgentLogPanel";
import { ModelPicker } from "./ModelPicker";
import { truncatePubkey } from "./agentUi";

export function ManagedAgentRow({
  agent,
  channelNames,
  isActionPending,
  isLogSelected,
  logContent,
  logError,
  logLoading,
  personaLabelsById,
  presenceLoaded,
  presenceLookup,
  onAddToChannel,
  onDelete,
  onSelectLogAgent,
  onStart,
  onStop,
  onToggleStartOnAppLaunch,
}: {
  agent: ManagedAgent;
  channelNames: string[];
  isActionPending: boolean;
  isLogSelected: boolean;
  logContent: string | null;
  logError: Error | null;
  logLoading: boolean;
  personaLabelsById: Record<string, string>;
  presenceLoaded: boolean;
  presenceLookup: PresenceLookup;
  onAddToChannel: (agent: ManagedAgent) => void;
  onDelete: (pubkey: string) => void;
  onSelectLogAgent: (pubkey: string | null) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
}) {
  const isActive = agent.status === "running" || agent.status === "deployed";
  const isLocal = agent.backend.type === "local";
  const runtimeSource =
    agent.backend.type === "provider" ? `Provider ${agent.backend.id}` : null;
  // sprout-agent's model is managed globally in Settings > Agent Provider, so
  // hide the per-agent ModelPicker and let `RuntimeBlock` surface a link there.
  const isSproutAgentRow = isSproutAgent(agent.agentCommand);
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
                channelNames={channelNames}
                isExpandable
                isLogSelected={isLogSelected}
                personaLabel={personaLabel}
                presenceStatus={presenceStatus}
              />
              <StatusBlock
                presenceLoaded={presenceLoaded}
                presenceStatus={presenceStatus}
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
                channelNames={channelNames}
                isExpandable={false}
                isLogSelected={false}
                personaLabel={personaLabel}
                presenceStatus={presenceStatus}
              />
              <StatusBlock
                presenceLoaded={presenceLoaded}
                presenceStatus={presenceStatus}
                processDetail={processDetail}
                status={agent.status}
              />
              <RuntimeBlock agent={agent} runtimeSource={runtimeSource} />
            </div>
          </div>
        )}

        <div
          className="flex shrink-0 items-start gap-2 lg:pt-0.5"
          data-testid={`managed-agent-row-actions-${agent.pubkey}`}
        >
          {isSproutAgentRow ? null : (
            <div data-testid={`managed-agent-model-picker-${agent.pubkey}`}>
              <ModelPicker agent={agent} />
            </div>
          )}
          <AgentActionsMenu
            agent={agent}
            isActionPending={isActionPending}
            isActive={isActive}
            onAddToChannel={onAddToChannel}
            onDelete={onDelete}
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
  channelNames,
  isExpandable,
  isLogSelected,
  personaLabel,
  presenceStatus,
}: {
  agent: ManagedAgent;
  channelNames: string[];
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
              <Badge variant="secondary">{personaLabel}</Badge>
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
          </div>
          {channelNames.length > 0 ? (
            <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
              {channelNames.map((name) => (
                <Badge className="normal-case" key={name} variant="secondary">
                  # {name}
                </Badge>
              ))}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function StatusBlock({
  presenceLoaded,
  presenceStatus,
  processDetail,
  status,
}: {
  presenceLoaded: boolean;
  presenceStatus: PresenceStatus | undefined;
  processDetail: string;
  status: ManagedAgent["status"];
}) {
  return (
    <div className="space-y-1 lg:pt-0.5">
      <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground lg:hidden">
        Status
      </p>
      <AgentStatusBadge
        presenceLoaded={presenceLoaded}
        presenceStatus={presenceStatus}
        status={status}
      />
      <p className="text-xs text-muted-foreground">{processDetail}</p>
    </div>
  );
}

function RuntimeBlock({
  agent,
  runtimeSource,
}: {
  agent: ManagedAgent;
  runtimeSource: string | null;
}) {
  const isSprout = isSproutAgent(agent.agentCommand);
  const { openSettings } = useAppShell();
  const showRuntimeSource = runtimeSource !== null;
  // For sprout-agent we don't render the per-agent model badge — model is set
  // globally in Settings > Agent Provider. We surface a small link there.
  const showAgentModelBadge = !isSprout && agent.model !== null;
  return (
    <div className="space-y-1 lg:pt-0.5">
      <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground lg:hidden">
        Runtime
      </p>
      <p className="truncate font-mono text-xs text-foreground">
        {agent.agentCommand}
      </p>
      {showRuntimeSource || showAgentModelBadge || isSprout ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          {showRuntimeSource ? <span>{runtimeSource}</span> : null}
          {showAgentModelBadge ? <span>{agent.model}</span> : null}
          {isSprout ? (
            // The outer row is already a <button> that toggles the log
            // panel, so we can't nest a real <button> here — browsers
            // auto-split the markup, and the click would bubble up and
            // toggle the row too. Use a span with role="button" and stop
            // propagation so this acts as its own focusable activator
            // without disturbing the row.
            // biome-ignore lint/a11y/useSemanticElements: intentional — nesting a real <button> inside the outer row <button> is invalid HTML and the browser auto-splits it.
            <span
              className="cursor-pointer text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1"
              data-testid="managed-agent-row-open-agent-provider-settings"
              onClick={(event) => {
                event.stopPropagation();
                openSettings("agent-provider");
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  event.stopPropagation();
                  openSettings("agent-provider");
                }
              }}
              role="button"
              tabIndex={0}
            >
              Model managed in Settings &rsaquo; Agent Provider
            </span>
          ) : null}
          {isSprout ? (
            <SproutProfileLabel
              pinnedProfileId={agent.providerProfileId}
              pubkey={agent.pubkey}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function AgentActionsMenu({
  agent,
  isActionPending,
  isActive,
  onAddToChannel,
  onDelete,
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
  onOpenLogs: (pubkey: string) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
  onToggleStartOnAppLaunch: (pubkey: string, startOnAppLaunch: boolean) => void;
}) {
  const [editOpen, setEditOpen] = React.useState(false);

  return (
    <>
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

          {agent.backend.type !== "provider" ? (
            <DropdownMenuItem onClick={() => setEditOpen(true)}>
              <Pencil className="h-4 w-4" />
              Edit
            </DropdownMenuItem>
          ) : null}

          <DropdownMenuItem
            disabled={isActionPending}
            onClick={() => onAddToChannel(agent)}
          >
            <UserPlus className="h-4 w-4" />
            Add to channel
          </DropdownMenuItem>

          <DropdownMenuItem
            onClick={async () => {
              await navigator.clipboard.writeText(agent.pubkey);
              toast.success("Copied pubkey to clipboard");
            }}
          >
            <Clipboard className="h-4 w-4" />
            Copy pubkey
          </DropdownMenuItem>

          {agent.backend.type === "local" ? (
            <DropdownMenuItem onClick={() => onOpenLogs(agent.pubkey)}>
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

      <EditAgentDialog
        agent={agent}
        onOpenChange={setEditOpen}
        open={editOpen}
      />
    </>
  );
}

function AgentOriginBadge({ agent }: { agent: ManagedAgent }) {
  return (
    <Badge variant="outline">
      {agent.backend.type === "local" ? "Local" : "Remote"}
    </Badge>
  );
}

/** Grace period after mount before treating "running + no presence" as "Starting…" */
const PRESENCE_GRACE_MS = 15_000;

function AgentStatusBadge({
  presenceLoaded,
  presenceStatus,
  status,
}: {
  presenceLoaded: boolean;
  presenceStatus: PresenceStatus | undefined;
  status: ManagedAgent["status"];
}) {
  const [inGracePeriod, setInGracePeriod] = React.useState(true);

  React.useEffect(() => {
    const timer = setTimeout(() => setInGracePeriod(false), PRESENCE_GRACE_MS);
    return () => clearTimeout(timer);
  }, []);

  const isActive = status === "running" || status === "deployed";
  const isStarting =
    !inGracePeriod &&
    presenceLoaded &&
    status === "running" &&
    (!presenceStatus || presenceStatus === "offline");

  const variant = isStarting ? "warning" : isActive ? "default" : "secondary";

  return (
    <Badge variant={variant}>
      {isStarting ? "Starting\u2026" : status.replace(/_/g, " ")}
    </Badge>
  );
}
