import * as React from "react";

import type {
  ConciergePhase,
  ConciergeTurn,
  DispatchIntent,
} from "@/features/concierge/types";
import { DispatchCard } from "@/features/concierge/ui/DispatchCard";
import { TranscriptLine } from "@/features/concierge/ui/TranscriptLine";
import { VoiceOrb } from "@/features/concierge/ui/VoiceOrb";

const PHASE_CYCLE: Record<ConciergePhase, ConciergePhase> = {
  idle: "listening",
  listening: "thinking",
  thinking: "speaking",
  speaking: "idle",
};

/** Demo seed — populates the view for the screenshot/aesthetics loop. The live
 *  screen will feed these from the persistent DM channel + dispatch intents. */
const SEED_TRANSCRIPT: ConciergeTurn[] = [
  { id: "t1", speaker: "you", text: "Has CI gone green on the relay PR yet?" },
  {
    id: "t2",
    speaker: "concierge",
    text: "Not yet — the build is still running. Want me to have Max keep an eye on it and ping you when it lands?",
  },
  { id: "t3", speaker: "you", text: "Yeah, do that." },
];

const SEED_DISPATCH: DispatchIntent = {
  id: "d1",
  agent: "Max",
  channel: "sprout-conversational-agents",
  instruction: "Watch CI on the relay PR; ping Tyler the moment it goes green.",
  status: "pending",
};

type ConciergeScreenProps = {
  /** Screenshot/test override — render a specific phase. */
  initialPhase?: ConciergePhase;
  /** Screenshot/test override — render the dispatch card in a settled state. */
  initialDispatchStatus?: DispatchIntent["status"];
};

export function ConciergeScreen({
  initialPhase = "idle",
  initialDispatchStatus = "pending",
}: ConciergeScreenProps) {
  const [phase, setPhase] = React.useState<ConciergePhase>(initialPhase);
  const [transcript] = React.useState(SEED_TRANSCRIPT);
  const [dispatch, setDispatch] = React.useState<DispatchIntent>({
    ...SEED_DISPATCH,
    status: initialDispatchStatus,
  });

  const handleActivate = React.useCallback(() => {
    setPhase((prev) => PHASE_CYCLE[prev]);
  }, []);

  const resolveDispatch = React.useCallback(
    (status: DispatchIntent["status"]) => (id: string) =>
      setDispatch((prev) => (prev.id === id ? { ...prev, status } : prev)),
    [],
  );

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-col items-center overflow-y-auto bg-background"
      data-testid="concierge-screen"
    >
      <div className="flex w-full max-w-xl flex-1 flex-col items-center px-6">
        {/* Focal orb */}
        <div className="flex flex-col items-center pt-16 pb-10">
          <VoiceOrb onActivate={handleActivate} phase={phase} />
        </div>

        {/* Transcript spine */}
        <div className="flex w-full flex-col gap-4 pb-12">
          {transcript.map((turn) => (
            <TranscriptLine key={turn.id} turn={turn} />
          ))}

          <DispatchCard
            intent={dispatch}
            onApprove={resolveDispatch("approved")}
            onDismiss={resolveDispatch("dismissed")}
          />
        </div>
      </div>
    </div>
  );
}
