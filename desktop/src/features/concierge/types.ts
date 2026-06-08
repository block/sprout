/** Concierge voice loop phase — drives the orb's visual state. */
export type ConciergePhase = "idle" | "listening" | "thinking" | "speaking";

/** A line in the Concierge transcript (the persistent DM channel, rendered). */
export type ConciergeTurn = {
  id: string;
  speaker: "you" | "concierge";
  text: string;
};

/**
 * A pending `dispatch_agent` intent the Concierge wants to act on. Rendered as
 * an approve-before-send card — the card is the safety boundary, not the prompt.
 */
export type DispatchIntent = {
  id: string;
  agent: string;
  channel: string;
  instruction: string;
  status: "pending" | "approved" | "dismissed";
};
