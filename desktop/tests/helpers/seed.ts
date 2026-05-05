import { request } from "@playwright/test";

const tylerPubkey =
  "e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34";
const isCi = Boolean(process.env.CI);
const relayBaseUrl =
  process.env.SPROUT_E2E_RELAY_URL ?? "http://127.0.0.1:3000";
const seedTimeoutMs = Number.parseInt(
  process.env.SPROUT_E2E_SEED_TIMEOUT_MS ?? (isCi ? "60000" : "25000"),
  10,
);
const requestTimeoutMs = Number.parseInt(
  process.env.SPROUT_E2E_SEED_REQUEST_TIMEOUT_MS ?? (isCi ? "5000" : "2000"),
  10,
);
const retryDelayMs = Number.parseInt(
  process.env.SPROUT_E2E_SEED_RETRY_DELAY_MS ?? "1000",
  10,
);

function delay(ms: number) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

export async function assertRelaySeeded() {
  const context = await request.newContext();
  const deadline = Date.now() + seedTimeoutMs;
  let lastFailure = "no checks attempted";

  try {
    while (Date.now() < deadline) {
      try {
        // Query channel metadata (kind:39000) via the Nostr HTTP bridge.
        // Uses X-Pubkey dev-mode header for auth (no NIP-98 needed in test).
        const response = await context.post(`${relayBaseUrl}/query`, {
          headers: {
            "X-Pubkey": tylerPubkey,
            "Content-Type": "application/json",
          },
          data: [{ kinds: [39000], limit: 50 }],
          timeout: requestTimeoutMs,
        });

        if (!response.ok()) {
          lastFailure = `HTTP ${response.status()} from POST /query`;
        } else {
          const events = (await response.json()) as Array<{
            tags: string[][];
            content: string;
          }>;
          // Check if any channel metadata event has name "general" in its tags
          const hasGeneral = events.some((event) =>
            event.tags.some((tag) => tag[0] === "name" && tag[1] === "general"),
          );
          if (hasGeneral) {
            return;
          }

          lastFailure =
            'seed data missing expected "general" channel from scripts/setup-desktop-test-data.sh';
        }
      } catch (error) {
        lastFailure =
          error instanceof Error ? error.message : "unknown relay check error";
      }

      await delay(retryDelayMs);
    }

    throw new Error(
      `Relay test data was not ready after ${seedTimeoutMs}ms. Last check: ${lastFailure}. Start the relay and run scripts/setup-desktop-test-data.sh.`,
    );
  } finally {
    await context.dispose();
  }
}
