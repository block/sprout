import * as React from "react";
import { ALargeSmall, AtSign, Paperclip, SendHorizontal } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { Toggle } from "@/shared/ui/toggle";
import { ComposerEmojiPicker } from "./ComposerEmojiPicker";

export const MessageComposerToolbar = React.memo(
  function MessageComposerToolbar({
    composerDisabled,
    isEmojiPickerOpen,
    isFormattingOpen,
    isSending,
    isUploading,
    onCaptureSelection,
    onEmojiPickerOpenChange,
    onEmojiSelect,
    onFormattingToggle,
    onOpenMentionPicker,
    onPaperclip,
    sendDisabled,
  }: {
    composerDisabled: boolean;
    isEmojiPickerOpen: boolean;
    isFormattingOpen: boolean;
    isSending: boolean;
    isUploading: boolean;
    onCaptureSelection: () => void;
    onEmojiPickerOpenChange: (open: boolean) => void;
    onEmojiSelect: (emoji: string) => void;
    onFormattingToggle: (pressed: boolean) => void;
    onOpenMentionPicker: () => void;
    onPaperclip: () => void;
    sendDisabled: boolean;
  }) {
    return (
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-1">
          <Toggle
            aria-label="Toggle formatting"
            disabled={composerDisabled}
            pressed={isFormattingOpen}
            onPressedChange={onFormattingToggle}
            size="sm"
            title="Formatting"
          >
            <ALargeSmall className="h-4 w-4" />
          </Toggle>

          <div className="mx-1 h-5 w-px bg-border/60" />

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
          className="gap-2"
          data-testid="send-message"
          disabled={sendDisabled || isSending}
          title="Send (Enter)"
          type="submit"
        >
          <SendHorizontal className="h-4 w-4" />
          {isSending ? "Sending" : "Send"}
        </Button>
      </div>
    );
  },
);
