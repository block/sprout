import * as React from "react";
import { Loader2, Send, Sparkles } from "lucide-react";

import type { PersonaCreatorOutput } from "@/features/agents/persona-creator";
import {
  extractJsonBlock,
  parsePersonaCreatorOutput,
} from "@/features/agents/persona-creator";
import { Button } from "@/shared/ui/button";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";
import { Textarea } from "@/shared/ui/textarea";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ChatMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
};

type PersonaChatSheetProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSendMessage: (messages: ChatMessage[]) => Promise<string>;
  onConfirmCreate: (output: PersonaCreatorOutput) => Promise<void>;
  isPending: boolean;
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function PersonaChatSheet({
  open,
  onOpenChange,
  onSendMessage,
  onConfirmCreate,
  isPending,
}: PersonaChatSheetProps) {
  const [messages, setMessages] = React.useState<ChatMessage[]>([]);
  const [input, setInput] = React.useState("");
  const [isSending, setIsSending] = React.useState(false);
  const [parsedOutput, setParsedOutput] =
    React.useState<PersonaCreatorOutput | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const messagesEndRef = React.useRef<HTMLDivElement>(null);
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  const cancelledRef = React.useRef(false);

  // Auto-scroll when messages change
  React.useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  });

  // Auto-focus textarea when sheet opens; reset state on close
  React.useEffect(() => {
    if (open) {
      cancelledRef.current = false;
      setTimeout(() => textareaRef.current?.focus(), 100);
    } else {
      cancelledRef.current = true;
      setMessages([]);
      setInput("");
      setIsSending(false);
      setParsedOutput(null);
      setError(null);
    }
  }, [open]);

  async function handleSend() {
    const trimmed = input.trim();
    if (!trimmed || isSending) return;

    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content: trimmed,
    };
    const updatedMessages = [...messages, userMessage];
    setMessages(updatedMessages);
    setInput("");
    setIsSending(true);
    setError(null);

    try {
      const response = await onSendMessage(updatedMessages);
      if (cancelledRef.current) return;

      const assistantMessage: ChatMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: response,
      };
      setMessages((prev) => [...prev, assistantMessage]);

      // Check if the response contains a finalized JSON block
      const jsonBlock = extractJsonBlock(response);
      if (jsonBlock) {
        try {
          const raw = JSON.parse(jsonBlock);
          const result = parsePersonaCreatorOutput(raw);
          if (result.ok) {
            setParsedOutput(result.data);
          }
        } catch {
          // Not valid JSON yet - that's fine, conversation continues
        }
      }
    } catch (err) {
      if (cancelledRef.current) return;
      setError(err instanceof Error ? err.message : "Failed to get response");
    } finally {
      if (!cancelledRef.current) {
        setIsSending(false);
      }
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  }

  async function handleCreate() {
    if (!parsedOutput) return;
    setError(null);
    try {
      await onConfirmCreate(parsedOutput);
      onOpenChange(false);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to create personas",
      );
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="flex h-full flex-col sm:max-w-lg" side="right">
        <SheetHeader>
          <SheetTitle className="flex items-center gap-2">
            <Sparkles className="h-4 w-4" />
            Create with AI
          </SheetTitle>
          <SheetDescription>
            Describe the personas you want and I'll help design them.
          </SheetDescription>
        </SheetHeader>

        {/* Message list */}
        <div className="min-h-0 flex-1 overflow-y-auto py-4">
          {messages.length === 0 ? (
            <p className="px-1 text-sm text-muted-foreground">
              Tell me what kind of agent personas you'd like to create. I'll
              help you design their system prompts, pick names, and optionally
              group them into a team.
            </p>
          ) : (
            <div className="flex flex-col gap-3">
              {messages.map((msg) => (
                <div
                  key={msg.id}
                  className={
                    msg.role === "user"
                      ? "ml-8 rounded-lg bg-primary/10 px-3 py-2 text-sm"
                      : "mr-8 rounded-lg bg-muted px-3 py-2 text-sm"
                  }
                >
                  <p className="mb-1 text-xs font-medium text-muted-foreground">
                    {msg.role === "user" ? "You" : "Persona Architect"}
                  </p>
                  <div className="whitespace-pre-wrap">{msg.content}</div>
                </div>
              ))}
              {isSending ? (
                <div className="mr-8 flex items-center gap-2 rounded-lg bg-muted px-3 py-2 text-sm text-muted-foreground">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  Thinking...
                </div>
              ) : null}
              <div ref={messagesEndRef} />
            </div>
          )}
        </div>

        {/* Preview cards when output is parsed */}
        {parsedOutput ? (
          <div className="border-t pt-3">
            <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Ready to create
            </p>
            <div className="flex flex-wrap gap-2">
              {parsedOutput.personas.map((p) => (
                <div
                  key={p.displayName}
                  className="rounded-md border bg-card px-2 py-1 text-xs"
                >
                  {p.displayName}
                </div>
              ))}
              {parsedOutput.team ? (
                <div className="rounded-md border border-primary/30 bg-primary/5 px-2 py-1 text-xs text-primary">
                  Team: {parsedOutput.team.name}
                </div>
              ) : null}
            </div>
            <Button
              className="mt-2 w-full"
              disabled={isPending}
              onClick={() => void handleCreate()}
              size="sm"
            >
              {isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              Create All
            </Button>
          </div>
        ) : null}

        {/* Error display */}
        {error ? (
          <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </p>
        ) : null}

        {/* Input */}
        <div className="flex items-end gap-2 border-t pt-3">
          <Textarea
            ref={textareaRef}
            className="min-h-10 max-h-32 flex-1 resize-none"
            disabled={isSending}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Describe your personas..."
            rows={1}
            value={input}
          />
          <Button
            aria-label="Send message"
            disabled={!input.trim() || isSending}
            onClick={() => void handleSend()}
            size="sm"
            variant="default"
          >
            <Send className="h-4 w-4" />
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  );
}
