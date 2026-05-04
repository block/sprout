import { Bot, Loader2 } from "lucide-react";

import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

type BotActivityBarProps = {
  agents: ManagedAgent[];
  onOpenAgentSession: (pubkey: string) => void;
  openAgentSessionPubkey: string | null;
  typingBotPubkeys: string[];
};

const COMPACT_THRESHOLD = 4;
const OVERFLOW_THRESHOLD = 6;
const MAX_VISIBLE_WITH_OVERFLOW = 5;

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

  const { hiddenAgents, visibleAgents } = splitVisibleAgents(
    typingAgents,
    openAgentSessionPubkey,
  );
  const isCompact = typingAgents.length >= COMPACT_THRESHOLD;

  return (
    <div
      className="flex min-w-0 shrink items-center justify-start gap-1 overflow-hidden"
      data-testid="bot-activity-bar"
    >
      {visibleAgents.map((agent) => {
        const isSelected =
          openAgentSessionPubkey?.toLowerCase() === agent.pubkey.toLowerCase();
        return (
          <Tooltip key={agent.pubkey}>
            <TooltipTrigger asChild>
              <button
                className={cn(
                  "inline-flex min-w-0 shrink items-center gap-1 rounded-full border py-1 text-xs font-medium transition-colors",
                  isCompact ? "max-w-[6.5rem] px-2" : "max-w-[9rem] px-2.5",
                  isSelected
                    ? "border-primary/40 bg-primary/10 text-primary"
                    : "border-border/60 bg-background text-muted-foreground hover:border-primary/30 hover:bg-primary/5 hover:text-foreground",
                )}
                data-testid={`bot-chip-${agent.pubkey}`}
                onClick={() => onOpenAgentSession(agent.pubkey)}
                type="button"
              >
                <Bot className="h-3 w-3 shrink-0" />
                <span className="min-w-0 truncate">{agent.name}</span>
                <Loader2 className="h-3 w-3 shrink-0 animate-spin opacity-60" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top" className="text-xs">
              {agent.name} is working — click to view activity
            </TooltipContent>
          </Tooltip>
        );
      })}

      {hiddenAgents.length > 0 ? (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              aria-label={`Show ${hiddenAgents.length} more working agents`}
              className="inline-flex shrink-0 items-center gap-1 rounded-full border border-border/60 bg-background px-2 py-1 text-xs font-medium text-muted-foreground transition-colors hover:border-primary/30 hover:bg-primary/5 hover:text-foreground data-[state=open]:border-primary/40 data-[state=open]:bg-primary/10 data-[state=open]:text-primary"
              data-testid="bot-chip-overflow"
              type="button"
            >
              +{hiddenAgents.length}
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            <DropdownMenuLabel className="px-2 py-1 text-xs text-muted-foreground">
              More agents working
            </DropdownMenuLabel>
            {hiddenAgents.map((agent) => (
              <DropdownMenuItem
                className="cursor-pointer"
                data-testid={`bot-chip-overflow-item-${agent.pubkey}`}
                key={agent.pubkey}
                onClick={() => onOpenAgentSession(agent.pubkey)}
              >
                <Bot className="h-4 w-4 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate">{agent.name}</span>
                <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground/70" />
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </div>
  );
}

function splitVisibleAgents(
  typingAgents: ManagedAgent[],
  openAgentSessionPubkey: string | null,
): { visibleAgents: ManagedAgent[]; hiddenAgents: ManagedAgent[] } {
  if (typingAgents.length < OVERFLOW_THRESHOLD) {
    return { visibleAgents: typingAgents, hiddenAgents: [] };
  }

  const selectedAgent = openAgentSessionPubkey
    ? typingAgents.find(
        (agent) =>
          agent.pubkey.toLowerCase() === openAgentSessionPubkey.toLowerCase(),
      )
    : null;

  const visibleAgents = typingAgents.slice(0, MAX_VISIBLE_WITH_OVERFLOW);

  if (
    selectedAgent &&
    !visibleAgents.some((agent) => agent.pubkey === selectedAgent.pubkey)
  ) {
    visibleAgents[visibleAgents.length - 1] = selectedAgent;
  }

  const visibleSet = new Set(visibleAgents.map((agent) => agent.pubkey));
  const hiddenAgents = typingAgents.filter(
    (agent) => !visibleSet.has(agent.pubkey),
  );

  return { visibleAgents, hiddenAgents };
}
