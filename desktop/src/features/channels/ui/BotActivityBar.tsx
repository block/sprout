import { Bot, Loader2 } from "lucide-react";

import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

type BotActivityBarProps = {
  agents: ManagedAgent[];
  onOpenAgentSession: (pubkey: string) => void;
  openAgentSessionPubkey: string | null;
  typingBotPubkeys: string[];
};

/**
 * Compact right-aligned row of clickable bot pills.
 * Only renders pills for bots that are currently typing (actively working).
 */
export function BotActivityBar({
  agents,
  onOpenAgentSession,
  openAgentSessionPubkey,
  typingBotPubkeys,
}: BotActivityBarProps) {
  if (typingBotPubkeys.length === 0) {
    return null;
  }

  const typingSet = new Set(
    typingBotPubkeys.map((pubkey) => pubkey.toLowerCase()),
  );

  const typingAgents = agents.filter((agent) =>
    typingSet.has(agent.pubkey.toLowerCase()),
  );

  if (typingAgents.length === 0) {
    return null;
  }

  return (
    <div
      className="flex shrink-0 items-center gap-1.5"
      data-testid="bot-activity-bar"
    >
      {typingAgents.map((agent) => {
        const isSelected =
          openAgentSessionPubkey?.toLowerCase() === agent.pubkey.toLowerCase();
        return (
          <Tooltip key={agent.pubkey}>
            <TooltipTrigger asChild>
              <button
                className={cn(
                  "inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors",
                  isSelected
                    ? "border-primary/40 bg-primary/10 text-primary"
                    : "border-border/60 bg-background text-muted-foreground hover:border-primary/30 hover:bg-primary/5 hover:text-foreground",
                )}
                data-testid={`bot-chip-${agent.pubkey}`}
                onClick={() => onOpenAgentSession(agent.pubkey)}
                type="button"
              >
                <Bot className="h-3 w-3 shrink-0" />
                <span className="max-w-[8rem] truncate">{agent.name}</span>
                <Loader2 className="h-3 w-3 shrink-0 animate-spin opacity-60" />
                <span className="text-[11px] opacity-60">working…</span>
              </button>
            </TooltipTrigger>
            <TooltipContent side="top" className="text-xs">
              {agent.name} is working — click to view activity
            </TooltipContent>
          </Tooltip>
        );
      })}
    </div>
  );
}
