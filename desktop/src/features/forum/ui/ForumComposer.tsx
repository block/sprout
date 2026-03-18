import { Send } from "lucide-react";
import * as React from "react";

import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";

type ForumComposerProps = {
  placeholder: string;
  submitLabel: string;
  disabled?: boolean;
  isSending?: boolean;
  onSubmit: (content: string) => void;
};

export function ForumComposer({
  placeholder,
  submitLabel,
  disabled,
  isSending,
  onSubmit,
}: ForumComposerProps) {
  const [value, setValue] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmed = value.trim();
    if (!trimmed) return;
    onSubmit(trimmed);
    setValue("");
  }

  function handleKeyDown(event: React.KeyboardEvent) {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      const trimmed = value.trim();
      if (trimmed) {
        onSubmit(trimmed);
        setValue("");
      }
    }
  }

  return (
    <form className="flex flex-col gap-2" onSubmit={handleSubmit}>
      <Textarea
        className="min-h-[100px] resize-none bg-background/80"
        disabled={disabled || isSending}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        ref={textareaRef}
        value={value}
      />
      <div className="flex justify-end">
        <Button
          disabled={disabled || isSending || value.trim().length === 0}
          size="sm"
          type="submit"
        >
          <Send className="mr-1.5 h-3.5 w-3.5" />
          {isSending ? "Sending..." : submitLabel}
        </Button>
      </div>
    </form>
  );
}
