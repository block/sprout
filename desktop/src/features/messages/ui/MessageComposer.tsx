import * as React from "react";

import { useChannelLinks } from "@/features/messages/lib/useChannelLinks";
import type { ChannelSuggestion } from "@/features/messages/lib/useChannelLinks";
import { useMentions } from "@/features/messages/lib/useMentions";
import {
  type BlobDescriptor,
  pickAndUploadMedia,
  uploadMediaBytes,
} from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";
import { ChannelAutocomplete } from "./ChannelAutocomplete";
import {
  MentionAutocomplete,
  type MentionSuggestion,
} from "./MentionAutocomplete";
import { MessageComposerToolbar } from "./MessageComposerToolbar";

type MessageComposerProps = {
  channelId?: string | null;
  channelName: string;
  disabled?: boolean;
  isSending?: boolean;
  onCancelReply?: () => void;
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
};

const MAX_TEXTAREA_ROWS = 4;

function detectMentionQuery(
  value: string,
  cursorPosition: number,
): { query: string; startIndex: number } | null {
  const beforeCursor = value.slice(0, cursorPosition);
  const match = beforeCursor.match(/(?:^|[\s])@([^\s]*)$/);
  if (!match) {
    return null;
  }

  const query = match[1];
  const startIndex = beforeCursor.length - query.length - 1; // -1 for @
  return { query, startIndex };
}

