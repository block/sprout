import * as React from "react";
import { toast } from "sonner";

import {
  ensureConciergeSession,
  type ConciergeSession,
} from "@/features/concierge/lib/conciergeSession";
import { useIdentityQuery } from "@/shared/api/hooks";

/**
 * Bootstrap the Concierge session (selected agent + persistent DM) once the
 * identity is known. Errors surface as a message + retry — mesh availability
 * is the common failure (no model being served), and it's user-fixable in
 * Settings. A stale selection (chosen agent was deleted) surfaces a toast
 * before falling back to the default Concierge.
 */
export function useConciergeSession() {
  const identityQuery = useIdentityQuery();
  const selfPubkey = identityQuery.data?.pubkey ?? null;
  const [session, setSession] = React.useState<ConciergeSession | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [isLoading, setIsLoading] = React.useState(true);
  const inFlightRef = React.useRef(false);

  const bootstrap = React.useCallback(async () => {
    if (!selfPubkey || inFlightRef.current) return;
    inFlightRef.current = true;
    setIsLoading(true);
    setError(null);
    try {
      const next = await ensureConciergeSession(selfPubkey);
      if (next.staleSelection) {
        toast.info(
          "Your chosen Concierge agent no longer exists — using the default.",
        );
      }
      setSession(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
      inFlightRef.current = false;
    }
  }, [selfPubkey]);

  React.useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  return { session, error, isLoading, retry: bootstrap };
}
