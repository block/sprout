import * as React from "react";

import { trimMapToSize } from "@/shared/lib/trimMapToSize";

export type DraftState = {
  content: string;
  selectionStart: number;
  selectionEnd: number;
};

export function useDrafts() {
  const draftsRef = React.useRef(new Map<string, DraftState>());

  const saveDraft = React.useCallback(
    (channelId: string, draft: DraftState) => {
      if (draft.content.trim().length === 0) {
        return;
      }
      const drafts = draftsRef.current;
      drafts.set(channelId, draft);
      trimMapToSize(drafts, 50);
    },
    [],
  );

  const loadDraft = React.useCallback(
    (channelId: string): DraftState | undefined => {
      return draftsRef.current.get(channelId);
    },
    [],
  );

  const clearDraft = React.useCallback((channelId: string) => {
    draftsRef.current.delete(channelId);
  }, []);

  return { saveDraft, loadDraft, clearDraft };
}