export function MessageComposer({
  channelId = null,
  channelName,
  disabled = false,
  isSending = false,
  onCancelReply,
  onSend,
  placeholder,
  replyTarget = null,
}: MessageComposerProps) {
  const [content, setContent] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  const pendingSelectionRef = React.useRef<number | null>(null);
  const draftSelectionRef = React.useRef({ end: 0, start: 0 });
  const [isEmojiPickerOpen, setIsEmojiPickerOpen] = React.useState(false);

  const mentions = useMentions(channelId);
  const channelLinks = useChannelLinks();

  const [uploadState, setUploadState] = React.useState<{
    status: "idle" | "uploading" | "error";
    message?: string;
  }>({ status: "idle" });
  const [pendingImeta, setPendingImeta] = React.useState<BlobDescriptor[]>([]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: channelId is the sole trigger — reset all composer state on channel switch to prevent draft/upload/autocomplete leaks
  React.useEffect(() => {
    setContent("");
    setPendingImeta([]);
    setUploadState({ status: "idle" });
    setIsEmojiPickerOpen(false);
    mentions.clearMentions();
    channelLinks.clearChannels();
    draftSelectionRef.current = { end: 0, start: 0 };
    pendingSelectionRef.current = null;
  }, [channelId]);
  const applyMentionInsert = React.useCallback(
    (suggestion: MentionSuggestion) => {
      const textarea = textareaRef.current;
      const result = mentions.insertMention(
        suggestion,
        content,
        textarea?.selectionEnd ?? content.length,
      );
      draftSelectionRef.current = {
        end: result.nextCursor,
        start: result.nextCursor,
      };
      pendingSelectionRef.current = result.nextCursor;
      setContent(result.nextContent);
    },
    [content, mentions.insertMention],
  );

  const applyChannelInsert = React.useCallback(
    (suggestion: ChannelSuggestion) => {
      const textarea = textareaRef.current;
      const result = channelLinks.insertChannel(
        suggestion,
        content,
        textarea?.selectionEnd ?? content.length,
      );
      draftSelectionRef.current = {
        end: result.nextCursor,
        start: result.nextCursor,
      };
      pendingSelectionRef.current = result.nextCursor;
      setContent(result.nextContent);
    },
    [content, channelLinks.insertChannel],
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
      const { end, start } = draftSelectionRef.current;
      const nextStart = Math.min(start, content.length);
      const nextEnd = Math.min(end, content.length);
      const nextCursor = nextStart + emoji.length;
      const nextContent = `${content.slice(0, nextStart)}${emoji}${content.slice(nextEnd)}`;

      draftSelectionRef.current = {
        end: nextCursor,
        start: nextCursor,
      };
      pendingSelectionRef.current = nextCursor;
      setContent(nextContent);
      setIsEmojiPickerOpen(false);
      mentions.clearMentions();
    },
    [content, mentions.clearMentions],
  );

  const openMentionPicker = React.useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    const cursorPosition = textarea.selectionStart ?? content.length;
    const existingMention = detectMentionQuery(content, cursorPosition);
    if (existingMention) {
      mentions.updateMentionQuery(content, cursorPosition);
      textarea.focus();
      return;
    }

    const { end, start } = draftSelectionRef.current;
    const nextStart = Math.min(start, content.length);
    const nextEnd = Math.min(end, content.length);
    const previousCharacter = content.slice(0, nextStart).slice(-1);
    const prefix =
      nextStart > 0 && previousCharacter && !/\s/.test(previousCharacter)
        ? " @"
        : "@";
    const nextContent = `${content.slice(0, nextStart)}${prefix}${content.slice(nextEnd)}`;
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
  }, [content, mentions.updateMentionQuery]);

  const onUploaded = React.useCallback((descriptor: BlobDescriptor) => {
    const markdown = `\n![image](${descriptor.url})\n`;
    setContent((prev) => prev + markdown);
    setPendingImeta((prev) => [...prev, descriptor]);
    setUploadState({ status: "idle" });
  }, []);

  const handlePaperclip = React.useCallback(async () => {
    setUploadState({ status: "uploading" });
    try {
      const descriptor = await pickAndUploadMedia();
      if (descriptor) {
        onUploaded(descriptor);
      } else {
        setUploadState({ status: "idle" });
      }
    } catch (err) {
      setUploadState({ status: "error", message: String(err) });
    }
  }, [onUploaded]);

  const handleDrop = React.useCallback(
    async (event: React.DragEvent<HTMLFormElement>) => {
      event.preventDefault();
      const files = Array.from(event.dataTransfer.files);
      if (files.length === 0) return;

      const file = files[0];
      if (!file) return;

      const ALLOWED_TYPES = [
        "image/jpeg",
        "image/png",
        "image/gif",
        "image/webp",
      ];
      if (!ALLOWED_TYPES.includes(file.type)) {
        setUploadState({
          status: "error",
          message: "Only JPEG, PNG, GIF, and WebP images are supported",
        });
        return;
      }

      setUploadState({ status: "uploading" });
      try {
        const buffer = await file.arrayBuffer();
        const descriptor = await uploadMediaBytes([...new Uint8Array(buffer)]);
        onUploaded(descriptor);
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    },
    [onUploaded],
  );

  const handleDragOver = React.useCallback(
    (event: React.DragEvent<HTMLFormElement>) => {
      event.preventDefault();
    },
    [],
  );

  const handlePaste = React.useCallback(
    async (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
      const items = Array.from(event.clipboardData.items);
      const ALLOWED_TYPES = [
        "image/jpeg",
        "image/png",
        "image/gif",
        "image/webp",
      ];
      const imageItem = items.find((item) => ALLOWED_TYPES.includes(item.type));
      if (!imageItem) return;

      event.preventDefault();
      const file = imageItem.getAsFile();
      if (!file) return;

      setUploadState({ status: "uploading" });
      try {
        const buffer = await file.arrayBuffer();
        const descriptor = await uploadMediaBytes([...new Uint8Array(buffer)]);
        onUploaded(descriptor);
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    },
    [onUploaded],
  );

  const submitMessage = React.useCallback(async () => {
    const trimmed = content.trim();
    const hasMedia = pendingImeta.length > 0;
    if ((!trimmed && !hasMedia) || disabled || isSending) {
      return;
    }

    const pubkeys = mentions.extractMentionPubkeys(trimmed);

    const mediaTags =
      pendingImeta.length > 0
        ? pendingImeta.map((d) => [
            "imeta",
            `url ${d.url}`,
            `m ${d.type}`,
            `x ${d.sha256}`,
            `size ${d.size}`,
            ...(d.dim ? [`dim ${d.dim}`] : []),
            ...(d.blurhash ? [`blurhash ${d.blurhash}`] : []),
            ...(d.thumb ? [`thumb ${d.thumb}`] : []),
          ])
        : undefined;

    const savedContent = trimmed;
    const savedImeta = [...pendingImeta];

    setContent("");
    draftSelectionRef.current = { end: 0, start: 0 };
    setPendingImeta([]);
    mentions.clearMentions();
    channelLinks.clearChannels();
    setIsEmojiPickerOpen(false);

    try {
      await onSend(trimmed, pubkeys, mediaTags);
    } catch {
      setContent(savedContent);
      setPendingImeta(savedImeta);
    }
  }, [
    content,
    disabled,
    isSending,
    onSend,
    mentions.extractMentionPubkeys,
    mentions.clearMentions,
    channelLinks.clearChannels,
    pendingImeta,
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
    },
    [
      updateDraftSelection,
      mentions.updateMentionQuery,
      channelLinks.updateChannelQuery,
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
    ],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: content triggers height recalc and pending selection restore
  React.useLayoutEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    const lineHeight =
      Number.parseFloat(window.getComputedStyle(textarea).lineHeight) || 24;
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

  const isUploading = uploadState.status === "uploading";

  return (
    <footer className="border-t border-border/80 bg-background p-4">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-3">
        <form
          className="relative rounded-2xl border border-input bg-card px-3 py-4 shadow-sm sm:px-4"
          data-testid="message-composer"
          onDragOver={handleDragOver}
          onDrop={(e) => {
            void handleDrop(e);
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

          {replyTarget ? (
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

          {uploadState.status === "error" ? (
            <div className="mb-2 rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
              Upload failed: {uploadState.message}
              <button
                className="ml-2 underline"
                onClick={() => setUploadState({ status: "idle" })}
                type="button"
              >
                Dismiss
              </button>
            </div>
          ) : null}

          <Textarea
            aria-label="Message channel"
            className="min-h-0 resize-none overflow-y-hidden border-0 bg-transparent px-0 py-0 text-sm leading-6 shadow-none focus-visible:ring-0"
            data-testid="message-input"
            disabled={disabled}
            onChange={handleChange}
            onKeyDown={handleKeyDown}
            onPaste={(e) => {
              void handlePaste(e);
            }}
            onSelect={(event) => {
              updateDraftSelection(event.currentTarget);
            }}
            placeholder={
              placeholder ??
              (replyTarget
                ? `Reply to ${replyTarget.author} in #${channelName}`
                : `Message #${channelName}`)
            }
            ref={textareaRef}
            rows={1}
            value={content}
          />

          <MessageComposerToolbar
            composerDisabled={disabled}
            isEmojiPickerOpen={isEmojiPickerOpen}
            isSending={isSending}
            isUploading={isUploading}
            onCaptureSelection={() => {
              updateDraftSelection(textareaRef.current);
            }}
            onEmojiPickerOpenChange={setIsEmojiPickerOpen}
            onEmojiSelect={insertEmoji}
            onOpenMentionPicker={openMentionPicker}
            onPaperclip={() => {
              void handlePaperclip();
            }}
            sendDisabled={
              disabled ||
              (content.trim().length === 0 && pendingImeta.length === 0)
            }
          />
        </form>
      </div>
    </footer>
  );
}
