import { MessageSquareText } from "lucide-react";
import * as React from "react";

import { getThreadReference } from "@/features/messages/lib/threading";
import { useProfileQuery, useUsersBatchQuery } from "@/features/profile/hooks";
import { mergeCurrentProfileIntoLookup } from "@/features/profile/lib/identity";
import type { Channel, RelayEvent } from "@/shared/api/types";
import { KIND_FORUM_COMMENT, KIND_FORUM_POST } from "@/shared/constants/kinds";
import { Skeleton } from "@/shared/ui/skeleton";

import {
  useCreateForumPostMutation,
  useCreateForumReplyMutation,
  useDeleteForumPostMutation,
  useDeleteForumReplyMutation,
  useForumPostsQuery,
  useForumThreadQuery,
} from "../hooks";
import { ForumComposer } from "./ForumComposer";
import { ForumPostCard } from "./ForumPostCard";
import { ForumThreadPanel } from "./ForumThreadPanel";

type ForumViewProps = {
  channel: Channel;
  currentPubkey?: string;
  onTargetReached?: (messageId: string) => void;
  targetEvent: RelayEvent | null;
  targetEventId: string | null;
  targetEventKind: number | null;
};

function canDelete(postPubkey: string, currentPubkey?: string): boolean {
  if (!currentPubkey) return false;
  // Author can always delete their own posts. Admin check would need
  // channel member role data — for now, author-only is sufficient.
  return postPubkey.toLowerCase() === currentPubkey.toLowerCase();
}

