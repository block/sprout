import { Mic } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import type { ConciergePhase } from "@/features/concierge/types";

import "./concierge.css";

const PHASE_LABEL: Record<ConciergePhase, string> = {
  idle: "Tap to speak",
  listening: "Listening…",
  thinking: "Thinking…",
  speaking: "Speaking…",
};

type VoiceOrbProps = {
  phase: ConciergePhase;
  onActivate: () => void;
};

/**
 * The focal control of the Concierge view. One glowing sphere whose animation
 * encodes the voice-loop phase. Clicking it toggles listening (push-to-talk in
 * v1; barge-in is deferred huddle-layer polish).
 */
export function VoiceOrb({ phase, onActivate }: VoiceOrbProps) {
  return (
    <div className="flex flex-col items-center gap-5">
      <button
        aria-label={PHASE_LABEL[phase]}
        className="concierge-orb rounded-full outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
        data-phase={phase}
        data-testid="concierge-orb"
        onClick={onActivate}
        type="button"
      >
        <span className="concierge-orb__ring" />
        <span className="concierge-orb__core" />
        <Mic
          className={cn(
            "pointer-events-none absolute h-7 w-7 text-primary-foreground/90 transition-opacity",
            phase === "idle" ? "opacity-80" : "opacity-0",
          )}
        />
      </button>
      <span
        className="text-sm font-medium tracking-wide text-muted-foreground"
        data-testid="concierge-phase-label"
      >
        {PHASE_LABEL[phase]}
      </span>
    </div>
  );
}
