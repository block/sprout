import { Send } from "lucide-react";
import * as React from "react";

import { useHuddle } from "@/features/huddle";
import { useChannelsQuery } from "@/features/channels/hooks";
import {
  useChannelMessagesQuery,
  useChannelSubscription,
  useSendMessageMutation,
} from "@/features/messages/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

import type {
  ConciergePhase,
  ConciergeTurn,
  DispatchIntent,
} from "@/features/concierge/types";
import { useConciergeSession } from "@/features/concierge/hooks";
import { postDispatch } from "@/features/concierge/lib/approveDispatch";
import {
  applySettledStatus,
  dispatchStorageKey,
  parseDispatchIntents,
  readSettledDispatches,
  type SettledDispatchMap,
} from "@/features/concierge/lib/dispatchIntent";
import { DispatchCard } from "@/features/concierge/ui/DispatchCard";
import { TranscriptLine } from "@/features/concierge/ui/TranscriptLine";
import { VoiceOrb } from "@/features/concierge/ui/VoiceOrb";

type TimelineEntry =
  | { kind: "turn"; turn: ConciergeTurn }
  | { kind: "dispatch"; intent: DispatchIntent };

/**
 * The live Concierge surface: persistent DM transcript + voice orb (huddle
 * attach) + dispatch confirm cards. All plumbing is existing machinery —
 * mesh-llm brain via the managed-agent preset, Parakeet STT + TTS via the
 * huddle stack, memory via the DM channel.
 */
