import type * as React from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

import { cn } from "@/shared/lib/cn";
import remarkMentions from "@/shared/lib/remarkMentions";

type MarkdownProps = {
  className?: string;
  compact?: boolean;
  content: string;
  tight?: boolean;
};

const markdownComponents: Components = {
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
      src={src}
    />
  ),
  li: ({ children }) => <li className="my-1 [&_p]:inline">{children}</li>,
  ol: ({ children }) => (
    <ol className="list-decimal space-y-1 pl-6 marker:text-muted-foreground">
      {children}
    </ol>
  ),
  p: ({ children }) => <p className="leading-7">{children}</p>,
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
    <ul className="list-disc space-y-1 pl-6 marker:text-muted-foreground">
      {children}
    </ul>
  ),
  mention: ({ children }: { children?: React.ReactNode }) => (
    <span className="rounded-md bg-primary/15 px-1 py-0.5 text-sm font-medium text-primary">
      {children}
    </span>
  ),
} as Components;

export function Markdown({
  className,
  compact = false,
  content,
  tight = false,
}: MarkdownProps) {
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
          ? "max-w-none break-words text-sm leading-5 text-foreground/90 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&>*]:my-1"
          : compact
            ? "max-w-none break-words text-[15px] leading-6 text-foreground/90 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&>*]:my-1.5"
            : "max-w-none break-words text-sm leading-7 text-foreground/90 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&>*]:my-3",
        className,
      )}
    >
      <ReactMarkdown
        components={markdownComponents}
        remarkPlugins={[remarkGfm, remarkBreaks, remarkMentions]}
      >
        {processedContent}
      </ReactMarkdown>
    </div>
  );
}
