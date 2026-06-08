import * as React from "react";

import {
  ensureConciergeSession,
  type ConciergeSession,
} from "@/features/concierge/lib/conciergeSession";

/**
 * Bootstrap the Concierge session (agent + persistent DM) once per mount.
 * Errors surface as a message + retry — mesh availability is the common
 * failure (no model being served), and it's user-fixable in Settings.
 */
export function useConciergeSession() {
  const [session, setSession] = React.useState<ConciergeSession | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [isLoading, setIsLoading] = React.useState(true);
  const inFlightRef = React.useRef(false);

  const bootstrap = React.useCallback(async () => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    setIsLoading(true);
    setError(null);
    try {
      setSession(await ensureConciergeSession());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
      inFlightRef.current = false;
    }
  }, []);

  React.useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  return { session, error, isLoading, retry: bootstrap };
}
