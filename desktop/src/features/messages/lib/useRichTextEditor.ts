import * as React from "react";

import { Markdown as TiptapMarkdown } from "tiptap-markdown";
import { useEditor, type Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Placeholder from "@tiptap/extension-placeholder";
import Link from "@tiptap/extension-link";

import { ImageRefNode } from "./imageRefExtension";

export type RichTextEditorOptions = {
  placeholder?: string;
  onUpdate?: (info: { markdown: string; text: string }) => void;
  editable?: boolean;
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
        }),
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
