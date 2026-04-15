import { Headphones } from "lucide-react";
import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { relayClient } from "@/shared/api/relayClient";
import type { RelayEvent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { useHuddle } from "../HuddleContext";

/** Huddle lifecycle event kinds */
const KIND_HUDDLE_STARTED = 48100;
const KIND_HUDDLE_PARTICIPANT_JOINED = 48101;
const KIND_HUDDLE_PARTICIPANT_LEFT = 48102;
const KIND_HUDDLE_ENDED = 48103;

type ActiveHuddle = {
  ephemeralChannelId: string;
  participants: Set<string>;
};

type HuddleIndicatorProps = {
  channelId: string;
  className?: string;
  /** Called when the user clicks the button and no huddle is active (start). */
  onStart?: () => void;
  /** Whether the start action is disabled (e.g., permissions, already starting). */
  startDisabled?: boolean;
};

/**
 * Detects active huddles in a channel via kind:48100-48103 events.
 * Shows a glowing headphone icon when a huddle is active, with participant count.
 * Click to join the huddle.
 */
export function HuddleIndicator({
  channelId,
  className,
  onStart,
  startDisabled,
}: HuddleIndicatorProps) {
  const { joinHuddle, isStarting } = useHuddle();
  const queryClient = useQueryClient();
  const [activeHuddle, setActiveHuddle] = React.useState<ActiveHuddle | null>(
    null,
  );
  const [isJoining, setIsJoining] = React.useState(false);

  React.useEffect(() => {
    if (!channelId) return;

    let disposed = false;
    let cleanup: (() => void) | null = null;

    // Track all seen events for reconstruction. Keyed by event.id for dedup.
    const seenEvents = new Map<string, RelayEvent>();

    /** Reconstruct huddle state from the full set of seen events.
     *  Sort by created_at, then kind (causal: start < join < left < end),
     *  then event id for final tiebreak. This handles out-of-order delivery,
     *  reconnect replay, late mounts, and same-second event batches.
     *
     *  Resilient to missing start event: if we see join/left events for an
     *  ephemeral channel without a prior start, we infer the huddle exists.
     *  This covers the edge case where >100 lifecycle events push the start
     *  event out of the subscription window. */
    function reconstruct() {
      const sorted = [...seenEvents.values()].sort(
        (a, b) =>
          a.created_at - b.created_at ||
          a.kind - b.kind ||
          a.id.localeCompare(b.id),
      );

      let huddle: ActiveHuddle | null = null;

      for (const ev of sorted) {
        let ephId: string | null = null;
        try {
          const content = JSON.parse(ev.content);
          ephId = content.ephemeral_channel_id ?? null;
        } catch {
          continue; // Malformed — skip
        }

        switch (ev.kind) {
          case KIND_HUDDLE_STARTED: {
            if (!ephId) break;
            huddle = {
              ephemeralChannelId: ephId,
              participants: new Set([ev.pubkey]),
            };
            break;
          }
          case KIND_HUDDLE_PARTICIPANT_JOINED: {
            if (!ephId) break;
            // 48101 events are relay-signed — the actual participant is in the "p" tag.
            const joinedPk =
              ev.tags.find((t) => t[0] === "p")?.[1] ?? ev.pubkey;
            if (!huddle || ephId !== huddle.ephemeralChannelId) {
              huddle = {
                ephemeralChannelId: ephId,
                participants: new Set(),
              };
            }
            huddle.participants.add(joinedPk);
            break;
          }
          case KIND_HUDDLE_PARTICIPANT_LEFT: {
            if (!ephId) break;
            // 48102 events are relay-signed — the actual participant is in the "p" tag.
            const leftPk =
              ev.tags.find((t) => t[0] === "p")?.[1] ?? ev.pubkey;
            if (!huddle || ephId !== huddle.ephemeralChannelId) {
              huddle = {
                ephemeralChannelId: ephId,
                participants: new Set(),
              };
            }
            huddle.participants.delete(leftPk);
            break;
          }
          case KIND_HUDDLE_ENDED: {
            if (!huddle || !ephId || ephId !== huddle.ephemeralChannelId) break;
            huddle = null;
            break;
          }
        }
      }

      if (!disposed) {
        setActiveHuddle(huddle);
      }
    }

    // Subscribe to huddle lifecycle events only (kinds 48100–48103).
    // limit: 100 covers long-lived huddles with many join/leave cycles.
    relayClient
      .subscribeToHuddleEvents(channelId, (event: RelayEvent) => {
        if (disposed) return;

        // Dedup by event ID — ignore replayed events from reconnect.
        if (seenEvents.has(event.id)) return;
        seenEvents.set(event.id, event);

        // Reconstruct from full history on every new event.
        // This is cheap — huddle lifecycle events are rare (typically <20).
        reconstruct();
      })
      .then((dispose) => {
        if (disposed) {
          void dispose();
          return;
        }
        cleanup = () => void dispose();
      })
      .catch((err) => {
        console.error("[HuddleIndicator] subscription failed:", err);
      });

    return () => {
      disposed = true;
      cleanup?.();
      setActiveHuddle(null);
    };
  }, [channelId]);

  // No active huddle — render the start button (if onStart provided).
  if (!activeHuddle) {
    if (!onStart) return null;
    return (
      <Button
        aria-label="Start huddle"
        className={cn("h-9 w-9 rounded-full", className)}
        data-testid="channel-start-huddle-trigger"
        disabled={startDisabled || isStarting}
        onClick={() => onStart()}
        size="icon"
        type="button"
        variant="outline"
      >
        <Headphones className="h-4 w-4" />
      </Button>
    );
  }

  // At least 1 participant must exist for the huddle to be active.
  // When START fell out of the event window, the creator isn't in the
  // reconstructed set — floor at 1 to avoid showing "0 participants".
  const participantCount = Math.max(1, activeHuddle.participants.size);

  async function handleJoin() {
    if (!activeHuddle || isJoining) return;
    setIsJoining(true);
    try {
      await joinHuddle(channelId, activeHuddle.ephemeralChannelId);
      // Refetch channels so the ephemeral channel appears in the sidebar.
      void queryClient.invalidateQueries({ queryKey: ["channels"] });
    } catch (e) {
      console.error("Failed to join huddle:", e);
    } finally {
      setIsJoining(false);
    }
  }

  return (
    <Button
      aria-label={`Join active huddle (${participantCount} participant${participantCount !== 1 ? "s" : ""})`}
      className={cn("h-9 w-9 rounded-full relative", className)}
      disabled={isJoining || isStarting}
      onClick={() => void handleJoin()}
      size="icon"
      type="button"
      variant="outline"
      title={`Huddle active — ${participantCount} participant${participantCount !== 1 ? "s" : ""}`}
    >
      <Headphones className="h-4 w-4 text-green-500" />
      {/* Green glow effect */}
      <span className="absolute inset-0 animate-pulse rounded-full ring-2 ring-green-500/40" />
      {/* Participant count badge */}
      {participantCount > 0 && (
        <span className="absolute -right-1 -top-1 flex h-4 min-w-4 items-center justify-center rounded-full bg-green-500 px-0.5 text-[10px] font-bold text-white">
          {participantCount}
        </span>
      )}
    </Button>
  );
}
