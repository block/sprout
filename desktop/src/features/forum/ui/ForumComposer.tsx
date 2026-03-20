import { Send } from "lucide-react";
import * as React from "react";

import { useChannelLinks } from "@/features/messages/lib/useChannelLinks";
import { useMentions } from "@/features/messages/lib/useMentions";
import { ChannelAutocomplete } from "@/features/messages/ui/ChannelAutocomplete";
import { MentionAutocomplete } from "@/features/messages/ui/MentionAutocomplete";
import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";

type ForumComposerProps = {
  channelId?: string | null;
  placeholder: string;
  submitLabel: string;
  disabled?: boolean;
  isSending?: boolean;
  onCancel?: () => void;
  onSubmit: (content: string, mentionPubkeys: string[]) => void;
};

export function ForumComposer({
  channelId = null,
  placeholder,
  submitLabel,
  disabled,
  isSending,
  onCancel,
  onSubmit,
}: ForumComposerProps) {
  const [value, setValue] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);

  const mentions = useMentions(channelId);
  const channelLinks = useChannelLinks();

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const trimmed = value.trim();
    if (!trimmed) return;
    const pubkeys = mentions.extractMentionPubkeys(trimmed);
    onSubmit(trimmed, pubkeys);
    setValue("");
    mentions.clearMentions();
    channelLinks.clearChannels();
  }

  function handleChange(event: React.ChangeEvent<HTMLTextAreaElement>) {
    const next = event.target.value;
    setValue(next);
    mentions.updateMentionQuery(next, event.target.selectionStart);
    channelLinks.updateChannelQuery(next, event.target.selectionStart);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    const channelResult = channelLinks.handleChannelKeyDown(event);
    if (channelResult.handled) {
      if (channelResult.suggestion) {
        const textarea = textareaRef.current;
        const result = channelLinks.insertChannel(
          channelResult.suggestion,
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
        channelLinks.clearChannels();
      }
    }
  }

  return (
    <form className="relative flex flex-col gap-2" onSubmit={handleSubmit}>
      <ChannelAutocomplete
        onSelect={(suggestion) => {
          const textarea = textareaRef.current;
          const result = channelLinks.insertChannel(
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
        selectedIndex={channelLinks.channelSelectedIndex}
        suggestions={
          channelLinks.isChannelOpen ? channelLinks.channelSuggestions : []
        }
      />
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
      <div className="flex justify-end gap-2">
        {onCancel ? (
          <Button
            disabled={isSending}
            onClick={onCancel}
            size="sm"
            type="button"
            variant="ghost"
          >
            Cancel
          </Button>
        ) : null}
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
