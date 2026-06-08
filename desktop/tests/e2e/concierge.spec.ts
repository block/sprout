import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

// Concierge: Home entry → live session bootstrap (managed agent on the
// relay-mesh preset + persistent DM) → voice/transcript surface. Asserts the
// bridge CALL CHAIN for the bootstrap, not just rendered labels.

type E2eWindow = Window & {
  __SPROUT_E2E_COMMANDS__?: string[];
  __SPROUT_E2E_INVOKE_MOCK_COMMAND__?: unknown;
  __TAURI_INTERNALS__?: { invoke?: unknown };
};

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

async function gotoApp(page: import("@playwright/test").Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => {
    const w = window as E2eWindow;
    return (
      typeof w.__SPROUT_E2E_INVOKE_MOCK_COMMAND__ === "function" ||
      typeof w.__TAURI_INTERNALS__?.invoke === "function"
    );
  }, null);
  await expect(page.getByTestId("open-agents-view")).toBeVisible({
    timeout: 10_000,
  });
}

async function commands(page: import("@playwright/test").Page) {
  return page.evaluate(
    () => (window as E2eWindow).__SPROUT_E2E_COMMANDS__ ?? [],
  );
}

test("home launcher opens the live concierge and bootstraps the session", async ({
  page,
}) => {
  await gotoApp(page);

  const launcher = page.getByTestId("concierge-launcher");
  await expect(launcher).toBeVisible();
  await launcher.click();
  await expect(page).toHaveURL(/#\/concierge$/);

  // Live surface: orb + composer render once the session resolves.
  await expect(page.getByTestId("concierge-orb")).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.getByTestId("concierge-input")).toBeVisible();

  // Bootstrap chain: no Concierge agent exists in the mock, so the session
  // must create one from the mesh preset and open the persistent DM.
  const recorded = await commands(page);
  const order = [
    "list_managed_agents",
    "mesh_availability",
    "mesh_agent_preset",
    "create_managed_agent",
    "open_dm",
  ].map((command) => recorded.indexOf(command));
  expect(order, `commands: ${recorded.join(", ")}`).not.toContain(-1);
  expect([...order].sort((a, b) => a - b)).toEqual(order);
});

test("sidebar entry navigates to the concierge", async ({ page }) => {
  await gotoApp(page);
  await page.getByTestId("open-concierge-view").click();
  await expect(page).toHaveURL(/#\/concierge$/);
  await expect(page.getByTestId("concierge-screen")).toBeVisible();
});

test("typed turn lands in the DM transcript", async ({ page }) => {
  await gotoApp(page);
  await page.getByTestId("concierge-launcher").click();

  const input = page.getByTestId("concierge-input");
  await expect(input).toBeVisible({ timeout: 10_000 });
  const message = `Concierge, status check ${Date.now()}`;
  await input.fill(message);
  await page.getByTestId("concierge-send").click();

  await expect(page.getByTestId("concierge-screen")).toContainText(message, {
    timeout: 10_000,
  });
});

test("demo search params still render the static screenshot screen", async ({
  page,
}) => {
  await gotoApp(page);
  await page.goto("/#/concierge?phase=listening", {
    waitUntil: "domcontentloaded",
  });

  await expect(page.getByTestId("concierge-orb")).toBeVisible();
  await expect(page.getByTestId("concierge-phase-label")).toHaveText(
    "Listening…",
  );
  // The demo screen never touches the session machinery.
  const recorded = await commands(page);
  expect(recorded).not.toContain("create_managed_agent");
});
