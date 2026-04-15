import * as React from "react";

import { useChannelLinks } from "@/features/messages/lib/useChannelLinks";
import type { ChannelSuggestion } from "@/features/messages/lib/useChannelLinks";
import { useDrafts } from "@/features/messages/lib/useDrafts";
import { useMediaUpload } from "@/features/messages/lib/useMediaUpload";
import { useMentions } from "@/features/messages/lib/useMentions";
import { useTypingBroadcast } from "@/features/messages/useTypingBroadcast";
import { escapeRegExp } from "@/shared/lib/mentionPattern";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";
import { ChannelAutocomplete } from "./ChannelAutocomplete";
import { ComposerMentionOverlay } from "./ComposerMentionOverlay";
import {
  MentionAutocomplete,
  type MentionSuggestion,
} from "./MentionAutocomplete";
import { MessageComposerToolbar } from "./MessageComposerToolbar";

type MessageComposerProps = {
  channelId?: string | null;
  /** When set, drafts are keyed by this (e.g. thread sidebar) instead of channelId alone. */
  draftStorageKey?: string | null;
  channelName: string;
  disabled?: boolean;
  editTarget?: {
    author: string;
    body: string;
    id: string;
  } | null;
  isSending?: boolean;
  onCancelEdit?: () => void;
  onCancelReply?: () => void;
  onEditSave?: (content: string) => Promise<void>;
  onSend: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  placeholder?: string;
  replyTarget?: {
    author: string;
    body: string;
    id: string;
  } | null;
  /**
   * Thread sidebar: when the thread is with an agent, we seed `@Name ` and ensure
   * sends include a mention + p-tag without the user typing @ every time.
   */
  implicitThreadAgentMention?: {
    displayName: string;
    pubkey: string;
  } | null;
  showTopBorder?: boolean;
  typingParentEventId?: string | null;
  typingRootEventId?: string | null;
};

const MAX_TEXTAREA_ROWS = 4;

function textAlreadyMentionsDisplayName(text: string, displayName: string) {
  const escaped = escapeRegExp(displayName);
  const pattern = new RegExp(`(?:^|\\s)@${escaped}(?=[\\s,;.!?:)\\]}]|$)`, "i");
  return pattern.test(text);
}

