import { GitBranch } from "lucide-react";
import { toast } from "sonner";
import { useEffect } from "react";

import { Card, CardContent, CardHeader } from "@/shared/ui/card";
import { useRepos } from "../use-repos";
import { RepoCard } from "./RepoCard";

function RepoCardSkeleton() {
  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-3">
        <div className="flex items-start gap-2">
          <div className="mt-0.5 h-4 w-4 shrink-0 animate-pulse rounded bg-muted" />
          <div className="min-w-0 flex-1 space-y-2">
            <div className="h-5 w-2/3 animate-pulse rounded bg-muted" />
            <div className="h-4 w-full animate-pulse rounded bg-muted" />
          </div>
        </div>
      </CardHeader>
      <CardContent className="flex-1 space-y-3">
        <div className="space-y-1.5">
          <div className="h-3 w-12 animate-pulse rounded bg-muted" />
          <div className="h-7 w-full animate-pulse rounded bg-muted" />
        </div>
        <div className="flex items-center justify-between">
          <div className="h-3 w-20 animate-pulse rounded bg-muted" />
          <div className="h-3 w-16 animate-pulse rounded bg-muted" />
        </div>
      </CardContent>
      <div className="flex gap-2 border-t p-6 pt-4">
        <div className="h-8 flex-1 animate-pulse rounded-md bg-muted" />
        <div className="h-8 w-8 animate-pulse rounded-md bg-muted" />
      </div>
    </Card>
  );
}

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-full bg-muted">
        <GitBranch className="h-7 w-7 text-muted-foreground" />
      </div>
      <h2 className="mt-4 text-lg font-semibold">No repositories yet</h2>
      <p className="mt-1 max-w-sm text-sm text-muted-foreground">
        Repositories published to this relay will appear here. Push a git repo
        using the Sprout desktop app to get started.
      </p>
    </div>
  );
}

export function ReposPage() {
  const { data: repos, isLoading, error } = useRepos();

  useEffect(() => {
    if (error) {
      toast.error("Failed to load repositories", {
        description: error.message,
      });
    }
  }, [error]);

  if (isLoading) {
    return (
      <div className="mx-auto w-full max-w-6xl px-4 py-8">
        <h1 className="mb-6 text-2xl font-semibold tracking-tight">
          Repositories
        </h1>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {["a", "b", "c", "d", "e", "f"].map((key) => (
            <RepoCardSkeleton key={key} />
          ))}
        </div>
      </div>
    );
  }

  if (!repos || repos.length === 0) {
    return (
      <div className="mx-auto w-full max-w-6xl px-4 py-8">
        <h1 className="mb-6 text-2xl font-semibold tracking-tight">
          Repositories
        </h1>
        <EmptyState />
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8">
      <h1 className="mb-6 text-2xl font-semibold tracking-tight">
        Repositories
      </h1>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {repos.map((repo) => (
          <RepoCard key={repo.id} repo={repo} />
        ))}
      </div>
    </div>
  );
}
