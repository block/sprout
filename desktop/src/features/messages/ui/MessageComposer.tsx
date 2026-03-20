import * as React from "react";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import {
  type BlobDescriptor,
  pickAndUploadMedia,
  uploadMediaBytes,
} from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";
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

  const [mentionQuery, setMentionQuery] = React.useState<string | null>(null);
  const [mentionStartIndex, setMentionStartIndex] = React.useState(0);
  const [mentionSelectedIndex, setMentionSelectedIndex] = React.useState(0);
  const mentionMapRef = React.useRef<Map<string, string>>(new Map());

  const [uploadState, setUploadState] = React.useState<{
    status: "idle" | "uploading" | "error";
    message?: string;
  }>({ status: "idle" });
  const [pendingImeta, setPendingImeta] = React.useState<BlobDescriptor[]>([]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: channelId is the sole trigger — reset all composer state on channel switch to prevent draft/upload/mention leaks
  React.useEffect(() => {
    setContent("");
    setPendingImeta([]);
    setUploadState({ status: "idle" });
    setMentionQuery(null);
    setMentionStartIndex(0);
    setMentionSelectedIndex(0);
    setIsEmojiPickerOpen(false);
    mentionMapRef.current.clear();
    draftSelectionRef.current = { end: 0, start: 0 };
    pendingSelectionRef.current = null;
  }, [channelId]);

  const membersQuery = useChannelMembersQuery(channelId);
  const members = membersQuery.data ?? [];
  const managedAgentsQuery = useManagedAgentsQuery();
  const managedAgentNamesByPubkey = React.useMemo(
    () =>
      new Map(
        (managedAgentsQuery.data ?? []).map((agent) => [
          agent.pubkey.toLowerCase(),
          agent.name,
        ]),
      ),
    [managedAgentsQuery.data],
  );

  const suggestions = React.useMemo<MentionSuggestion[]>(() => {
    if (mentionQuery === null) {
      return [];
    }

    const lowerQuery = mentionQuery.toLowerCase();
    return members
      .map((member) => {
        const fallbackName =
          managedAgentNamesByPubkey.get(member.pubkey.toLowerCase()) ??
          member.pubkey.slice(0, 8);

        return {
          member,
          label: member.displayName ?? fallbackName,
        };
      })
      .filter(
        ({ label, member }) =>
          label.toLowerCase().includes(lowerQuery) ||
          member.pubkey.toLowerCase().includes(lowerQuery),
      )
      .slice(0, 8)
      .map(({ member, label }) => ({
        pubkey: member.pubkey,
        displayName: label,
        role: member.role === "admin" ? "admin" : null,
      }));
  }, [managedAgentNamesByPubkey, members, mentionQuery]);

  const isMentionOpen = mentionQuery !== null && suggestions.length > 0;

  const insertMention = React.useCallback(
    (suggestion: MentionSuggestion) => {
      const textarea = textareaRef.current;
      if (!textarea) {
        return;
      }

      const displayName = suggestion.displayName;
      const before = content.slice(0, mentionStartIndex);
      const after = content.slice(textarea.selectionEnd);
      const inserted = `@${displayName} `;
      const nextContent = `${before}${inserted}${after}`;
      const nextCursor = before.length + inserted.length;

      mentionMapRef.current.set(displayName, suggestion.pubkey);
      draftSelectionRef.current = {
        end: nextCursor,
        start: nextCursor,
      };
      pendingSelectionRef.current = nextCursor;
      setContent(nextContent);
      setMentionQuery(null);
      setMentionSelectedIndex(0);
    },
    [content, mentionStartIndex],
  );

  const extractMentionPubkeys = React.useCallback(
    (text: string): string[] => {
      const pubkeys: string[] = [];

      const hasMention = (name: string): boolean => {
        const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        const pattern = new RegExp(
          `(?:^|\\s)@${escaped}(?=[\\s,;.!?:)\\]}]|$)`,
          "i",
        );
        return pattern.test(text);
      };

      for (const [displayName, pubkey] of mentionMapRef.current) {
        if (hasMention(displayName)) {
          pubkeys.push(pubkey);
        }
      }

      for (const member of members) {
        if (pubkeys.includes(member.pubkey)) {
          continue;
        }
        const name =
          member.displayName ??
          managedAgentNamesByPubkey.get(member.pubkey.toLowerCase());
        if (name && hasMention(name)) {
          pubkeys.push(member.pubkey);
        }
      }

      return [...new Set(pubkeys)];
    },
    [members, managedAgentNamesByPubkey],
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
      setMentionQuery(null);
      setMentionSelectedIndex(0);
    },
    [content],
  );

  const openMentionPicker = React.useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) {
      return;
    }

    const cursorPosition = textarea.selectionStart ?? content.length;
    const existingMention = detectMentionQuery(content, cursorPosition);
    if (existingMention) {
      setMentionStartIndex(existingMention.startIndex);
      setMentionQuery(existingMention.query);
      setMentionSelectedIndex(0);
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
    setMentionStartIndex(mentionIndex);
    setMentionQuery("");
    setMentionSelectedIndex(0);
  }, [content]);

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

    const pubkeys = extractMentionPubkeys(trimmed);

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
    mentionMapRef.current.clear();
    setMentionQuery(null);
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
    extractMentionPubkeys,
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

      const mention = detectMentionQuery(nextContent, cursorPos);
      if (mention) {
        setMentionQuery(mention.query);
        setMentionStartIndex(mention.startIndex);
        setMentionSelectedIndex(0);
      } else {
        setMentionQuery(null);
      }
    },
    [updateDraftSelection],
  );

  const handleKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (isMentionOpen) {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          setMentionSelectedIndex((current) =>
            current < suggestions.length - 1 ? current + 1 : 0,
          );
          return;
        }

        if (event.key === "ArrowUp") {
          event.preventDefault();
          setMentionSelectedIndex((current) =>
            current > 0 ? current - 1 : suggestions.length - 1,
          );
          return;
        }

        if (
          event.key === "Tab" ||
          (event.key === "Enter" &&
            !event.ctrlKey &&
            !event.metaKey &&
            !event.altKey &&
            !event.shiftKey)
        ) {
          event.preventDefault();
          insertMention(suggestions[mentionSelectedIndex]);
          return;
        }

        if (event.key === "Escape") {
          event.preventDefault();
          setMentionQuery(null);
          return;
        }
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
      isMentionOpen,
      suggestions,
      mentionSelectedIndex,
      insertMention,
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
          <MentionAutocomplete
            onSelect={insertMention}
            selectedIndex={mentionSelectedIndex}
            suggestions={isMentionOpen ? suggestions : []}
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
