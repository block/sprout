import * as React from "react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

// ---------------------------------------------------------------------------
// Curated emoji list
// ---------------------------------------------------------------------------

const EMOJI_OPTIONS = [
  { emoji: "\uD83D\uDDE3\uFE0F", label: "In a meeting" },
  { emoji: "\uD83D\uDE8C", label: "Commuting" },
  { emoji: "\uD83E\uDD12", label: "Out sick" },
  { emoji: "\uD83C\uDFD6\uFE0F", label: "Vacationing" },
  { emoji: "\uD83C\uDFE0", label: "Working remotely" },
  { emoji: "\uD83C\uDF54", label: "Lunch" },
  { emoji: "\uD83C\uDFAF", label: "Focus" },
  { emoji: "\uD83D\uDCAA", label: "Exercising" },
] as const;

const PRESETS = [
  { text: "In a meeting", emoji: "\uD83D\uDDE3\uFE0F" },
  { text: "Commuting", emoji: "\uD83D\uDE8C" },
  { text: "Out sick", emoji: "\uD83E\uDD12" },
  { text: "Vacationing", emoji: "\uD83C\uDFD6\uFE0F" },
  { text: "Working remotely", emoji: "\uD83C\uDFE0" },
] as const;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type SetStatusDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialText?: string;
  initialEmoji?: string;
  onSave: (text: string, emoji: string) => void;
  onClear: () => void;
  hasExistingStatus: boolean;
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SetStatusDialog({
  open,
  onOpenChange,
  initialText = "",
  initialEmoji = "",
  onSave,
  onClear,
  hasExistingStatus,
}: SetStatusDialogProps) {
  const [text, setText] = React.useState(initialText);
  const [emoji, setEmoji] = React.useState(initialEmoji);

  React.useEffect(() => {
    if (open) {
      setText(initialText);
      setEmoji(initialEmoji);
    }
  }, [open, initialText, initialEmoji]);

  function handleEmojiClick(clickedEmoji: string) {
    setEmoji((prev) => (prev === clickedEmoji ? "" : clickedEmoji));
  }

  function handlePresetClick(preset: { text: string; emoji: string }) {
    setText(preset.text);
    setEmoji(preset.emoji);
  }

  function handleSave() {
    onSave(text.trim(), emoji);
    onOpenChange(false);
  }

  function handleClear() {
    onClear();
    onOpenChange(false);
  }

  function handleKeyDown(event: React.KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      handleSave();
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-[420px]"
        data-testid="set-status-dialog"
      >
        <DialogHeader>
          <DialogTitle>Set a status</DialogTitle>
          <DialogDescription>
            Let others know what you're up to.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4 pt-2">
          <div className="flex items-center gap-2">
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-input text-lg">
              {emoji || "\uD83D\uDCAC"}
            </span>
            <Input
              autoFocus
              data-testid="set-status-input"
              onChange={(event) => setText(event.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="What's your status?"
              value={text}
            />
          </div>

          <div className="flex flex-wrap gap-1.5">
            {EMOJI_OPTIONS.map((option) => (
              <button
                aria-label={option.label}
                className={`flex h-8 w-8 items-center justify-center rounded-md text-base transition-colors ${
                  emoji === option.emoji
                    ? "bg-accent ring-1 ring-ring"
                    : "hover:bg-accent/60"
                }`}
                data-testid={`set-status-emoji-${option.label.toLowerCase().replace(/\s+/g, "-")}`}
                key={option.emoji}
                onClick={() => handleEmojiClick(option.emoji)}
                title={option.label}
                type="button"
              >
                {option.emoji}
              </button>
            ))}
          </div>

          <div className="flex flex-wrap gap-1.5">
            {PRESETS.map((preset) => (
              <button
                className="rounded-full border border-input px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                data-testid={`set-status-preset-${preset.text.toLowerCase().replace(/\s+/g, "-")}`}
                key={preset.text}
                onClick={() => handlePresetClick(preset)}
                type="button"
              >
                {preset.emoji} {preset.text}
              </button>
            ))}
          </div>

          <div className="flex items-center justify-between gap-2 pt-1">
            <div>
              {hasExistingStatus ? (
                <Button
                  data-testid="set-status-clear"
                  onClick={handleClear}
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  Clear status
                </Button>
              ) : null}
            </div>
            <div className="flex items-center gap-2">
              <Button
                data-testid="set-status-cancel"
                onClick={() => onOpenChange(false)}
                size="sm"
                type="button"
                variant="ghost"
              >
                Cancel
              </Button>
              <Button
                data-testid="set-status-save"
                disabled={!text.trim() && !emoji}
                onClick={handleSave}
                size="sm"
                type="button"
              >
                Save
              </Button>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
