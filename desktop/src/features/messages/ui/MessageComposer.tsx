import * as React from "react";

import { EditorContent } from "@tiptap/react";
import { useChannelLinks } from "@/features/messages/lib/useChannelLinks";
import type { ChannelSuggestion } from "@/features/messages/lib/useChannelLinks";
import { useDrafts } from "@/features/messages/lib/useDrafts";
import {
  useImageRefSuggestions,
  type ImageRefSuggestion,
} from "@/features/messages/lib/useImageRefSuggestions";
import {
  ALLOWED_MEDIA_TYPES,
  useMediaUpload,
} from "@/features/messages/lib/useMediaUpload";
import { useMentions } from "@/features/messages/lib/useMentions";
import { useRichTextEditor } from "@/features/messages/lib/useRichTextEditor";
import { useTypingBroadcast } from "@/features/messages/useTypingBroadcast";
import { cn } from "@/shared/lib/cn";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { Button } from "@/shared/ui/button";
import { ChannelAutocomplete } from "./ChannelAutocomplete";
import { ComposerAttachments } from "./ComposerAttachments";
import { FormattingToolbar } from "./FormattingToolbar";
import { ImageRefAutocomplete } from "./ImageRefAutocomplete";
import {
  MentionAutocomplete,
  type MentionSuggestion,
} from "./MentionAutocomplete";
import { MessageComposerToolbar } from "./MessageComposerToolbar";

type MessageComposerProps = {
  channelId?: string | null;
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
  showTopBorder?: boolean;
  typingParentEventId?: string | null;
  typingRootEventId?: string | null;
};

