import * as React from "react";
import { GroupedVirtuoso, type GroupedVirtuosoHandle } from "react-virtuoso";
import { ArrowDown, Loader2 } from "lucide-react";

import {
  formatDayHeading,
  isSameDay,
} from "@/features/messages/lib/dateFormatters";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import { Button } from "@/shared/ui/button";
import { Separator } from "@/shared/ui/separator";
import { TooltipProvider } from "@/shared/ui/tooltip";
import { DayDivider } from "./DayDivider";
import { MessageRow } from "./MessageRow";
import { SystemMessageRow } from "./SystemMessageRow";
import { TimelineSkeleton } from "./TimelineSkeleton";

type MessageTimelineProps = {
  channelId?: string | null;
  messages: TimelineMessage[];
  isLoading?: boolean;
  emptyTitle?: string;
  emptyDescription?: string;
  activeReplyTargetId?: string | null;
  currentPubkey?: string;
  fetchOlder?: () => Promise<void>;
  hasOlderMessages?: boolean;
  isFetchingOlder?: boolean;
  profiles?: UserProfileLookup;
  onEdit?: (message: TimelineMessage) => void;
  onReply?: (message: TimelineMessage) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  targetMessageId?: string | null;
  onTargetReached?: (messageId: string) => void;
};

/**
 * Compute day-based groups from a sorted messages array.
 * Returns { groupCounts, groupLabels } for GroupedVirtuoso.
 */
function useTimelineGroups(messages: TimelineMessage[]) {
  return React.useMemo(() => {
    if (messages.length === 0) {
      return { groupCounts: [] as number[], groupLabels: [] as string[] };
    }

    const groupCounts: number[] = [];
    const groupLabels: string[] = [];
    let currentCount = 1;

    groupLabels.push(formatDayHeading(messages[0].createdAt));

    for (let i = 1; i < messages.length; i++) {
      if (!isSameDay(messages[i - 1].createdAt, messages[i].createdAt)) {
        groupCounts.push(currentCount);
        groupLabels.push(formatDayHeading(messages[i].createdAt));
        currentCount = 1;
      } else {
        currentCount++;
      }
    }
    groupCounts.push(currentCount);

    return { groupCounts, groupLabels };
  }, [messages]);
}

