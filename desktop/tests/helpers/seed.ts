import { request } from "@playwright/test";

const tylerPubkey =
  "e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34";

export async function assertRelaySeeded() {
  const context = await request.newContext();

  try {
    const response = await context.get("http://localhost:3000/api/channels", {
      headers: {
        "X-Pubkey": tylerPubkey,
      },
    });

    if (!response.ok()) {
      throw new Error(
        "Relay test data is unavailable. Start the relay and run scripts/setup-desktop-test-data.sh.",
      );
    }

    const channels = (await response.json()) as Array<{ name: string }>;
    if (!channels.some((channel) => channel.name === "general")) {
      throw new Error(
        'Relay test data is incomplete. Expected a "general" channel from scripts/setup-desktop-test-data.sh.',
      );
    }
  } finally {
    await context.dispose();
  }
}
