import type {
  ForumPost,
  ForumPostsResponse,
  ForumThreadResponse,
  ThreadReply,
} from "@/shared/api/types";
import { KIND_FORUM_POST } from "@/shared/constants/kinds";

import { invokeTauri } from "./tauri";

type RawThreadSummary = {
  reply_count: number;
  descendant_count: number;
  last_reply_at: number | null;
  participants: string[];
};

type RawForumPost = {
  event_id: string;
  pubkey: string;
  content: string;
  kind: number;
  created_at: number;
  channel_id: string;
  tags: string[][];
  thread_summary: RawThreadSummary | null;
  reactions: unknown;
};

type RawForumPostsResponse = {
  messages: RawForumPost[];
  next_cursor: number | null;
};

type RawThreadReply = {
  event_id: string;
  pubkey: string;
  content: string;
  kind: number;
  created_at: number;
  channel_id: string;
  tags: string[][];
  parent_event_id: string | null;
  root_event_id: string | null;
  depth: number;
  broadcast: boolean;
  reactions: unknown;
};

type RawForumThreadResponse = {
  root: RawForumPost;
  replies: RawThreadReply[];
  total_replies: number;
  next_cursor: string | null;
};

function fromRawForumPost(post: RawForumPost): ForumPost {
  return {
    eventId: post.event_id,
    pubkey: post.pubkey,
    content: post.content,
    kind: post.kind,
    createdAt: post.created_at,
    channelId: post.channel_id,
    tags: post.tags,
    threadSummary: post.thread_summary
      ? {
          replyCount: post.thread_summary.reply_count,
          descendantCount: post.thread_summary.descendant_count,
          lastReplyAt: post.thread_summary.last_reply_at,
          participants: post.thread_summary.participants,
        }
      : null,
  };
}

function fromRawThreadReply(reply: RawThreadReply): ThreadReply {
  return {
    eventId: reply.event_id,
    pubkey: reply.pubkey,
    content: reply.content,
    kind: reply.kind,
    createdAt: reply.created_at,
    channelId: reply.channel_id,
    tags: reply.tags,
    parentEventId: reply.parent_event_id,
    rootEventId: reply.root_event_id,
    depth: reply.depth,
  };
}

export async function deleteMessage(eventId: string): Promise<void> {
  await invokeTauri("delete_message", { eventId });
}

export async function getForumPosts(
  channelId: string,
  limit?: number,
  before?: number,
): Promise<ForumPostsResponse> {
  const response = await invokeTauri<RawForumPostsResponse>("get_forum_posts", {
    channelId,
    limit: limit ?? null,
    before: before ?? null,
  });

  return {
    posts: response.messages
      .filter((m) => m.kind === KIND_FORUM_POST)
      .map(fromRawForumPost),
    nextCursor: response.next_cursor,
  };
}

export async function getForumThread(
  channelId: string,
  eventId: string,
  limit?: number,
  cursor?: string,
): Promise<ForumThreadResponse> {
  const response = await invokeTauri<RawForumThreadResponse>(
    "get_forum_thread",
    {
      channelId,
      eventId,
      limit: limit ?? null,
      cursor: cursor ?? null,
    },
  );

  return {
    post: fromRawForumPost(response.root),
    replies: response.replies.map(fromRawThreadReply),
    totalReplies: response.total_replies,
    nextCursor: response.next_cursor,
  };
}
