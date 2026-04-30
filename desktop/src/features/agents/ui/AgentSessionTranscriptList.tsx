import { Brain, ChevronDown, Radio, TerminalSquare } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import { Markdown } from "@/shared/ui/markdown";
import { UserAvatar } from "@/shared/ui/UserAvatar";
import type { TranscriptItem } from "./agentSessionTypes";
import { ToolItem } from "./AgentSessionToolItem";

export function AgentSessionTranscriptList({
  agentAvatarUrl,
  agentName,
  emptyDescription,
  items,
}: {
  agentAvatarUrl: string | null;
  agentName: string;
  emptyDescription: string;
  items: TranscriptItem[];
}) {
  if (items.length === 0) {
    return (
      <div className="flex min-h-56 flex-col items-center justify-center px-6 py-10 text-center">
        <Radio className="mx-auto h-5 w-5 text-muted-foreground" />
        <p className="mt-3 text-sm font-medium">No ACP activity yet</p>
        <p className="mt-1 text-sm text-muted-foreground">{emptyDescription}</p>
      </div>
    );
  }

  return (
    <div
      aria-label="Live ACP transcript"
      aria-live="polite"
      className="w-full space-y-3 py-1"
      role="log"
    >
      {items.map((item) => (
        <div key={item.id}>
          <TranscriptItemView
            agentAvatarUrl={agentAvatarUrl}
            agentName={agentName}
            item={item}
          />
        </div>
      ))}
    </div>
  );
}

function TranscriptItemView({
  agentAvatarUrl,
  agentName,
  item,
}: {
  agentAvatarUrl: string | null;
  agentName: string;
  item: TranscriptItem;
}) {
  if (item.type === "message") {
    return (
      <MessageItem
        agentAvatarUrl={agentAvatarUrl}
        agentName={agentName}
        item={item}
      />
    );
  }
  if (item.type === "tool") {
    return <ToolItem item={item} />;
  }
  if (item.type === "thought") {
    return <ThoughtItem item={item} />;
  }
  if (item.type === "metadata") {
    return <MetadataItem item={item} />;
  }
  return <LifecycleItem item={item} />;
}

function MessageItem({
  agentAvatarUrl,
  agentName,
  item,
}: {
  agentAvatarUrl: string | null;
  agentName: string;
  item: Extract<TranscriptItem, { type: "message" }>;
}) {
  const isAssistant = item.role === "assistant";
  const text = item.text.trim();
  const label = isAssistant ? agentName : item.title;
  return (
    <div
      className="flex px-0 py-0.5 animate-in fade-in duration-200 motion-reduce:animate-none"
      data-role={isAssistant ? "assistant-message" : "user-message"}
    >
      <div className="group relative flex min-w-0 flex-1 flex-col items-start gap-1">
        <div className="flex items-center gap-1.5 text-xs">
          {isAssistant ? (
            <UserAvatar
              avatarUrl={agentAvatarUrl}
              className="rounded-full shadow-none"
              displayName={agentName}
              size="xs"
            />
          ) : null}
          <span className="font-medium text-foreground">{label}</span>
        </div>
        <div
          className={cn(
            "w-full min-w-0 text-left text-sm leading-relaxed",
            !isAssistant &&
              "rounded-xl border border-border/60 bg-muted/35 px-3 py-2 text-foreground",
          )}
        >
          {isAssistant ? (
            <Markdown compact content={text || " "} />
          ) : (
            <p className="whitespace-pre-wrap break-words">{text}</p>
          )}
        </div>
      </div>
    </div>
  );
}

function ThoughtItem({
  item,
}: {
  item: Extract<TranscriptItem, { type: "thought" }>;
}) {
  return (
    <details className="group not-prose w-full px-0">
      <summary className="inline-flex max-w-full cursor-pointer list-none items-center gap-1.5 py-0.5 text-left text-muted-foreground">
        <Brain className="h-4 w-4" />
        <span className="truncate text-sm font-medium">{item.title}</span>
        <ChevronDown className="h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-180" />
      </summary>
      <div className="py-2 pl-5 text-sm leading-6 text-muted-foreground">
        <Markdown compact content={item.text.trim() || " "} />
      </div>
    </details>
  );
}

function MetadataItem({
  item,
}: {
  item: Extract<TranscriptItem, { type: "metadata" }>;
}) {
  return (
    <details className="group not-prose w-full px-0">
      <summary className="inline-flex max-w-full cursor-pointer list-none items-center gap-1.5 py-0.5 text-left text-muted-foreground">
        <TerminalSquare className="h-4 w-4" />
        <span className="truncate text-sm font-medium">{item.title}</span>
        <span className="shrink-0 text-xs">
          {item.sections.length} section{item.sections.length === 1 ? "" : "s"}
        </span>
        <ChevronDown className="h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-180" />
      </summary>
      <div className="space-y-3 py-2 pl-5">
        {item.sections.map((section) => (
          <details
            className="group/section"
            key={`${section.title}:${section.body.slice(0, 48)}`}
          >
            <summary className="inline-flex max-w-full cursor-pointer list-none items-center gap-1.5 text-xs font-medium text-foreground/80">
              <span className="truncate">{section.title}</span>
              <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform group-open/section:rotate-180" />
            </summary>
            <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted/50 px-3 py-2 font-mono text-[11px] leading-5 text-muted-foreground">
              {section.body.trim() || "No metadata."}
            </pre>
          </details>
        ))}
      </div>
    </details>
  );
}

function LifecycleItem({
  item,
}: {
  item: Extract<TranscriptItem, { type: "lifecycle" }>;
}) {
  const isError = item.title.toLowerCase().includes("error");
  return (
    <div
      className={cn(
        "px-0 py-1 text-left text-xs leading-5",
        isError ? "text-destructive" : "text-muted-foreground",
      )}
    >
      <span className="font-medium">{item.title}</span>
      {item.text ? <span> - {item.text}</span> : null}
    </div>
  );
}
