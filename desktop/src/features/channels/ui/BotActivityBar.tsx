import * as React from "react";

import { ExternalLink, Loader2 } from "lucide-react";

import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { UserAvatar } from "@/shared/ui/UserAvatar";

const AGENT_LIST_HEIGHT_ESTIMATE_PX = 220;
const MAX_INLINE_AGENT_AVATARS = 3;

type BotActivityBarProps = {
  agents: ManagedAgent[];
  onDetachAgentSession?: (agent: ManagedAgent) => void;
  onOpenAgentSession: (pubkey: string) => void;
  openAgentSessionPubkey: string | null;
  profiles?: UserProfileLookup;
  typingBotPubkeys: string[];
};

/**
 * Single collected active-agent pill. The dropdown exposes the individual
 * active agents while keeping the composer area visually quiet.
 */
export function BotActivityBar({
  agents,
  onDetachAgentSession,
  onOpenAgentSession,
  openAgentSessionPubkey,
  profiles,
  typingBotPubkeys,
}: BotActivityBarProps) {
  const [isOpen, setIsOpen] = React.useState(false);
  const [contentSide, setContentSide] = React.useState<"top" | "bottom">(
    "bottom",
  );
  const closeTimeoutRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const typingSet = React.useMemo(
    () => new Set(typingBotPubkeys.map((pubkey) => pubkey.toLowerCase())),
    [typingBotPubkeys],
  );
  const typingAgents = React.useMemo(
    () => agents.filter((agent) => typingSet.has(agent.pubkey.toLowerCase())),
    [agents, typingSet],
  );
  const typingAgentKey = typingAgents
    .map((agent) => agent.pubkey.toLowerCase())
    .join("|");

  const clearCloseTimeout = React.useCallback(() => {
    if (closeTimeoutRef.current) {
      window.clearTimeout(closeTimeoutRef.current);
      closeTimeoutRef.current = null;
    }
  }, []);

  const openAgentList = React.useCallback(() => {
    clearCloseTimeout();
    setIsOpen(true);
  }, [clearCloseTimeout]);

  const openAgentListFromTrigger = React.useCallback(
    (event: React.PointerEvent<HTMLButtonElement>) => {
      const triggerRect = event.currentTarget.getBoundingClientRect();
      const availableBelow = window.innerHeight - triggerRect.bottom;
      setContentSide(
        availableBelow < AGENT_LIST_HEIGHT_ESTIMATE_PX ? "top" : "bottom",
      );
      openAgentList();
    },
    [openAgentList],
  );

  const scheduleCloseAgentList = React.useCallback(() => {
    clearCloseTimeout();
    closeTimeoutRef.current = window.setTimeout(() => {
      setIsOpen(false);
      closeTimeoutRef.current = null;
    }, 250);
  }, [clearCloseTimeout]);

  React.useEffect(() => {
    return clearCloseTimeout;
  }, [clearCloseTimeout]);

  if (typingAgents.length === 0) {
    return null;
  }

  const selectedAgent = openAgentSessionPubkey
    ? typingAgents.find(
        (agent) =>
          agent.pubkey.toLowerCase() === openAgentSessionPubkey.toLowerCase(),
      )
    : null;
  const label =
    typingAgents.length === 1
      ? typingAgents[0]?.name
      : `${typingAgents[0]?.name ?? "Agent"} +${typingAgents.length - 1}`;
  const activeAgentCountLabel =
    typingAgents.length === 1
      ? "1 active agent"
      : `${typingAgents.length} active agents`;
  const visibleInlineAgents = typingAgents.slice(0, MAX_INLINE_AGENT_AVATARS);

  return (
    <div className="min-w-0" data-testid="bot-activity-bar">
      <Popover open={isOpen} onOpenChange={setIsOpen}>
        <PopoverTrigger asChild>
          <button
            aria-label={`Show ${typingAgents.length} active agents`}
            className={cn(
              "inline-flex max-w-[18rem] items-center gap-1 rounded-full border bg-background px-2 py-1 text-xs font-medium transition-colors hover:border-primary/30 hover:bg-primary/5 hover:text-foreground data-[state=open]:border-primary/40 data-[state=open]:bg-primary/10 data-[state=open]:text-primary",
              "animate-in fade-in-0 zoom-in-95 duration-200 motion-reduce:animate-none",
              selectedAgent
                ? "border-primary/40 text-primary"
                : "border-border/60 text-muted-foreground",
            )}
            data-agent-key={typingAgentKey}
            data-testid="bot-chip-overflow"
            onPointerEnter={openAgentListFromTrigger}
            onPointerLeave={scheduleCloseAgentList}
            type="button"
          >
            <div className="flex shrink-0 items-center">
              {visibleInlineAgents.map((agent, index) => (
                <div
                  className={cn(index > 0 && "-ml-1.5")}
                  key={agent.pubkey}
                  style={{ zIndex: visibleInlineAgents.length - index }}
                >
                  <UserAvatar
                    avatarUrl={
                      profiles?.[agent.pubkey.toLowerCase()]?.avatarUrl ?? null
                    }
                    className="rounded-full border border-background shadow-none"
                    displayName={agent.name}
                    size="xs"
                  />
                </div>
              ))}
            </div>
            <span className="min-w-0 truncate">{label}</span>
            <Loader2 className="h-3 w-3 shrink-0 animate-spin opacity-60" />
          </button>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          collisionPadding={12}
          className="w-56 p-1"
          onOpenAutoFocus={(event) => event.preventDefault()}
          onPointerEnter={openAgentList}
          onPointerLeave={scheduleCloseAgentList}
          side={contentSide}
          sideOffset={8}
        >
          <div className="px-2 py-1 text-xs font-semibold text-muted-foreground">
            {activeAgentCountLabel}
          </div>
          {typingAgents.map((agent) => (
            <div
              className={cn(
                "flex w-full select-none items-center gap-1 rounded-sm outline-none transition-colors hover:bg-accent hover:text-accent-foreground",
                "animate-in fade-in-0 duration-150 motion-reduce:animate-none",
                selectedAgent?.pubkey === agent.pubkey && "bg-primary/10",
              )}
              key={agent.pubkey}
            >
              <button
                className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 px-2 py-1.5 text-left text-sm outline-none focus-visible:bg-accent focus-visible:text-accent-foreground"
                data-testid={`bot-chip-overflow-item-${agent.pubkey}`}
                onClick={() => {
                  setIsOpen(false);
                  onOpenAgentSession(agent.pubkey);
                }}
                type="button"
              >
                <UserAvatar
                  avatarUrl={
                    profiles?.[agent.pubkey.toLowerCase()]?.avatarUrl ?? null
                  }
                  className="rounded-full"
                  displayName={agent.name}
                  size="sm"
                />
                <span className="min-w-0 flex-1 truncate">{agent.name}</span>
                <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground/70" />
              </button>
              {onDetachAgentSession ? (
                <button
                  aria-label={`Detach ${agent.name} activity`}
                  className="mr-1 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-background/80 hover:text-foreground focus-visible:bg-background/80 focus-visible:text-foreground focus-visible:outline-none"
                  data-testid={`bot-chip-detach-${agent.pubkey}`}
                  onClick={() => {
                    setIsOpen(false);
                    onDetachAgentSession(agent);
                  }}
                  title={`Detach ${agent.name} activity`}
                  type="button"
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                </button>
              ) : null}
            </div>
          ))}
        </PopoverContent>
      </Popover>
    </div>
  );
}
