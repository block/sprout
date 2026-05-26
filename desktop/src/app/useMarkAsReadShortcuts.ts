import * as React from "react";

import type { Channel } from "@/shared/api/types";
import { hasPrimaryShortcutModifier } from "@/shared/lib/platform";

export function useMarkAsReadShortcuts({
  activeChannel,
  markAllChannelsRead,
  markChannelRead,
  selectedView,
}: {
  activeChannel: Channel | null;
  markAllChannelsRead: () => void;
  markChannelRead: (
    channelId: string,
    lastMessageAt: string | null | undefined,
  ) => void;
  selectedView: string;
}) {
  React.useLayoutEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      if (event.defaultPrevented) return;
      if (hasPrimaryShortcutModifier(event) || event.altKey) return;

      if (event.shiftKey) {
        event.preventDefault();
        markAllChannelsRead();
        return;
      }

      if (selectedView === "channel" && activeChannel) {
        event.preventDefault();
        markChannelRead(activeChannel.id, activeChannel.lastMessageAt ?? null);
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [activeChannel, markAllChannelsRead, markChannelRead, selectedView]);
}
