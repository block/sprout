import type { RelayAgent } from "@/shared/api/types";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import { Skeleton } from "@/shared/ui/skeleton";
import { truncatePubkey } from "./agentUi";

export function RelayDirectorySection({
  error,
  isLoading,
  managedPubkeys,
  relayAgents,
}: {
  error: Error | null;
  isLoading: boolean;
  managedPubkeys: Set<string>;
  relayAgents: RelayAgent[];
}) {
  const sortedAgents = [...relayAgents].sort((left, right) => {
    const leftManaged = managedPubkeys.has(left.pubkey);
    const rightManaged = managedPubkeys.has(right.pubkey);

    if (leftManaged !== rightManaged) {
      return leftManaged ? -1 : 1;
    }

    return left.name.localeCompare(right.name);
  });

  return (
    <section className="space-y-4">
      <div>
        <h3 className="text-sm font-semibold tracking-tight">
          Relay directory
        </h3>
        <p className="text-sm text-muted-foreground">
          Bot and agent identities visible to the current desktop user.
        </p>
      </div>

      {isLoading ? (
        <div className="overflow-hidden rounded-3xl border border-border/70 bg-card/80 shadow-sm">
          <div className="grid gap-0">
            {["directory-1", "directory-2", "directory-3"].map((key) => (
              <div
                className="grid grid-cols-[minmax(0,2fr)_auto_minmax(0,1fr)_minmax(0,1.6fr)_auto] items-center gap-4 border-b border-border/60 px-4 py-3 last:border-b-0"
                key={key}
              >
                <div className="min-w-0 space-y-2">
                  <Skeleton className="h-4 w-28" />
                  <Skeleton className="h-3 w-24" />
                </div>
                <Skeleton className="h-6 w-16 rounded-full" />
                <Skeleton className="h-4 w-16" />
                <Skeleton className="h-4 w-32" />
                <Skeleton className="h-5 w-12 rounded-full" />
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {!isLoading && relayAgents.length === 0 ? (
        <div className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center">
          <p className="text-sm font-semibold tracking-tight">
            No relay-visible agents yet
          </p>
          <p className="mt-2 text-sm text-muted-foreground">
            Start one of your local harnesses or join an existing bot to a
            channel and it will appear here.
          </p>
        </div>
      ) : null}

      {!isLoading && relayAgents.length > 0 ? (
        <div className="overflow-hidden rounded-3xl border border-border/70 bg-card/80 shadow-sm">
          <div className="overflow-x-auto">
            <table
              className="w-full border-collapse text-left text-sm"
              data-testid="relay-directory-table"
            >
              <thead className="bg-muted/35 text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                <tr>
                  <th className="px-4 py-3">Agent</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Type</th>
                  <th className="px-4 py-3">Channels</th>
                  <th className="px-4 py-3">Source</th>
                </tr>
              </thead>
              <tbody>
                {sortedAgents.map((agent) => {
                  const isManagedLocally = managedPubkeys.has(agent.pubkey);

                  return (
                    <tr
                      className="border-b border-border/60 last:border-b-0"
                      key={agent.pubkey}
                    >
                      <td className="min-w-[16rem] px-4 py-3 align-top">
                        <div className="min-w-0">
                          <p className="truncate font-medium text-foreground">
                            {agent.name}
                          </p>
                          <p className="mt-1 text-xs text-muted-foreground">
                            {truncatePubkey(agent.pubkey)}
                          </p>
                        </div>
                      </td>
                      <td className="px-4 py-3 align-top">
                        <PresenceBadge
                          className="px-2.5 py-0.5 text-[11px]"
                          status={agent.status}
                        />
                      </td>
                      <td className="px-4 py-3 align-top text-muted-foreground">
                        {agent.agentType || "Unknown"}
                      </td>
                      <td className="max-w-[20rem] px-4 py-3 align-top text-muted-foreground">
                        <span className="block truncate">
                          {agent.channels.length > 0
                            ? agent.channels.join(", ")
                            : "No visible channel memberships"}
                        </span>
                      </td>
                      <td className="px-4 py-3 align-top">
                        <span className="inline-flex rounded-full border border-border/70 bg-background/70 px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                          {isManagedLocally ? "Local" : "Relay"}
                        </span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}

      {error ? (
        <p className="rounded-2xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error.message}
        </p>
      ) : null}
    </section>
  );
}
