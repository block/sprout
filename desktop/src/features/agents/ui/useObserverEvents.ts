import * as React from "react";

import type { ConnectionState, ObserverEvent } from "./agentSessionTypes";

const MAX_OBSERVER_EVENTS = 800;

export function useObserverEvents(
  observerUrl: string | null,
  enabled: boolean,
) {
  const [events, setEvents] = React.useState<ObserverEvent[]>([]);
  const [connectionState, setConnectionState] =
    React.useState<ConnectionState>("idle");
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);

  React.useEffect(() => {
    setEvents([]);
    setErrorMessage(null);

    if (!observerUrl || !enabled) {
      setConnectionState("idle");
      return;
    }

    setConnectionState("connecting");
    const source = new EventSource(observerUrl);

    source.onopen = () => {
      setConnectionState("open");
      setErrorMessage(null);
    };

    source.onmessage = (event) => {
      try {
        const parsed = JSON.parse(event.data) as ObserverEvent;
        setEvents((current) => {
          if (current.some((existing) => existing.seq === parsed.seq)) {
            return current;
          }
          const next = [...current, parsed];
          return next.length > MAX_OBSERVER_EVENTS
            ? next.slice(next.length - MAX_OBSERVER_EVENTS)
            : next;
        });
      } catch (error) {
        setErrorMessage(
          error instanceof Error
            ? `Observer event parse failed: ${error.message}`
            : "Observer event parse failed.",
        );
      }
    };

    source.onerror = () => {
      setConnectionState((current) =>
        current === "open" ? "closed" : "error",
      );
      setErrorMessage("Observer stream is not available right now.");
    };

    return () => {
      source.close();
      setConnectionState("closed");
    };
  }, [enabled, observerUrl]);

  return { connectionState, errorMessage, events };
}
