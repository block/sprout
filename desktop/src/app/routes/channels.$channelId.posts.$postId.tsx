import { createFileRoute } from "@tanstack/react-router";

import { ChannelRouteScreen } from "@/app/routes/channels.$channelId";

type ForumPostRouteSearch = {
  replyId?: string;
};

function validateForumPostSearch(
  search: Record<string, unknown>,
): ForumPostRouteSearch {
  return {
    replyId:
      typeof search.replyId === "string" && search.replyId.length > 0
        ? search.replyId
        : undefined,
  };
}

export const Route = createFileRoute("/channels/$channelId/posts/$postId")({
  validateSearch: validateForumPostSearch,
  component: ForumPostRouteComponent,
});

function ForumPostRouteComponent() {
  const { channelId, postId } = Route.useParams();
  const search = Route.useSearch();

  return (
    <ChannelRouteScreen
      channelId={channelId}
      selectedPostId={postId}
      targetMessageId={null}
      targetReplyId={search.replyId ?? null}
    />
  );
}
