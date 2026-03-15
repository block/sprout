import { Paperclip, SendHorizontal, SmilePlus } from "lucide-react";
import * as React from "react";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import { type BlobDescriptor, uploadMedia } from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";
import {
  MentionAutocomplete,
  type MentionSuggestion,
} from "./MentionAutocomplete";

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

/**
 * Detect an @mention query at the cursor position.
 * Returns the query string (after @) or null if no active mention trigger.
 */
function detectMentionQuery(
  value: string,
  cursorPosition: number,
): { query: string; startIndex: number } | null {
  const beforeCursor = value.slice(0, cursorPosition);
  // Find the last @ that is preceded by whitespace or is at the start
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

  // Mention state
  const [mentionQuery, setMentionQuery] = React.useState<string | null>(null);
  const [mentionStartIndex, setMentionStartIndex] = React.useState(0);
  const [mentionSelectedIndex, setMentionSelectedIndex] = React.useState(0);
  const mentionMapRef = React.useRef<Map<string, string>>(new Map());

  // Upload state
  const [uploadState, setUploadState] = React.useState<{
    status: "idle" | "uploading" | "error";
    message?: string;
  }>({ status: "idle" });
  const [pendingImeta, setPendingImeta] = React.useState<BlobDescriptor[]>([]);

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
      pendingSelectionRef.current = nextCursor;
      setContent(nextContent);
      setMentionQuery(null);
      setMentionSelectedIndex(0);
    },
    [content, mentionStartIndex],
  );

  const extractMentionPubkeys = React.useCallback((text: string): string[] => {
    const pubkeys: string[] = [];
    const mentionMap = mentionMapRef.current;

    for (const [displayName, pubkey] of mentionMap) {
      if (text.includes(`@${displayName}`)) {
        pubkeys.push(pubkey);
      }
    }

    return [...new Set(pubkeys)];
  }, []);

  const handleFileUpload = React.useCallback(
    async (filePath: string, isTemp: boolean, filename?: string) => {
      setUploadState({ status: "uploading" });
      try {
        const descriptor = await uploadMedia(filePath, isTemp, filename);
        const name = (filename ?? "").replace(/[^a-zA-Z0-9._\- ]/g, "_").slice(0, 100);
        const markdown = `\n![${name}](${descriptor.url})\n`;
        setContent((prev) => prev + markdown);
        setPendingImeta((prev) => [...prev, descriptor]);
        setUploadState({ status: "idle" });
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    },
    [],
  );

  const handlePaperclip = React.useCallback(async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: "Images",
          extensions: ["jpg", "jpeg", "png", "gif", "webp"],
        },
      ],
    });
    if (selected) {
      const filePath = typeof selected === "string" ? selected : selected.path;
      // Copy to temp dir before upload — the Tauri command restricts reads to
      // the OS temp directory to prevent file exfiltration from compromised renderers.
      try {
        const { tempDir } = await import("@tauri-apps/api/path");
        const { copyFile } = await import("@tauri-apps/plugin-fs");
        const tmp = await tempDir();
        const ext = filePath.split(".").pop() ?? "bin";
        const safeName = filePath.split(/[/\\]/).pop()?.replace(/[^a-zA-Z0-9._\- ]/g, "_") ?? "file";
        const tempPath = `${tmp}sprout-dialog-${crypto.randomUUID()}-${safeName}`;
        await copyFile(filePath, tempPath);
        await handleFileUpload(tempPath, true, safeName);
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    }
  }, [handleFileUpload]);

  const handleDrop = React.useCallback(
    async (event: React.DragEvent<HTMLFormElement>) => {
      event.preventDefault();
      const files = Array.from(event.dataTransfer.files);
      if (files.length === 0) return;

      const file = files[0];
      if (!file) return;

      // Client-side image filter — server also validates, but reject early for UX
      const ALLOWED_TYPES = ["image/jpeg", "image/png", "image/gif", "image/webp"];
      if (!ALLOWED_TYPES.includes(file.type)) {
        setUploadState({ status: "error", message: "Only JPEG, PNG, GIF, and WebP images are supported" });
        return;
      }

      // Write to temp file via Tauri fs plugin to avoid large IPC serialization
      try {
        const { tempDir } = await import("@tauri-apps/api/path");
        const { writeFile } = await import("@tauri-apps/plugin-fs");
        const tmp = await tempDir();
        const safeName = file.name.replace(/[/\\]/g, "_");
        const tempPath = `${tmp}sprout-upload-${crypto.randomUUID()}-${safeName}`;
        const buffer = await file.arrayBuffer();
        await writeFile(tempPath, new Uint8Array(buffer));
        await handleFileUpload(tempPath, true, file.name);
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    },
    [handleFileUpload],
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
      const ALLOWED_TYPES = ["image/jpeg", "image/png", "image/gif", "image/webp"];
      const imageItem = items.find((item) => ALLOWED_TYPES.includes(item.type));
      if (!imageItem) return;

      event.preventDefault();
      const file = imageItem.getAsFile();
      if (!file) return;

      try {
        const { tempDir } = await import("@tauri-apps/api/path");
        const { writeFile } = await import("@tauri-apps/plugin-fs");
        const tmp = await tempDir();
        const ext = file.type.split("/")[1] ?? "png";
        const tempPath = `${tmp}sprout-paste-${crypto.randomUUID()}.${ext}`;
        const buffer = await file.arrayBuffer();
        await writeFile(tempPath, new Uint8Array(buffer));
        await handleFileUpload(tempPath, true, file.name || `image.${ext}`);
      } catch (err) {
        setUploadState({ status: "error", message: String(err) });
      }
    },
    [handleFileUpload],
  );

  const submitMessage = React.useCallback(async () => {
    const trimmed = content.trim();
    const hasMedia = pendingImeta.length > 0;
    if ((!trimmed && !hasMedia) || disabled || isSending) {
      return;
    }

    const pubkeys = extractMentionPubkeys(trimmed);

    // Build imeta tags from pending descriptors
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
    setPendingImeta([]);
    mentionMapRef.current.clear();
    setMentionQuery(null);

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

      const mention = detectMentionQuery(nextContent, cursorPos);
      if (mention) {
        setMentionQuery(mention.query);
        setMentionStartIndex(mention.startIndex);
        setMentionSelectedIndex(0);
      } else {
        setMentionQuery(null);
      }
    },
    [],
  );

  const handleKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      // Handle mention autocomplete keyboard navigation
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
      textarea.setSelectionRange(pendingSelection, pendingSelection);
      pendingSelectionRef.current = null;
    }
  });

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

          <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <Button
                disabled={disabled || isUploading}
                onClick={() => {
                  void handlePaperclip();
                }}
                size="icon"
                title="Attach image"
                type="button"
                variant="ghost"
              >
                {isUploading ? (
                  <span className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
                ) : (
                  <Paperclip className="h-4 w-4" />
                )}
              </Button>
              <Button disabled size="icon" type="button" variant="ghost">
                <SmilePlus className="h-4 w-4" />
              </Button>
            </div>

            <Button
              className="gap-2"
              data-testid="send-message"
              disabled={disabled || isSending || (content.trim().length === 0 && pendingImeta.length === 0)}
              title="Send (Enter)"
              type="submit"
            >
              <SendHorizontal className="h-4 w-4" />
              {isSending ? "Sending" : "Send"}
            </Button>
          </div>
        </form>
      </div>
    </footer>
  );
}
