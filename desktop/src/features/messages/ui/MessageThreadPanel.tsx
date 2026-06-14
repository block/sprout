import * as React from "react";
import { ArrowDown, ArrowLeft, X } from "lucide-react";

import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
import type { ImetaMedia } from "@/features/messages/lib/imetaMediaMarkdown";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { Channel } from "@/shared/api/types";
import { useEscapeKey } from "@/shared/hooks/useEscapeKey";
import { useIsThreadPanelOverlay } from "@/shared/hooks/use-mobile";
import { THREAD_PANEL_MIN_WIDTH_PX } from "@/shared/hooks/useThreadPanelWidth";
import { cn } from "@/shared/lib/cn";
import {
  AuxiliaryPanelHeader,
  AuxiliaryPanelHeaderGroup,
  AuxiliaryPanelTitle,
  auxiliaryPanelContentPaddingClass,
} from "@/shared/layout/AuxiliaryPanelHeader";
import { Button } from "@/shared/ui/button";
import {
  OverlayPanelBackdrop,
  PANEL_BASE_CLASS,
  PANEL_OVERLAY_CLASS,
  PANEL_SINGLE_COLUMN_HEADER_LAYER_CLASS,
} from "@/shared/ui/OverlayPanelBackdrop";
import { MessageComposer } from "./MessageComposer";
import { MessageRow } from "./MessageRow";
import { MessageThreadSummaryRow } from "./MessageThreadSummaryRow";
import { TypingIndicatorRow } from "./TypingIndicatorRow";
import { useComposerHeightPadding } from "./useComposerHeightPadding";
import { useTimelineScrollManager } from "./useTimelineScrollManager";
import { selectDeferredListRenderState } from "@/features/messages/lib/timelineDecisions";

type MessageThreadPanelProps = {
  agentPubkeys?: ReadonlySet<string>;
  channel: Channel | null;
  channelId: string | null;
  channelName: string;
  currentPubkey?: string;
  disabled?: boolean;
  layout?: "standalone" | "split";
  editTarget?: {
    author: string;
    body: string;
    id: string;
    imetaMedia?: ImetaMedia[];
  } | null;
  isSending: boolean;
  isSinglePanelView?: boolean;
  onCancelEdit?: () => void;
  onCancelReply: () => void;
  onClose: () => void;
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onEditLastOwnMessage?: () => boolean;
  onEditSave?: (content: string, mediaTags?: string[][]) => Promise<void>;
  onMarkUnread?: (message: TimelineMessage) => void;
  onExpandReplies: (message: TimelineMessage) => void;
  onScrollTargetResolved: () => void;
  onSelectReplyTarget: (message: TimelineMessage) => void;
  onSend: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  profiles?: UserProfileLookup;
  replyTargetMessage: TimelineMessage | null;
  scrollTargetId: string | null;
  threadHead: TimelineMessage | null;
  threadReplies: MainTimelineEntry[];
  threadTypingPubkeys: string[];
  toolbarExtraActions?: React.ReactNode;
  widthPx: number;
  isFollowingThread?: boolean;
  onFollowThread?: () => void;
  onUnfollowThread?: () => void;
};

/** Stable empty reference used as the `useDeferredValue` initial value so the
 *  first render when a thread opens stays light instead of blocking on the full
 *  reply list. Must be module-level so its identity never changes. Mirrors
 *  `EMPTY_MESSAGES` in MessageTimeline (Phase A.1). */
const EMPTY_THREAD_REPLIES: MainTimelineEntry[] = [];

function canManageMessage(
  message: TimelineMessage,
  currentPubkey: string | undefined,
): boolean {
  return Boolean(
    currentPubkey &&
      message.pubkey &&
      currentPubkey.toLowerCase() === message.pubkey.toLowerCase(),
  );
}

