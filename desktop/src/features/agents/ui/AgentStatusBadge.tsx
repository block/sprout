import * as React from "react";

import { Badge } from "@/shared/ui/badge";
import type { ManagedAgent, PresenceStatus } from "@/shared/api/types";

/** Grace period after mount before treating "running + no presence" as "Starting…" */
const PRESENCE_GRACE_MS = 15_000;

export function AgentStatusBadge({
  presenceLoaded,
  presenceStatus,
  status,
}: {
  presenceLoaded: boolean;
  presenceStatus: PresenceStatus | undefined;
  status: ManagedAgent["status"];
}) {
  const [inGracePeriod, setInGracePeriod] = React.useState(true);

  React.useEffect(() => {
    const timer = setTimeout(() => setInGracePeriod(false), PRESENCE_GRACE_MS);
    return () => clearTimeout(timer);
  }, []);

  const isActive = status === "running" || status === "deployed";
  const isStarting =
    !inGracePeriod &&
    presenceLoaded &&
    status === "running" &&
    (!presenceStatus || presenceStatus === "offline");

  const variant = isStarting ? "warning" : isActive ? "default" : "secondary";

  return (
    <Badge variant={variant}>
      {isStarting ? "Starting\u2026" : status.replace(/_/g, " ")}
    </Badge>
  );
}