export function ForumView({
  channel,
  currentPubkey,
  onTargetReached,
  targetEvent,
  targetEventId,
  targetEventKind,
}: ForumViewProps) {
  const [expandedPostId, setExpandedPostId] = React.useState<string | null>(
    null,
  );
  const [isComposerOpen, setIsComposerOpen] = React.useState(false);

  const profileQuery = useProfileQuery();
  const postsQuery = useForumPostsQuery(channel);
  const threadQuery = useForumThreadQuery(
    expandedPostId ? channel.id : null,
    expandedPostId,
  );
  const createPostMutation = useCreateForumPostMutation(channel);
  const createReplyMutation = useCreateForumReplyMutation(channel);
  const deletePostMutation = useDeleteForumPostMutation(channel);
  const deleteReplyMutation = useDeleteForumReplyMutation(
    channel,
    expandedPostId,
  );

  const posts = postsQuery.data?.posts ?? [];

  // Collect all pubkeys from posts and thread for profile resolution
  const allPubkeys = React.useMemo(() => {
    const pubkeys = new Set<string>();
    for (const post of posts) {
      pubkeys.add(post.pubkey);
      if (post.threadSummary?.participants) {
        for (const pk of post.threadSummary.participants) {
          pubkeys.add(pk);
        }
      }
    }
    if (threadQuery.data) {
      pubkeys.add(threadQuery.data.post.pubkey);
      for (const reply of threadQuery.data.replies) {
        pubkeys.add(reply.pubkey);
      }
    }
    return [...pubkeys];
  }, [posts, threadQuery.data]);

  const profilesQuery = useUsersBatchQuery(allPubkeys, {
    enabled: allPubkeys.length > 0,
  });
  const effectiveCurrentPubkey = currentPubkey ?? profileQuery.data?.pubkey;
  const profiles = React.useMemo(
    () =>
      mergeCurrentProfileIntoLookup(
        profilesQuery.data?.profiles,
        profileQuery.data,
      ),
    [profileQuery.data, profilesQuery.data?.profiles],
  );
  const targetThreadId = React.useMemo(() => {
    if (!targetEventId) {
      return null;
    }

    if (targetEventKind === KIND_FORUM_POST) {
      return targetEventId;
    }

    if (!targetEvent || targetEvent.kind !== KIND_FORUM_COMMENT) {
      return null;
    }

    const thread = getThreadReference(targetEvent.tags);
    return thread.rootId ?? thread.parentId ?? null;
  }, [targetEvent, targetEventId, targetEventKind]);

  // Reset expanded post when channel changes
  const previousChannelIdRef = React.useRef(channel.id);
  if (previousChannelIdRef.current !== channel.id) {
    previousChannelIdRef.current = channel.id;
    setExpandedPostId(null);
    setIsComposerOpen(false);
  }

  React.useEffect(() => {
    if (!targetThreadId || expandedPostId === targetThreadId) {
      return;
    }

    setExpandedPostId(targetThreadId);
  }, [expandedPostId, targetThreadId]);

  if (expandedPostId) {
    const threadPost = threadQuery.data?.post;
    const canDeleteExpandedPost = threadPost
      ? canDelete(threadPost.pubkey, effectiveCurrentPubkey)
      : false;

    return (
      <ForumThreadPanel
        canDeletePost={canDeleteExpandedPost}
        currentPubkey={effectiveCurrentPubkey}
        isDeletingPost={deletePostMutation.isPending}
        isLoading={threadQuery.isLoading}
        isSendingReply={createReplyMutation.isPending}
        onBack={() => setExpandedPostId(null)}
        onDeletePost={(eventId) => {
          deletePostMutation.mutate(
            { eventId },
            { onSuccess: () => setExpandedPostId(null) },
          );
        }}
        onDeleteReply={(eventId) => {
          deleteReplyMutation.mutate({ eventId });
        }}
        channelId={channel.id}
        onReply={(content, mentionPubkeys) => {
          createReplyMutation.mutate({
            content,
            parentEventId: expandedPostId,
            mentionPubkeys,
          });
        }}
        onTargetReached={onTargetReached}
        profiles={profiles}
        targetEventId={targetEventId}
        thread={threadQuery.data}
      />
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* New post area */}
      <div className="border-b border-border/60 p-4">
        {isComposerOpen ? (
          <ForumComposer
            channelId={channel.id}
            isSending={createPostMutation.isPending}
            onCancel={() => setIsComposerOpen(false)}
            onSubmit={(content, mentionPubkeys) => {
              createPostMutation.mutate(
                { content, mentionPubkeys },
                {
                  onSuccess: () => {
                    setIsComposerOpen(false);
                  },
                },
              );
            }}
            placeholder="Write your post..."
            submitLabel="Post"
          />
        ) : (
          <button
            className="w-full rounded-xl border border-dashed border-border/80 px-4 py-3 text-left text-sm text-muted-foreground transition-colors hover:border-border hover:bg-accent/30 hover:text-foreground"
            disabled={!channel.isMember || channel.archivedAt !== null}
            onClick={() => setIsComposerOpen(true)}
            type="button"
          >
            {channel.archivedAt
              ? "This forum is archived."
              : !channel.isMember
                ? "Join this forum to create posts."
                : "Start a new post..."}
          </button>
        )}
      </div>

      {/* Post list */}
      <div className="flex-1 overflow-y-auto">
        {postsQuery.isLoading ? (
          <div className="space-y-3 p-4">
            <Skeleton className="h-24 w-full rounded-xl" />
            <Skeleton className="h-24 w-full rounded-xl" />
            <Skeleton className="h-24 w-full rounded-xl" />
          </div>
        ) : posts.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-3 px-4 py-16 text-center">
            <MessageSquareText className="h-10 w-10 text-muted-foreground/40" />
            <div>
              <p className="text-sm font-medium text-foreground/70">
                No posts yet
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                Start a discussion by creating the first post.
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-3 p-4">
            {posts.map((post) => (
              <ForumPostCard
                canDelete={canDelete(post.pubkey, effectiveCurrentPubkey)}
                currentPubkey={effectiveCurrentPubkey}
                isActive={false}
                isDeleting={
                  deletePostMutation.isPending &&
                  deletePostMutation.variables?.eventId === post.eventId
                }
                key={post.eventId}
                onClick={() => setExpandedPostId(post.eventId)}
                onDelete={(eventId) => {
                  deletePostMutation.mutate({ eventId });
                }}
                post={post}
                profiles={profiles}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