export function ConciergeLiveScreen() {
  const { session, error, isLoading, retry } = useConciergeSession();
  const identityQuery = useIdentityQuery();
  const selfPubkey = identityQuery.data?.pubkey?.toLowerCase() ?? null;
  const dm = session?.dm ?? null;
  const agentPubkey = session?.agent.pubkey.toLowerCase() ?? null;

  const messagesQuery = useChannelMessagesQuery(dm);
  useChannelSubscription(dm);
  const sendMessage = useSendMessageMutation(dm, identityQuery.data);

  const {
    startHuddle,
    leaveHuddle,
    micConnected,
    isStarting,
    activeSpeakers,
    huddleError,
  } = useHuddle();

  // ── Dispatch settled-state (persisted per identity) ─────────────────────
  const [settled, setSettled] = React.useState<SettledDispatchMap>({});
  React.useEffect(() => {
    if (!selfPubkey) return;
    setSettled(
      readSettledDispatches(
        localStorage.getItem(dispatchStorageKey(selfPubkey)),
      ),
    );
  }, [selfPubkey]);
  const settleIntent = React.useCallback(
    (id: string, status: "approved" | "dismissed") => {
      setSettled((prev) => {
        const next = { ...prev, [id]: status };
        if (selfPubkey) {
          localStorage.setItem(
            dispatchStorageKey(selfPubkey),
            JSON.stringify(next),
          );
        }
        return next;
      });
    },
    [selfPubkey],
  );

  const channelsQuery = useChannelsQuery();
  const [dispatchError, setDispatchError] = React.useState<string | null>(null);
  const handleApprove = React.useCallback(
    (intent: DispatchIntent) => {
      setDispatchError(null);
      void postDispatch(intent, channelsQuery.data ?? [])
        .then(() => settleIntent(intent.id, "approved"))
        .catch((e: unknown) =>
          setDispatchError(e instanceof Error ? e.message : String(e)),
        );
    },
    [channelsQuery.data, settleIntent],
  );

  // ── Timeline: DM kind:9 events → turns + dispatch cards ─────────────────
  const timeline = React.useMemo<TimelineEntry[]>(() => {
    const events = messagesQuery.data ?? [];
    const entries: TimelineEntry[] = [];
    for (const event of events) {
      if (event.kind !== KIND_STREAM_MESSAGE) continue;
      const isYou = event.pubkey.toLowerCase() === selfPubkey;
      const { intents, cleanedContent } = isYou
        ? { intents: [], cleanedContent: event.content }
        : parseDispatchIntents(event.id, event.content);
      if (cleanedContent.length > 0) {
        entries.push({
          kind: "turn",
          turn: {
            id: event.id,
            speaker: isYou ? "you" : "concierge",
            text: cleanedContent,
          },
        });
      }
      for (const intent of intents) {
        entries.push({
          kind: "dispatch",
          intent: applySettledStatus(intent, settled),
        });
      }
    }
    return entries;
  }, [messagesQuery.data, selfPubkey, settled]);

  // ── Voice phase ──────────────────────────────────────────────────────────
  const agentSpeaking =
    agentPubkey != null &&
    activeSpeakers.some((pubkey) => pubkey.toLowerCase() === agentPubkey);
  const lastTurn = [...timeline]
    .reverse()
    .find(
      (entry): entry is Extract<TimelineEntry, { kind: "turn" }> =>
        entry.kind === "turn",
    );
  const phase: ConciergePhase = agentSpeaking
    ? "speaking"
    : micConnected
      ? lastTurn?.turn.speaker === "you"
        ? "thinking"
        : "listening"
      : "idle";

  const handleOrb = React.useCallback(() => {
    if (!dm || !session) return;
    if (micConnected) {
      void leaveHuddle();
    } else if (!isStarting) {
      void startHuddle(dm.id, [session.agent.pubkey]).catch(() => {
        /* surfaced via huddleError */
      });
    }
  }, [dm, session, micConnected, isStarting, leaveHuddle, startHuddle]);

  // ── Text composer (voice-optional path) ──────────────────────────────────
  const [draft, setDraft] = React.useState("");
  const handleSend = React.useCallback(() => {
    const content = draft.trim();
    if (!content || !dm) return;
    setDraft("");
    sendMessage.mutate({ content });
  }, [draft, dm, sendMessage]);

  if (isLoading || error) {
    return (
      <div
        className="flex min-h-0 flex-1 flex-col items-center justify-center gap-4 bg-background px-6"
        data-testid="concierge-screen"
      >
        {error ? (
          <>
            <p
              className="max-w-md text-center text-sm text-muted-foreground"
              data-testid="concierge-error"
            >
              {error}
            </p>
            <Button onClick={() => void retry()} size="sm" type="button">
              Try again
            </Button>
          </>
        ) : (
          <p className="text-sm text-muted-foreground">Waking the Concierge…</p>
        )}
      </div>
    );
  }

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-col items-center overflow-y-auto bg-background"
      data-testid="concierge-screen"
    >
      <div className="flex w-full max-w-xl flex-1 flex-col items-center px-6">
        <div className="flex flex-col items-center pt-16 pb-10">
          <VoiceOrb onActivate={handleOrb} phase={phase} />
          {huddleError ? (
            <p className="mt-3 max-w-sm text-center text-xs text-destructive">
              {huddleError}
            </p>
          ) : null}
        </div>

        <div className="flex w-full flex-col gap-4 pb-6">
          {timeline.map((entry) =>
            entry.kind === "turn" ? (
              <TranscriptLine key={entry.turn.id} turn={entry.turn} />
            ) : (
              <DispatchCard
                intent={entry.intent}
                key={entry.intent.id}
                onApprove={() => handleApprove(entry.intent)}
                onDismiss={() => settleIntent(entry.intent.id, "dismissed")}
              />
            ),
          )}
          {dispatchError ? (
            <p
              className="text-xs text-destructive"
              data-testid="concierge-dispatch-error"
            >
              {dispatchError}
            </p>
          ) : null}
        </div>

        <div className="sticky bottom-0 flex w-full gap-2 bg-background pb-6 pt-2">
          <input
            className={cn(
              "flex-1 rounded-full border border-border/70 bg-muted/30 px-4 py-2 text-sm",
              "outline-none focus-visible:ring-2 focus-visible:ring-primary/50",
            )}
            data-testid="concierge-input"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                handleSend();
              }
            }}
            placeholder="Or type to the Concierge…"
            value={draft}
          />
          <Button
            aria-label="Send"
            className="rounded-full"
            data-testid="concierge-send"
            disabled={draft.trim().length === 0}
            onClick={handleSend}
            size="icon"
            type="button"
          >
            <Send className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}
