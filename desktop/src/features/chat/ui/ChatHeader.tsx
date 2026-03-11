import { getCurrentWindow } from "@tauri-apps/api/window";
import { CircleDot, FileText, Hash, Home } from "lucide-react";
import type * as React from "react";

import type { ChannelType } from "@/shared/api/types";

type ChatHeaderProps = {
  actions?: React.ReactNode;
  title: string;
  description: string;
  channelType?: ChannelType;
  mode?: "home" | "channel";
};

function ChannelIcon({
  channelType,
  mode = "channel",
}: {
  channelType?: ChannelType;
  mode?: "home" | "channel";
}) {
  if (mode === "home") {
    return <Home className="h-5 w-5 text-primary" />;
  }

  if (channelType === "dm") {
    return <CircleDot className="h-5 w-5 text-primary" />;
  }

  if (channelType === "forum") {
    return <FileText className="h-5 w-5 text-primary" />;
  }

  return <Hash className="h-5 w-5 text-primary" />;
}

function handlePointerDown(e: React.PointerEvent) {
  if (e.button !== 0) return;
  const target = e.target as HTMLElement;
  if (target.closest('button, a, input, [role="button"]')) return;
  e.preventDefault();
  getCurrentWindow().startDragging();
}

export function ChatHeader({
  actions,
  title,
  description,
  channelType,
  mode = "channel",
}: ChatHeaderProps) {
  return (
    <header
      className="flex min-w-0 items-center gap-3 border-b border-border/80 bg-background px-4 py-3 sm:px-6"
      data-testid="chat-header"
      onPointerDown={handlePointerDown}
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <ChannelIcon channelType={channelType} mode={mode} />
          <h1
            className="truncate text-lg font-semibold tracking-tight"
            data-testid="chat-title"
          >
            {title}
          </h1>
        </div>
        <p
          className="truncate text-sm text-muted-foreground"
          data-testid="chat-description"
        >
          {description}
        </p>
      </div>

      {actions ? <div className="shrink-0">{actions}</div> : null}
    </header>
  );
}
