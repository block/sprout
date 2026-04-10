import * as React from "react";

export type DraftState = {
  content: string;
  selectionStart: number;
  selectionEnd: number;
};

const sharedDrafts = new Map<string, DraftState>();

export function useDrafts() {
  const saveDraft = React.useCallback(
    (draftKey: string, draft: DraftState) => {
      if (draft.content.trim().length === 0) {
        return;
      }
      sharedDrafts.set(draftKey, draft);
    },
    [],
  );

  const loadDraft = React.useCallback(
    (draftKey: string): DraftState | undefined => {
      return sharedDrafts.get(draftKey);
    },
    [],
  );

  const clearDraft = React.useCallback((draftKey: string) => {
    sharedDrafts.delete(draftKey);
  }, []);

  return { saveDraft, loadDraft, clearDraft };
}
