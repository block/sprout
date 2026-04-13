import { Channel, invoke } from "@tauri-apps/api/core";

import {
  createAuthEvent,
  getRelayWsUrl,
  signRelayEvent,
} from "@/shared/api/tauri";
import type { PresenceStatus, RelayEvent } from "@/shared/api/types";
import {
  CHANNEL_EVENT_KINDS,
  HOME_MENTION_EVENT_KINDS,
  KIND_STREAM_MESSAGE,
  KIND_TYPING_INDICATOR,
} from "@/shared/constants/kinds";
import {
  getTextPayload,
  sortEvents,
  type PendingEvent,
  type RelaySubscription,
  type RelaySubscriptionFilter,
} from "@/shared/api/relayClientShared";

const RECONNECT_BASE_DELAY_MS = 1_000;
const RECONNECT_MAX_DELAY_MS = 30_000;
const RECONNECT_REPLAY_SKEW_SECS = 5;
const EVENT_BATCH_MS = 16;

export class RelayClient {
  private wsId: number | null = null;
  private relayUrl: string | null = null;
  private connectPromise: Promise<void> | null = null;
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  private reconnectDelayMs = RECONNECT_BASE_DELAY_MS;
  private keepAliveRequested = false;
  private authRequest: {
    pendingEventId: string;
    resolve: () => void;
    reject: (error: Error) => void;
    timeout: ReturnType<typeof setTimeout>;
  } | null = null;
  private subscriptions = new Map<string, RelaySubscription>();
  private pendingEvents = new Map<string, PendingEvent>();
  private eventBuffer: Array<{ subId: string; event: RelayEvent }> = [];
  private flushTimeout: ReturnType<typeof setTimeout> | null = null;
  private reconnectListeners = new Set<() => void>();
  private hasConnectedOnce = false;
  private notifyReconnectListeners = false;
  private onMessageChannel: Channel<unknown> | null = null;

  async fetchChannelHistory(channelId: string, limit = 50) {
    return this.fetchHistory(this.buildChannelFilter(channelId, limit));
  }

  async fetchChannelHistoryBefore(
    channelId: string,
    before: number,
    limit = 50,
  ) {
    return this.fetchHistory(this.buildChannelFilter(channelId, limit, before));
  }

