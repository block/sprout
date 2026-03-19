import { request } from "@playwright/test";

const tylerPubkey =
  "e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34";
const relayBaseUrl =
  process.env.SPROUT_E2E_RELAY_URL ?? "http://localhost:3000";
const seedTimeoutMs = Number.parseInt(
  process.env.SPROUT_E2E_SEED_TIMEOUT_MS ?? "25000",
  10,
);
const requestTimeoutMs = Number.parseInt(
  process.env.SPROUT_E2E_SEED_REQUEST_TIMEOUT_MS ?? "2000",
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
        const response = await context.get(`${relayBaseUrl}/api/channels`, {
          headers: {
            "X-Pubkey": tylerPubkey,
          },
          timeout: requestTimeoutMs,
        });

        if (!response.ok()) {
          lastFailure = `HTTP ${response.status()} from /api/channels`;
        } else {
          const channels = (await response.json()) as Array<{ name: string }>;
          if (channels.some((channel) => channel.name === "general")) {
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
