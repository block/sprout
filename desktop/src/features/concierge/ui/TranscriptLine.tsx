import { cn } from "@/shared/lib/cn";
import type { ConciergeTurn } from "@/features/concierge/types";

/** One line in the Concierge transcript — you right-aligned, agent left. */
export function TranscriptLine({ turn }: { turn: ConciergeTurn }) {
  const isYou = turn.speaker === "you";
  return (
    <div
      className={cn("flex flex-col gap-1", isYou ? "items-end" : "items-start")}
      data-speaker={turn.speaker}
    >
      <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground/70">
        {isYou ? "You" : "Concierge"}
      </span>
      <p
        className={cn(
          "max-w-[88%] rounded-2xl px-4 py-2.5 text-sm leading-relaxed",
          isYou
            ? "bg-muted/60 text-foreground"
            : "bg-primary/10 text-foreground",
        )}
      >
        {turn.text}
      </p>
    </div>
  );
}
