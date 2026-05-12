import { BookMarked } from "lucide-react";
import { Link } from "@tanstack/react-router";

import { Badge } from "@/shared/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import { relativeTime } from "@/shared/lib/relative-time";
import type { Repo } from "../use-repos";

function truncateHex(hex: string): string {
  if (hex.length <= 12) return hex;
  return `${hex.slice(0, 8)}...${hex.slice(-4)}`;
}

export function RepoListItem({ repo }: { repo: Repo }) {
  return (
    <div className="py-6">
      {/* Row 1: Name + badge */}
      <div className="flex items-center gap-2">
        <BookMarked className="h-4 w-4 shrink-0 text-muted-foreground" />
        <Link
          to="/repos/$repoId"
          params={{ repoId: repo.id }}
          className="text-lg font-semibold text-primary hover:underline"
        >
          {repo.name}
        </Link>
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
