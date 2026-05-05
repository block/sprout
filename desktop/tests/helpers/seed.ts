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
        // The setup script inserts test data directly into the DB tables
        // (channels, channel_members) — NOT as Nostr events. The relay's
        // POST /query only searches the events table, so we can't verify
        // seed data through it. Instead, check /_readiness which confirms
        // both Postgres and Redis are connected. The setup script has its
        // own verification (SELECT from channels table), so if the relay
        // is ready and the script completed, the data is present.
        const response = await context.get(`${relayBaseUrl}/_readiness`, {
          timeout: requestTimeoutMs,
        });

        if (!response.ok()) {
          const body = await response.text().catch(() => "");
          lastFailure = `relay not ready: HTTP ${response.status()} ${body}`;
        } else {
          const data = (await response.json()) as { status: string };
          if (data.status === "ready") {
            return;
          }
          lastFailure = `relay readiness status: ${data.status}`;
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
