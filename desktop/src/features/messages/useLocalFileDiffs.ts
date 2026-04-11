import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { TimelineMessage } from "@/features/messages/types";
import { KIND_STREAM_MESSAGE_DIFF } from "@/shared/constants/kinds";
import { formatTime } from "@/features/messages/lib/dateFormatters";

/**
 * Payload emitted by the Rust file watcher via Tauri events.
 * Must stay in sync with `FileDiffEvent` in `file_watcher.rs`.
 */
type FileDiffPayload = {
  channelId: string;
  filePath: string;
  unifiedDiff: string;
  timestamp: number;
};

/**
 * Listens for local `"file-diff"` Tauri events and converts them into
 * synthetic `TimelineMessage` entries that render through the existing
 * `DiffMessage` → `DiffViewer` pipeline.
 *
 * Only events matching `channelId` are kept. The hook accumulates diffs
 * for the lifetime of the channel view and resets when the channel changes.
 */
export function useLocalFileDiffs(channelId: string | null) {
  const [localDiffs, setLocalDiffs] = useState<TimelineMessage[]>([]);
  const channelIdRef = useRef(channelId);

  // Reset when the channel changes.
  useEffect(() => {
    if (channelIdRef.current !== channelId) {
      channelIdRef.current = channelId;
      setLocalDiffs([]);
    }
  }, [channelId]);

  useEffect(() => {
    if (!channelId) {
      return;
    }

    let unlisten: UnlistenFn | null = null;

    const setup = async () => {
      unlisten = await listen<FileDiffPayload>("file-diff", (event) => {
        const payload = event.payload;

        // Only process events for the active channel.
        if (payload.channelId !== channelId) {
          return;
        }

        const syntheticId = `local-diff-${payload.timestamp}-${payload.filePath}`;

        const message: TimelineMessage = {
          id: syntheticId,
          createdAt: payload.timestamp,
          author: "File Change",
          time: formatTime(payload.timestamp),
          body: payload.unifiedDiff,
          depth: 0,
          kind: KIND_STREAM_MESSAGE_DIFF,
          tags: [
            ["file", payload.filePath],
            ["local-diff", "true"],
          ],
        };

        setLocalDiffs((prev) => {
          // Dedupe by file path + timestamp (in case of rapid re-fires).
          const exists = prev.some((m) => m.id === syntheticId);
          if (exists) {
            return prev;
          }
          return [...prev, message];
        });
      });
    };

    void setup();

    return () => {
      unlisten?.();
    };
  }, [channelId]);

  const clearLocalDiffs = useCallback(() => {
    setLocalDiffs([]);
  }, []);

  return { localDiffs, clearLocalDiffs };
}
