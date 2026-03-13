import { Paperclip, SendHorizontal, SmilePlus } from "lucide-react";
import * as React from "react";

import { useChannelMembersQuery } from "@/features/channels/hooks";
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
  onSend: (content: string, mentionPubkeys: string[]) => Promise<void>;
  placeholder?: string;
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
  onSend,
  placeholder,
}: MessageComposerProps) {
  const [content, setContent] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  const pendingSelectionRef = React.useRef<number | null>(null);

  // Mention state
  const [mentionQuery, setMentionQuery] = React.useState<string | null>(null);
  const [mentionStartIndex, setMentionStartIndex] = React.useState(0);
  const [mentionSelectedIndex, setMentionSelectedIndex] = React.useState(0);
  const mentionMapRef = React.useRef<Map<string, string>>(new Map());

  const membersQuery = useChannelMembersQuery(channelId);
  const members = membersQuery.data ?? [];

  const suggestions = React.useMemo<MentionSuggestion[]>(() => {
    if (mentionQuery === null) {
      return [];
    }

    const lowerQuery = mentionQuery.toLowerCase();
    return members
      .filter((member) =>
        member.displayName?.toLowerCase().includes(lowerQuery),
      )
      .slice(0, 8)
      .map((member) => ({
        pubkey: member.pubkey,
        displayName: member.displayName ?? member.pubkey.slice(0, 8),
        role: member.role === "admin" ? "admin" : null,
      }));
  }, [members, mentionQuery]);

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

  const submitMessage = React.useCallback(async () => {
    const trimmed = content.trim();
    if (!trimmed || disabled || isSending) {
      return;
    }

    const pubkeys = extractMentionPubkeys(trimmed);
    setContent("");
    mentionMapRef.current.clear();
    setMentionQuery(null);

    try {
      await onSend(trimmed, pubkeys);
    } catch {
      setContent(trimmed);
    }
  }, [content, disabled, isSending, onSend, extractMentionPubkeys]);

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

  return (
    <footer className="border-t border-border/80 bg-background p-4">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-3">
        <form
          className="relative rounded-2xl border border-input bg-card px-3 py-4 shadow-sm sm:px-4"
          data-testid="message-composer"
          onSubmit={(event) => {
            handleSubmit(event);
          }}
        >
          <MentionAutocomplete
            onSelect={insertMention}
            selectedIndex={mentionSelectedIndex}
            suggestions={isMentionOpen ? suggestions : []}
          />

          <Textarea
            aria-label="Message channel"
            className="min-h-0 resize-none overflow-y-hidden border-0 bg-transparent px-0 py-0 text-sm leading-6 shadow-none focus-visible:ring-0"
            data-testid="message-input"
            disabled={disabled}
            onChange={handleChange}
            onKeyDown={handleKeyDown}
            placeholder={placeholder ?? `Message #${channelName}`}
            ref={textareaRef}
            rows={1}
            value={content}
          />

          <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <Button disabled size="icon" type="button" variant="ghost">
                <Paperclip className="h-4 w-4" />
              </Button>
              <Button disabled size="icon" type="button" variant="ghost">
                <SmilePlus className="h-4 w-4" />
              </Button>
            </div>

            <Button
              className="gap-2"
              data-testid="send-message"
              disabled={disabled || isSending || content.trim().length === 0}
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
