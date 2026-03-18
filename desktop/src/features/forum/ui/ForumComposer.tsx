import { Send } from "lucide-react";
import * as React from "react";

import { useMentions } from "@/features/messages/lib/useMentions";
import { MentionAutocomplete } from "@/features/messages/ui/MentionAutocomplete";
import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";

type ForumComposerProps = {
  channelId?: string | null;
  placeholder: string;
  submitLabel: string;
  disabled?: boolean;
  isSending?: boolean;
  onSubmit: (content: string, mentionPubkeys: string[]) => void;
};

export function ForumComposer({
  channelId = null,
  placeholder,
  submitLabel,
  disabled,
  isSending,
  onSubmit,
}: ForumComposerProps) {
  const [value, setValue] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);

  const mentions = useMentions(channelId);

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmed = value.trim();
    if (!trimmed) return;
    const pubkeys = mentions.extractMentionPubkeys(trimmed);
    onSubmit(trimmed, pubkeys);
    setValue("");
    mentions.clearMentions();
  }

  function handleChange(event: React.ChangeEvent<HTMLTextAreaElement>) {
    const next = event.target.value;
    setValue(next);
    mentions.updateMentionQuery(next, event.target.selectionStart);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    const { handled, suggestion } = mentions.handleMentionKeyDown(event);
    if (handled) {
      if (suggestion) {
        const textarea = textareaRef.current;
        const result = mentions.insertMention(
          suggestion,
          value,
          textarea?.selectionEnd ?? value.length,
        );
        setValue(result.nextContent);
        requestAnimationFrame(() => {
          textarea?.setSelectionRange(result.nextCursor, result.nextCursor);
        });
      }
      return;
    }

    if (
      event.key === "Enter" &&
      (event.metaKey || event.ctrlKey) &&
      !event.nativeEvent.isComposing
    ) {
      event.preventDefault();
      const trimmed = value.trim();
      if (trimmed) {
        const pubkeys = mentions.extractMentionPubkeys(trimmed);
        onSubmit(trimmed, pubkeys);
        setValue("");
        mentions.clearMentions();
      }
    }
  }

  return (
    <form className="relative flex flex-col gap-2" onSubmit={handleSubmit}>
      <MentionAutocomplete
        onSelect={(suggestion) => {
          const textarea = textareaRef.current;
          const result = mentions.insertMention(
            suggestion,
            value,
            textarea?.selectionEnd ?? value.length,
          );
          setValue(result.nextContent);
          requestAnimationFrame(() => {
            textarea?.setSelectionRange(result.nextCursor, result.nextCursor);
            textarea?.focus();
          });
        }}
        selectedIndex={mentions.mentionSelectedIndex}
        suggestions={mentions.isMentionOpen ? mentions.suggestions : []}
      />
      <Textarea
        className="min-h-[100px] resize-none bg-background/80"
        disabled={disabled || isSending}
        onChange={handleChange}
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
