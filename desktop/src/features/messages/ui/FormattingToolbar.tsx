import * as React from "react";
import { useEditorState, type Editor } from "@tiptap/react";
import {
  Bold,
  Code,
  Italic,
  Link,
  List,
  ListOrdered,
  Quote,
  Strikethrough,
} from "lucide-react";

import { Toggle } from "@/shared/ui/toggle";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

type FormattingToolbarProps = {
  editor: Editor | null;
  disabled?: boolean;
};

/**
 * Formatting bar shown above the editor when the format toggle is active.
 * Renders all formatting buttons in a single row with bg-muted styling.
 */
export const FormattingToolbar = React.memo(function FormattingToolbar({
  editor,
  disabled = false,
}: FormattingToolbarProps) {
  const toggleBold = React.useCallback(() => {
    editor?.chain().focus().toggleBold().run();
  }, [editor]);

  const toggleItalic = React.useCallback(() => {
    editor?.chain().focus().toggleItalic().run();
  }, [editor]);

  const toggleStrike = React.useCallback(() => {
    editor?.chain().focus().toggleStrike().run();
  }, [editor]);

  const toggleCode = React.useCallback(() => {
    editor?.chain().focus().toggleCode().run();
  }, [editor]);

  const toggleLink = React.useCallback(() => {
    if (!editor) return;

    if (editor.isActive("link")) {
      editor.chain().focus().unsetLink().run();
      return;
    }

    const { from, to } = editor.state.selection;
    const hasSelection = from !== to;

    if (hasSelection) {
      const url = window.prompt("Enter URL:");
      if (url) {
        editor.chain().focus().setLink({ href: url }).run();
      }
    } else {
      const url = window.prompt("Enter URL:");
      if (url) {
        const label = window.prompt("Link text:", url) || url;
        editor.chain().focus().insertContent(`[${label}](${url})`).run();
      }
    }
  }, [editor]);

  const toggleBulletList = React.useCallback(() => {
    editor?.chain().focus().toggleBulletList().run();
  }, [editor]);

  const toggleOrderedList = React.useCallback(() => {
    editor?.chain().focus().toggleOrderedList().run();
  }, [editor]);

  const toggleBlockquote = React.useCallback(() => {
    editor?.chain().focus().toggleBlockquote().run();
  }, [editor]);

  // Subscribe to editor state changes so active marks/nodes update on
  // selection change — useEditorState triggers re-renders when the
  // selector result changes.
  const activeStates = useEditorState({
    editor,
    selector: ({ editor: ed }) =>
      ed
        ? {
            bold: ed.isActive("bold"),
            italic: ed.isActive("italic"),
            strike: ed.isActive("strike"),
            code: ed.isActive("code"),
            link: ed.isActive("link"),
            bulletList: ed.isActive("bulletList"),
            orderedList: ed.isActive("orderedList"),
            blockquote: ed.isActive("blockquote"),
          }
        : null,
  });

  if (!editor || !activeStates) return null;

  const items = [
    {
      icon: Bold,
      label: "Bold",
      shortcut: "⌘B",
      action: toggleBold,
      active: activeStates.bold,
    },
    {
      icon: Italic,
      label: "Italic",
      shortcut: "⌘I",
      action: toggleItalic,
      active: activeStates.italic,
    },
    {
      icon: Strikethrough,
      label: "Strikethrough",
      shortcut: "⌘⇧X",
      action: toggleStrike,
      active: activeStates.strike,
    },
    {
      icon: Code,
      label: "Code",
      shortcut: "⌘E",
      action: toggleCode,
      active: activeStates.code,
    },
    {
      icon: Link,
      label: "Link",
      shortcut: "⌘K",
      action: toggleLink,
      active: activeStates.link,
    },
    {
      icon: List,
      label: "Bullet list",
      action: toggleBulletList,
      active: activeStates.bulletList,
    },
    {
      icon: ListOrdered,
      label: "Ordered list",
      action: toggleOrderedList,
      active: activeStates.orderedList,
    },
    {
      icon: Quote,
      label: "Quote",
      action: toggleBlockquote,
      active: activeStates.blockquote,
    },
  ] as const;

  return (
    <div className="flex items-center gap-0.5">
      {items.map((item) => (
        <Tooltip key={item.label}>
          <TooltipTrigger asChild>
            <Toggle
              aria-label={item.label}
              disabled={disabled}
              pressed={item.active}
              onPressedChange={() => item.action()}
              className="h-7 w-7 min-w-7 [&_svg]:size-3.5"
            >
              <item.icon className="h-3.5 w-3.5" />
            </Toggle>
          </TooltipTrigger>
          <TooltipContent>
            {"shortcut" in item
              ? `${item.label} (${item.shortcut})`
              : item.label}
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
  );
});