export function MessageComposer({
  channelId = null,
  draftStorageKey = null,
  channelName,
  disabled = false,
  editTarget = null,
  isSending = false,
  onCancelEdit,
  onCancelReply,
  onEditSave,
  onSend,
  placeholder,
  replyTarget = null,
  implicitThreadAgentMention = null,
  showTopBorder = true,
  typingParentEventId = null,
  typingRootEventId = null,
}: MessageComposerProps) {
  const [content, setContent] = React.useState("");
  const contentRef = React.useRef(content);
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  const pendingSelectionRef = React.useRef<number | null>(null);
  const draftSelectionRef = React.useRef({ end: 0, start: 0 });
  const [isEmojiPickerOpen, setIsEmojiPickerOpen] = React.useState(false);
  const [composerScrollTop, setComposerScrollTop] = React.useState(0);
  const lineHeightRef = React.useRef<number | null>(null);

  // Keep contentRef in sync — no extra re-render, just a ref assignment.
  contentRef.current = content;

  const drafts = useDrafts();
  const previousDraftKeyRef = React.useRef<string | null>(null);

  const mentions = useMentions(channelId);
  const channelLinks = useChannelLinks();
  const notifyTyping = useTypingBroadcast(
    channelId,
    typingParentEventId,
    typingRootEventId,
  );

  const media = useMediaUpload(setContent);

  // Stable refs for values read inside callbacks that should not cause
  // callback identity changes when they update.
  const disabledRef = React.useRef(disabled);
  const isSendingRef = React.useRef(isSending);
  const onSendRef = React.useRef(onSend);
  const onEditSaveRef = React.useRef(onEditSave);
  const editTargetRef = React.useRef(editTarget);
  const channelIdRef = React.useRef(channelId);
  const effectiveDraftKeyRef = React.useRef<string | null>(null);
  const implicitThreadAgentMentionRef = React.useRef(
    implicitThreadAgentMention,
  );
  disabledRef.current = disabled;
  isSendingRef.current = isSending;
  onSendRef.current = onSend;
  onEditSaveRef.current = onEditSave;
  editTargetRef.current = editTarget;
  channelIdRef.current = channelId;
  effectiveDraftKeyRef.current = draftStorageKey ?? channelId ?? null;
  implicitThreadAgentMentionRef.current = implicitThreadAgentMention;

  const mentionNamesForOverlay = React.useMemo(() => {
    const list = [...mentions.knownNames];
    const extra = implicitThreadAgentMention?.displayName;
    if (extra && !list.some((n) => n.toLowerCase() === extra.toLowerCase())) {
      list.push(extra);
    }
    return list;
  }, [mentions.knownNames, implicitThreadAgentMention?.displayName]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: draft key change saves/restores draft and resets transient state
  React.useEffect(() => {
    const nextKey = draftStorageKey ?? channelId ?? null;

    // Save draft for the storage key we're leaving
    const prevKey = previousDraftKeyRef.current;
    if (prevKey) {
      const currentContent = contentRef.current;
      const sel = draftSelectionRef.current;
      if (currentContent.trim().length > 0) {
        drafts.saveDraft(prevKey, {
          content: currentContent,
          selectionEnd: sel.end,
          selectionStart: sel.start,
        });
      } else {
        drafts.clearDraft(prevKey);
      }
    }
    previousDraftKeyRef.current = nextKey;

    // Restore draft for the storage key we're entering
    const saved = nextKey ? drafts.loadDraft(nextKey) : undefined;
    if (saved) {
      setContent(saved.content);
      contentRef.current = saved.content;
      draftSelectionRef.current = {
        end: saved.selectionEnd,
        start: saved.selectionStart,
      };
      pendingSelectionRef.current = saved.selectionStart;
    } else {
      setContent("");
      contentRef.current = "";
      draftSelectionRef.current = { end: 0, start: 0 };
    }

    // Always reset transient state
    media.setPendingImeta([]);
    media.setUploadState({ status: "idle" });
    setIsEmojiPickerOpen(false);
    setComposerScrollTop(0);
    mentions.clearMentions();
    channelLinks.clearChannels();
    lineHeightRef.current = null;
  }, [channelId, draftStorageKey]);

  const applyMentionInsert = React.useCallback(
    (suggestion: MentionSuggestion) => {
      const textarea = textareaRef.current;
      const currentContent = contentRef.current;
      const result = mentions.insertMention(
        suggestion,
        currentContent,
        textarea?.selectionEnd ?? currentContent.length,
      );
      draftSelectionRef.current = {
        end: result.nextCursor,
        start: result.nextCursor,
      };
      pendingSelectionRef.current = result.nextCursor;
      setContent(result.nextContent);
    },
    [mentions.insertMention],
  );

  const applyChannelInsert = React.useCallback(
    (suggestion: ChannelSuggestion) => {
      const textarea = textareaRef.current;
      const currentContent = contentRef.current;
      const result = channelLinks.insertChannel(
        suggestion,
        currentContent,
        textarea?.selectionEnd ?? currentContent.length,
      );
      draftSelectionRef.current = {
        end: result.nextCursor,
        start: result.nextCursor,
      };
      pendingSelectionRef.current = result.nextCursor;
      setContent(result.nextContent);
    },
    [channelLinks.insertChannel],
  );

  const updateDraftSelection = React.useCallback(
    (target: HTMLTextAreaElement | null) => {
      if (!target) {
        return;
      }

      draftSelectionRef.current = {
        end: target.selectionEnd ?? target.value.length,
        start: target.selectionStart ?? target.value.length,
      };
    },
    [],
  );

  const insertEmoji = React.useCallback(
    (emoji: string) => {
      const currentContent = contentRef.current;
      const { end, start } = draftSelectionRef.current;
      const nextStart = Math.min(start, currentContent.length);
      const nextEnd = Math.min(end, currentContent.length);
      const nextCursor = nextStart + emoji.length;
      const nextContent = `${currentContent.slice(0, nextStart)}${emoji}${currentContent.slice(nextEnd)}`;

      draftSelectionRef.current = {
        end: nextCursor,
        start: nextCursor,
      };
      pendingSelectionRef.current = nextCursor;
      setContent(nextContent);
      setIsEmojiPickerOpen(false);
      mentions.clearMentions();
    },
    [mentions.clearMentions],
  );

  const openMentionPicker = React.useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    const currentContent = contentRef.current;
    const cursorPosition = textarea.selectionStart ?? currentContent.length;
    // Quick check: is there already an @-query in progress?
    const beforeCursor = currentContent.slice(0, cursorPosition);
    if (/(?:^|[\s])@[^\s]*$/.test(beforeCursor)) {
      mentions.updateMentionQuery(currentContent, cursorPosition);
      textarea.focus();
      return;
    }

    const { end, start } = draftSelectionRef.current;
    const nextStart = Math.min(start, currentContent.length);
    const nextEnd = Math.min(end, currentContent.length);
    const previousCharacter = currentContent.slice(0, nextStart).slice(-1);
    const prefix =
      nextStart > 0 && previousCharacter && !/\s/.test(previousCharacter)
        ? " @"
        : "@";
    const nextContent = `${currentContent.slice(0, nextStart)}${prefix}${currentContent.slice(nextEnd)}`;
    const mentionIndex = nextStart + (prefix.startsWith(" ") ? 1 : 0);
    const nextCursor = mentionIndex + 1;

    draftSelectionRef.current = {
      end: nextCursor,
      start: nextCursor,
    };
    pendingSelectionRef.current = nextCursor;
    setContent(nextContent);
    setIsEmojiPickerOpen(false);
    mentions.updateMentionQuery(nextContent, nextCursor);
  }, [mentions.updateMentionQuery]);

  const handleScroll = React.useCallback(
    (event: React.UIEvent<HTMLTextAreaElement>) => {
      setComposerScrollTop(event.currentTarget.scrollTop);
    },
    [],
  );

  const submitMessage = React.useCallback(async () => {
    let trimmed = contentRef.current.trim();

    // Edit mode: save the edit and return.
    if (editTargetRef.current && onEditSaveRef.current) {
      if (!trimmed || isSendingRef.current) {
        return;
      }

      const savedContent = trimmed;
      setContent("");
      draftSelectionRef.current = { end: 0, start: 0 };
      mentions.clearMentions();
      channelLinks.clearChannels();
      setIsEmojiPickerOpen(false);

      try {
        await onEditSaveRef.current(trimmed);
      } catch {
        setContent(savedContent);
      }
      return;
    }

    const currentPendingImeta = media.pendingImetaRef.current;
    const hasMedia = currentPendingImeta.length > 0;
    if (
      (!trimmed && !hasMedia) ||
      disabledRef.current ||
      isSendingRef.current
    ) {
      return;
    }

    const implicit = implicitThreadAgentMentionRef.current;
    if (implicit && !editTargetRef.current) {
      if (!textAlreadyMentionsDisplayName(trimmed, implicit.displayName)) {
        trimmed =
          trimmed.length > 0
            ? `@${implicit.displayName} ${trimmed}`
            : `@${implicit.displayName}`;
      }
    }

    let pubkeys = mentions.extractMentionPubkeys(trimmed);
    if (implicit && !editTargetRef.current) {
      const set = new Set(pubkeys);
      set.add(implicit.pubkey);
      pubkeys = [...set];
    }

    const mediaTags =
      currentPendingImeta.length > 0
        ? currentPendingImeta.map((d) => [
            "imeta",
            `url ${d.url}`,
            `m ${d.type}`,
            `x ${d.sha256}`,
            `size ${d.size}`,
            ...(d.dim ? [`dim ${d.dim}`] : []),
            ...(d.blurhash ? [`blurhash ${d.blurhash}`] : []),
            ...(d.thumb ? [`thumb ${d.thumb}`] : []),
            ...(d.duration != null ? [`duration ${d.duration}`] : []),
            ...(d.image ? [`image ${d.image}`] : []),
          ])
        : undefined;

    const savedContent = trimmed;
    const savedImeta = [...currentPendingImeta];

    setContent("");
    draftSelectionRef.current = { end: 0, start: 0 };
    media.setPendingImeta([]);
    mentions.clearMentions();
    channelLinks.clearChannels();
    setIsEmojiPickerOpen(false);

    const sendDraftKey = effectiveDraftKeyRef.current;
    try {
      await onSendRef.current(trimmed, pubkeys, mediaTags);
      if (sendDraftKey) {
        drafts.clearDraft(sendDraftKey);
      }
      const implicitAfter = implicitThreadAgentMentionRef.current;
      if (implicitAfter && !editTargetRef.current) {
        const seed = `@${implicitAfter.displayName} `;
        setContent(seed);
        contentRef.current = seed;
        draftSelectionRef.current = {
          end: seed.length,
          start: seed.length,
        };
        pendingSelectionRef.current = seed.length;
      }
    } catch {
      setContent(savedContent);
      media.setPendingImeta(savedImeta);
    }
  }, [
    drafts.clearDraft,
    media.pendingImetaRef,
    media.setPendingImeta,
    mentions.extractMentionPubkeys,
    mentions.clearMentions,
    channelLinks.clearChannels,
  ]);

  const handleSubmit = React.useCallback(
    (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      void submitMessage();
    },
    [submitMessage],
  );

  const handleChange = React.useCallback(
    (event: React.ChangeEvent<HTMLTextAreaElement>) => {
      const nextContent = event.target.value;
      const cursorPos = event.target.selectionStart;
      setContent(nextContent);
      updateDraftSelection(event.target);
      mentions.updateMentionQuery(nextContent, cursorPos);
      channelLinks.updateChannelQuery(nextContent, cursorPos);
      if (nextContent.trim().length > 0) {
        notifyTyping();
      }
    },
    [
      updateDraftSelection,
      mentions.updateMentionQuery,
      channelLinks.updateChannelQuery,
      notifyTyping,
    ],
  );

  const handleKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      const channelResult = channelLinks.handleChannelKeyDown(event);
      if (channelResult.handled) {
        if (channelResult.suggestion) {
          applyChannelInsert(channelResult.suggestion);
        }
        return;
      }

      const { handled, suggestion } = mentions.handleMentionKeyDown(event);
      if (handled) {
        if (suggestion) {
          applyMentionInsert(suggestion);
        }
        return;
      }

      if (event.key === "Escape" && editTargetRef.current && onCancelEdit) {
        event.preventDefault();
        onCancelEdit();
        return;
      }

      if (event.key !== "Enter" || event.nativeEvent.isComposing) {
        return;
      }

      if (event.ctrlKey) {
        const textarea = event.currentTarget;
        const { selectionEnd, selectionStart, value } = textarea;
        const nextContent = `${value.slice(0, selectionStart)}\n${value.slice(selectionEnd)}`;

        event.preventDefault();
        pendingSelectionRef.current = selectionStart + 1;
        draftSelectionRef.current = {
          end: selectionStart + 1,
          start: selectionStart + 1,
        };
        setContent(nextContent);
        return;
      }

      if (event.metaKey || event.altKey || event.shiftKey) {
        return;
      }

      event.preventDefault();
      void submitMessage();
    },
    [
      channelLinks.handleChannelKeyDown,
      applyChannelInsert,
      mentions.handleMentionKeyDown,
      applyMentionInsert,
      submitMessage,
      onCancelEdit,
    ],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: content triggers height recalc and pending selection restore
  React.useLayoutEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    if (lineHeightRef.current === null) {
      lineHeightRef.current =
        Number.parseFloat(window.getComputedStyle(textarea).lineHeight) || 24;
    }
    const lineHeight = lineHeightRef.current;
    const maxHeight = lineHeight * MAX_TEXTAREA_ROWS;

    textarea.style.height = "auto";
    const nextHeight = Math.max(
      lineHeight,
      Math.min(textarea.scrollHeight, maxHeight),
    );
    textarea.style.height = `${nextHeight}px`;
    textarea.style.overflowY =
      textarea.scrollHeight > maxHeight ? "auto" : "hidden";

    const pendingSelection = pendingSelectionRef.current;
    if (pendingSelection !== null) {
      textarea.focus();
      textarea.setSelectionRange(pendingSelection, pendingSelection);
      pendingSelectionRef.current = null;
    }
  }, [content]);

  React.useEffect(() => {
    if (!replyTarget || disabled) {
      return;
    }

    textareaRef.current?.focus();
  }, [disabled, replyTarget]);

  // Pre-fill content when entering edit mode.
  // biome-ignore lint/correctness/useExhaustiveDependencies: editTarget?.id is the trigger — only reset when the edited message changes
  React.useEffect(() => {
    if (!editTarget) {
      return;
    }

    setContent(editTarget.body);
    contentRef.current = editTarget.body;
    const cursorPos = editTarget.body.length;
    draftSelectionRef.current = { end: cursorPos, start: cursorPos };
    pendingSelectionRef.current = cursorPos;
    textareaRef.current?.focus();
  }, [editTarget?.id]);

  // Seed `@AgentName ` in agent threads so users need not type @ every time.
  // biome-ignore lint/correctness/useExhaustiveDependencies: only re-seed when implicit agent or edit mode changes
  React.useEffect(() => {
    const implicit = implicitThreadAgentMention;
    if (!implicit || editTarget) {
      return;
    }

    if (contentRef.current.trim().length > 0) {
      return;
    }

    const seed = `@${implicit.displayName} `;
    setContent(seed);
    contentRef.current = seed;
    draftSelectionRef.current = { end: seed.length, start: seed.length };
    pendingSelectionRef.current = seed.length;
  }, [
    implicitThreadAgentMention?.pubkey,
    implicitThreadAgentMention?.displayName,
    editTarget?.id,
  ]);

  const sendDisabled = React.useMemo(
    () =>
      disabled ||
      (content.trim().length === 0 && media.pendingImeta.length === 0),
    [disabled, content, media.pendingImeta.length],
  );

  const handleCaptureSelection = React.useCallback(() => {
    updateDraftSelection(textareaRef.current);
  }, [updateDraftSelection]);

  const handlePaperclipClick = React.useCallback(() => {
    void media.handlePaperclip();
  }, [media.handlePaperclip]);

  return (
    <footer
      className={cn(
        "shrink-0 bg-transparent px-4 pb-4",
        showTopBorder ? "border-t border-border/80 pt-4" : "pt-0",
      )}
    >
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-3">
        <form
          className="relative isolate rounded-2xl border border-border/50 bg-background/25 px-3 py-4 shadow-[0_4px_24px_rgba(0,0,0,0.08)] backdrop-blur-xl supports-[backdrop-filter]:bg-background/20 dark:shadow-[0_4px_24px_rgba(0,0,0,0.35)] sm:px-4"
          data-testid="message-composer"
          onDragOver={media.handleDragOver}
          onDrop={(e) => {
            void media.handleDrop(e);
          }}
          onSubmit={(event) => {
            handleSubmit(event);
          }}
        >
          <ChannelAutocomplete
            onSelect={applyChannelInsert}
            selectedIndex={channelLinks.channelSelectedIndex}
            suggestions={
              channelLinks.isChannelOpen ? channelLinks.channelSuggestions : []
            }
          />
          <MentionAutocomplete
            onSelect={applyMentionInsert}
            selectedIndex={mentions.mentionSelectedIndex}
            suggestions={mentions.isMentionOpen ? mentions.suggestions : []}
          />

          {editTarget ? (
            <div
              className="mb-3 flex items-start justify-between gap-3 rounded-2xl border border-primary/30 bg-primary/5 px-3 py-2"
              data-testid="edit-target"
            >
              <div className="min-w-0">
                <p className="text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                  Editing message
                </p>
                <p className="truncate text-sm text-foreground/80">
                  {editTarget.body}
                </p>
              </div>
              <Button
                className="shrink-0"
                onClick={onCancelEdit}
                size="sm"
                type="button"
                variant="ghost"
              >
                Cancel
              </Button>
            </div>
          ) : replyTarget ? (
            <div
              className="mb-3 flex items-start justify-between gap-3 rounded-2xl border border-border/70 bg-muted/40 px-3 py-2"
              data-testid="reply-target"
            >
              <div className="min-w-0">
                <p className="text-[11px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                  Replying to {replyTarget.author}
                </p>
                <p className="truncate text-sm text-foreground/80">
                  {replyTarget.body}
                </p>
              </div>
              <Button
                className="shrink-0"
                onClick={onCancelReply}
                size="sm"
                type="button"
                variant="ghost"
              >
                Cancel
              </Button>
            </div>
          ) : null}

          {media.uploadState.status === "error" ? (
            <div className="mb-2 rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
              Upload failed: {media.uploadState.message}
              <button
                className="ml-2 underline"
                onClick={() => media.setUploadState({ status: "idle" })}
                type="button"
              >
                Dismiss
              </button>
            </div>
          ) : null}

          <div className="relative">
            <div
              aria-hidden
              className="pointer-events-none absolute inset-0 overflow-hidden"
            >
              <ComposerMentionOverlay
                channelNames={channelLinks.knownChannelNames}
                content={content}
                mentionNames={mentionNamesForOverlay}
                scrollTop={composerScrollTop}
              />
            </div>
            <Textarea
              aria-label="Message channel"
              className="min-h-0 resize-none overflow-y-hidden border-0 bg-transparent px-0 py-0 text-sm leading-6 md:leading-6 shadow-none focus-visible:ring-0 caret-foreground text-transparent selection:bg-primary/20 selection:text-transparent"
              data-testid="message-input"
              disabled={disabled}
              onChange={handleChange}
              onKeyDown={handleKeyDown}
              onPaste={(e) => {
                void media.handlePaste(e);
              }}
              onScroll={handleScroll}
              onSelect={(event) => {
                updateDraftSelection(event.currentTarget);
              }}
              placeholder={
                editTarget
                  ? "Edit your message"
                  : (placeholder ??
                    (replyTarget
                      ? `Reply to ${replyTarget.author} in #${channelName}`
                      : `Message #${channelName}`))
              }
              ref={textareaRef}
              rows={1}
              value={content}
            />
          </div>

          <MessageComposerToolbar
            composerDisabled={disabled}
            isEmojiPickerOpen={isEmojiPickerOpen}
            isSending={isSending}
            isUploading={media.isUploading}
            onCaptureSelection={handleCaptureSelection}
            onEmojiPickerOpenChange={setIsEmojiPickerOpen}
            onEmojiSelect={insertEmoji}
            onOpenMentionPicker={openMentionPicker}
            onPaperclip={handlePaperclipClick}
            sendDisabled={sendDisabled}
          />
        </form>
      </div>
    </footer>
  );
}
