import * as React from "react";

import { Markdown as TiptapMarkdown } from "tiptap-markdown";
import { useEditor, type Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import Link from "@tiptap/extension-link";
import { Extension } from "@tiptap/core";
import { TextSelection } from "@tiptap/pm/state";

import { ImageRefNode } from "./imageRefExtension";
import { MentionHighlightExtension, mentionHighlightKey } from "./mentionHighlightExtension";

export type RichTextEditorOptions = {
  placeholder?: string;
  onUpdate?: (info: { markdown: string; text: string }) => void;
  editable?: boolean;
  mentionNames?: string[];
};

/**
 * Creates and manages a Tiptap editor configured for Markdown output.
 *
 * The editor uses StarterKit (bold, italic, strike, code, blockquote, lists,
 * headings, code blocks, hard breaks) plus Link and the tiptap-markdown
 * extension for serialisation.
 *
 * `getMarkdown()` returns the current document as a Markdown string.
 */
export function useRichTextEditor({
  placeholder,
  onUpdate,
  editable = true,
  mentionNames,
}: RichTextEditorOptions) {
  const onUpdateRef = React.useRef(onUpdate);
  onUpdateRef.current = onUpdate;

  const editor = useEditor(
    {
      extensions: [
        StarterKit.configure({
          // Use hard breaks (Shift+Enter) — Enter submits the message.
          hardBreak: {
            keepMarks: true,
          },
          // Disable the trailing-node plugin — it forces an empty paragraph
          // after block nodes (lists, blockquotes, code blocks) which creates
          // a phantom empty line in the compact message composer.
          trailingNode: false,
        }),
        // Shift+Enter inside lists/blockquotes: split the node instead of
        // inserting a hard break so continuation lines keep their formatting.
        Extension.create({
          name: "smartShiftEnter",
          addKeyboardShortcuts() {
            // Exit a list by removing the empty last item and inserting a
            // paragraph after the list. Works for both single-item and
            // multi-item lists.
            const exitListIfEmptyLast = (ed: typeof this.editor): boolean => {
              if (!ed.isActive("listItem")) return false;
              const { $from } = ed.state.selection;

              // Walk up to find the listItem node (handles nested structures).
              let listItemDepth = -1;
              for (let d = $from.depth; d >= 1; d--) {
                if ($from.node(d).type.name === "listItem") {
                  listItemDepth = d;
                  break;
                }
              }
              if (listItemDepth < 1) return false;

              const listItem = $from.node(listItemDepth);
              const isEmpty =
                listItem.childCount === 1 &&
                listItem.firstChild?.textContent === "";
              if (!isEmpty) return false;

              // Only trigger on the last item in the list.
              const listDepth = listItemDepth - 1;
              const list = $from.node(listDepth);
              const itemIndex = $from.index(listDepth);
              if (itemIndex !== list.childCount - 1) return false;

              const { tr, schema } = ed.state;
              if (list.childCount === 1) {
                // Only item → replace the entire list with an empty paragraph.
                const listStart = $from.before(listDepth);
                const listEnd = $from.after(listDepth);
                const para = schema.nodes.paragraph.create();
                tr.replaceWith(listStart, listEnd, para);
                tr.setSelection(
                  TextSelection.near(tr.doc.resolve(listStart + 1)),
                );
              } else {
                // Multiple items → delete the empty item, insert paragraph
                // after the list, and move cursor there.
                const itemStart = $from.before(listItemDepth);
                const itemEnd = $from.after(listItemDepth);
                tr.delete(itemStart, itemEnd);
                const listEnd = tr.mapping.map($from.after(listDepth));
                const para = schema.nodes.paragraph.create();
                tr.insert(listEnd, para);
                tr.setSelection(
                  TextSelection.near(tr.doc.resolve(listEnd + 1)),
                );
              }
              ed.view.dispatch(tr);
              return true;
            };

            return {
              "Shift-Enter": ({ editor: ed }) => {
                // Empty last list item → exit list to paragraph below.
                if (exitListIfEmptyLast(ed)) return true;
                // Non-empty or non-last list item → split.
                if (ed.isActive("listItem")) {
                  return ed.commands.splitListItem("listItem");
                }
                if (ed.isActive("blockquote")) {
                  // Empty blockquote paragraph → exit the blockquote.
                  const { $from } = ed.state.selection;
                  if ($from.parent.textContent === "") {
                    return ed.commands.lift("blockquote");
                  }
                  // Non-empty → split the paragraph within the blockquote.
                  return ed.chain().splitBlock().focus().run();
                }
                // Default: hard break (StarterKit handles it).
                return false;
              },
              ArrowDown: ({ editor: ed }) => {
                // Empty last list item + Down → exit list to paragraph below.
                return exitListIfEmptyLast(ed);
              },
            };
          },
        }),
        MentionHighlightExtension,
        Placeholder.configure({
          placeholder: placeholder ?? "Write a message…",
        }),
        Link.configure({
          openOnClick: false,
          autolink: true,
          linkOnPaste: true,
          HTMLAttributes: {
            class: "text-primary underline underline-offset-4 cursor-pointer",
          },
        }),
        TiptapMarkdown.configure({
          html: false,
          transformPastedText: true,
          transformCopiedText: true,
          breaks: true,
        }),
        ImageRefNode,
      ],
      editorProps: {
        attributes: {
          class:
            "min-h-0 resize-none overflow-y-hidden border-0 bg-transparent px-0 py-0 text-sm leading-6 md:leading-6 shadow-none focus-visible:ring-0 caret-foreground outline-none prose-sm max-w-none",
          "data-testid": "message-input",
        },
      },
      onUpdate: ({ editor: ed }) => {
        const markdown = getMarkdownFromEditor(ed);
        const text = ed.state.doc.textContent;
        onUpdateRef.current?.({ markdown, text });
      },
    },
    [placeholder],
  );

  // Toggle editable without destroying the editor instance.
  React.useEffect(() => {
    if (editor && editor.isEditable !== editable) {
      editor.setEditable(editable);
    }
  }, [editor, editable]);

  // Keep mention-highlight decorations in sync with the current member list.
  // NOTE: We use `editor.storage.mentionHighlight` (the mutable storage object
  // shared with the ProseMirror plugin closure) rather than finding the
  // extension instance via extensionManager — the instance's `.storage` getter
  // returns a fresh spread-copy on every access, so mutations are silently lost.
  React.useEffect(() => {
    if (!editor) return;
    // biome-ignore lint/suspicious/noExplicitAny: TipTap's Storage type doesn't include dynamic extension keys
    const storage = (editor.storage as any).mentionHighlight as
      | { names: string[] }
      | undefined;
    if (storage) {
      storage.names = mentionNames ?? [];
      // Force the plugin to re-decorate by dispatching a metadata transaction.
      const { tr } = editor.state;
      editor.view.dispatch(tr.setMeta(mentionHighlightKey, true));
    }
  }, [editor, mentionNames]);

  const getMarkdown = React.useCallback((): string => {
    if (!editor) return "";
    return getMarkdownFromEditor(editor);
  }, [editor]);

  const isEmpty = React.useCallback((): boolean => {
    if (!editor) return true;
    return editor.isEmpty;
  }, [editor]);

  const clearContent = React.useCallback(() => {
    editor?.commands.clearContent(true);
  }, [editor]);

  const setContent = React.useCallback(
    (markdown: string) => {
      if (!editor) return;
      editor.commands.setContent(markdown);
    },
    [editor],
  );

  const focus = React.useCallback(() => {
    editor?.commands.focus("end");
  }, [editor]);

  /**
   * Returns the plain-text content and an approximate cursor offset.
   * Used to bridge the existing useMentions / useChannelLinks hooks which
   * were designed for a plain <textarea>.
   */
  const getTextAndCursor = React.useCallback((): {
    text: string;
    cursor: number;
  } => {
    if (!editor) return { text: "", cursor: 0 };

    const { state } = editor;
    const text = state.doc.textContent;
    // Map ProseMirror position → plain-text offset.
    // Walk through text nodes and accumulate length until we pass the anchor.
    const anchor = state.selection.anchor;
    let offset = 0;
    let found = false;
    state.doc.descendants((node, pos) => {
      if (found) return false;
      if (node.isText) {
        const nodeEnd = pos + node.nodeSize;
        if (anchor <= nodeEnd) {
          offset += anchor - pos;
          found = true;
          return false;
        }
        offset += node.nodeSize;
      } else if (node.isBlock && pos > 0) {
        // Block boundaries add a newline in textContent
        // (but only between blocks, not at the very start)
      }
      return undefined;
    });
    if (!found) {
      offset = text.length;
    }

    return { text, cursor: offset };
  }, [editor]);

  /** Insert an image reference chip at the current cursor position. */
  const insertImageRef = React.useCallback(
    (
      url: string,
      hash: string,
      mediaType: string = "image",
      thumb?: string,
    ) => {
      if (!editor) return;
      editor
        .chain()
        .focus()
        .insertContent({
          type: "imageRef",
          attrs: { url, hash, mediaType, thumb },
        })
        .insertContent(" ")
        .run();
    },
    [editor],
  );

  return {
    editor,
    getMarkdown,
    insertImageRef,
    isEmpty,
    clearContent,
    setContent,
    focus,
    getTextAndCursor,
  };
}

function getMarkdownFromEditor(editor: Editor): string {
  // biome-ignore lint/suspicious/noExplicitAny: tiptap-markdown storage is untyped
  const storage = (editor.storage as any).markdown as
    | { getMarkdown?: () => string }
    | undefined;
  if (storage?.getMarkdown) {
    return storage.getMarkdown();
  }
  // Fallback: plain text
  return editor.state.doc.textContent;
}
