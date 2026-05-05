import * as React from "react";
import { Hash, LogIn } from "lucide-react";

import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { MessageThreadPanel } from "@/features/messages/ui/MessageThreadPanel";
import { MessageTimeline } from "@/features/messages/ui/MessageTimeline";
import { TypingIndicatorRow } from "@/features/messages/ui/TypingIndicatorRow";
import type { TypingIndicatorEntry } from "@/features/messages/useChannelTyping";
import { UserProfilePanel } from "@/features/profile/ui/UserProfilePanel";
import { ChannelFindBar } from "@/features/search/ui/ChannelFindBar";
import { AgentSessionThreadPanel } from "@/features/channels/ui/AgentSessionThreadPanel";
import { BotActivityBar } from "@/features/channels/ui/BotActivityBar";
import { Button } from "@/shared/ui/button";
import type { useChannelFind } from "@/features/search/useChannelFind";
import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { Channel, ManagedAgent } from "@/shared/api/types";

const THREAD_PANEL_DEFAULT_WIDTH_PX = 380;
const THREAD_PANEL_MIN_WIDTH_PX = 320;
const THREAD_PANEL_MAX_WIDTH_PX = 720;
const THREAD_PANEL_WIDTH_SESSION_KEY = "sprout.desktop.thread-panel-width";

function clampThreadPanelWidth(width: number): number {
  return Math.max(
    THREAD_PANEL_MIN_WIDTH_PX,
    Math.min(THREAD_PANEL_MAX_WIDTH_PX, width),
  );
}

function getInitialThreadPanelWidth(): number {
  if (typeof window === "undefined") {
    return THREAD_PANEL_DEFAULT_WIDTH_PX;
  }

  try {
    const raw = window.sessionStorage.getItem(THREAD_PANEL_WIDTH_SESSION_KEY);
    if (!raw) {
      return THREAD_PANEL_DEFAULT_WIDTH_PX;
    }

    const parsed = Number.parseInt(raw, 10);
    if (!Number.isFinite(parsed)) {
      return THREAD_PANEL_DEFAULT_WIDTH_PX;
    }

    return clampThreadPanelWidth(parsed);
  } catch {
    return THREAD_PANEL_DEFAULT_WIDTH_PX;
  }
}

function messageMentionsPubkey(message: TimelineMessage, pubkey: string) {
  const normalizedPubkey = pubkey.toLowerCase();
  if (message.pubkey?.toLowerCase() === normalizedPubkey) {
    return false;
  }

  return (
    message.tags?.some(
      (tag) => tag[0] === "p" && tag[1]?.toLowerCase() === normalizedPubkey,
    ) ?? false
  );
}

function findLatestMentionedMessageId(
  messages: TimelineMessage[],
  pubkey: string,
) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message && messageMentionsPubkey(message, pubkey)) {
      return message.id;
    }
  }

  return null;
}

function addPubkey(
  map: Map<string, string[]>,
  messageId: string,
  pubkey: string,
) {
  const current = map.get(messageId) ?? [];
  if (!current.some((value) => value.toLowerCase() === pubkey.toLowerCase())) {
    current.push(pubkey);
  }
  map.set(messageId, current);
}

type ChannelPaneProps = {
  activeChannel: Channel | null;
  agentSessionAgents: ManagedAgent[];
  botTypingEntries: TypingIndicatorEntry[];
  channelFind: ReturnType<typeof useChannelFind>;
  currentPubkey?: string;
  editTarget?: {
    author: string;
    body: string;
    id: string;
  } | null;
  fetchOlder?: () => Promise<void>;
  hasOlderMessages?: boolean;
  isFetchingOlder?: boolean;
  isJoining?: boolean;
  isSending: boolean;
  isTimelineLoading: boolean;
  messages: TimelineMessage[];
  onCancelEdit?: () => void;
  onCancelThreadReply: () => void;
  onCloseAgentSession: () => void;
  onCloseProfilePanel: () => void;
  onCloseThread: () => void;
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onEditSave?: (content: string) => Promise<void>;
  onExpandThreadReplies: (message: TimelineMessage) => void;
  onJoinChannel?: () => Promise<void>;
  onOpenAgentSession: (pubkey: string) => void;
  onOpenDm?: (pubkeys: string[]) => void;
  onOpenThread: (message: TimelineMessage) => void;
  onSelectThreadReplyTarget: (message: TimelineMessage) => void;
  onSendMessage: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  onSendThreadReply: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  onTargetReached?: (messageId: string) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  onThreadScrollTargetResolved: () => void;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
  profiles?: UserProfileLookup;
  openThreadHeadId: string | null;
  openAgentSessionPubkey: string | null;
  profilePanelPubkey?: string | null;
  threadHeadMessage: TimelineMessage | null;
  threadMessages: MainTimelineEntry[];
  threadTypingPubkeys: string[];
  threadReplyTargetId: string | null;
  threadReplyTargetMessage: TimelineMessage | null;
  threadScrollTargetId: string | null;
  targetMessageId: string | null;
  typingPubkeys: string[];
};

