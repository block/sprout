import type { RelayEvent } from "@/shared/api/types";

export type RelaySubscriptionFilter = {
  kinds: number[];
  limit: number;
  since?: number;
  until?: number;
} & Partial<Record<`#${string}`, string[]>>;

type HistorySubscription = {
  mode: "history";
  events: RelayEvent[];
  resolve: (events: RelayEvent[]) => void;
  reject: (error: Error) => void;
  timeout: number;
};

type LiveSubscription = {
  mode: "live";
  filter: RelaySubscriptionFilter;
  onEvent: (event: RelayEvent) => void;
  resolveReady?: () => void;
  lastSeenCreatedAt?: number;
};

export type PendingEvent = {
  event: RelayEvent;
  resolve: (event: RelayEvent) => void;
  reject: (error: Error) => void;
  timeout: number;
};

export type RelaySubscription = HistorySubscription | LiveSubscription;

export function sortEvents(events: RelayEvent[]) {
  return [...events].sort((left, right) => left.created_at - right.created_at);
}

export function getTextPayload(message: unknown) {
  if (typeof message === "string") {
    return message;
  }

  if (
    typeof message === "object" &&
    message !== null &&
    "type" in message &&
    message.type === "Text" &&
    "data" in message &&
    typeof message.data === "string"
  ) {
    return message.data;
  }

  if (
    typeof message === "object" &&
    message !== null &&
    "Text" in message &&
    typeof message.Text === "string"
  ) {
    return message.Text;
  }

  return null;
}
