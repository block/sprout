import * as React from "react";
import type { Editor } from "@tiptap/react";
import { AnimatePresence, motion } from "motion/react";
import {
  ALargeSmall,
  ArrowUp,
  AtSign,
  Paperclip,
  X,
} from "lucide-react";

import { Button } from "@/shared/ui/button";
import { Toggle } from "@/shared/ui/toggle";
import { ComposerEmojiPicker } from "./ComposerEmojiPicker";
import { FormattingToolbar } from "./FormattingToolbar";

/** Spring for enter/exit of button groups — all fire simultaneously. */
const presenceSpring = {
  type: "spring",
  stiffness: 400,
  damping: 28,
} as const;

export const MessageComposerToolbar = React.memo(
  function MessageComposerToolbar({
    composerDisabled,
    editor,
    formattingDisabled,
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
    editor: Editor | null;
    formattingDisabled: boolean;
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
        <div className="flex min-w-0 flex-1 items-center gap-1 py-1 min-h-11">
          {/*
           * AnimatePresence with mode="popLayout" — exiting elements
           * are popped out of flow immediately so entering elements
           * can animate in simultaneously. No sequencing.
           *
           * The Aa toggle is duplicated inside both groups so
           * AnimatePresence handles the crossfade. No layoutId,
           * no order hacks, no overflow clipping needed.
           */}
          <AnimatePresence mode="popLayout" initial={false}>
            {isFormattingOpen ? (
              /*
               * ── Expanded: [Aa] [✕] | [formatting buttons] ──
               */
              <motion.div
                key="formatting-controls"
                className="flex min-w-0 flex-1 items-center gap-1"
                initial={false}
                animate={{}}
                exit={{ opacity: 0 }}
                transition={presenceSpring}
              >
                <motion.div
                  initial={{ x: 8, opacity: 0 }}
                  animate={{ x: 0, opacity: 1 }}
                  exit={{ x: 8, opacity: 0 }}
                  transition={presenceSpring}
                >
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
                </motion.div>
                <motion.div
                  className="flex items-center gap-1"
                  initial={{ opacity: 0, scale: 0.95 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.95 }}
                  transition={{ ...presenceSpring, delay: 0.15 }}
                >
                  <Button
                    aria-label="Close formatting"
                    disabled={composerDisabled}
                    onClick={() => onFormattingToggle(false)}
                    size="icon"
                    title="Close formatting"
                    type="button"
                    variant="ghost"
                    className="h-7 w-7 shrink-0"
                  >
                    <X className="h-3.5 w-3.5" />
                  </Button>
                  <div className="mx-1 h-5 w-px shrink-0 bg-border/60" />
                </motion.div>
                <motion.div
                  className="min-w-0 flex-1 overflow-x-auto"
                  initial={{ opacity: 0, scale: 0.95 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.95 }}
                  transition={{ ...presenceSpring, delay: 0.15 }}
                >
                  <FormattingToolbar
                    editor={editor}
                    disabled={formattingDisabled}
                  />
                </motion.div>
              </motion.div>
            ) : (
              /*
               * ── Passive: [@ 📎 😊] [Aa] ──
               */
              <motion.div
                key="ingress-controls"
                className="flex items-center gap-1"
                initial={{ opacity: 0, x: -12 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: -12 }}
                transition={presenceSpring}
              >
                <Button
                  aria-label="Mention someone"
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
                  aria-label="Attach image"
                  disabled={composerDisabled || isUploading}
                  onClick={onPaperclip}
                  size="icon"
                  title="Attach image"
                  type="button"
                  variant="ghost"
                >
                  {isUploading ? (
                    <span
                      className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent"
                      role="status"
                      aria-label="Uploading"
                    />
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
                <motion.div
                  initial={{ x: -8, opacity: 0 }}
                  animate={{ x: 0, opacity: 1 }}
                  exit={{ x: -8, opacity: 0 }}
                  transition={presenceSpring}
                >
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
                </motion.div>
              </motion.div>
            )}
          </AnimatePresence>
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