export const ChannelPane = React.memo(function ChannelPane({
  activeChannel,
  agentSessionAgents,
  botTypingEntries,
  channelFind,
  currentPubkey,
  editTarget = null,
  fetchOlder,
  hasOlderMessages,
  isFetchingOlder,
  isJoining = false,
  isSending,
  isTimelineLoading,
  messages,
  onCancelEdit,
  onCancelThreadReply,
  onCloseAgentSession,
  onCloseProfilePanel,
  onCloseThread,
  onDelete,
  onEdit,
  onEditSave,
  onExpandThreadReplies,
  onJoinChannel,
  onOpenAgentSession,
  onOpenDm,
  onOpenThread,
  onSelectThreadReplyTarget,
  onSendMessage,
  onSendThreadReply,
  onThreadScrollTargetResolved,
  onTargetReached,
  onToggleReaction,
  personaLookup,
  profiles,
  openThreadHeadId,
  openAgentSessionPubkey,
  profilePanelPubkey,
  targetMessageId,
  threadHeadMessage,
  threadMessages,
  threadScrollTargetId,
  threadTypingPubkeys,
  threadReplyTargetId,
  threadReplyTargetMessage,
  typingPubkeys,
}: ChannelPaneProps) {
  const [threadPanelWidthPx, setThreadPanelWidthPx] = React.useState<number>(
    () => getInitialThreadPanelWidth(),
  );

  React.useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    try {
      window.sessionStorage.setItem(
        THREAD_PANEL_WIDTH_SESSION_KEY,
        String(threadPanelWidthPx),
      );
    } catch {
      // Ignore storage failures and keep in-memory width for this session.
    }
  }, [threadPanelWidthPx]);

  const handleThreadPanelResizeStart = React.useCallback(
    (event: React.PointerEvent<HTMLButtonElement>) => {
      event.preventDefault();

      const startX = event.clientX;
      const startWidth = threadPanelWidthPx;
      const previousCursor = document.body.style.cursor;
      const previousUserSelect = document.body.style.userSelect;

      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";

      const handlePointerMove = (moveEvent: PointerEvent) => {
        const deltaX = startX - moveEvent.clientX;
        const nextWidth = clampThreadPanelWidth(startWidth + deltaX);
        setThreadPanelWidthPx(nextWidth);
      };

      const handlePointerUp = () => {
        document.body.style.cursor = previousCursor;
        document.body.style.userSelect = previousUserSelect;
        window.removeEventListener("pointermove", handlePointerMove);
      };

      window.addEventListener("pointermove", handlePointerMove);
      window.addEventListener("pointerup", handlePointerUp, { once: true });
    },
    [threadPanelWidthPx],
  );

  const handleThreadPanelWidthReset = React.useCallback(() => {
    setThreadPanelWidthPx(THREAD_PANEL_DEFAULT_WIDTH_PX);
  }, []);

  const canResetThreadPanelWidth =
    threadPanelWidthPx !== THREAD_PANEL_DEFAULT_WIDTH_PX;

  const isNonMemberView =
    activeChannel !== null &&
    !activeChannel.isMember &&
    activeChannel.visibility === "open" &&
    !activeChannel.archivedAt;

  const isComposerDisabled =
    !activeChannel?.isMember ||
    activeChannel.archivedAt !== null ||
    activeChannel.channelType === "forum" ||
    isSending;
  const hasTypingActivity = typingPubkeys.length > 0;
  const { messageActivityFooters, unanchoredBotTypingPubkeys } =
    React.useMemo(() => {
      const botPubkeysByMessageId = new Map<string, string[]>();
      const unanchoredPubkeys: string[] = [];
      for (const entry of botTypingEntries) {
        const messageId =
          entry.threadHeadId ??
          findLatestMentionedMessageId(messages, entry.pubkey);
        if (messageId) {
          addPubkey(botPubkeysByMessageId, messageId, entry.pubkey);
        } else if (
          !unanchoredPubkeys.some(
            (pubkey) => pubkey.toLowerCase() === entry.pubkey.toLowerCase(),
          )
        ) {
          unanchoredPubkeys.push(entry.pubkey);
        }
      }

      const footers: Record<string, React.ReactNode> = {};
      for (const [messageId, pubkeys] of botPubkeysByMessageId) {
        footers[messageId] = (
          <div className="flex justify-start pl-[3.125rem]">
            <BotActivityBar
              agents={agentSessionAgents}
              onOpenAgentSession={onOpenAgentSession}
              openAgentSessionPubkey={openAgentSessionPubkey}
              profiles={profiles}
              typingBotPubkeys={pubkeys}
            />
          </div>
        );
      }

      return {
        messageActivityFooters: footers,
        unanchoredBotTypingPubkeys: unanchoredPubkeys,
      };
    }, [
      agentSessionAgents,
      botTypingEntries,
      messages,
      onOpenAgentSession,
      openAgentSessionPubkey,
      profiles,
    ]);
  const hasBotActivity = unanchoredBotTypingPubkeys.length > 0;

  const selectedAgent = React.useMemo(
    () =>
      openAgentSessionPubkey
        ? (agentSessionAgents.find(
            (agent) => agent.pubkey === openAgentSessionPubkey,
          ) ?? null)
        : null,
    [agentSessionAgents, openAgentSessionPubkey],
  );

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-row overflow-hidden">
      <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {channelFind.isOpen ? (
          <ChannelFindBar
            matchCount={channelFind.matchCount}
            matchIndex={channelFind.activeIndex}
            onClose={channelFind.close}
            onNext={channelFind.goToNext}
            onPrevious={channelFind.goToPrevious}
            onQueryChange={channelFind.setQuery}
            query={channelFind.query}
          />
        ) : null}
        <MessageTimeline
          channelId={activeChannel?.id}
          activeReplyTargetId={openThreadHeadId}
          currentPubkey={currentPubkey}
          fetchOlder={fetchOlder}
          hasOlderMessages={hasOlderMessages}
          isFetchingOlder={isFetchingOlder}
          messageFooters={messageActivityFooters}
          personaLookup={personaLookup}
          profiles={profiles}
          emptyDescription={
            activeChannel?.channelType === "forum"
              ? "Select a stream or DM to load real message history in this first integration pass."
              : "Messages and sub-replies will appear here once the relay has history for this channel."
          }
          emptyTitle={
            activeChannel
              ? activeChannel.channelType === "forum"
                ? "Forum channels are next"
                : "No messages yet"
              : "No channel selected"
          }
          isLoading={isTimelineLoading}
          messages={messages}
          onDelete={onDelete}
          onEdit={onEdit}
          onReply={onOpenThread}
          onTargetReached={onTargetReached}
          onToggleReaction={onToggleReaction}
          searchActiveMessageId={channelFind.activeMatch?.messageId ?? null}
          searchMatchingMessageIds={channelFind.matchingMessageIds}
          searchQuery={channelFind.query}
          targetMessageId={targetMessageId}
        />
        {hasTypingActivity ? (
          <div className="relative bg-background">
            <TypingIndicatorRow
              channel={activeChannel}
              currentPubkey={currentPubkey}
              profiles={profiles}
              typingPubkeys={typingPubkeys}
            />
          </div>
        ) : null}
        {hasBotActivity ? (
          <div className="relative z-10 bg-background px-4 pb-2 sm:px-6">
            <div className="mx-auto flex w-full max-w-4xl justify-start pl-[3.125rem]">
              <BotActivityBar
                agents={agentSessionAgents}
                onOpenAgentSession={onOpenAgentSession}
                openAgentSessionPubkey={openAgentSessionPubkey}
                profiles={profiles}
                typingBotPubkeys={unanchoredBotTypingPubkeys}
              />
            </div>
          </div>
        ) : null}
        {isNonMemberView ? (
          <div
            data-testid="join-banner"
            className="flex items-center gap-3 border-t border-border/80 bg-card/50 px-4 py-3"
          >
            <div className="flex min-w-0 flex-1 items-center gap-2 text-sm text-muted-foreground">
              <Hash className="h-4 w-4 shrink-0" />
              <span className="truncate">
                Viewing{" "}
                <span className="font-medium text-foreground">
                  #{activeChannel?.name}
                </span>
              </span>
            </div>
            <Button
              disabled={isJoining}
              onClick={() => {
                void onJoinChannel?.();
              }}
              size="sm"
              variant="default"
            >
              <LogIn className="mr-1.5 h-3.5 w-3.5" />
              {isJoining ? "Joining..." : "Join to participate"}
            </Button>
          </div>
        ) : (
          <div className="relative z-10 shrink-0">
            <MessageComposer
              channelId={activeChannel?.id ?? null}
              channelName={activeChannel?.name ?? "channel"}
              disabled={isComposerDisabled}
              editTarget={editTarget}
              isSending={isSending}
              onCancelEdit={onCancelEdit}
              onEditSave={onEditSave}
              onSend={onSendMessage}
              profiles={profiles}
              placeholder={
                activeChannel?.archivedAt
                  ? "Archived channels are read-only."
                  : activeChannel?.channelType === "forum"
                    ? "Forum posting is not wired in this pass."
                    : activeChannel
                      ? `Message #${activeChannel.name}`
                      : "Select a channel"
              }
              showTopBorder={false}
            />
          </div>
        )}
      </div>

      {threadHeadMessage ? (
        <MessageThreadPanel
          channel={activeChannel}
          channelId={activeChannel?.id ?? null}
          channelName={activeChannel?.name ?? "channel"}
          currentPubkey={currentPubkey}
          disabled={isComposerDisabled}
          editTarget={editTarget}
          isSending={isSending}
          onCancelEdit={onCancelEdit}
          onCancelReply={onCancelThreadReply}
          onClose={onCloseThread}
          onDelete={onDelete}
          onEdit={onEdit}
          onEditSave={onEditSave}
          onExpandReplies={onExpandThreadReplies}
          onSelectReplyTarget={onSelectThreadReplyTarget}
          onSend={onSendThreadReply}
          onScrollTargetResolved={onThreadScrollTargetResolved}
          onToggleReaction={onToggleReaction}
          profiles={profiles}
          replyTargetId={threadReplyTargetId}
          replyTargetMessage={threadReplyTargetMessage}
          scrollTargetId={threadScrollTargetId}
          canResetWidth={canResetThreadPanelWidth}
          onResetWidth={handleThreadPanelWidthReset}
          onResizeStart={handleThreadPanelResizeStart}
          threadHead={threadHeadMessage}
          widthPx={threadPanelWidthPx}
          threadReplies={threadMessages}
          threadTypingPubkeys={threadTypingPubkeys}
        />
      ) : activeChannel && selectedAgent ? (
        <AgentSessionThreadPanel
          agent={selectedAgent}
          canResetWidth={canResetThreadPanelWidth}
          channel={activeChannel}
          isWorking={botTypingEntries.some(
            (entry) =>
              entry.pubkey.toLowerCase() === selectedAgent.pubkey.toLowerCase(),
          )}
          onClose={onCloseAgentSession}
          onResetWidth={handleThreadPanelWidthReset}
          onResizeStart={handleThreadPanelResizeStart}
          widthPx={threadPanelWidthPx}
        />
      ) : profilePanelPubkey ? (
        <UserProfilePanel
          canResetWidth={canResetThreadPanelWidth}
          currentPubkey={currentPubkey}
          onClose={onCloseProfilePanel}
          onOpenDm={onOpenDm}
          onResetWidth={handleThreadPanelWidthReset}
          onResizeStart={handleThreadPanelResizeStart}
          pubkey={profilePanelPubkey}
          widthPx={threadPanelWidthPx}
        />
      ) : null}
    </div>
  );
});
