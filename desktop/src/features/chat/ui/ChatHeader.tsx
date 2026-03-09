import { Hash } from "lucide-react";

import { SidebarTrigger } from "@/shared/ui/sidebar";

type ChatHeaderProps = {
  title: string;
  description: string;
};

export function ChatHeader({ title, description }: ChatHeaderProps) {
  return (
    <header className="flex min-w-0 items-center gap-3 border-b border-border/80 bg-background px-4 py-3 sm:px-6">
      <SidebarTrigger />

      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <Hash className="h-5 w-5 text-primary" />
          <h1 className="truncate text-lg font-semibold tracking-tight">
            {title}
          </h1>
        </div>
        <p className="truncate text-sm text-muted-foreground">{description}</p>
      </div>
    </header>
  );
}
