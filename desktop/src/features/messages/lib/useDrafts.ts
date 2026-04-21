import * as React from "react";

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
      const maxDrafts = 50;
      if (drafts.size > maxDrafts) {
        const excess = drafts.size - maxDrafts;
        let removed = 0;
        for (const key of drafts.keys()) {
          if (removed >= excess) break;
          drafts.delete(key);
          removed++;
        }
      }
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
