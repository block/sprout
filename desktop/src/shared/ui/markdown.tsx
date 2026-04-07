import * as React from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

import { cn } from "@/shared/lib/cn";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import remarkChannelLinks from "@/shared/lib/remarkChannelLinks";
import remarkMentions from "@/shared/lib/remarkMentions";
import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import type { Channel } from "@/shared/api/types";

type MarkdownProps = {
  channelNames?: string[];
  className?: string;
  compact?: boolean;
  content: string;
  mentionNames?: string[];
  tight?: boolean;
};

type MarkdownVariant = "default" | "compact" | "tight";

function createMarkdownComponents(
  variant: MarkdownVariant,
  channels: Channel[],
  onOpenChannel: (channelId: string) => void,
): Components {
  const paragraphClassName =
    variant === "tight"
      ? "leading-5"
      : variant === "compact"
        ? "leading-6"
        : "leading-7";
  const listItemClassName =
    variant === "tight" ? "my-0.5 [&_p]:inline" : "my-1 [&_p]:inline";
  const listClassName =
    variant === "tight"
      ? "space-y-0.5 pl-6 marker:text-muted-foreground"
      : "space-y-1 pl-6 marker:text-muted-foreground";

  return {
    a: ({ children, href, ...props }) => (
      <a
        {...props}
        className="font-medium text-primary underline underline-offset-4 transition-colors hover:text-primary/80"
        href={href}
        rel="noreferrer"
        target="_blank"
      >
        {children}
      </a>
    ),
    blockquote: ({ children }) => (
      <blockquote className="border-l-2 border-border pl-4 italic text-muted-foreground">
        {children}
      </blockquote>
    ),
    br: () => <br />,
    code: ({
      children,
      className,
      ...props
    }: React.ComponentProps<"code"> & { inline?: boolean }) => {
      const code = String(children).replace(/\n$/, "");
      const isBlock =
        typeof className === "string" && className.includes("language-")
          ? true
          : code.includes("\n");

      if (isBlock) {
        return (
          <code
            {...props}
            className={cn(
              "block min-w-full whitespace-pre font-mono text-[13px] leading-6 text-foreground",
              className,
            )}
          >
            {code}
          </code>
        );
      }

      return (
        <code
          {...props}
          className={cn(
            "rounded-md bg-muted px-1.5 py-0.5 font-mono text-[13px] text-foreground",
            className,
          )}
        >
          {children}
        </code>
      );
    },
    h1: ({ children }) => (
      <h1 className="text-lg font-semibold tracking-tight">{children}</h1>
    ),
    h2: ({ children }) => (
      <h2 className="text-base font-semibold tracking-tight">{children}</h2>
    ),
    h3: ({ children }) => (
      <h3 className="font-semibold tracking-tight">{children}</h3>
    ),
    hr: () => <hr className="border-border/80" />,
    img: ({ alt, src }) => (
      <img
        alt={alt}
        className="max-h-96 rounded-2xl border border-border/70 object-cover"
        src={src ? rewriteRelayUrl(src) : src}
      />
    ),
    li: ({ children }) => <li className={listItemClassName}>{children}</li>,
    ol: ({ children }) => (
      <ol className={cn("list-decimal", listClassName)}>{children}</ol>
    ),
    p: ({ children }) => <p className={paragraphClassName}>{children}</p>,
    pre: ({ children }) => (
      <pre className="overflow-x-auto rounded-2xl border border-border/70 bg-muted/60 px-4 py-3 shadow-sm">
        {children}
      </pre>
    ),
    strong: ({ children }) => (
      <strong className="font-semibold">{children}</strong>
    ),
    table: ({ children }) => (
      <div className="overflow-x-auto rounded-2xl border border-border/70">
        <table className="w-full border-collapse text-left text-sm">
          {children}
        </table>
      </div>
    ),
    td: ({ children }) => (
      <td className="border-t border-border/70 px-3 py-2 align-top">
        {children}
      </td>
    ),
    th: ({ children }) => (
      <th className="bg-muted/60 px-3 py-2 font-semibold text-foreground">
        {children}
      </th>
    ),
    ul: ({ children }) => (
      <ul className={cn("list-disc", listClassName)}>{children}</ul>
    ),
    mention: ({ children }: { children?: React.ReactNode }) => (
      <span className="rounded-md bg-primary/15 px-1 py-0.5 text-sm font-medium text-primary">
        {children}
      </span>
    ),
    "channel-link": ({ children }: { children?: React.ReactNode }) => {
      const text = String(children ?? "");
      const channelName = text.startsWith("#") ? text.slice(1) : text;
      const channel = channels.find(
        (c) =>
          c.channelType !== "dm" &&
          c.name.toLowerCase() === channelName.toLowerCase(),
      );

      if (channel) {
        return (
          <button
            type="button"
            aria-label={`Open channel ${channelName}`}
            className="rounded-md bg-primary/15 px-1 py-0.5 text-sm font-medium text-primary cursor-pointer hover:bg-primary/25 transition-colors"
            onClick={() => {
              onOpenChannel(channel.id);
            }}
          >
            {children}
          </button>
        );
      }

      return (
        <span className="rounded-md bg-primary/15 px-1 py-0.5 text-sm font-medium text-primary">
          {children}
        </span>
      );
    },
  } as Components;
}

function shallowArrayEqual(a?: string[], b?: string[]): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function MarkdownInner({
  channelNames,
  className,
  compact = false,
  content,
  mentionNames,
  tight = false,
}: MarkdownProps) {
  const variant: MarkdownVariant = tight
    ? "tight"
    : compact
      ? "compact"
      : "default";
  const { channels, onOpenChannel } = useChannelNavigation();

  const components = React.useMemo(
    () => createMarkdownComponents(variant, channels, onOpenChannel),
    [variant, channels, onOpenChannel],
  );

  // biome-ignore lint/suspicious/noExplicitAny: PluggableList type not directly importable
  const remarkPlugins = React.useMemo<any[]>(
    () => [
      remarkGfm,
      remarkBreaks,
      [remarkMentions, { mentionNames }],
      [remarkChannelLinks, { channelNames }],
    ],
    [mentionNames, channelNames],
  );

  let processedContent = content;

  if (/^(?:\s{2}\n)+/.test(content)) {
    processedContent = `\u200B${processedContent}`;
  }

  if (/(?:\s{2}\n)+$/.test(content)) {
    processedContent = `${processedContent}\u200B`;
  }

  return (
    <div
      className={cn(
        tight
          ? "max-w-none break-words text-sm leading-5 text-foreground/90 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&>*]:my-0 [&>*+*]:mt-1"
          : compact
            ? "max-w-none break-words text-[15px] leading-6 text-foreground/90 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&>*]:my-1.5"
            : "max-w-none break-words text-sm leading-7 text-foreground/90 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&>*]:my-3",
        className,
      )}
    >
      <ReactMarkdown components={components} remarkPlugins={remarkPlugins}>
        {processedContent}
      </ReactMarkdown>
    </div>
  );
}

export const Markdown = React.memo(
  MarkdownInner,
  (prev, next) =>
    prev.content === next.content &&
    prev.className === next.className &&
    prev.compact === next.compact &&
    prev.tight === next.tight &&
    shallowArrayEqual(prev.mentionNames, next.mentionNames) &&
    shallowArrayEqual(prev.channelNames, next.channelNames),
);

Markdown.displayName = "Markdown";
