import { Skeleton } from "@/shared/ui/skeleton";

type ViewLoadingFallbackKind = "agents" | "channel" | "forum" | "workflows";

type ViewLoadingFallbackProps = {
  includeHeader?: boolean;
  kind: ViewLoadingFallbackKind;
};

function LoadingHeaderSkeleton() {
  return (
    <header
      className="flex min-w-0 items-center gap-3 border-b border-border/80 bg-background px-4 pb-3 pt-8 sm:px-6"
      data-tauri-drag-region
    >
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <Skeleton className="h-5 w-5 rounded-sm" />
          <Skeleton className="h-6 w-36 max-w-[50vw]" />
        </div>
        <Skeleton className="mt-2 h-4 w-full max-w-2xl" />
      </div>
      <div className="hidden shrink-0 items-center gap-2 sm:flex">
        <Skeleton className="h-8 w-8 rounded-lg" />
        <Skeleton className="h-8 w-8 rounded-lg" />
      </div>
    </header>
  );
}

function MessageRowsSkeleton() {
  return (
    <>
      {["first", "second", "third", "fourth", "fifth"].map((row) => (
        <div className="flex gap-3" key={row}>
          <Skeleton className="h-8 w-8 shrink-0 rounded-lg" />
          <div className="min-w-0 flex-1 space-y-2 pt-0.5">
            <Skeleton className="h-3.5 w-40 max-w-[40%]" />
            <Skeleton className="h-4 w-full max-w-3xl" />
            <Skeleton className="h-4 w-full max-w-xl" />
          </div>
        </div>
      ))}
    </>
  );
}

