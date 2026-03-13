import {
  KeyRound,
  Play,
  Square,
  TerminalSquare,
  Trash2,
  UserPlus,
} from "lucide-react";

import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { CopyButton } from "./CopyButton";
import { formatTimestamp, truncatePubkey } from "./agentUi";

export function ManagedAgentCard({
  agent,
  onAddToChannel,
  isSelected,
  onDelete,
  onMintToken,
  onSelect,
  onStart,
  onStop,
}: {
  agent: ManagedAgent;
  onAddToChannel: (agent: ManagedAgent) => void;
  isSelected: boolean;
  onDelete: (pubkey: string) => void;
  onMintToken: (pubkey: string, name: string) => void;
  onSelect: (pubkey: string) => void;
  onStart: (pubkey: string) => void;
  onStop: (pubkey: string) => void;
}) {
  const statusBadgeClass =
    agent.status === "running"
      ? "bg-primary text-primary-foreground"
      : "bg-muted text-muted-foreground";

  return (
    <article
      className={cn(
        "rounded-3xl border p-4 shadow-sm transition-colors",
        isSelected
          ? "border-primary/60 bg-primary/5"
          : "border-border/70 bg-card/80 hover:bg-card",
      )}
      data-testid={`managed-agent-${agent.pubkey}`}
    >
      <button
        className="w-full text-left"
        onClick={() => onSelect(agent.pubkey)}
        type="button"
      >
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-sm font-semibold tracking-tight">
                {agent.name}
              </h3>
              <span
                className={cn(
                  "rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.18em]",
                  statusBadgeClass,
                )}
              >
                {agent.status}
              </span>
            </div>
            <p className="mt-1 text-xs text-muted-foreground">
              {truncatePubkey(agent.pubkey)}
              {agent.pid ? ` · pid ${agent.pid}` : ""}
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
            <TerminalSquare className="h-4 w-4" />
            <span>{agent.agentCommand}</span>
          </div>
        </div>
      </button>

      <div className="mt-4 grid gap-2 sm:grid-cols-2">
        <div className="rounded-2xl border border-border/60 bg-background/70 px-3 py-2">
          <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Relay
          </p>
          <p className="mt-1 truncate text-sm">{agent.relayUrl}</p>
        </div>
        <div className="rounded-2xl border border-border/60 bg-background/70 px-3 py-2">
          <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Auth
          </p>
          <p className="mt-1 text-sm">
            {agent.hasApiToken ? "Bearer token saved" : "Key-only dev mode"}
          </p>
        </div>
      </div>

      <div className="mt-4 flex flex-wrap gap-2">
        <CopyButton label="Copy pubkey" value={agent.pubkey} />

        {agent.status === "running" ? (
          <Button
            onClick={() => onStop(agent.pubkey)}
            size="sm"
            type="button"
            variant="outline"
          >
            <Square className="h-3.5 w-3.5" />
            Stop
          </Button>
        ) : (
          <Button onClick={() => onStart(agent.pubkey)} size="sm" type="button">
            <Play className="h-3.5 w-3.5" />
            Spawn
          </Button>
        )}

        <Button
          onClick={() => onAddToChannel(agent)}
          size="sm"
          type="button"
          variant="outline"
        >
          <UserPlus className="h-3.5 w-3.5" />
          Add to channel
        </Button>

        <Button
          onClick={() => onMintToken(agent.pubkey, agent.name)}
          size="sm"
          type="button"
          variant="outline"
        >
          <KeyRound className="h-3.5 w-3.5" />
          Mint token
        </Button>

        <Button
          onClick={() => onDelete(agent.pubkey)}
          size="sm"
          type="button"
          variant="ghost"
        >
          <Trash2 className="h-3.5 w-3.5" />
          Delete
        </Button>
      </div>

      <div className="mt-4 grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
        <p>Started {formatTimestamp(agent.lastStartedAt)}</p>
        <p>Stopped {formatTimestamp(agent.lastStoppedAt)}</p>
      </div>

      {agent.lastError ? (
        <p className="mt-3 rounded-2xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {agent.lastError}
        </p>
      ) : null}
    </article>
  );
}
