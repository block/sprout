import { SmilePlus } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { DEFAULT_EMOJI_OPTIONS } from "./messageTimelineUtils";

type ComposerEmojiPickerProps = {
  disabled?: boolean;
  onEmojiSelect: (emoji: string) => void;
  onOpenChange: (open: boolean) => void;
  onTriggerMouseDown: () => void;
  open: boolean;
};

export function ComposerEmojiPicker({
  disabled = false,
  onEmojiSelect,
  onOpenChange,
  onTriggerMouseDown,
  open,
}: ComposerEmojiPickerProps) {
  return (
    <Popover onOpenChange={onOpenChange} open={open}>
      <PopoverTrigger asChild>
        <Button
          aria-label="Insert emoji"
          data-testid="composer-emoji-button"
          disabled={disabled}
          onMouseDown={onTriggerMouseDown}
          size="icon"
          title="Insert emoji"
          type="button"
          variant="ghost"
        >
          <SmilePlus className="h-4 w-4" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-56 rounded-2xl p-3"
        side="top"
        sideOffset={10}
      >
        <div className="space-y-3">
          <div className="space-y-1">
            <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
              Emoji
            </p>
            <p className="text-xs text-muted-foreground">
              Insert an emoji into your message.
            </p>
          </div>
          <div className="grid grid-cols-4 gap-1">
            {DEFAULT_EMOJI_OPTIONS.map((emoji) => (
              <button
                aria-label={`Insert ${emoji}`}
                className="flex h-10 items-center justify-center rounded-xl border border-border/70 bg-muted/40 text-lg transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="composer-emoji-option"
                key={emoji}
                onClick={() => {
                  onEmojiSelect(emoji);
                }}
                type="button"
              >
                {emoji}
              </button>
            ))}
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
