import type { Page } from "@playwright/test";

export const TEST_IDENTITIES = {
  tyler: {
    privateKey:
      "3dbaebadb5dfd777ff25149ee230d907a15a9e1294b40b830661e65bb42f6c03",
    pubkey: "e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34",
    username: "tyler",
  },
  alice: {
    privateKey:
      "3fa69cbac1dcb9b7b6ac83117c74bd23bb1e717fe8fc7cfda67b47bb4323383d",
    pubkey: "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f",
    username: "alice",
  },
  bob: {
    privateKey:
      "7667ae87cbc50ac0b2251b115c9c51aca7e2da65301b28ecf82f4e4c5260a6bb",
    pubkey: "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260",
    username: "bob",
  },
  charlie: {
    privateKey:
      "813fc3bb90587a82b2bfee9b833503e7686c7480681850b3d789c6987e997fc8",
    pubkey: "554cef57437abac34522ac2c9f0490d685b72c80478cf9f7ed6f9570ee8624ea",
    username: "charlie",
  },
  outsider: {
    privateKey:
      "91bd673543195c0c78fc74a881545dcc8cd6ea6d0f9f8efb3225d58c4bc70dad",
    pubkey: "df8e91b86fda13a9a67896df77232f7bdab2ba9c3e165378e1ba3d24c13a328e",
    username: "outsider",
  },
} as const;

type BridgeMode = "mock" | "relay";

type BridgeOptions = {
  mode: BridgeMode;
  relayHttpUrl?: string;
  relayWsUrl?: string;
  user?: keyof typeof TEST_IDENTITIES;
};

export async function installBridge(page: Page, options: BridgeOptions) {
  const identity =
    options.mode === "relay"
      ? TEST_IDENTITIES[options.user ?? "tyler"]
      : undefined;

  await page.addInitScript(
    ({ identity: bridgeIdentity, mode, relayHttpUrl, relayWsUrl }) => {
      (
        window as Window & {
          __SPROUT_E2E__?: Record<string, unknown>;
        }
      ).__SPROUT_E2E__ = {
        identity: bridgeIdentity,
        mode,
        relayHttpUrl,
        relayWsUrl,
      };
    },
    {
      identity,
      mode: options.mode,
      relayHttpUrl: options.relayHttpUrl,
      relayWsUrl: options.relayWsUrl,
    },
  );
}

export async function installMockBridge(page: Page) {
  await installBridge(page, { mode: "mock" });
}

export async function installRelayBridge(
  page: Page,
  user: keyof typeof TEST_IDENTITIES = "tyler",
) {
  await installBridge(page, { mode: "relay", user });
}
