import { html } from 'diff2html';
import 'diff2html/bundles/css/diff2html.min.css';
import DOMPurify from 'dompurify';
import { FileDiff, Maximize2 } from 'lucide-react';
import { useMemo } from 'react';

import { Button } from '@/shared/ui/button';

type DiffMessageProps = {
  content: string;
  repoUrl?: string;
  filePath?: string;
  commitSha?: string;
  description?: string;
  truncated?: boolean;
  onExpand?: () => void;
};

function isSafeUrl(url: string | undefined): url is string {
  if (!url) return false;
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:';
  } catch {
    return false;
  }
}

function getHostname(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url;
  }
}

const ALLOWED_TAGS = [
  'div', 'span', 'table', 'thead', 'tbody', 'tr', 'th', 'td',
  'pre', 'code', 'ins', 'del', 'a', 'i', 'em', 'strong', 'small',
];

const ALLOWED_ATTR = [
  'class', 'id', 'data-line-number', 'href', 'title', 'aria-label',
];

const ALLOWED_URI_REGEXP = /^https?:\/\//i;

export function DiffMessage({
  content,
  repoUrl,
  filePath,
  commitSha,
  description,
  truncated,
  onExpand,
}: DiffMessageProps) {
  const { diffHtml, renderError } = useMemo(() => {
    try {
      const rawHtml = html(content, {
        drawFileList: false,
        matching: 'lines',
        outputFormat: 'side-by-side',
      });
      const sanitized = DOMPurify.sanitize(rawHtml, {
        ALLOWED_TAGS,
        ALLOWED_ATTR,
        ALLOWED_URI_REGEXP,
      });
      return { diffHtml: sanitized, renderError: false };
    } catch {
      return { diffHtml: '', renderError: true };
    }
  }, [content]);

  const safeRepoUrl = isSafeUrl(repoUrl) ? repoUrl : undefined;

  const commitUrl =
    safeRepoUrl && commitSha
      ? `${safeRepoUrl}/commit/${commitSha}`
      : undefined;

  const shortSha = commitSha ? commitSha.slice(0, 7) : undefined;

  return (
    <div className="rounded-xl border border-border/70 bg-card/60 overflow-hidden text-sm">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border/50 bg-muted/40">
        <FileDiff className="h-4 w-4 shrink-0 text-muted-foreground" />
        <span className="flex-1 truncate font-mono text-xs text-foreground/80">
          {filePath ?? 'diff'}
        </span>
        {shortSha && (
          <span className="text-xs text-muted-foreground font-mono">
            {commitUrl ? (
              <a
                className="hover:underline"
                href={commitUrl}
                rel="noreferrer noopener"
                target="_blank"
              >
                {shortSha}
              </a>
            ) : (
              shortSha
            )}
          </span>
        )}
        {safeRepoUrl && !commitUrl && (
          <span className="text-xs text-muted-foreground">
            <a
              className="hover:underline"
              href={safeRepoUrl}
              rel="noreferrer noopener"
              target="_blank"
            >
              {getHostname(safeRepoUrl)}
            </a>
          </span>
        )}
        {onExpand && (
          <Button
            className="h-6 w-6 p-0 text-muted-foreground hover:text-foreground"
            onClick={onExpand}
            size="sm"
            title="Expand diff"
            type="button"
            variant="ghost"
          >
            <Maximize2 className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>

      {/* Description */}
      {description && (
        <div className="px-3 py-1.5 text-xs text-muted-foreground border-b border-border/40 bg-muted/20">
          {description}
        </div>
      )}

      {/* Diff content — max 400px height, scrollable */}
      <div className="max-h-[400px] overflow-auto text-xs">
        {renderError ? (
          <pre className="p-3 whitespace-pre-wrap font-mono text-muted-foreground">{content}</pre>
        ) : diffHtml ? (
          // biome-ignore lint/security/noDangerouslySetInnerHtml: sanitized by DOMPurify
          <div dangerouslySetInnerHTML={{ __html: diffHtml }} />
        ) : (
          <div className="p-3 text-xs text-muted-foreground italic">No diff content</div>
        )}
      </div>

      {/* Truncation warning */}
      {truncated && (
        <div className="px-3 py-2 border-t border-border/50 bg-amber-500/10 text-xs text-amber-700 dark:text-amber-400">
          Diff truncated.{' '}
          {safeRepoUrl && commitUrl ? (
            <a
              className="underline hover:no-underline"
              href={commitUrl}
              rel="noreferrer noopener"
              target="_blank"
            >
              View full diff on {getHostname(safeRepoUrl)}
            </a>
          ) : (
            'View the full diff at the source repository.'
          )}
        </div>
      )}
    </div>
  );
}
