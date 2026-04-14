import * as React from "react";
import { ArrowUp, AtSign, Paperclip } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { ComposerEmojiPicker } from "./ComposerEmojiPicker";

export const MessageComposerToolbar = React.memo(
  function MessageComposerToolbar({
    composerDisabled,
    isEmojiPickerOpen,
    isSending,
    isUploading,
    onCaptureSelection,
    onEmojiPickerOpenChange,
    onEmojiSelect,
    onOpenMentionPicker,
    onPaperclip,
    sendDisabled,
  }: {
    composerDisabled: boolean;
    isEmojiPickerOpen: boolean;
    isSending: boolean;
    isUploading: boolean;
    onCaptureSelection: () => void;
    onEmojiPickerOpenChange: (open: boolean) => void;
    onEmojiSelect: (emoji: string) => void;
    onOpenMentionPicker: () => void;
    onPaperclip: () => void;
    sendDisabled: boolean;
  }) {
    return (
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <Button
            data-testid="message-insert-mention"
            disabled={composerDisabled}
            onClick={onOpenMentionPicker}
            onMouseDown={onCaptureSelection}
            size="icon"
            title="Mention someone"
            type="button"
            variant="ghost"
          >
            <AtSign className="h-4 w-4" />
          </Button>
          <Button
            disabled={composerDisabled || isUploading}
            onClick={onPaperclip}
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
          <ComposerEmojiPicker
            disabled={composerDisabled}
            onEmojiSelect={onEmojiSelect}
            onOpenChange={onEmojiPickerOpenChange}
            onTriggerMouseDown={onCaptureSelection}
            open={isEmojiPickerOpen}
          />
        </div>

        <Button
          aria-label={isSending ? "Sending" : "Send message"}
          className="rounded-full"
          data-testid="send-message"
          disabled={sendDisabled || isSending}
          size="icon"
          title={isSending ? "Sending..." : "Send (Enter)"}
          type="submit"
        >
          {isSending ? (
            <span
              aria-hidden
              className="h-4 w-4 animate-spin rounded-full border-2 border-primary-foreground border-t-transparent"
            />
          ) : (
            <ArrowUp aria-hidden className="h-4 w-4" />
          )}
        </Button>
      </div>
    );
  },
);
