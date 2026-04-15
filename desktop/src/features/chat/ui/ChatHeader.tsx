import {
  Activity,
  Bot,
  CircleDot,
  FileText,
  Hash,
  Home,
  Lock,
  Zap,
} from "lucide-react";
import type * as React from "react";

import type { ChannelType, ChannelVisibility } from "@/shared/api/types";

type ChatHeaderProps = {
  actions?: React.ReactNode;
  title: string;
  /** Subtitle under the title; omitted when empty. */
  description?: string;
  channelType?: ChannelType;
  visibility?: ChannelVisibility;
  mode?: "home" | "channel" | "agents" | "workflows" | "pulse";
  statusBadge?: React.ReactNode;
};

function ChannelIcon({
  channelType,
  visibility,
  mode = "channel",
}: {
  channelType?: ChannelType;
  visibility?: ChannelVisibility;
  mode?: "home" | "channel" | "agents" | "workflows" | "pulse";
}) {
  if (mode === "home") {
    return <Home className="h-5 w-5 text-primary" />;
  }

  if (mode === "agents") {
    return <Bot className="h-5 w-5 text-primary" />;
  }

  if (mode === "workflows") {
    return <Zap className="h-5 w-5 text-primary" />;
  }

  if (mode === "pulse") {
    return <Activity className="h-5 w-5 text-primary" />;
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
      className="relative z-20 flex shrink-0 min-w-0 items-center gap-3 bg-background/25 px-4 pb-2 pt-6 shadow-[0_4px_24px_rgba(0,0,0,0.06)] backdrop-blur-xl supports-[backdrop-filter]:bg-background/20 dark:shadow-[0_4px_24px_rgba(0,0,0,0.25)] sm:px-6"
      data-testid="chat-header"
      data-tauri-drag-region
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <ChannelIcon
            channelType={channelType}
            mode={mode}
            visibility={visibility}
          />
          <h1
            className="min-w-0 truncate text-lg font-semibold tracking-tight"
            data-testid="chat-title"
          >
            {title}
          </h1>
          {statusBadge ? (
            <div className="flex shrink-0 flex-wrap items-center gap-2">
              {statusBadge}
            </div>
          ) : null}
        </div>
        {description?.trim() ? (
          <p
            className="truncate text-sm text-muted-foreground"
            data-testid="chat-description"
          >
            {description}
          </p>
        ) : null}
      </div>

      {actions ? <div className="shrink-0">{actions}</div> : null}
    </header>
  );
}