  private async fetchHistory(filter: RelaySubscriptionFilter) {
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

      void this.sendRaw(["REQ", subId, filter]).catch((error) => {
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

  async sendMessage(
    channelId: string,
    content: string,
    mentionPubkeys: string[] = [],
    extraTags: string[][] = [],
  ) {
    await this.ensureConnected();

    const tags: string[][] = [["h", channelId]];
    for (const pubkey of mentionPubkeys) {
      tags.push(["p", pubkey]);
    }
    for (const tag of extraTags) {
      tags.push(tag);
    }

    const event = await signRelayEvent({
      kind: KIND_STREAM_MESSAGE,
      content: content.trim(),
      tags,
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

  /**
   * @param replyParentEventId Optional NIP-10 reply target — same id you pass as `parentEventId`
   *   when sending a message. Enables scoped “is typing” in thread UIs.
   */
  async sendTypingIndicator(
    channelId: string,
    replyParentEventId?: string | null,
  ) {
    // Bail when disconnected — not worth triggering a reconnect for ephemeral typing events.
    if (this.wsId === null) {
      return;
    }
    const tags: string[][] = [["h", channelId]];
    if (replyParentEventId) {
      tags.push(["e", replyParentEventId, "", "reply"]);
    }
    const event = await signRelayEvent({
      kind: KIND_TYPING_INDICATOR,
      content: "",
      tags,
    });

    // Fire-and-forget: no need to wait for relay acknowledgement.
    void this.sendRaw(["EVENT", event]).catch(() => {});
  }

  async subscribeToChannel(
    channelId: string,
    onEvent: (event: RelayEvent) => void,
  ) {
    return this.subscribe(this.buildChannelFilter(channelId, 50), onEvent);
  }

  async subscribeToTypingIndicators(
    channelId: string,
    onEvent: (event: RelayEvent) => void,
  ) {
    return this.subscribe(
      {
        kinds: [KIND_TYPING_INDICATOR],
        "#h": [channelId],
        limit: 10,
      },
      onEvent,
    );
  }

  async subscribeToAllStreamMessages(onEvent: (event: RelayEvent) => void) {
    return this.subscribe(this.buildGlobalStreamFilter(50), onEvent);
  }

  async subscribeToChannelMentionEvents(
    channelId: string,
    pubkey: string,
    onEvent: (event: RelayEvent) => void,
  ) {
    return this.subscribe(
      this.buildChannelMentionFilter(channelId, pubkey, 50),
      onEvent,
    );
  }

  async preconnect() {
    this.keepAliveRequested = true;
    await this.ensureConnected();
  }

  subscribeToReconnects(listener: () => void) {
    this.reconnectListeners.add(listener);

    return () => {
      this.reconnectListeners.delete(listener);
    };
  }

  private async ensureConnected() {
    if (this.connectPromise) {
      return this.connectPromise;
    }

    if (this.wsId !== null) {
      return;
    }

    if (this.reconnectTimeout) {
      window.clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

    const connectPromise = this.connect();
    this.connectPromise = connectPromise;

    try {
      await connectPromise;
    } finally {
      if (this.connectPromise === connectPromise) {
        this.connectPromise = null;
      }
    }
  }

  private async connect() {
    if (!this.relayUrl) {
      this.relayUrl = await getRelayWsUrl();
    }

    this.onMessageChannel = new Channel<unknown>((message) => {
      void this.handleWsMessage(message);
    });

    this.wsId = await invoke<number>("plugin:websocket|connect", {
      url: this.relayUrl,
      onMessage: this.onMessageChannel,
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

    this.reconnectDelayMs = RECONNECT_BASE_DELAY_MS;
    await this.replayLiveSubscriptions();
    this.emitReconnectIfNeeded();
  }

  private buildChannelFilter(
    channelId: string,
    limit: number,
    until?: number,
  ): RelaySubscriptionFilter {
    const filter: RelaySubscriptionFilter = {
      kinds: [...CHANNEL_EVENT_KINDS],
      "#h": [channelId],
      limit,
    };

    if (until !== undefined) {
      filter.until = until;
    }

    return filter;
  }

  private buildGlobalStreamFilter(limit: number): RelaySubscriptionFilter {
    return {
      kinds: [...CHANNEL_EVENT_KINDS],
      limit,
    };
  }

  private buildChannelMentionFilter(
    channelId: string,
    pubkey: string,
    limit: number,
  ): RelaySubscriptionFilter {
    return {
      kinds: [...HOME_MENTION_EVENT_KINDS],
      "#h": [channelId],
      "#p": [pubkey],
      limit,
      since: Math.floor(Date.now() / 1_000),
    };
  }

  private async subscribe(
    filter: RelaySubscriptionFilter,
    onEvent: (event: RelayEvent) => void,
  ) {
    await this.ensureConnected();

    const subId = `live-${crypto.randomUUID()}`;
    let resolveReady = () => {
      return;
    };
    const ready = new Promise<void>((resolve) => {
      resolveReady = () => {
        window.clearTimeout(fallbackTimeout);
        resolve();
      };
    });
    const fallbackTimeout = window.setTimeout(() => {
      resolveReady();
    }, 250);

    this.subscriptions.set(subId, {
      mode: "live",
      filter,
      onEvent,
      resolveReady,
    });

    try {
      await this.sendRawWithReconnectRetry(
        ["REQ", subId, filter],
        "Failed to restore relay subscription.",
      );
    } catch (error) {
      window.clearTimeout(fallbackTimeout);
      this.subscriptions.delete(subId);
      throw error;
    }
    await ready;

    return async () => {
      const active = this.subscriptions.get(subId);
      if (!active || active.mode !== "live") {
        return;
      }

      this.subscriptions.delete(subId);
      await this.closeSubscription(subId);
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

  private normalizeRelayError(error: unknown, fallbackMessage: string) {
    return error instanceof Error ? error : new Error(fallbackMessage);
  }

  private recoverFromSocketFailure(
    error: unknown,
    fallbackMessage: string,
  ): Error {
    const normalizedError = this.normalizeRelayError(error, fallbackMessage);
    this.resetConnection(normalizedError);
    return normalizedError;
  }

  private async sendRawWithReconnectRetry(
    payload: unknown[],
    fallbackMessage: string,
  ) {
    try {
      await this.sendRaw(payload);
    } catch (error) {
      const normalizedError = this.recoverFromSocketFailure(
        error,
        fallbackMessage,
      );

      try {
        await this.ensureConnected();
        await this.sendRaw(payload);
      } catch (retryError) {
        throw this.recoverFromSocketFailure(
          retryError,
          normalizedError.message,
        );
      }
    }
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

      void this.sendRaw(["EVENT", event]).catch(async (error) => {
        const pendingEvent = this.pendingEvents.get(event.id);
        this.pendingEvents.delete(event.id);
        const normalizedError = this.recoverFromSocketFailure(
          error,
          sendErrorMessage,
        );

        try {
          await this.ensureConnected();
          if (!pendingEvent) {
            throw normalizedError;
          }

          this.pendingEvents.set(event.id, pendingEvent);
          await this.sendRaw(["EVENT", event]);
        } catch (retryError) {
          window.clearTimeout(timeout);
          this.pendingEvents.delete(event.id);
          reject(
            this.recoverFromSocketFailure(retryError, normalizedError.message),
          );
        }
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

    subscription.lastSeenCreatedAt = Math.max(
      subscription.lastSeenCreatedAt ?? 0,
      event.created_at,
    );

    this.eventBuffer.push({ subId, event });
    this.flushTimeout ??= window.setTimeout(
      () => this.flushEventBuffer(),
      EVENT_BATCH_MS,
    );
  }

  private flushEventBuffer() {
    this.flushTimeout = null;
    const buffer = this.eventBuffer;
    this.eventBuffer = [];

    // Re-lookup: subscriptions removed during batch window are intentionally skipped.
    for (const { subId, event } of buffer) {
      const subscription = this.subscriptions.get(subId);
      if (subscription?.mode === "live") {
        subscription.onEvent(event);
      }
    }
  }

  private handleEose(subId: string) {
    const subscription = this.subscriptions.get(subId);
    if (!subscription) {
      return;
    }

    if (subscription.mode === "live") {
      subscription.resolveReady?.();
      subscription.resolveReady = undefined;
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
        this.resetConnection(error, { reconnect: false });
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

  private hasLiveSubscriptions() {
    for (const subscription of this.subscriptions.values()) {
      if (subscription.mode === "live") {
        return true;
      }
    }

    return false;
  }

  private buildReplayFilter(filter: RelaySubscriptionFilter, since?: number) {
    if (since === undefined) {
      return filter;
    }

    return {
      ...filter,
      since: filter.since === undefined ? since : Math.max(filter.since, since),
    };
  }

  private async replayLiveSubscriptions() {
    for (const [subId, subscription] of this.subscriptions) {
      if (subscription.mode !== "live") {
        continue;
      }

      const replaySince =
        subscription.lastSeenCreatedAt === undefined
          ? undefined
          : Math.max(
              0,
              subscription.lastSeenCreatedAt - RECONNECT_REPLAY_SKEW_SECS,
            );

      try {
        await this.sendRaw([
          "REQ",
          subId,
          this.buildReplayFilter(subscription.filter, replaySince),
        ]);
      } catch (error) {
        const reconnectError =
          error instanceof Error
            ? error
            : new Error("Failed to restore relay subscriptions.");
        this.resetConnection(reconnectError);
        throw reconnectError;
      }
    }
  }

  private scheduleReconnect() {
    if (
      this.reconnectTimeout ||
      this.wsId !== null ||
      (!this.keepAliveRequested && !this.hasLiveSubscriptions())
    ) {
      return;
    }

    const delay = this.reconnectDelayMs;
    this.reconnectDelayMs = Math.min(
      this.reconnectDelayMs * 2,
      RECONNECT_MAX_DELAY_MS,
    );

    this.reconnectTimeout = window.setTimeout(() => {
      this.reconnectTimeout = null;
      void this.ensureConnected().catch(() => {
        this.scheduleReconnect();
      });
    }, delay);
  }

  private emitReconnectIfNeeded() {
    const shouldNotifyReconnectListeners =
      this.hasConnectedOnce && this.notifyReconnectListeners;

    this.hasConnectedOnce = true;
    this.notifyReconnectListeners = false;

    if (!shouldNotifyReconnectListeners) {
      return;
    }

    for (const listener of this.reconnectListeners) {
      try {
        listener();
      } catch (error) {
        console.error("Failed to handle relay reconnect", error);
      }
    }
  }

  private resetConnection(
    error: Error,
    options?: {
      reconnect?: boolean;
    },
  ) {
    this.onMessageChannel = null;
    if (this.flushTimeout !== null) window.clearTimeout(this.flushTimeout);
    this.flushTimeout = null;
    this.eventBuffer = [];

    if (options?.reconnect !== false && this.hasConnectedOnce) {
      this.notifyReconnectListeners = true;
    }

    if (options?.reconnect === false && this.reconnectTimeout) {
      window.clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

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
        this.subscriptions.delete(subId);
        continue;
      }

      subscription.resolveReady?.();
      subscription.resolveReady = undefined;
    }

    for (const [eventId, pendingEvent] of this.pendingEvents) {
      window.clearTimeout(pendingEvent.timeout);
      pendingEvent.reject(error);
      this.pendingEvents.delete(eventId);
    }

    if (options?.reconnect !== false) {
      this.scheduleReconnect();
    }
  }
}
