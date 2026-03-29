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
      draftsRef.current.set(channelId, draft);
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
