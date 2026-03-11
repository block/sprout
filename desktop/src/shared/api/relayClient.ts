import { Channel, invoke } from "@tauri-apps/api/core";

import {
  createAuthEvent,
  getRelayWsUrl,
  signRelayEvent,
} from "@/shared/api/tauri";
import type { PresenceStatus, RelayEvent } from "@/shared/api/types";
import { STREAM_MESSAGE_KINDS } from "@/shared/constants/kinds";

type RelaySubscriptionFilter = {
  kinds: number[];
  "#h": string[];
  limit: number;
};

type HistorySubscription = {
  mode: "history";
  events: RelayEvent[];
  resolve: (events: RelayEvent[]) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

type LiveSubscription = {
  mode: "live";
  onEvent: (event: RelayEvent) => void;
};

type PendingEvent = {
  event: RelayEvent;
  resolve: (event: RelayEvent) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

type RelaySubscription = HistorySubscription | LiveSubscription;

function sortEvents(events: RelayEvent[]) {
  return [...events].sort((left, right) => left.created_at - right.created_at);
}

function getTextPayload(message: unknown) {
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

class RelayClient {
  private wsId: number | null = null;
  private relayUrl: string | null = null;
  private connectPromise: Promise<void> | null = null;
  private authRequest: {
    pendingEventId: string;
    resolve: () => void;
    reject: (error: Error) => void;
    timeout: ReturnType<typeof setTimeout>;
  } | null = null;
  private subscriptions = new Map<string, RelaySubscription>();
  private pendingEvents = new Map<string, PendingEvent>();

  async fetchChannelHistory(channelId: string, limit = 50) {
    await this.ensureConnected();

    return new Promise<RelayEvent[]>((resolve, reject) => {
      const subId = `history-${crypto.randomUUID()}`;
      const timeout = window.setTimeout(() => {
        this.subscriptions.delete(subId);
        void this.closeSubscription(subId);
        reject(new Error("Timed out while loading channel history."));
      }, 8_000);

      this.subscriptions.set(subId, {
        mode: "history",
        events: [],
        resolve,
        reject,
        timeout,
      });

      void this.sendRaw([
        "REQ",
        subId,
        this.buildChannelFilter(channelId, limit),
      ]).catch((error) => {
        window.clearTimeout(timeout);
        this.subscriptions.delete(subId);
        reject(
          error instanceof Error
            ? error
            : new Error("Failed to request channel history."),
        );
      });
    });
  }

  async sendMessage(channelId: string, content: string) {
    await this.ensureConnected();

    const event = await signRelayEvent({
      kind: 40001,
      content: content.trim(),
      // Channel-scoped events use the NIP-29 `h` tag.
      tags: [["h", channelId]],
    });

    return this.publishEvent(
      event,
      "Timed out while sending the message.",
      "Failed to send the message.",
    );
  }

  async sendPresence(status: PresenceStatus) {
    await this.ensureConnected();

    const event = await signRelayEvent({
      kind: 20001,
      content: status,
      tags: [],
    });

    return this.publishEvent(
      event,
      "Timed out while updating presence.",
      "Failed to update presence.",
    );
  }

  async subscribeToChannel(
    channelId: string,
    onEvent: (event: RelayEvent) => void,
  ) {
    await this.ensureConnected();

    const subId = `live-${crypto.randomUUID()}`;

    this.subscriptions.set(subId, {
      mode: "live",
      onEvent,
    });

    await this.sendRaw(["REQ", subId, this.buildChannelFilter(channelId, 50)]);

    return async () => {
      const active = this.subscriptions.get(subId);
      if (!active || active.mode !== "live") {
        return;
      }

      this.subscriptions.delete(subId);
      await this.closeSubscription(subId);
    };
  }

  private async ensureConnected() {
    if (this.connectPromise) {
      return this.connectPromise;
    }

    if (this.wsId !== null) {
      return;
    }

    this.connectPromise = this.connect();

    try {
      await this.connectPromise;
    } finally {
      this.connectPromise = null;
    }
  }

  private async connect() {
    if (!this.relayUrl) {
      this.relayUrl = await getRelayWsUrl();
    }

    const onMessageChannel = new Channel<unknown>((message) => {
      void this.handleWsMessage(message);
    });

    this.wsId = await invoke<number>("plugin:websocket|connect", {
      url: this.relayUrl,
      onMessage: onMessageChannel,
      config: {},
    });

    await new Promise<void>((resolve, reject) => {
      const timeout = window.setTimeout(() => {
        this.authRequest = null;
        this.resetConnection(
          new Error("Timed out while waiting for relay authentication."),
        );
        reject(new Error("Timed out while waiting for relay authentication."));
      }, 8_000);

      this.authRequest = {
        pendingEventId: "",
        resolve,
        reject,
        timeout,
      };
    });
  }

  private buildChannelFilter(
    channelId: string,
    limit: number,
  ): RelaySubscriptionFilter {
    return {
      kinds: [...STREAM_MESSAGE_KINDS],
      "#h": [channelId],
      limit,
    };
  }

  private async sendRaw(payload: unknown[]) {
    if (this.wsId === null) {
      throw new Error("Relay socket is not connected.");
    }

    await invoke("plugin:websocket|send", {
      id: this.wsId,
      message: {
        type: "Text",
        data: JSON.stringify(payload),
      },
    });
  }

  private async closeSubscription(subId: string) {
    if (this.wsId === null) {
      return;
    }

    await this.sendRaw(["CLOSE", subId]);
  }

  private publishEvent(
    event: RelayEvent,
    timeoutMessage: string,
    sendErrorMessage: string,
  ) {
    return new Promise<RelayEvent>((resolve, reject) => {
      const timeout = window.setTimeout(() => {
        this.pendingEvents.delete(event.id);
        reject(new Error(timeoutMessage));
      }, 8_000);

      this.pendingEvents.set(event.id, {
        event,
        resolve,
        reject,
        timeout,
      });

      void this.sendRaw(["EVENT", event]).catch((error) => {
        window.clearTimeout(timeout);
        this.pendingEvents.delete(event.id);
        reject(error instanceof Error ? error : new Error(sendErrorMessage));
      });
    });
  }

  private async handleWsMessage(message: unknown) {
    if (
      typeof message === "object" &&
      message !== null &&
      "type" in message &&
      message.type === "Close"
    ) {
      this.resetConnection(new Error("Relay connection closed."));
      return;
    }

    if (
      typeof message === "object" &&
      message !== null &&
      "type" in message &&
      message.type === "Error"
    ) {
      this.resetConnection(new Error("Relay connection errored."));
      return;
    }

    const payload = getTextPayload(message);
    if (!payload) {
      return;
    }

    let data: unknown;
    try {
      data = JSON.parse(payload);
    } catch {
      return;
    }

    if (!Array.isArray(data) || data.length === 0) {
      return;
    }

    const [type, ...rest] = data;
    if (type === "AUTH" && typeof rest[0] === "string") {
      await this.handleAuthChallenge(rest[0]);
      return;
    }

    if (type === "EVENT" && typeof rest[0] === "string" && rest[1]) {
      this.handleEvent(rest[0], rest[1] as RelayEvent);
      return;
    }

    if (
      type === "OK" &&
      typeof rest[0] === "string" &&
      typeof rest[1] === "boolean"
    ) {
      this.handleOk(
        rest[0],
        rest[1],
        typeof rest[2] === "string" ? rest[2] : "",
      );
      return;
    }

    if (type === "EOSE" && typeof rest[0] === "string") {
      this.handleEose(rest[0]);
    }
  }

  private async handleAuthChallenge(challenge: string) {
    if (!this.relayUrl) {
      this.relayUrl = await getRelayWsUrl();
    }

    const event = await createAuthEvent({
      challenge,
      relayUrl: this.relayUrl,
    });

    if (!this.authRequest) {
      return;
    }

    this.authRequest.pendingEventId = event.id;
    await this.sendRaw(["AUTH", event]);
  }

  private handleEvent(subId: string, event: RelayEvent) {
    const subscription = this.subscriptions.get(subId);
    if (!subscription) {
      return;
    }

    if (subscription.mode === "history") {
      subscription.events.push(event);
      return;
    }

    subscription.onEvent(event);
  }

  private handleEose(subId: string) {
    const subscription = this.subscriptions.get(subId);
    if (!subscription || subscription.mode !== "history") {
      return;
    }

    window.clearTimeout(subscription.timeout);
    this.subscriptions.delete(subId);
    void this.closeSubscription(subId);
    subscription.resolve(sortEvents(subscription.events));
  }

  private handleOk(eventId: string, success: boolean, message: string) {
    if (this.authRequest && this.authRequest.pendingEventId === eventId) {
      window.clearTimeout(this.authRequest.timeout);
      const authRequest = this.authRequest;
      this.authRequest = null;

      if (success) {
        authRequest.resolve();
      } else {
        const error = new Error(message || "Relay authentication rejected.");
        authRequest.reject(error);
        this.resetConnection(error);
      }

      return;
    }

    const pendingEvent = this.pendingEvents.get(eventId);
    if (!pendingEvent) {
      return;
    }

    window.clearTimeout(pendingEvent.timeout);
    this.pendingEvents.delete(eventId);

    if (success) {
      pendingEvent.resolve(pendingEvent.event);
    } else {
      pendingEvent.reject(new Error(message || "Relay rejected the event."));
    }
  }

  private resetConnection(error: Error) {
    if (this.wsId !== null) {
      void invoke("plugin:websocket|disconnect", { id: this.wsId }).catch(
        () => {
          return;
        },
      );
    }

    this.wsId = null;

    if (this.authRequest) {
      window.clearTimeout(this.authRequest.timeout);
      this.authRequest.reject(error);
      this.authRequest = null;
    }

    for (const [subId, subscription] of this.subscriptions) {
      if (subscription.mode === "history") {
        window.clearTimeout(subscription.timeout);
        subscription.reject(error);
      }
      this.subscriptions.delete(subId);
    }

    for (const [eventId, pendingEvent] of this.pendingEvents) {
      window.clearTimeout(pendingEvent.timeout);
      pendingEvent.reject(error);
      this.pendingEvents.delete(eventId);
    }
  }
}

export const relayClient = new RelayClient();
