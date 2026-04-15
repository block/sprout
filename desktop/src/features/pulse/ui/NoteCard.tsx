import { Bot, UserMinus, UserPlus } from "lucide-react";

import type { UserNote } from "@/shared/api/socialTypes";
import type { UserProfileSummary } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";
import { UserAvatar } from "@/shared/ui/UserAvatar";

type NoteCardProps = {
  note: UserNote;
  profile?: UserProfileSummary | null;
  isAgent?: boolean;
  isOwnNote: boolean;
  isFollowing: boolean;
  onFollow?: (pubkey: string) => void;
  onUnfollow?: (pubkey: string) => void;
};

function formatRelativeTime(unixSeconds: number): string {
  const now = Date.now() / 1_000;
  const diff = now - unixSeconds;

  if (diff < 60) return "just now";
  if (diff < 3_600) return `${Math.floor(diff / 60)}m`;
  if (diff < 86_400) return `${Math.floor(diff / 3_600)}h`;
  if (diff < 604_800) return `${Math.floor(diff / 86_400)}d`;

  return new Date(unixSeconds * 1_000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

export function NoteCard({
  note,
  profile,
  isAgent,
  isOwnNote,
  isFollowing,
  onFollow,
  onUnfollow,
}: NoteCardProps) {
  const displayName = profile?.displayName ?? `${note.pubkey.slice(0, 8)}...`;
  const avatarUrl = profile?.avatarUrl ?? null;

  return (
    <article className="group flex gap-3 border-b border-border/50 px-4 py-3 transition-colors hover:bg-muted/30 sm:px-6">
      <div className="relative shrink-0 pt-0.5">
        <UserAvatar avatarUrl={avatarUrl} displayName={displayName} />
        {isAgent ? (
          <Bot className="absolute -bottom-0.5 -right-0.5 h-3.5 w-3.5 rounded-full bg-background p-0.5 text-muted-foreground" />
        ) : null}
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-semibold">{displayName}</span>
          {isAgent ? (
            <span className="inline-flex h-4 items-center rounded bg-muted px-1 text-[10px] font-medium text-muted-foreground">
              bot
            </span>
          ) : null}
          {profile?.nip05Handle ? (
            <span className="truncate text-xs text-muted-foreground">
              {profile.nip05Handle}
            </span>
          ) : null}
          <span className="shrink-0 text-xs text-muted-foreground">
            {formatRelativeTime(note.createdAt)}
          </span>
          {!isOwnNote ? (
            <div className="ml-auto hidden shrink-0 group-hover:block">
              {isFollowing ? (
                <Button
                  className="h-6 gap-1 px-2 text-xs"
                  onClick={() => onUnfollow?.(note.pubkey)}
                  size="sm"
                  variant="ghost"
                >
                  <UserMinus className="h-3 w-3" />
                  Unfollow
                </Button>
              ) : (
                <Button
                  className="h-6 gap-1 px-2 text-xs"
                  onClick={() => onFollow?.(note.pubkey)}
                  size="sm"
                  variant="ghost"
                >
                  <UserPlus className="h-3 w-3" />
                  Follow
                </Button>
              )}
            </div>
          ) : null}
        </div>

        <div className="mt-1 text-sm leading-relaxed">
          <Markdown content={note.content} />
        </div>
      </div>
    </article>
  );
}
