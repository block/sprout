import {
  ArrowLeft,
  BookMarked,
  Check,
  Copy,
  ExternalLink,
  Users,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useParams } from "@tanstack/react-router";
import { toast } from "sonner";

import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import { useRepo } from "../use-repos";
import { ConnectButton } from "./ConnectButton";
import { PubkeyAvatar } from "./PubkeyAvatar";

function relativeTime(unix: number): string {
  const now = Date.now();
  const diff = now - unix * 1000;
  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (days > 30) {
    const months = Math.floor(days / 30);
    return months === 1 ? "1 month ago" : `${months} months ago`;
  }
  if (days > 0) return days === 1 ? "1 day ago" : `${days} days ago`;
  if (hours > 0) return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
  if (minutes > 0)
    return minutes === 1 ? "1 minute ago" : `${minutes} minutes ago`;
  return "just now";
}

function CopyableUrl({ url }: { url: string }) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      toast.success("Copied to clipboard");
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error("Failed to copy to clipboard");
    }
  }

  return (
    <div className="flex items-center gap-2 rounded-md border border-input bg-muted/50 px-3 py-2">
      <code className="min-w-0 flex-1 truncate text-sm">{url}</code>
      <button
        type="button"
        onClick={handleCopy}
        className="shrink-0 text-muted-foreground hover:text-foreground"
        aria-label="Copy clone URL"
      >
        {copied ? (
          <Check className="h-4 w-4 text-green-500" />
        ) : (
          <Copy className="h-4 w-4" />
        )}
      </button>
    </div>
  );
}

function DetailSkeleton() {
  return (
    <div className="mx-auto w-full max-w-3xl px-4 py-8">
      <div className="h-5 w-24 animate-pulse rounded bg-muted" />
      <div className="mt-6 h-8 w-64 animate-pulse rounded bg-muted" />
      <div className="mt-3 h-5 w-96 animate-pulse rounded bg-muted" />
      <div className="mt-8 space-y-3">
        <div className="h-4 w-32 animate-pulse rounded bg-muted" />
        <div className="h-10 w-full animate-pulse rounded bg-muted" />
      </div>
    </div>
  );
}

export function RepoDetailPage() {
  const { repoId } = useParams({ from: "/repos/$repoId" });
  const { data: repo, isLoading, error } = useRepo(repoId);

  useEffect(() => {
    if (error) {
      toast.error("Failed to load repository", {
        description: error.message,
      });
    }
  }, [error]);

  if (isLoading) return <DetailSkeleton />;

  if (!repo) {
    return (
      <div className="mx-auto w-full max-w-3xl px-4 py-8">
        <Link
          to="/"
          className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to repositories
        </Link>
        <div className="mt-12 text-center">
          <BookMarked className="mx-auto h-10 w-10 text-muted-foreground" />
          <h1 className="mt-4 text-xl font-semibold">Repository not found</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            This repository may have been removed or doesn't exist on this
            relay.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-3xl px-4 py-8">
      {/* Back link */}
      <Link
        to="/"
        className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
      >
        <ArrowLeft className="h-4 w-4" />
        Back to repositories
      </Link>

      {/* Header */}
      <div className="mt-6">
        <div className="flex items-center gap-3">
          <BookMarked className="h-6 w-6 shrink-0 text-muted-foreground" />
          <h1 className="text-2xl font-semibold tracking-tight">{repo.name}</h1>
          <Badge variant="outline">Public</Badge>
        </div>
        {repo.description && (
          <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
            {repo.description}
          </p>
        )}
        <p className="mt-2 text-xs text-muted-foreground">
          Updated {relativeTime(repo.createdAt)}
        </p>
      </div>

      {/* Clone URLs */}
      {repo.cloneUrls.length > 0 && (
        <div className="mt-8">
          <h2 className="mb-3 text-sm font-semibold">Clone</h2>
          <div className="space-y-2">
            {repo.cloneUrls.map((url) => (
              <CopyableUrl key={url} url={url} />
            ))}
          </div>
        </div>
      )}

      {/* External link */}
      {repo.webUrl && (
        <div className="mt-6">
          <Button variant="outline" asChild>
            <a href={repo.webUrl} target="_blank" rel="noopener noreferrer">
              <ExternalLink className="h-4 w-4" />
              View on web
            </a>
          </Button>
        </div>
      )}

      {/* Owner & Contributors */}
      <div className="mt-8">
        <h2 className="mb-3 flex items-center gap-2 text-sm font-semibold">
          <Users className="h-4 w-4" />
          People
        </h2>
        <div className="flex flex-wrap gap-2">
          <PubkeyAvatar pubkey={repo.owner} />
          {repo.contributors
            .filter((c) => c !== repo.owner)
            .map((c) => (
              <PubkeyAvatar key={c} pubkey={c} />
            ))}
        </div>
      </div>

      {/* Open in Sprout CTA */}
      <div className="mt-8 rounded-lg border border-border bg-muted/30 p-6 text-center">
        <p className="mb-3 text-sm text-muted-foreground">
          Open this relay in the Sprout desktop app to push code and
          collaborate.
        </p>
        <ConnectButton />
      </div>
    </div>
  );
}
