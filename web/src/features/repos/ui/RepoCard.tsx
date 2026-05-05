import { Check, Copy, ExternalLink, GitBranch } from "lucide-react";
import { useCallback, useState } from "react";
import { toast } from "sonner";

import { relayWsUrl } from "@/shared/lib/relay-url";
import { Button } from "@/shared/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/shared/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import type { Repo } from "../use-repos";

function truncateHex(hex: string): string {
  if (hex.length <= 12) return hex;
  return `${hex.slice(0, 8)}...${hex.slice(-4)}`;
}

function formatDate(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function CopyButton({ value, label }: { value: string; label: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(value).then(
      () => {
        setCopied(true);
        toast.success("Copied to clipboard");
        setTimeout(() => setCopied(false), 2000);
      },
      () => {
        toast.error("Failed to copy to clipboard");
      },
    );
  }, [value]);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0"
          onClick={handleCopy}
          aria-label={label}
        >
          {copied ? (
            <Check className="h-3.5 w-3.5 text-green-500" />
          ) : (
            <Copy className="h-3.5 w-3.5" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>Copy</TooltipContent>
    </Tooltip>
  );
}

export function RepoCard({ repo }: { repo: Repo }) {
  const relayUrl = relayWsUrl();
  const deepLink = `sprout://connect?relay=${encodeURIComponent(relayUrl)}`;

  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-3">
        <div className="flex items-start gap-2">
          <GitBranch className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0 flex-1">
            <CardTitle className="text-base">{repo.name}</CardTitle>
            {repo.description && (
              <CardDescription className="mt-1 line-clamp-2">
                {repo.description}
              </CardDescription>
            )}
          </div>
        </div>
      </CardHeader>

      <CardContent className="flex-1 space-y-3 text-sm">
        {repo.cloneUrls.length > 0 && (
          <div className="space-y-1.5">
            <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
              Clone
            </span>
            {repo.cloneUrls.map((url) => (
              <div key={url} className="flex items-center gap-1">
                <code className="min-w-0 flex-1 truncate rounded bg-muted px-2 py-1 font-mono text-xs text-muted-foreground">
                  {url}
                </code>
                <CopyButton value={url} label={`Copy clone URL: ${url}`} />
              </div>
            ))}
          </div>
        )}

        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="cursor-default font-mono">
                {truncateHex(repo.owner)}
              </span>
            </TooltipTrigger>
            <TooltipContent>{repo.owner}</TooltipContent>
          </Tooltip>
          <span>{formatDate(repo.createdAt)}</span>
        </div>
      </CardContent>

      <CardFooter className="gap-2 border-t pt-4">
        <Button asChild size="sm" className="flex-1">
          <a href={deepLink} aria-label={`Open ${repo.name} in Sprout`}>
            <ExternalLink className="h-3.5 w-3.5" />
            Open in Sprout
          </a>
        </Button>
        <CopyButton value={relayUrl} label="Copy relay URL" />
      </CardFooter>
    </Card>
  );
}
