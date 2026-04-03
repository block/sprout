import * as React from "react";

import { getEventById } from "@/shared/api/tauri";
import type { RelayEvent, SearchHit } from "@/shared/api/types";

type UseSearchSessionResult = {
  isSearchOpen: boolean;
  setIsSearchOpen: React.Dispatch<React.SetStateAction<boolean>>;
  searchAnchor: SearchHit | null;
  searchAnchorChannelId: string | null;
  searchAnchorEvent: RelayEvent | null;
  handleOpenSearchResult: (
    hit: SearchHit,
    openChannel: (channelId: string) => void,
  ) => void;
  handleSearchTargetReached: (messageId: string) => void;
};

export function useSearchSession(): UseSearchSessionResult {
  const [isSearchOpen, setIsSearchOpen] = React.useState(false);
  const [searchAnchor, setSearchAnchor] = React.useState<SearchHit | null>(
    null,
  );
  const [searchAnchorChannelId, setSearchAnchorChannelId] = React.useState<
    string | null
  >(null);
  const [searchAnchorEvent, setSearchAnchorEvent] =
    React.useState<RelayEvent | null>(null);

  const handleOpenSearchResult = React.useCallback(
    (hit: SearchHit, openChannel: (channelId: string) => void) => {
      setSearchAnchor(hit);
      setSearchAnchorChannelId(hit.channelId);
      setSearchAnchorEvent({
        id: hit.eventId,
        pubkey: hit.pubkey,
        created_at: hit.createdAt,
        kind: hit.kind,
        tags: [["h", hit.channelId]],
        content: hit.content,
        sig: "",
      });
      openChannel(hit.channelId);

      void getEventById(hit.eventId)
        .then((event) => {
          setSearchAnchorEvent((current) => {
            if (current?.id !== hit.eventId) {
              return current;
            }
            return event;
          });
        })
        .catch((error) => {
          console.error(
            "Failed to load search result event",
            hit.eventId,
            error,
          );
        });
    },
    [],
  );

  const handleSearchTargetReached = React.useCallback((messageId: string) => {
    setSearchAnchor((current) =>
      current?.eventId === messageId ? null : current,
    );
  }, []);

  return {
    isSearchOpen,
    setIsSearchOpen,
    searchAnchor,
    searchAnchorChannelId,
    searchAnchorEvent,
    handleOpenSearchResult,
    handleSearchTargetReached,
  };
}
