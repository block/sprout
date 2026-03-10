import { CircleDot, FileText, Hash } from "lucide-react";

import type { ChannelType } from "@/shared/api/types";
import { SidebarTrigger } from "@/shared/ui/sidebar";

type ChatHeaderProps = {
  title: string;
  description: string;
  channelType?: ChannelType;
};

function ChannelIcon({ channelType }: { channelType?: ChannelType }) {
  if (channelType === "dm") {
    return <CircleDot className="h-5 w-5 text-primary" />;
  }

  if (channelType === "forum") {
    return <FileText className="h-5 w-5 text-primary" />;
  }

  return <Hash className="h-5 w-5 text-primary" />;
}

export function ChatHeader({
  title,
  description,
  channelType,
}: ChatHeaderProps) {
  return (
    <header
      className="flex min-w-0 items-center gap-3 border-b border-border/80 bg-background px-4 py-3 sm:px-6"
      data-testid="chat-header"
    >
      <SidebarTrigger />

      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <ChannelIcon channelType={channelType} />
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
    </header>
  );
}
