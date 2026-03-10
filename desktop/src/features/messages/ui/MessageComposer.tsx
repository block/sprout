import { Paperclip, SendHorizontal, SmilePlus } from "lucide-react";
import * as React from "react";

import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

type MessageComposerProps = {
  channelName: string;
  disabled?: boolean;
  isSending?: boolean;
  onSend: (content: string) => Promise<void>;
  placeholder?: string;
};

export function MessageComposer({
  channelName,
  disabled = false,
  isSending = false,
  onSend,
  placeholder,
}: MessageComposerProps) {
  const [content, setContent] = React.useState("");

  const handleSubmit = React.useCallback(
    async (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();

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
    },
    [content, disabled, isSending, onSend],
  );

  return (
    <footer className="border-t border-border/80 bg-background p-4">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-3">
        <form
          className="rounded-2xl border border-input bg-card px-3 py-4 shadow-sm sm:px-4"
          data-testid="message-composer"
          onSubmit={(event) => {
            void handleSubmit(event);
          }}
        >
          <Input
            aria-label="Message channel"
            className="h-auto border-0 bg-transparent px-0 py-0 text-sm leading-6 shadow-none focus-visible:ring-0"
            data-testid="message-input"
            disabled={disabled}
            onChange={(event) => setContent(event.target.value)}
            placeholder={placeholder ?? `Message #${channelName}`}
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