export function MessageThreadPanel({
  agentPubkeys,
  channel,
  channelId,
  channelName,
  currentPubkey,
  disabled = false,
  layout = "standalone",
  editTarget,
  isSending,
  isSinglePanelView = false,
  isFollowingThread,
  onCancelEdit,
  onCancelReply,
  onClose,
  onDelete,
  onEdit,
  onEditLastOwnMessage,
  onEditSave,
  onFollowThread,
  onMarkUnread,
  onExpandReplies,
  onScrollTargetResolved,
  onSelectReplyTarget,
  onSend,
  onToggleReaction,
  onUnfollowThread,
  profiles,
  replyTargetMessage,
  scrollTargetId,
  threadHead,
  threadReplies,
  threadTypingPubkeys,
  toolbarExtraActions,
  widthPx,
}: MessageThreadPanelProps) {
  const threadBodyRef = React.useRef<HTMLDivElement>(null);
  const threadComposerWrapperRef = React.useRef<HTMLDivElement>(null);
  const isOverlay = useIsThreadPanelOverlay();
  const isFloatingOverlay = isOverlay && !isSinglePanelView;
  const isSplitLayout = layout === "split";
  useEscapeKey(onClose, isOverlay || isSinglePanelView);
  useComposerHeightPadding(
    threadBodyRef,
    threadComposerWrapperRef,
    isSinglePanelView,
  );

  const threadHeadId = threadHead?.id ?? null;

  const composerReplyTarget =
    replyTargetMessage && threadHead && replyTargetMessage.id !== threadHead.id
      ? {
          author: replyTargetMessage.author,
          body: replyTargetMessage.body,
          id: replyTargetMessage.id,
        }
      : null;

  // Phase A.2 perf: the thread side pane renders its reply list straight into
  // heavy `react-markdown` rows (`MessageRow`) with no deferral, so opening a
  // deep thread blocks the main thread and the OS shows the busy cursor —
  // exactly the freeze Phase A.1 fixed on the main timeline. Gate the reply
  // render behind the same React concurrency primitive. `initialValue: []`
  // keeps even the FIRST render on thread-open light; the heavy list streams in
  // on a deferred, interruptible commit. We deliberately drive BOTH the scroll
  // manager and the rendered list off the SAME deferred value — sticky-bottom /
  // deep-link logic reads the DOM (`scrollIntoView`), so it must stay consistent
  // with what's actually painted. You can't scroll to a reply that hasn't
  // committed yet. This is the shared-snapshot / no-tearing guarantee, here
  // inherited for free: the thread pane routes through the same
  // `useTimelineScrollManager` (and its `timelineDecisions` helpers) as A.1.
  const deferredThreadReplies = React.useDeferredValue(
    threadReplies,
    EMPTY_THREAD_REPLIES,
  );
  const isRepliesPending = deferredThreadReplies !== threadReplies;

  // Which of the three states the reply region paints this frame. Delegated to
  // a pure helper so the "don't flash empty over an incoming list" rule is
  // covered in the lib test suite (see selectDeferredListRenderState).
  const repliesRenderState = selectDeferredListRenderState(
    deferredThreadReplies.length,
    threadReplies.length,
  );

  const threadMessages = React.useMemo(
    () => deferredThreadReplies.map((entry) => entry.message),
    [deferredThreadReplies],
  );

  const {
    bottomAnchorRef,
    contentRef,
    isAtBottom,
    newMessageCount,
    scrollToBottom,
    syncScrollState,
  } = useTimelineScrollManager({
    channelId: threadHeadId,
    isLoading: false,
    messages: threadMessages,
    onTargetReached: onScrollTargetResolved,
    scrollContainerRef: threadBodyRef,
    targetMessageId: scrollTargetId,
  });

  if (!threadHead) {
    return null;
  }

  const threadScrollRegion = (
    <div
      className={cn(
        "min-h-0 flex-1 overflow-y-auto overflow-x-hidden overscroll-contain pb-24 [overflow-anchor:none]",
        isSplitLayout && auxiliaryPanelContentPaddingClass,
        !isSplitLayout && !isFloatingOverlay && "pt-[4.75rem]",
      )}
      data-testid="message-thread-body"
      onScroll={syncScrollState}
      ref={threadBodyRef}
    >
      <div ref={contentRef}>
        <div className="px-3 pb-1 pt-0" data-testid="message-thread-head">
          <div className="rounded-2xl">
            <MessageRow
              agentPubkeys={agentPubkeys}
              channelId={channelId}
              isFollowingThread={isFollowingThread}
              layoutVariant="thread-reply"
              message={threadHead}
              onDelete={
                onDelete && canManageMessage(threadHead, currentPubkey)
                  ? onDelete
                  : undefined
              }
              onEdit={
                onEdit && canManageMessage(threadHead, currentPubkey)
                  ? onEdit
                  : undefined
              }
              onFollowThread={
                onFollowThread ? (_msg) => onFollowThread() : undefined
              }
              onMarkUnread={onMarkUnread}
              onToggleReaction={onToggleReaction}
              onUnfollowThread={
                onUnfollowThread ? (_msg) => onUnfollowThread() : undefined
              }
              profiles={profiles}
            />
          </div>
        </div>

        <div className="px-3 pb-3 pt-1" data-testid="message-thread-replies">
          {repliesRenderState === "list" ? (
            <div
              className={cn(
                "space-y-2.5",
                // Phase A.2: while a deferred render is in flight the painted
                // reply list lags the latest `threadReplies`. Dim it slightly so
                // the streaming-in reads as intentional instead of frozen —
                // mirrors the main timeline (A.1).
                isRepliesPending && "opacity-60 transition-opacity",
              )}
              data-render-pending={isRepliesPending ? "true" : undefined}
            >
              {deferredThreadReplies.map((entry) => {
                return (
                  <div
                    className={cn(
                      "flex flex-col gap-1",
                      entry.summary &&
                        "group/message -mx-1 rounded-2xl px-1 py-1 transition-colors hover:bg-muted/50 focus-within:bg-muted/50",
                    )}
                    key={entry.message.renderKey ?? entry.message.id}
                  >
                    <MessageRow
                      agentPubkeys={agentPubkeys}
                      channelId={channelId}
                      hoverBackground={!entry.summary}
                      layoutVariant="thread-reply"
                      message={entry.message}
                      onDelete={
                        onDelete &&
                        canManageMessage(entry.message, currentPubkey)
                          ? onDelete
                          : undefined
                      }
                      onEdit={
                        onEdit && canManageMessage(entry.message, currentPubkey)
                          ? onEdit
                          : undefined
                      }
                      onMarkUnread={onMarkUnread}
                      onReply={onSelectReplyTarget}
                      onToggleReaction={onToggleReaction}
                      profiles={profiles}
                    />
                    {entry.summary ? (
                      <MessageThreadSummaryRow
                        depth={entry.message.depth}
                        message={entry.message}
                        onOpenThread={onExpandReplies}
                        summary={entry.summary}
                      />
                    ) : null}
                  </div>
                );
              })}
            </div>
          ) : repliesRenderState === "empty" ? (
            // Only show the empty state when the thread is GENUINELY empty.
            // Keying off `deferredThreadReplies` would flash "No replies" for a
            // frame while a non-empty list streams in on the deferred commit.
            <div className="rounded-2xl border border-dashed border-border/70 bg-card/40 px-4 py-6 text-center">
              <p className="text-sm font-medium text-foreground/80">
                No replies in this branch yet
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                Reply in the thread to continue this branch.
              </p>
            </div>
          ) : // "pending": deferred list is empty but the live list has content —
          // rows are streaming in on the deferred commit. Paint nothing rather
          // than flashing the empty state.
          null}
          <div aria-hidden className="h-px" ref={bottomAnchorRef} />
        </div>
      </div>
    </div>
  );

  const threadFooter = (
    <>
      {!isAtBottom ? (
        <div className="pointer-events-none absolute inset-x-0 bottom-36 z-20 flex justify-center px-4">
          <Button
            className="pointer-events-auto h-7 min-h-7 gap-1.5 rounded-full border-border/50 bg-background/85 px-2.5 text-[11px] font-medium text-muted-foreground shadow-xs backdrop-blur-sm hover:bg-muted/70 hover:text-foreground [&_svg]:size-3.5"
            data-testid="thread-scroll-to-latest"
            onClick={() => scrollToBottom("smooth")}
            size="sm"
            type="button"
            variant="outline"
          >
            <ArrowDown aria-hidden />
            {newMessageCount > 0
              ? `${newMessageCount} new message${newMessageCount === 1 ? "" : "s"}`
              : "Jump to latest"}
          </Button>
        </div>
      ) : null}

      <div
        className="pointer-events-none absolute inset-x-0 bottom-0 z-10"
        ref={threadComposerWrapperRef}
      >
        <div className="pointer-events-auto">
          <MessageComposer
            channelId={channelId}
            channelName={channelName}
            channelType={channel?.channelType ?? null}
            disabled={disabled || isSending || !channelId}
            draftKey={`thread:${threadHead.id}`}
            editTarget={editTarget}
            isSending={isSending}
            onCancelEdit={onCancelEdit}
            onCancelReply={composerReplyTarget ? onCancelReply : undefined}
            onEditLastOwnMessage={onEditLastOwnMessage}
            onEditSave={onEditSave}
            onSend={onSend}
            placeholder={`Reply in thread to ${threadHead.author}`}
            profiles={profiles}
            replyTarget={composerReplyTarget}
            typingParentEventId={threadHead.id}
            typingRootEventId={threadHead.rootId}
          />
          <div className="h-7 bg-background px-4 pb-1 pt-0 sm:px-6 -mt-1">
            <div className="mx-auto flex h-full w-full max-w-4xl items-center gap-2">
              {toolbarExtraActions ? (
                <div className="shrink-0">{toolbarExtraActions}</div>
              ) : null}
              {threadTypingPubkeys.length > 0 ? (
                <TypingIndicatorRow
                  channel={channel}
                  className="min-w-0 flex-1 px-0 py-0"
                  currentPubkey={currentPubkey}
                  profiles={profiles}
                  typingPubkeys={threadTypingPubkeys}
                  variant="activity"
                />
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </>
  );

  const threadHeaderContent = (
    <>
      <AuxiliaryPanelHeaderGroup>
        {isSinglePanelView ? (
          <Button
            aria-label="Back to conversation"
            className="shrink-0"
            data-testid="message-thread-back"
            onClick={onClose}
            size="icon"
            type="button"
            variant="outline"
          >
            <ArrowLeft />
          </Button>
        ) : null}
        <AuxiliaryPanelTitle>Thread</AuxiliaryPanelTitle>
      </AuxiliaryPanelHeaderGroup>
      <Button
        aria-label="Close thread"
        className="ml-auto"
        data-testid="message-thread-close"
        onClick={onClose}
        size="icon"
        type="button"
        variant="ghost"
      >
        <X />
      </Button>
    </>
  );

  if (isSplitLayout) {
    return (
      <div className="relative flex min-h-0 flex-1 flex-col">
        <AuxiliaryPanelHeader>{threadHeaderContent}</AuxiliaryPanelHeader>
        {threadScrollRegion}
        {threadFooter}
      </div>
    );
  }

  return (
    <>
      {isFloatingOverlay && <OverlayPanelBackdrop onClose={onClose} />}
      <aside
        className={cn(
          PANEL_BASE_CLASS,
          isSinglePanelView && "border-l-0",
          isFloatingOverlay && PANEL_OVERLAY_CLASS,
        )}
        data-testid="message-thread-panel"
        style={{
          width: isSinglePanelView
            ? "100%"
            : `min(${widthPx}px, calc(100% - ${THREAD_PANEL_MIN_WIDTH_PX}px))`,
        }}
      >
        <div
          className={cn(
            "flex cursor-default select-none items-center",
            isSinglePanelView
              ? `relative ${PANEL_SINGLE_COLUMN_HEADER_LAYER_CLASS} -mb-[4.75rem] min-h-[4.75rem] shrink-0 gap-2.5 bg-background/80 pb-[0.1875rem] pl-4 pr-2 pt-[2.6875rem] backdrop-blur-md supports-[backdrop-filter]:bg-background/70 sm:pr-3 dark:bg-background/70 dark:backdrop-blur-xl dark:supports-[backdrop-filter]:bg-background/55`
              : "relative z-50 min-h-11 shrink-0 gap-3 bg-background/80 px-3 py-1.5 backdrop-blur-md supports-[backdrop-filter]:bg-background/70 dark:bg-background/70 dark:backdrop-blur-xl dark:supports-[backdrop-filter]:bg-background/55",
          )}
          data-tauri-drag-region
        >
          {threadHeaderContent}
        </div>

        {threadScrollRegion}
        {threadFooter}
      </aside>
    </>
  );
}
