import { Bot, CircleDot, FileText, Hash, Home, Lock, Settings2 } from "lucide-react";
import type * as React from "react";

import type { ChannelType, ChannelVisibility } from "@/shared/api/types";

type ChatHeaderProps = {
  actions?: React.ReactNode;
  title: string;
  description: string;
  channelType?: ChannelType;
  visibility?: ChannelVisibility;
  mode?: "home" | "channel" | "settings" | "agents";
  statusBadge?: React.ReactNode;
};

function ChannelIcon({
  channelType,
  visibility,
  mode = "channel",
}: {
  channelType?: ChannelType;
  visibility?: ChannelVisibility;
  mode?: "home" | "channel" | "settings" | "agents";
}) {
  if (mode === "home") {
    return <Home className="h-5 w-5 text-primary" />;
  }

  if (mode === "agents") {
    return <Bot className="h-5 w-5 text-primary" />;
  }

  if (mode === "settings") {
    return <Settings2 className="h-5 w-5 text-primary" />;
  }

  if (channelType === "dm") {
    return <CircleDot className="h-5 w-5 text-primary" />;
  }

  if (visibility === "private") {
    return <Lock className="h-5 w-5 text-primary" />;
  }

  if (channelType === "forum") {
    return <FileText className="h-5 w-5 text-primary" />;
  }

  return <Hash className="h-5 w-5 text-primary" />;
}

export function ChatHeader({
  actions,
  title,
  description,
  channelType,
  visibility,
  mode = "channel",
  statusBadge,
}: ChatHeaderProps) {
  return (
    <header
      className="flex min-w-0 items-center gap-3 border-b border-border/80 bg-background px-4 pb-3 pt-8 sm:px-6"
      data-testid="chat-header"
      data-tauri-drag-region
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <ChannelIcon channelType={channelType} mode={mode} visibility={visibility} />
          <h1
            className="truncate text-lg font-semibold tracking-tight"
            data-testid="chat-title"
          >
            {title}
          </h1>
          {statusBadge ? <div className="shrink-0">{statusBadge}</div> : null}
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