export const MessageTimeline = React.memo(function MessageTimeline({
  channelId,
  messages,
  isLoading = false,
  emptyTitle = "No messages yet",
  emptyDescription = "Send the first message to start the thread.",
  activeReplyTargetId = null,
  currentPubkey,
  fetchOlder,
  hasOlderMessages = true,
  isFetchingOlder = false,
  profiles,
  onEdit,
  onReply,
  onToggleReaction,
  targetMessageId = null,
  onTargetReached,
}: MessageTimelineProps) {
  const virtuosoRef = React.useRef<GroupedVirtuosoHandle>(null);
  const [isAtBottom, setIsAtBottom] = React.useState(true);
  const [newMessageCount, setNewMessageCount] = React.useState(0);
  const [highlightedMessageId, setHighlightedMessageId] = React.useState<
    string | null
  >(null);
  const previousLastMessageIdRef = React.useRef<string | undefined>(undefined);
  const previousMessageCountRef = React.useRef(0);
  const isFetchingOlderRef = React.useRef(false);
  const handledTargetMessageIdRef = React.useRef<string | null>(null);

  const { groupCounts, groupLabels } = useTimelineGroups(messages);

  // Reset state on channel change
  // biome-ignore lint/correctness/useExhaustiveDependencies: channelId is the sole trigger for resetting scroll state
  React.useEffect(() => {
    setIsAtBottom(true);
    setNewMessageCount(0);
    setHighlightedMessageId(null);
    previousLastMessageIdRef.current = undefined;
    previousMessageCountRef.current = 0;
    handledTargetMessageIdRef.current = null;
  }, [channelId]);

  // Track new messages when scrolled up
  React.useEffect(() => {
    if (isLoading || messages.length === 0) {
      previousLastMessageIdRef.current =
        messages.length > 0 ? messages[messages.length - 1]?.id : undefined;
      previousMessageCountRef.current = messages.length;
      return;
    }

    const latestMessage = messages[messages.length - 1];
    const previousLastMessageId = previousLastMessageIdRef.current;
    const previousMessageCount = previousMessageCountRef.current;

    if (
      latestMessage.id !== previousLastMessageId &&
      previousLastMessageId !== undefined
    ) {
      if (!isAtBottom && !latestMessage.accent) {
        const addedMessages = Math.max(
          1,
          messages.length - previousMessageCount,
        );
        setNewMessageCount((c) => c + addedMessages);
      }
    }

    previousLastMessageIdRef.current = latestMessage.id;
    previousMessageCountRef.current = messages.length;
  }, [isLoading, messages, isAtBottom]);

  const handleAtBottomStateChange = React.useCallback((atBottom: boolean) => {
    setIsAtBottom(atBottom);
    if (atBottom) {
      setNewMessageCount(0);
    }
  }, []);

  // followOutput: auto-scroll to bottom when new messages arrive (if already at bottom)
  const followOutput = React.useCallback((isAtBottomParam: boolean) => {
    if (isAtBottomParam) {
      return "smooth";
    }
    return false;
  }, []);

  // Load older messages when scrolling to top
  const handleStartReached = React.useCallback(() => {
    if (!fetchOlder || !hasOlderMessages || isFetchingOlderRef.current) {
      return;
    }
    isFetchingOlderRef.current = true;
    void fetchOlder().finally(() => {
      isFetchingOlderRef.current = false;
    });
  }, [fetchOlder, hasOlderMessages]);

  const scrollToBottom = React.useCallback(
    (behavior: "auto" | "smooth" = "smooth") => {
      virtuosoRef.current?.scrollToIndex({
        index: "LAST",
        behavior,
      });
      setNewMessageCount(0);
    },
    [],
  );

  // Handle target message scrolling (e.g. jump-to-message)
  React.useEffect(() => {
    if (!targetMessageId) {
      handledTargetMessageIdRef.current = null;
      setHighlightedMessageId(null);
      return;
    }

    if (handledTargetMessageIdRef.current === targetMessageId || isLoading) {
      return;
    }

    const targetIndex = messages.findIndex((m) => m.id === targetMessageId);
    if (targetIndex === -1) {
      return;
    }

    handledTargetMessageIdRef.current = targetMessageId;
    setHighlightedMessageId(targetMessageId);
    onTargetReached?.(targetMessageId);

    virtuosoRef.current?.scrollToIndex({
      index: targetIndex,
      align: "center",
      behavior: "smooth",
    });

    const timeout = window.setTimeout(() => {
      setHighlightedMessageId((current) =>
        current === targetMessageId ? null : current,
      );
    }, 2_000);

    return () => {
      window.clearTimeout(timeout);
    };
  }, [isLoading, messages, onTargetReached, targetMessageId]);

  const groupContent = React.useCallback(
    (index: number) => <DayDivider label={groupLabels[index]} />,
    [groupLabels],
  );

  const itemContent = React.useCallback(
    (index: number) => {
      const message = messages[index];
      if (!message) {
        return null;
      }

      if (message.kind === KIND_SYSTEM_MESSAGE) {
        return (
          <SystemMessageRow
            body={message.body}
            createdAt={message.createdAt}
            currentPubkey={currentPubkey}
            profiles={profiles}
            time={message.time}
          />
        );
      }

      return (
        <MessageRow
          activeReplyTargetId={activeReplyTargetId}
          highlighted={message.id === highlightedMessageId}
          message={message}
          onEdit={
            onEdit && currentPubkey && message.pubkey === currentPubkey
              ? onEdit
              : undefined
          }
          onToggleReaction={onToggleReaction}
          onReply={onReply}
          profiles={profiles}
        />
      );
    },
    [
      messages,
      activeReplyTargetId,
      currentPubkey,
      highlightedMessageId,
      onEdit,
      onReply,
      onToggleReaction,
      profiles,
    ],
  );

  const Header = React.useCallback(() => {
    if (isFetchingOlder) {
      return (
        <div className="flex justify-center py-2">
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        </div>
      );
    }
    if (!hasOlderMessages && !isLoading && messages.length > 0) {
      return (
        <div
          className="flex items-center gap-3 py-2"
          data-testid="message-timeline-beginning"
        >
          <Separator className="flex-1" />
          <p className="text-xs font-semibold uppercase tracking-[0.22em] text-muted-foreground">
            Beginning of conversation
          </p>
          <Separator className="flex-1" />
        </div>
      );
    }
    return null;
  }, [isFetchingOlder, hasOlderMessages, isLoading, messages.length]);

  const showVirtuoso = !isLoading && messages.length > 0;

  return (
    <TooltipProvider delayDuration={200}>
      <div className="relative min-h-0 flex-1">
        {isLoading ? (
          <div className="h-full overflow-y-auto px-4 py-3 sm:px-6">
            <div className="mx-auto w-full max-w-4xl">
              <TimelineSkeleton />
            </div>
          </div>
        ) : null}

        {!isLoading && messages.length === 0 ? (
          <div className="h-full overflow-y-auto px-4 py-3 sm:px-6">
            <div className="mx-auto w-full max-w-4xl">
              <div
                className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center shadow-sm"
                data-testid="message-empty"
              >
                <p className="text-base font-semibold tracking-tight">
                  {emptyTitle}
                </p>
                <p className="mt-2 text-sm text-muted-foreground">
                  {emptyDescription}
                </p>
              </div>
            </div>
          </div>
        ) : null}

        {showVirtuoso ? (
          <GroupedVirtuoso
            ref={virtuosoRef}
            className="h-full overflow-x-hidden overscroll-contain px-4 py-3 sm:px-6"
            data-testid="message-timeline"
            groupCounts={groupCounts}
            groupContent={groupContent}
            itemContent={itemContent}
            components={{ Header }}
            initialTopMostItemIndex={messages.length - 1}
            followOutput={followOutput}
            atBottomStateChange={handleAtBottomStateChange}
            atBottomThreshold={72}
            startReached={handleStartReached}
            increaseViewportBy={200}
            style={{
              // Outer container needs flex-1 + full height
              height: "100%",
            }}
          />
        ) : null}

        {!isAtBottom && showVirtuoso ? (
          <div className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center px-4">
            <Button
              className="pointer-events-auto rounded-full shadow-lg"
              data-testid="message-scroll-to-latest"
              onClick={() => scrollToBottom("smooth")}
              size="sm"
              type="button"
            >
              <ArrowDown className="h-4 w-4" />
              {newMessageCount > 0
                ? `${newMessageCount} new message${newMessageCount === 1 ? "" : "s"}`
                : "Jump to latest"}
            </Button>
          </div>
        ) : null}
      </div>
    </TooltipProvider>
  );
});
