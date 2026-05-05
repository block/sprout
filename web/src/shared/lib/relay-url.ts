/** Convert a WebSocket relay URL to its HTTP equivalent. */
export function relayHttpUrl(wsUrl: string): string {
  if (wsUrl.startsWith("wss://")) {
    return `https://${wsUrl.slice(6)}`;
  }
  if (wsUrl.startsWith("ws://")) {
    return `http://${wsUrl.slice(5)}`;
  }
  return wsUrl;
}

/** Read the relay WebSocket URL from environment or fall back to localhost. */
export function relayWsUrl(): string {
  return import.meta.env.VITE_RELAY_URL || "ws://localhost:3000";
}

/** HTTP base URL for the relay (derived from the WS URL). */
export function relayHttpBaseUrl(): string {
  return relayHttpUrl(relayWsUrl());
}
