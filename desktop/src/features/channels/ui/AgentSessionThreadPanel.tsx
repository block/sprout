import type * as React from "react";
import { Activity, Bot, CircleDot, X } from "lucide-react";

import { ManagedAgentSessionPanel } from "@/features/agents/ui/ManagedAgentSessionPanel";
import type { Channel, ManagedAgent } from "@/shared/api/types";
import { useStickToBottom } from "@/shared/hooks/useStickToBottom";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";

type AgentSessionThreadPanelProps = {
  agent: ManagedAgent;
  canResetWidth: boolean;
  channel: Channel;
  onClose: () => void;
  onResetWidth: () => void;
  onResizeStart: (event: React.PointerEvent<HTMLButtonElement>) => void;
  widthPx: number;
};

export function AgentSessionThreadPanel({
  agent,
  canResetWidth,
  channel,
  onClose,
  onResetWidth,
  onResizeStart,
  widthPx,
}: AgentSessionThreadPanelProps) {
  const isLive = agent.status === "running" && Boolean(agent.observerUrl);
  const { ref: scrollRef, onScroll } = useStickToBottom<HTMLDivElement>();

  return (
    <aside
      className="relative hidden h-full shrink-0 flex-col border-l border-border/80 bg-background lg:flex"
      data-testid="agent-session-thread-panel"
      style={{ width: `${widthPx}px` }}
    >
      <button
        aria-label="Resize agent session panel"
        className="group absolute inset-y-0 left-0 z-20 w-3 -translate-x-1/2 cursor-col-resize"
        data-testid="agent-session-resize-handle"
        onDoubleClick={canResetWidth ? onResetWidth : undefined}
        onPointerDown={onResizeStart}
        title={
          canResetWidth
            ? "Drag to resize. Double-click to reset width."
            : "Drag to resize."
        }
        type="button"
      >
        <span className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-transparent transition-colors group-hover:bg-border/80" />
      </button>

      <div className="flex items-center gap-3 border-b border-border/70 px-4 py-2.5">
        <Bot className="h-4 w-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-sm font-semibold tracking-tight">
            {agent.name}
          </h2>
          <div className="flex items-center gap-1.5">
            <Activity className="h-3 w-3 text-muted-foreground" />
            <p className="truncate text-xs text-muted-foreground">
              Agent activity log
            </p>
          </div>
        </div>
        {isLive ? (
          <Badge className="shrink-0 gap-1" variant="default">
            <CircleDot className="h-3 w-3" />
            Live
          </Badge>
        ) : (
          <Badge className="shrink-0" variant="secondary">
            Idle
          </Badge>
        )}
        <Button
          aria-label="Close activity panel"
          data-testid="agent-session-close"
          onClick={onClose}
          size="icon"
          type="button"
          variant="ghost"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="min-h-0 flex-1 overflow-y-auto px-3 py-4"
      >
        <ManagedAgentSessionPanel
          agent={agent}
          channelId={channel.id}
          className="border-0 bg-transparent p-0 shadow-none"
          emptyDescription={`Mention ${agent.name} in the channel to see its work here.`}
          showHeader={false}
          showRaw={false}
        />
      </div>
    </aside>
  );
}