export function MessageComposer({
  channelId = null,
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
  showTopBorder = true,
  typingParentEventId = null,
  typingRootEventId = null,
}: MessageComposerProps) {
  // ── Markdown content state (synced from Tiptap on every update) ──────
  const [content, setContent] = React.useState("");
  const contentRef = React.useRef(content);
  contentRef.current = content;

  const [isEmojiPickerOpen, setIsEmojiPickerOpen] = React.useState(false);
  const [isFormattingOpen, setIsFormattingOpen] = React.useState(false);

  const drafts = useDrafts();
  const previousChannelIdRef = React.useRef<string | null>(null);

  const mentions = useMentions(channelId);
  const channelLinks = useChannelLinks();
  const notifyTyping = useTypingBroadcast(
    channelId,
    typingParentEventId,
    typingRootEventId,
  );

  // ── Media upload ─────────────────────────────────────────────────────
  // We pass a custom setter that both updates React state AND inserts
  // markdown into the Tiptap editor when media upload completes.
  const media = useMediaUpload();
  const imageRefs = useImageRefSuggestions(media.pendingImeta);

  // ── Stable refs for callbacks ────────────────────────────────────────
  const disabledRef = React.useRef(disabled);
  const isSendingRef = React.useRef(isSending);
  const onSendRef = React.useRef(onSend);
  const onEditSaveRef = React.useRef(onEditSave);
  const editTargetRef = React.useRef(editTarget);
  const channelIdRef = React.useRef(channelId);
  disabledRef.current = disabled;
  isSendingRef.current = isSending;
  onSendRef.current = onSend;
  onEditSaveRef.current = onEditSave;
  editTargetRef.current = editTarget;
  channelIdRef.current = channelId;

  // ── Computed placeholder ─────────────────────────────────────────────
  const computedPlaceholder = editTarget
    ? "Edit your message"
    : (placeholder ??
      (replyTarget
        ? `Reply to ${replyTarget.author} in #${channelName}`
        : `Message #${channelName}`));

  // ── Tiptap editor ───────────────────────────────────────────────────
  const richText = useRichTextEditor({
    placeholder: computedPlaceholder,
    editable: !disabled,
    onUpdate: ({ markdown, text }) => {
      setContent(markdown);
      contentRef.current = markdown;

      // Bridge to existing mention/channel/imageRef detection hooks.
      const { cursor } = richText.getTextAndCursor();
      mentions.updateMentionQuery(text, cursor);
      channelLinks.updateChannelQuery(text, cursor);
      imageRefs.updateQuery(text, cursor);

      if (text.trim().length > 0) {
        notifyTyping();
      }
    },
  });

  // ── Channel switching: save/restore drafts ──────────────────────────
  // biome-ignore lint/correctness/useExhaustiveDependencies: channelId is the sole trigger
  React.useEffect(() => {
    const prevId = previousChannelIdRef.current;
    if (prevId) {
      const currentContent = contentRef.current;
      if (currentContent.trim().length > 0) {
        drafts.saveDraft(prevId, {
          content: currentContent,
          selectionEnd: currentContent.length,
          selectionStart: currentContent.length,
        });
      } else {
        drafts.clearDraft(prevId);
      }
    }
    previousChannelIdRef.current = channelId;

    const saved = channelId ? drafts.loadDraft(channelId) : undefined;
    if (saved) {
      setContent(saved.content);
      contentRef.current = saved.content;
      richText.setContent(saved.content);
    } else {
      setContent("");
      contentRef.current = "";
      richText.clearContent();
    }

    media.setPendingImeta([]);
    media.setUploadState({ status: "idle" });
    setIsEmojiPickerOpen(false);
    mentions.clearMentions();
    channelLinks.clearChannels();
    imageRefs.clear();
  }, [channelId]);

  // ── Edit mode: pre-fill content ─────────────────────────────────────
  // biome-ignore lint/correctness/useExhaustiveDependencies: editTarget?.id is the trigger
  React.useEffect(() => {
    if (!editTarget) return;
    setContent(editTarget.body);
    contentRef.current = editTarget.body;
    richText.setContent(editTarget.body);
    richText.focus();
  }, [editTarget?.id]);

  // ── Focus on reply ──────────────────────────────────────────────────
  React.useEffect(() => {
    if (!replyTarget || disabled) return;
    richText.focus();
  }, [disabled, replyTarget, richText.focus]);

  // ── Mention / channel autocomplete insertion ────────────────────────
  const applyMentionInsert = React.useCallback(
    (suggestion: MentionSuggestion) => {
      const { text, cursor } = richText.getTextAndCursor();
      const result = mentions.insertMention(suggestion, text, cursor);
      // Replace the editor content with the updated text.
      richText.setContent(result.nextContent);
      setContent(result.nextContent);
      contentRef.current = result.nextContent;
      // Move cursor to end — Tiptap will focus there.
      richText.focus();
    },
    [
      mentions.insertMention,
      richText.getTextAndCursor,
      richText.setContent,
      richText.focus,
    ],
  );

  const applyChannelInsert = React.useCallback(
    (suggestion: ChannelSuggestion) => {
      const { text, cursor } = richText.getTextAndCursor();
      const result = channelLinks.insertChannel(suggestion, text, cursor);
      richText.setContent(result.nextContent);
      setContent(result.nextContent);
      contentRef.current = result.nextContent;
      richText.focus();
    },
    [
      channelLinks.insertChannel,
      richText.getTextAndCursor,
      richText.setContent,
      richText.focus,
    ],
  );

  // ── Image ref insertion ───────────────────────────────────────────
  const applyImageRefInsert = React.useCallback(
    (suggestion: ImageRefSuggestion) => {
      if (!richText.editor) return;

      // Delete the `![query` trigger text before inserting the node.
      const { text, cursor } = richText.getTextAndCursor();
      const before = text.slice(0, cursor);
      const match = /!\[([^\]]*)$/.exec(before);
      if (match) {
        const triggerStart = cursor - match[0].length;
        // Delete from triggerStart to cursor in the editor
        const { state } = richText.editor;
        // Map text offset to ProseMirror position
        let pmPos = 0;
        let textOffset = 0;
        state.doc.descendants((node, pos) => {
          if (
            node.isText &&
            textOffset + (node.text?.length ?? 0) >= triggerStart &&
            pmPos === 0
          ) {
            pmPos = pos + (triggerStart - textOffset);
          }
          if (node.isText) {
            textOffset += node.text?.length ?? 0;
          }
        });
        if (pmPos > 0) {
          const cursorPm = pmPos + match[0].length;
          richText.editor
            .chain()
            .focus()
            .deleteRange({ from: pmPos, to: cursorPm })
            .run();
        }
      }

      // Insert the image ref node
      const mediaType = suggestion.type.startsWith("video/")
        ? "video"
        : "image";
      const thumbUrl = suggestion.thumb
        ? rewriteRelayUrl(suggestion.thumb)
        : rewriteRelayUrl(suggestion.url);
      richText.insertImageRef(
        suggestion.url,
        suggestion.hash,
        mediaType,
        thumbUrl,
      );
      imageRefs.clear();
    },
    [
      richText.editor,
      richText.getTextAndCursor,
      richText.insertImageRef,
      imageRefs.clear,
    ],
  );

  // ── Emoji insertion ─────────────────────────────────────────────────
  const insertEmoji = React.useCallback(
    (emoji: string) => {
      if (!richText.editor) return;
      richText.editor.chain().focus().insertContent(emoji).run();
      setIsEmojiPickerOpen(false);
      mentions.clearMentions();
    },
    [richText.editor, mentions.clearMentions],
  );

  // ── @ mention picker (toolbar button) ───────────────────────────────
  const openMentionPicker = React.useCallback(() => {
    if (!richText.editor) return;
    const { text, cursor } = richText.getTextAndCursor();

    // Check if there's already an @-query in progress
    const beforeCursor = text.slice(0, cursor);
    if (/(?:^|[\s])@[^\s]*$/.test(beforeCursor)) {
      mentions.updateMentionQuery(text, cursor);
      richText.focus();
      return;
    }

    // Insert @ at cursor
    const previousChar = text.slice(0, cursor).slice(-1);
    const prefix =
      cursor > 0 && previousChar && !/\s/.test(previousChar) ? " @" : "@";
    richText.editor.chain().focus().insertContent(prefix).run();
    setIsEmojiPickerOpen(false);

    // Trigger mention detection after inserting @
    const updatedText = richText.editor.state.doc.textContent;
    const { cursor: updatedCursor } = richText.getTextAndCursor();
    mentions.updateMentionQuery(updatedText, updatedCursor);
  }, [
    richText.editor,
    richText.getTextAndCursor,
    richText.focus,
    mentions.updateMentionQuery,
  ]);

  // ── Submit message ──────────────────────────────────────────────────
  const submitMessage = React.useCallback(async () => {
    const trimmed = contentRef.current.trim();

    // Edit mode
    if (editTargetRef.current && onEditSaveRef.current) {
      if (!trimmed || isSendingRef.current) return;

      const savedContent = trimmed;
      setContent("");
      contentRef.current = "";
      richText.clearContent();
      mentions.clearMentions();
      channelLinks.clearChannels();
      setIsEmojiPickerOpen(false);

      try {
        await onEditSaveRef.current(trimmed);
      } catch {
        setContent(savedContent);
        contentRef.current = savedContent;
        richText.setContent(savedContent);
      }
      return;
    }

    // Normal send
    const currentPendingImeta = media.pendingImetaRef.current;
    const hasMedia = currentPendingImeta.length > 0;
    if (
      (!trimmed && !hasMedia) ||
      disabledRef.current ||
      isSendingRef.current
    ) {
      return;
    }

    const pubkeys = mentions.extractMentionPubkeys(trimmed);

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

    // Resolve imageRef nodes to markdown and collect referenced URLs.
    // Then append any remaining unreferenced attachments.
    const referencedUrls = new Set<string>();
    let finalContent = trimmed;

    // Scan editor doc for imageRef nodes and replace their serialized
    // output with proper markdown.
    if (richText.editor) {
      richText.editor.state.doc.descendants((node) => {
        if (node.type.name === "imageRef" && node.attrs.url) {
          referencedUrls.add(node.attrs.url as string);
          const url = node.attrs.url as string;
          const label = node.attrs.mediaType === "video" ? "video" : "image";
          // Replace the chip placeholder text with real markdown
          // Use split+join for global replace (avoids ES2021 replaceAll)
          const chip = `![${node.attrs.hash as string}]`;
          finalContent = finalContent.split(chip).join(`![${label}](${url})`);
        }
      });
    }

    // Append unreferenced attachments at the end
    for (const d of currentPendingImeta) {
      if (referencedUrls.has(d.url)) continue;
      const isVideo = d.type.startsWith("video/");
      finalContent += isVideo ? `\n![video](${d.url})` : `\n![image](${d.url})`;
    }

    const savedContent = trimmed;
    const savedImeta = [...currentPendingImeta];

    setContent("");
    contentRef.current = "";
    richText.clearContent();
    media.setPendingImeta([]);
    mentions.clearMentions();
    channelLinks.clearChannels();
    imageRefs.clear();
    setIsEmojiPickerOpen(false);

    const sendChannelId = channelIdRef.current;
    try {
      await onSendRef.current(finalContent, pubkeys, mediaTags);
      if (sendChannelId) {
        drafts.clearDraft(sendChannelId);
      }
    } catch {
      setContent(savedContent);
      contentRef.current = savedContent;
      richText.setContent(savedContent);
      media.setPendingImeta(savedImeta);
    }
  }, [
    drafts.clearDraft,
    imageRefs.clear,
    media.pendingImetaRef,
    media.setPendingImeta,
    mentions.extractMentionPubkeys,
    mentions.clearMentions,
    channelLinks.clearChannels,
    richText.clearContent,
    richText.editor,
    richText.setContent,
  ]);

  const handleSubmit = React.useCallback(
    (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      void submitMessage();
    },
    [submitMessage],
  );

  // ── Keyboard handling ───────────────────────────────────────────────
  // Tiptap handles formatting shortcuts (⌘B, ⌘I, etc.) natively.
  // We intercept Enter (submit) and arrow keys (autocomplete) via a
  // keydown listener on the editor wrapper.
  const handleEditorKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      // Let autocomplete handle keys first
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

      const imageRefResult = imageRefs.handleKeyDown(event);
      if (imageRefResult.handled) {
        if (imageRefResult.suggestion) {
          applyImageRefInsert(imageRefResult.suggestion);
        }
        return;
      }

      // Escape in edit mode
      if (event.key === "Escape" && editTargetRef.current && onCancelEdit) {
        event.preventDefault();
        onCancelEdit();
        return;
      }

      if (event.key !== "Enter" || event.nativeEvent.isComposing) {
        return;
      }

      // Shift+Enter or Ctrl+Enter → newline (Tiptap handles this natively
      // via hard break, so let it through)
      if (event.shiftKey || event.ctrlKey) {
        return;
      }

      // Meta/Alt+Enter → let through (system shortcuts)
      if (event.metaKey || event.altKey) {
        return;
      }

      // Plain Enter → submit
      event.preventDefault();
      void submitMessage();
    },
    [
      channelLinks.handleChannelKeyDown,
      applyChannelInsert,
      mentions.handleMentionKeyDown,
      applyMentionInsert,
      imageRefs.handleKeyDown,
      applyImageRefInsert,
      submitMessage,
      onCancelEdit,
    ],
  );

  // ── Media paste + ⌘K link shortcut via Tiptap editorProps ──────────
  const uploadFileRef = React.useRef(media.uploadFile);
  uploadFileRef.current = media.uploadFile;

  React.useEffect(() => {
    if (!richText.editor) return;

    richText.editor.setOptions({
      editorProps: {
        ...richText.editor.options.editorProps,
        handlePaste: (_view, event) => {
          const items = Array.from(event.clipboardData?.items ?? []);
          const mediaItem = items.find((item) =>
            ALLOWED_MEDIA_TYPES.includes(item.type),
          );
          if (!mediaItem) return false;

          const file = mediaItem.getAsFile();
          if (file) {
            void uploadFileRef.current(file);
          }
          return true;
        },
      },
    });
  }, [richText.editor]);

  // ── Send button state ───────────────────────────────────────────────
  const sendDisabled = React.useMemo(
    () =>
      disabled ||
      (content.trim().length === 0 && media.pendingImeta.length === 0),
    [disabled, content, media.pendingImeta.length],
  );

  const handleCaptureSelection = React.useCallback(() => {
    // No-op for Tiptap — selection is managed by ProseMirror.
  }, []);

  const handlePaperclipClick = React.useCallback(() => {
    void media.handlePaperclip();
  }, [media.handlePaperclip]);

  // ── Render ──────────────────────────────────────────────────────────
  return (
    <footer
      className={cn(
        "bg-background p-4",
        showTopBorder ? "border-t border-border/80" : "",
      )}
    >
      <div className="flex w-full flex-col gap-3">
        <form
          className="relative rounded-2xl border border-input bg-card px-3 py-4 shadow-sm sm:px-4"
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
          <ImageRefAutocomplete
            onSelect={applyImageRefInsert}
            selectedIndex={imageRefs.selectedIndex}
            suggestions={imageRefs.isOpen ? imageRefs.suggestions : []}
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

          {(isFormattingOpen || media.pendingImeta.length > 0) && (
            <div className="mb-1 flex items-center gap-2">
              {isFormattingOpen && (
                <FormattingToolbar
                  editor={richText.editor}
                  disabled={disabled}
                />
              )}
              <ComposerAttachments
                attachments={media.pendingImeta}
                onRemove={media.removeAttachment}
              />
            </div>
          )}

          {/* biome-ignore lint/a11y/noStaticElementInteractions: keydown handler bridges Tiptap editor to autocomplete and submit */}
          <div
            className="rich-text-composer max-h-32 overflow-y-auto"
            onKeyDown={handleEditorKeyDown}
          >
            <EditorContent editor={richText.editor} />
          </div>

          <MessageComposerToolbar
            composerDisabled={disabled}
            isEmojiPickerOpen={isEmojiPickerOpen}
            isFormattingOpen={isFormattingOpen}
            isSending={isSending}
            isUploading={media.isUploading}
            onCaptureSelection={handleCaptureSelection}
            onEmojiPickerOpenChange={setIsEmojiPickerOpen}
            onEmojiSelect={insertEmoji}
            onFormattingToggle={setIsFormattingOpen}
            onOpenMentionPicker={openMentionPicker}
            onPaperclip={handlePaperclipClick}
            sendDisabled={sendDisabled}
          />
        </form>
      </div>
    </footer>
  );
}
