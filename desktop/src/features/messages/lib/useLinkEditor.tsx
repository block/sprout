import * as React from "react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

import type {
  LinkSelectionInfo,
  UseRichTextEditorResult,
} from "./useRichTextEditor";

type DraftState = {
  text: string;
  url: string;
  from: number;
  to: number;
  /** Whether the targeted range already carried a link (enables Remove). */
  isExistingLink: boolean;
};

/**
 * Owns the link-edit modal for a composer. Replaces the old `window.prompt`
 * flow (a no-op in the Tauri WebView) with a shadcn dialog that edits both
 * the display text and the URL, and offers a Remove action for existing
 * links.
 *
 * Returns:
 * - `openFromToolbar` — wire to the formatting toolbar's link button. Seeds
 *   the modal from the current selection (existing link or selected text).
 * - `openFromClick` — wire to `useRichTextEditor`'s `onEditLink`. Seeds the
 *   modal from the clicked link's range.
 * - `dialog` — render once inside the composer tree.
 */
export function useLinkEditor(richText: UseRichTextEditorResult) {
  const { getLinkSelectionInfo, applyLink, removeLink } = richText;
  const [draft, setDraft] = React.useState<DraftState | null>(null);
  const textId = React.useId();
  const urlId = React.useId();

  const openFromClick = React.useCallback((info: LinkSelectionInfo) => {
    setDraft({
      text: info.text,
      url: info.href,
      from: info.from,
      to: info.to,
      isExistingLink: info.href.length > 0,
    });
  }, []);

  const openFromToolbar = React.useCallback(() => {
    const info = getLinkSelectionInfo();
    if (info) {
      openFromClick(info);
      return;
    }
    // No selection and no link under the caret — open an empty modal that
    // inserts a fresh link at the caret on save.
    setDraft({ text: "", url: "", from: 0, to: 0, isExistingLink: false });
  }, [getLinkSelectionInfo, openFromClick]);

  const close = React.useCallback(() => setDraft(null), []);

  const save = React.useCallback(() => {
    if (!draft) return;
    const url = draft.url.trim();
    if (!url) return;
    if (draft.from === 0 && draft.to === 0) {
      // Empty-caret insert: fall back to current selection range.
      const info = getLinkSelectionInfo();
      applyLink({
        href: url,
        text: draft.text,
        from: info?.from ?? 0,
        to: info?.to ?? 0,
      });
    } else {
      applyLink({ href: url, text: draft.text, from: draft.from, to: draft.to });
    }
    close();
  }, [draft, applyLink, getLinkSelectionInfo, close]);

  const remove = React.useCallback(() => {
    if (!draft) return;
    removeLink({ from: draft.from, to: draft.to });
    close();
  }, [draft, removeLink, close]);

  const dialog = (
    <Dialog
      open={draft !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {draft?.isExistingLink ? "Edit link" : "Add link"}
          </DialogTitle>
          <DialogDescription>
            Set the text shown in the message and the URL it points to.
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            save();
          }}
        >
          <label
            className="flex flex-col gap-1 text-sm font-medium"
            htmlFor={textId}
          >
            Display text
            <Input
              id={textId}
              autoFocus
              placeholder="Text to display"
              value={draft?.text ?? ""}
              onChange={(event) =>
                setDraft((prev) =>
                  prev ? { ...prev, text: event.target.value } : prev,
                )
              }
            />
          </label>
          <label
            className="flex flex-col gap-1 text-sm font-medium"
            htmlFor={urlId}
          >
            URL
            <Input
              id={urlId}
              placeholder="https://example.com"
              value={draft?.url ?? ""}
              onChange={(event) =>
                setDraft((prev) =>
                  prev ? { ...prev, url: event.target.value } : prev,
                )
              }
            />
          </label>
          <div className="mt-2 flex items-center justify-between gap-2">
            {draft?.isExistingLink ? (
              <Button type="button" variant="destructive" onClick={remove}>
                Remove
              </Button>
            ) : (
              <span />
            )}
            <div className="flex items-center gap-2">
              <Button type="button" variant="ghost" onClick={close}>
                Cancel
              </Button>
              <Button type="submit" disabled={!draft?.url.trim()}>
                Save
              </Button>
            </div>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );

  return { openFromToolbar, openFromClick, dialog };
}
