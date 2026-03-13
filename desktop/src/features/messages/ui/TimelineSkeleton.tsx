import { Skeleton } from "@/shared/ui/skeleton";

export function TimelineSkeleton() {
  const skeletonRows = ["first", "second", "third", "fourth"];

  return (
    <>
      {skeletonRows.map((row) => (
        <div className="flex gap-3" key={row}>
          <Skeleton className="h-9 w-9 rounded-xl" />
          <div className="min-w-0 flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-44" />
            <Skeleton className="h-4 w-full max-w-2xl" />
            <Skeleton className="h-4 w-full max-w-xl" />
          </div>
        </div>
      ))}
    </>
  );
}
