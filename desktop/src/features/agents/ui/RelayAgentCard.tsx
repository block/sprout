import type { RelayAgent } from "@/shared/api/types";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import { truncatePubkey } from "./agentUi";

export function RelayAgentCard({
  agent,
  isManagedLocally,
}: {
  agent: RelayAgent;
  isManagedLocally: boolean;
}) {
  const visibleCapabilities = agent.capabilities.slice(0, 4);
  const hiddenCapabilityCount =
    agent.capabilities.length - visibleCapabilities.length;

  return (
    <article className="rounded-3xl border border-border/70 bg-card/80 p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-sm font-semibold tracking-tight">
              {agent.name}
            </h3>
            {isManagedLocally ? (
              <span className="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.18em] text-primary-foreground">
                Local
              </span>
            ) : null}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {truncatePubkey(agent.pubkey)}
            {agent.agentType ? ` · ${agent.agentType}` : ""}
          </p>
        </div>
        <PresenceBadge status={agent.status} />
      </div>

      <div className="mt-4 flex flex-wrap gap-2">
        {visibleCapabilities.map((capability) => (
          <span
            className="rounded-full border border-border/70 bg-background/70 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground"
            key={capability}
          >
            {capability}
          </span>
        ))}
        {hiddenCapabilityCount > 0 ? (
          <span className="rounded-full border border-border/70 bg-background/70 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
            +{hiddenCapabilityCount}
          </span>
        ) : null}
      </div>

      <p className="mt-4 text-xs text-muted-foreground">
        {agent.channels.length > 0
          ? `Visible in ${agent.channels.join(", ")}`
          : "No visible channel memberships yet."}
      </p>
    </article>
  );
}
