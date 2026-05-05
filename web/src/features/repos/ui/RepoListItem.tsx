import { BookMarked } from "lucide-react";

import { Badge } from "@/shared/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import type { Repo } from "../use-repos";

function truncateHex(hex: string): string {
  if (hex.length <= 12) return hex;
  return `${hex.slice(0, 8)}...${hex.slice(-4)}`;
}

function relativeTime(unix: number): string {
  const now = Date.now();
  const diff = now - unix * 1000;
  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (days > 30) {
    const months = Math.floor(days / 30);
    return months === 1 ? "1 month ago" : `${months} months ago`;
  }
  if (days > 0) return days === 1 ? "1 day ago" : `${days} days ago`;
  if (hours > 0) return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
  if (minutes > 0)
    return minutes === 1 ? "1 minute ago" : `${minutes} minutes ago`;
  return "just now";
}

export function RepoListItem({ repo }: { repo: Repo }) {
  const link = repo.webUrl ?? "#";

  return (
    <div className="py-6">
      {/* Row 1: Name + badge */}
      <div className="flex items-center gap-2">
        <BookMarked className="h-4 w-4 shrink-0 text-muted-foreground" />
        <a
          href={link}
          className="text-lg font-semibold text-primary hover:underline"
        >
          {repo.name}
        </a>
        <Badge variant="outline" className="ml-1">
          Public
        </Badge>
      </div>

      {/* Row 2: Description */}
      {repo.description && (
        <p className="mt-1 line-clamp-2 text-sm text-muted-foreground">
          {repo.description}
        </p>
      )}

      {/* Row 3: Metadata */}
      <div className="mt-2 flex items-center gap-4 text-xs text-muted-foreground">
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="cursor-default font-mono">
              {truncateHex(repo.owner)}
            </span>
          </TooltipTrigger>
          <TooltipContent>{repo.owner}</TooltipContent>
        </Tooltip>
        <span>Updated {relativeTime(repo.createdAt)}</span>
      </div>
    </div>
  );
}
