import { Paperclip, SendHorizontal, SmilePlus } from "lucide-react";
import * as React from "react";

import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";

type MessageComposerProps = {
  channelName: string;
  disabled?: boolean;
  isSending?: boolean;
  onSend: (content: string) => Promise<void>;
  placeholder?: string;
};

const MAX_TEXTAREA_ROWS = 4;

export function MessageComposer({
  channelName,
  disabled = false,
  isSending = false,
  onSend,
  placeholder,
}: MessageComposerProps) {
  const [content, setContent] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  const pendingSelectionRef = React.useRef<number | null>(null);

  const submitMessage = React.useCallback(async () => {
    const trimmed = content.trim();
    if (!trimmed || disabled || isSending) {
      return;
    }

    setContent("");

    try {
      await onSend(trimmed);
    } catch {
      setContent(trimmed);
    }
  }, [content, disabled, isSending, onSend]);

  const handleSubmit = React.useCallback(
    (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      void submitMessage();
    },
    [submitMessage],
  );

  const handleKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
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
    [submitMessage],
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
          className="rounded-2xl border border-input bg-card px-3 py-4 shadow-sm sm:px-4"
          data-testid="message-composer"
          onSubmit={(event) => {
            handleSubmit(event);
          }}
        >
          <Textarea
            aria-label="Message channel"
            className="min-h-0 resize-none overflow-y-hidden border-0 bg-transparent px-0 py-0 text-sm leading-6 shadow-none focus-visible:ring-0"
            data-testid="message-input"
            disabled={disabled}
            onChange={(event) => setContent(event.target.value)}
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