function AgentsLoadingBody() {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto p-4">
      <div className="space-y-6">
        <section className="space-y-4">
          <div className="flex items-center justify-between gap-3">
            <div className="space-y-2">
              <Skeleton className="h-5 w-28" />
              <Skeleton className="h-4 w-72 max-w-full" />
            </div>
            <Skeleton className="h-8 w-8 rounded-lg" />
          </div>
          <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-4">
            {["first", "second", "third", "fourth"].map((card) => (
              <div
                className="rounded-xl border border-border/70 bg-card/80 p-3 shadow-sm"
                key={card}
              >
                <div className="flex items-center gap-2.5">
                  <Skeleton className="h-8 w-8 rounded-lg" />
                  <div className="min-w-0 flex-1 space-y-2">
                    <Skeleton className="h-4 w-24" />
                    <Skeleton className="h-3 w-16 rounded-full" />
                  </div>
                </div>
              </div>
            ))}
          </div>
        </section>

        <section className="space-y-4">
          <div className="space-y-2">
            <Skeleton className="h-5 w-24" />
            <Skeleton className="h-4 w-80 max-w-full" />
          </div>
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
            {["alpha", "beta", "gamma"].map((card) => (
              <div
                className="rounded-xl border border-border/70 bg-card/80 p-4 shadow-sm"
                key={card}
              >
                <div className="space-y-3">
                  <div className="flex items-center justify-between gap-3">
                    <div className="space-y-2">
                      <Skeleton className="h-4 w-28" />
                      <Skeleton className="h-3 w-20 rounded-full" />
                    </div>
                    <Skeleton className="h-7 w-16 rounded-full" />
                  </div>
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-4/5" />
                </div>
              </div>
            ))}
          </div>
        </section>

        <section className="space-y-4">
          <div className="space-y-2">
            <Skeleton className="h-5 w-36" />
            <Skeleton className="h-4 w-72 max-w-full" />
          </div>
          <div className="overflow-hidden rounded-xl border border-border/70 bg-card/80 shadow-sm">
            {["one", "two", "three"].map((row) => (
              <div
                className="flex items-center gap-4 border-b border-border/60 px-4 py-3 last:border-b-0"
                key={row}
              >
                <Skeleton className="h-4 w-28" />
                <Skeleton className="h-5 w-16 rounded-full" />
                <Skeleton className="h-4 w-24" />
                <Skeleton className="ml-auto h-8 w-24 rounded-lg" />
              </div>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}

function WorkflowsLoadingBody() {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto p-4">
        <div className="mb-4 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <Skeleton className="h-6 w-28" />
            <Skeleton className="h-8 w-8 rounded-lg" />
          </div>
          <Skeleton className="h-9 w-36 rounded-lg" />
        </div>

        <div className="space-y-2">
          {["first", "second", "third", "fourth"].map((card) => (
            <div
              className="rounded-xl border border-border/70 bg-card/80 p-4 shadow-sm"
              key={card}
            >
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0 flex-1 space-y-3">
                  <div className="flex items-center gap-2">
                    <Skeleton className="h-5 w-44" />
                    <Skeleton className="h-5 w-16 rounded-full" />
                  </div>
                  <Skeleton className="h-4 w-full max-w-2xl" />
                  <div className="flex flex-wrap gap-2">
                    <Skeleton className="h-5 w-20 rounded-full" />
                    <Skeleton className="h-5 w-24 rounded-full" />
                    <Skeleton className="h-5 w-16 rounded-full" />
                  </div>
                </div>
                <div className="hidden shrink-0 gap-2 sm:flex">
                  <Skeleton className="h-8 w-8 rounded-lg" />
                  <Skeleton className="h-8 w-8 rounded-lg" />
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="hidden w-[400px] shrink-0 border-l border-border/60 bg-background/70 lg:block">
        <div className="space-y-4 p-4">
          <div className="space-y-2">
            <Skeleton className="h-5 w-40" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-4/5" />
          </div>
          <Skeleton className="h-40 w-full rounded-xl" />
          <div className="space-y-2">
            <Skeleton className="h-4 w-28" />
            <Skeleton className="h-16 w-full rounded-xl" />
            <Skeleton className="h-16 w-full rounded-xl" />
          </div>
        </div>
      </div>
    </div>
  );
}

function ChannelLoadingBody() {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto px-4 py-3 sm:px-6">
        <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
          <MessageRowsSkeleton />
        </div>
      </div>

      <div className="border-t border-border/60 bg-background px-4 py-4 sm:px-6">
        <div className="mx-auto w-full max-w-4xl space-y-3">
          <Skeleton className="h-10 w-full rounded-2xl" />
          <div className="flex items-center gap-2">
            <Skeleton className="h-8 w-20 rounded-lg" />
            <Skeleton className="h-8 w-16 rounded-lg" />
            <Skeleton className="ml-auto h-8 w-24 rounded-lg" />
          </div>
        </div>
      </div>
    </div>
  );
}

function ForumLoadingBody() {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div className="border-b border-border/60 p-4">
        <Skeleton className="h-10 w-full rounded-xl" />
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        <div className="space-y-3">
          {["first", "second", "third"].map((card) => (
            <div
              className="rounded-xl border border-border/70 bg-card/80 p-4 shadow-sm"
              key={card}
            >
              <div className="space-y-3">
                <div className="flex items-center gap-2">
                  <Skeleton className="h-4 w-40" />
                  <Skeleton className="h-5 w-16 rounded-full" />
                </div>
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-5/6" />
                <div className="flex items-center gap-3">
                  <Skeleton className="h-4 w-20" />
                  <Skeleton className="h-4 w-16" />
                  <Skeleton className="h-4 w-14" />
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export function ViewLoadingFallback({
  includeHeader = false,
  kind,
}: ViewLoadingFallbackProps) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {includeHeader ? <LoadingHeaderSkeleton /> : null}
      {kind === "agents" ? <AgentsLoadingBody /> : null}
      {kind === "workflows" ? <WorkflowsLoadingBody /> : null}
      {kind === "channel" ? <ChannelLoadingBody /> : null}
      {kind === "forum" ? <ForumLoadingBody /> : null}
    </div>
  );
}
