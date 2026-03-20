import * as React from "react";
import Picker from "@emoji-mart/react";
import data from "@emoji-mart/data";
import { SmilePlus } from "lucide-react";

import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

type ComposerEmojiPickerProps = {
  disabled?: boolean;
  onEmojiSelect: (emoji: string) => void;
  onOpenChange: (open: boolean) => void;
  onTriggerMouseDown: () => void;
  open: boolean;
};

export const ComposerEmojiPicker = React.memo(function ComposerEmojiPicker({
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
        className="w-auto p-0 rounded-2xl overflow-hidden border-0 bg-transparent shadow-none"
        side="top"
        sideOffset={10}
      >
        <Picker
          data={data}
          onEmojiSelect={(emoji: any) => onEmojiSelect(emoji.native)}
          theme="auto"
          previewPosition="none"
          skinTonePosition="search"
          set="native"
          maxFrequentRows={2}
          perLine={8}
        />
      </PopoverContent>
    </Popover>
  );
});
