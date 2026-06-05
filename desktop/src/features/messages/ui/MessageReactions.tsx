import * as React from "react";

import type { TimelineReaction } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import { emojiDisplayName } from "@/shared/lib/emojiName";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

/**
 * Render a reaction's emoji: a custom (image) emoji when `emojiUrl` is set,
 * otherwise the unicode/text glyph. `className` sizes the image to match the
 * surrounding text. The relay URL is rewritten through the localhost media
 * proxy (like every other relay-hosted <img>) — WKWebView bypasses WARP, so a
 * direct relay URL gets a Cloudflare Access 403 and renders as a broken image.
 */
function EmojiGlyph({
  reaction,
  className,
}: {
  reaction: TimelineReaction;
  className?: string;
}) {
  const displayName = emojiDisplayName(reaction.emoji);
  if (reaction.emojiUrl) {
    return (
      <img
        alt={reaction.emoji}
        title={displayName}
        src={rewriteRelayUrl(reaction.emojiUrl)}
        className={cn(
          "inline-block object-contain align-text-bottom",
          className,
        )}
        draggable={false}
      />
    );
  }
  return (
    <span
      className={cn("inline-block leading-none", className)}
      title={displayName}
    >
      {reaction.emoji}
    </span>
  );
}

function formatReactionUsers(reaction: TimelineReaction): string {
  const names = reaction.users.map((user) => user.displayName).filter(Boolean);
  if (reaction.reactedByCurrentUser) {
    const others = names.filter((name) => name !== "You");
    names.splice(0, names.length, "You (click to remove)", ...others);
  }
  if (names.length === 0) return `${reaction.count} people`;
  if (names.length === 1) return names[0];
  if (names.length === 2) return `${names[0]} and ${names[1]}`;
  return `${names.slice(0, -1).join(", ")}, and ${names[names.length - 1]}`;
}

function ReactionPopoverContent({ reaction }: { reaction: TimelineReaction }) {
  const displayName = emojiDisplayName(reaction.emoji);
  const userText = formatReactionUsers(reaction);

  return (
    <div className="flex flex-col items-center text-center">
      <div className="mb-2 flex h-14 w-14 items-center justify-center">
        <EmojiGlyph
          reaction={reaction}
          className={reaction.emojiUrl ? "h-12 w-12" : "text-4xl"}
        />
      </div>
      <div className="max-w-[14rem] text-balance text-sm font-semibold leading-snug text-popover-foreground">
        {userText} <span className="text-muted-foreground">reacted with</span>
      </div>
      <div className="mt-0.5 text-sm font-semibold leading-snug text-muted-foreground">
        {displayName}
      </div>
    </div>
  );
}

export function MessageReactions({
  messageId,
  reactions,
  canToggle,
  pending,
  onSelect,
  className,
}: {
  messageId: string;
  reactions: TimelineReaction[];
  canToggle: boolean;
  pending: boolean;
  onSelect: (emoji: string) => void;
  className?: string;
}) {
  if (reactions.length === 0) {
    return null;
  }

  return (
    <div
      className={cn(
        "mt-1.5 flex flex-wrap items-center gap-1.5 pt-1",
        className,
      )}
    >
      {reactions.map((reaction) => (
        <ReactionPill
          key={`${messageId}-${reaction.emoji}`}
          canToggle={canToggle}
          pending={pending}
          reaction={reaction}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}

function ReactionPill({
  reaction,
  canToggle,
  pending,
  onSelect,
}: {
  reaction: TimelineReaction;
  canToggle: boolean;
  pending: boolean;
  onSelect: (emoji: string) => void;
}) {
  const [open, setOpen] = React.useState(false);
  const openTimeout = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTimeout = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimers = React.useCallback(() => {
    if (openTimeout.current) {
      clearTimeout(openTimeout.current);
      openTimeout.current = null;
    }
    if (closeTimeout.current) {
      clearTimeout(closeTimeout.current);
      closeTimeout.current = null;
    }
  }, []);

  const handleMouseEnter = React.useCallback(() => {
    if (reaction.users.length === 0) return;
    clearTimers();
    openTimeout.current = setTimeout(() => setOpen(true), 200);
  }, [reaction.users.length, clearTimers]);

  const scheduleClose = React.useCallback(() => {
    clearTimers();
    closeTimeout.current = setTimeout(() => setOpen(false), 150);
  }, [clearTimers]);

  const handleFocus = React.useCallback(() => {
    if (reaction.users.length === 0) return;
    clearTimers();
    setOpen(true);
  }, [reaction.users.length, clearTimers]);

  React.useEffect(() => {
    return clearTimers;
  }, [clearTimers]);

  const pillClasses = cn(
    "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium transition-colors",
    reaction.reactedByCurrentUser
      ? "border-primary/40 bg-primary/10 text-primary"
      : "border-border/70 bg-muted/70 text-foreground/90",
    canToggle
      ? "hover:bg-accent hover:text-accent-foreground focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
      : "cursor-default",
  );

  const handleClick = () => {
    if (!canToggle) return;
    onSelect(reaction.emoji);
  };

  const displayName = emojiDisplayName(reaction.emoji);

  if (reaction.users.length === 0) {
    return (
      <button
        aria-label={`Toggle ${reaction.emoji} reaction`}
        aria-pressed={reaction.reactedByCurrentUser}
        title={displayName}
        className={pillClasses}
        disabled={!canToggle || pending}
        onClick={handleClick}
        type="button"
      >
        <EmojiGlyph reaction={reaction} className="h-[1.1em] w-[1.1em]" />
        <span className="text-muted-foreground">{reaction.count}</span>
      </button>
    );
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        {/* biome-ignore lint/a11y/noStaticElementInteractions: span delegates hover/focus to disabled button */}
        <span
          className="inline-flex"
          onMouseEnter={handleMouseEnter}
          onMouseLeave={scheduleClose}
          onFocus={handleFocus}
          onBlur={scheduleClose}
        >
          <button
            aria-label={`Toggle ${reaction.emoji} reaction`}
            aria-pressed={reaction.reactedByCurrentUser}
            title={displayName}
            className={pillClasses}
            disabled={!canToggle || pending}
            onClick={handleClick}
            type="button"
          >
            <EmojiGlyph reaction={reaction} className="h-[1.1em] w-[1.1em]" />
            <span className="text-muted-foreground">{reaction.count}</span>
          </button>
        </span>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        side="top"
        sideOffset={6}
        className="w-auto min-w-56 max-w-72 rounded-xl p-3"
        onMouseEnter={handleMouseEnter}
        onMouseLeave={scheduleClose}
        onOpenAutoFocus={(e) => e.preventDefault()}
        onCloseAutoFocus={(e) => e.preventDefault()}
      >
        <ReactionPopoverContent reaction={reaction} />
      </PopoverContent>
    </Popover>
  );
}
