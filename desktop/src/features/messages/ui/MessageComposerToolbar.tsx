import * as React from "react";
import type { Editor } from "@tiptap/react";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import {
  ALargeSmall,
  AtSign,
  Paperclip,
  SendHorizontal,
  X,
} from "lucide-react";

import { Button } from "@/shared/ui/button";
import { Toggle } from "@/shared/ui/toggle";
import { ComposerEmojiPicker } from "./ComposerEmojiPicker";
import { FormattingToolbar } from "./FormattingToolbar";

/** Shared spring for layout shifts (Aa sliding between positions). */
const layoutSpring = { type: "spring", stiffness: 500, damping: 35 } as const;

/** Slightly softer spring for enter/exit of button groups. */
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
    /* ── Aa toggle (shared across both states via layoutId) ── */
    const aaToggle = (
      <motion.div layoutId="aa-toggle" transition={layoutSpring}>
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
    );

    return (
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
        <LayoutGroup>
          <motion.div layout transition={layoutSpring} className="flex items-center gap-1">
            <AnimatePresence mode="popLayout" initial={false}>
              {isFormattingOpen ? (
                /* ── Expanded: [Aa] [✕] | [formatting buttons] ── */
                <React.Fragment key="expanded">
                  {aaToggle}

                  {/* ✕ close button */}
                  <motion.div
                    key="close-btn"
                    initial={{ opacity: 0, scale: 0.6 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.6 }}
                    transition={presenceSpring}
                  >
                    <Button
                      aria-label="Close formatting"
                      disabled={composerDisabled}
                      onClick={() => onFormattingToggle(false)}
                      size="icon"
                      title="Close formatting"
                      type="button"
                      variant="ghost"
                      className="h-7 w-7"
                    >
                      <X className="h-3.5 w-3.5" />
                    </Button>
                  </motion.div>

                  {/* Separator + formatting buttons — slide in from left */}
                  <motion.div
                    key="formatting-group"
                    className="flex items-center gap-1"
                    initial={{ opacity: 0, x: -12 }}
                    animate={{ opacity: 1, x: 0 }}
                    exit={{ opacity: 0, x: -12 }}
                    transition={presenceSpring}
                  >
                    <div className="mx-1 h-5 w-px bg-border/60" />
                    <FormattingToolbar
                      editor={editor}
                      disabled={formattingDisabled}
                    />
                  </motion.div>
                </React.Fragment>
              ) : (
                /* ── Passive: [@ 📎 😊 Aa] ── */
                <React.Fragment key="passive">
                  {/* Ingress buttons — fade in + scale up */}
                  <motion.div
                    key="ingress-group"
                    className="flex items-center gap-1"
                    initial={{ opacity: 0, scale: 0.85 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.85 }}
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
                  </motion.div>

                  {aaToggle}
                </React.Fragment>
              )}
            </AnimatePresence>
          </motion.div>
        </LayoutGroup>

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
