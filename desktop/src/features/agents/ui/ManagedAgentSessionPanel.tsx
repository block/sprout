import * as React from "react";
import {
  CircleAlert,
  CircleDot,
  Clock3,
  Loader2,
  TerminalSquare,
  XCircle,
} from "lucide-react";

import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Badge } from "@/shared/ui/badge";
import { Skeleton } from "@/shared/ui/skeleton";
import { AgentSessionTranscriptList } from "./AgentSessionTranscriptList";
import { RawEventRail } from "./RawEventRail";
import type {
  ConnectionState,
  ObserverEvent,
  TranscriptItem,
} from "./agentSessionTypes";
import { buildTranscript } from "./agentSessionTranscript";
import { shorten } from "./agentSessionUtils";
import { useObserverEvents } from "./useObserverEvents";

type ManagedAgentSessionPanelProps = {
  agent: ManagedAgent;
  agentAvatarUrl?: string | null;
  channelId?: string | null;
  className?: string;
  emptyDescription?: string;
  enableInlineNavigation?: boolean;
  showHeader?: boolean;
  showRaw?: boolean;
};

export function ManagedAgentSessionPanel({
  agent,
  agentAvatarUrl = null,
  channelId = null,
  className,
  emptyDescription = "Mention this agent in a channel to watch the next turn.",
  enableInlineNavigation = true,
  showHeader = true,
  showRaw = true,
}: ManagedAgentSessionPanelProps) {
  const { connectionState, errorMessage, events } = useObserverEvents(
    agent.status === "running",
    agent.pubkey,
  );
  const scopedEvents = React.useMemo(
    () =>
      channelId
        ? events.filter((event) => event.channelId === channelId)
        : events,
    [channelId, events],
  );
  const transcript = React.useMemo(
    () => buildTranscript(scopedEvents),
    [scopedEvents],
  );
  const latestSessionId = React.useMemo(
    () =>
      [...scopedEvents].reverse().find((event) => event.sessionId)?.sessionId,
    [scopedEvents],
  );

  return (
    <section
      className={cn(
        "rounded-lg border border-border/70 bg-background/80 p-4 shadow-sm",
        className,
      )}
    >
      {showHeader ? (
        <SessionHeader
          connectionState={connectionState}
          eventCount={scopedEvents.length}
          hasObserver={agent.status === "running"}
          latestSessionId={latestSessionId}
        />
      ) : null}

      <SessionBody
        agentAvatarUrl={agentAvatarUrl}
        agentName={agent.name}
        connectionState={connectionState}
        emptyDescription={emptyDescription}
        enableInlineNavigation={enableInlineNavigation}
        errorMessage={errorMessage}
        events={scopedEvents}
        hasObserver={agent.status === "running"}
        showRaw={showRaw}
        transcript={transcript}
      />
    </section>
  );
}

function SessionHeader({
  connectionState,
  eventCount,
  hasObserver,
  latestSessionId,
}: {
  connectionState: ConnectionState;
  eventCount: number;
  hasObserver: boolean;
  latestSessionId: string | null | undefined;
}) {
  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold tracking-tight">
            Live ACP session
          </h3>
          <ObserverStatusBadge state={connectionState} />
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          {hasObserver
            ? latestSessionId
              ? `Session ${shorten(latestSessionId)}`
              : "Waiting for the next agent turn."
            : "Restart this local agent to attach the observer feed."}
        </p>
      </div>
      <Badge className="w-fit font-mono" variant="outline">
        {eventCount} event{eventCount === 1 ? "" : "s"}
      </Badge>
    </div>
  );
}

function SessionBody({
  agentAvatarUrl,
  agentName,
  connectionState,
  emptyDescription,
  enableInlineNavigation,
  errorMessage,
  events,
  hasObserver,
  showRaw,
  transcript,
}: {
  agentAvatarUrl: string | null;
  agentName: string;
  connectionState: ConnectionState;
  emptyDescription: string;
  enableInlineNavigation: boolean;
  errorMessage: string | null;
  events: ObserverEvent[];
  hasObserver: boolean;
  showRaw: boolean;
  transcript: TranscriptItem[];
}) {
  return (
    <>
      {!hasObserver ? (
        <EmptyObserverState />
      ) : connectionState === "connecting" && events.length === 0 ? (
        <SessionLoadingSkeleton />
      ) : (
        <div
          className={cn(
            showRaw
              ? "mt-4 grid gap-4 xl:grid-cols-[minmax(0,1fr)_20rem]"
              : "mt-0",
          )}
        >
          <AgentSessionTranscriptList
            agentAvatarUrl={agentAvatarUrl}
            agentName={agentName}
            enableInlineNavigation={enableInlineNavigation}
            emptyDescription={emptyDescription}
            items={transcript}
          />
          {showRaw ? <RawEventRail events={events} /> : null}
        </div>
      )}

      {errorMessage ? (
        <p className="mt-4 inline-flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          <CircleAlert className="h-4 w-4" />
          {errorMessage}
        </p>
      ) : null}
    </>
  );
}

function SessionLoadingSkeleton() {
  return (
    <div className="mx-auto w-full max-w-3xl space-y-6 py-4">
      <div className="flex justify-end">
        <div className="max-w-[70%] space-y-2">
          <Skeleton className="h-4 w-48 rounded-lg" />
        </div>
      </div>
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Skeleton className="h-5 w-5 rounded-full" />
          <Skeleton className="h-3 w-16 rounded-full" />
        </div>
        <div className="space-y-2">
          <Skeleton className="h-4 w-full rounded-lg" />
          <Skeleton className="h-4 w-[86%] rounded-lg" />
          <Skeleton className="h-4 w-[58%] rounded-lg" />
        </div>
      </div>
      <div className="space-y-2">
        <Skeleton className="h-4 w-44 rounded-lg" />
        <Skeleton className="h-4 w-[68%] rounded-lg" />
      </div>
    </div>
  );
}

function ObserverStatusBadge({ state }: { state: ConnectionState }) {
  const display =
    state === "open"
      ? { label: "Live", Icon: CircleDot, variant: "default" as const }
      : state === "connecting"
        ? { label: "Connecting", Icon: Loader2, variant: "secondary" as const }
        : state === "error"
          ? {
              label: "Unavailable",
              Icon: XCircle,
              variant: "destructive" as const,
            }
          : state === "closed"
            ? { label: "Closed", Icon: Clock3, variant: "secondary" as const }
            : { label: "Idle", Icon: Clock3, variant: "secondary" as const };

  return (
    <Badge className="gap-1.5" variant={display.variant}>
      <display.Icon
        className={cn("h-3.5 w-3.5", state === "connecting" && "animate-spin")}
      />
      {display.label}
    </Badge>
  );
}

function EmptyObserverState() {
  return (
    <div className="mt-4 flex min-h-48 flex-col items-center justify-center px-6 py-8 text-center">
      <TerminalSquare className="mx-auto h-5 w-5 text-muted-foreground" />
      <p className="mt-3 text-sm font-medium">Observer not attached</p>
      <p className="mt-1 text-sm text-muted-foreground">
        The live feed is available for local agents started after this update.
      </p>
    </div>
  );
}
