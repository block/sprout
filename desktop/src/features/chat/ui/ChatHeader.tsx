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
import { cn } from "@/shared/lib/cn";
import { useSidebar } from "@/shared/ui/sidebar";

type ChatHeaderProps = {
  actions?: React.ReactNode;
  title: string;
  description: string;
  channelType?: ChannelType;
  visibility?: ChannelVisibility;
  mode?: "home" | "channel" | "agents" | "workflows" | "pulse";
  statusBadge?: React.ReactNode;
};

const HEADER_ICON_CLASS = "h-4 w-4 text-muted-foreground";

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
    return <Home className={HEADER_ICON_CLASS} />;
  }

  if (mode === "agents") {
    return <Bot className={HEADER_ICON_CLASS} />;
  }

  if (mode === "workflows") {
    return <Zap className={HEADER_ICON_CLASS} />;
  }

  if (mode === "pulse") {
    return <Activity className={HEADER_ICON_CLASS} />;
  }

  if (channelType === "dm") {
    return <CircleDot className={HEADER_ICON_CLASS} />;
  }

  if (visibility === "private") {
    return <Lock className={HEADER_ICON_CLASS} />;
  }

  if (channelType === "forum") {
    return <FileText className={HEADER_ICON_CLASS} />;
  }

  return <Hash className={HEADER_ICON_CLASS} />;
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
  const trimmedDescription = description.trim();
  const { state: sidebarState } = useSidebar();
  const reserveGlobalControls = sidebarState === "collapsed";

  return (
    <header
      className={cn(
        "relative z-20 flex min-w-0 shrink-0 items-center gap-3 bg-background/25 px-4 py-1.5 shadow-[0_4px_24px_rgba(0,0,0,0.06)] backdrop-blur-xl transition-[padding] duration-200 ease-linear supports-[backdrop-filter]:bg-background/20 dark:shadow-[0_4px_24px_rgba(0,0,0,0.25)] sm:px-6",
        reserveGlobalControls && "md:pl-40",
      )}
      data-testid="chat-header"
      data-tauri-drag-region
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 -translate-y-px flex-wrap items-center gap-1.5">
          <ChannelIcon
            channelType={channelType}
            mode={mode}
            visibility={visibility}
          />
          <h1
            className="min-w-0 truncate text-base font-semibold leading-none tracking-tight"
            data-testid="chat-title"
            title={trimmedDescription || undefined}
          >
            {title}
          </h1>
          {statusBadge ? (
            <div className="flex shrink-0 flex-wrap items-center gap-1.5">
              {statusBadge}
            </div>
          ) : null}
        </div>
      </div>

      {actions ? <div className="shrink-0">{actions}</div> : null}
    </header>
  );
}
