import { Send } from "lucide-react";
import * as React from "react";

import { Button } from "@/shared/ui/button";

type ComposeNoteProps = {
  isPending: boolean;
  errorMessage?: string;
  onPublish: (content: string) => Promise<void>;
};

export function ComposeNote({
  isPending,
  errorMessage,
  onPublish,
}: ComposeNoteProps) {
  const [draft, setDraft] = React.useState("");
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);

  const canSubmit = draft.trim().length > 0 && !isPending;

  async function handleSubmit() {
    if (!canSubmit) return;
    try {
      await onPublish(draft.trim());
      setDraft("");
      textareaRef.current?.focus();
    } catch {
      // Draft preserved — error shown via errorMessage prop.
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && canSubmit) {
      e.preventDefault();
      void handleSubmit();
    }
  }

  return (
    <div className="border-t border-border/60 px-4 py-3 sm:px-6">
      <div className="flex gap-3">
        <div className="min-w-0 flex-1">
          <textarea
            ref={textareaRef}
            aria-label="Compose a note"
            className="w-full resize-none rounded-lg border border-border/60 bg-muted/30 px-3 py-2 text-sm placeholder:text-muted-foreground/60 focus:border-primary/50 focus:outline-none focus:ring-1 focus:ring-primary/30"
            disabled={isPending}
            maxLength={65536}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Post to Pulse..."
            rows={2}
            value={draft}
          />
          <div className="mt-1.5 flex items-center justify-between">
            <div className="flex items-center gap-2">
              {errorMessage ? (
                <span aria-live="polite" className="text-xs text-destructive">
                  {errorMessage}
                </span>
              ) : null}
            </div>
            <Button
              className="gap-1.5"
              disabled={!canSubmit}
              onClick={() => void handleSubmit()}
              size="sm"
            >
              <Send className="h-3.5 w-3.5" />
              Post
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
