import { expect, test, type Browser } from "@playwright/test";

import { installRelayBridge } from "../helpers/bridge";
import { assertRelaySeeded } from "../helpers/seed";

test.beforeAll(async () => {
  await assertRelaySeeded();
});

test("loads channels from the relay", async ({ page }) => {
  await installRelayBridge(page, "tyler");
  await page.goto("/");

  await expect(page.getByTestId("stream-list")).toContainText("general");
  await expect(page.getByTestId("stream-list")).toContainText("random");
  await expect(page.getByTestId("forum-list")).toContainText("watercooler");
  await expect(page.getByTestId("dm-list")).toContainText("alice-tyler");
});

test("loads the home feed from the relay", async ({ page }) => {
  await installRelayBridge(page, "tyler");
  await page.goto("/");

  await expect(page.getByTestId("chat-title")).toHaveText("Home");
  await expect(
    page.getByRole("heading", { name: "Focus queue" }),
  ).toBeVisible();
  await expect(page.getByRole("heading", { name: "@Mentions" })).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Needs Action" }),
  ).toBeVisible();
});

test("creates a relay-backed stream", async ({ page }) => {
  const channelName = `desktop-e2e-${Date.now()}`;

  await installRelayBridge(page, "tyler");
  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page
    .getByTestId("create-stream-description")
    .fill("Created from Playwright");
  await page.getByRole("button", { name: "Create" }).click();

  await expect(page.getByTestId("stream-list")).toContainText(channelName);
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
});

test("sends a message through the real relay", async ({ page }) => {
  const message = `Integration message ${Date.now()}`;

  await installRelayBridge(page, "tyler");
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await page.getByTestId("message-input").fill(message);
  await page.getByTestId("send-message").click();

  await expect(page.getByTestId("message-timeline")).toContainText(message);
});

test("delivers a message to a second browser context in real time", async ({
  browser,
}: {
  browser: Browser;
}) => {
  const contextOne = await browser.newContext();
  const contextTwo = await browser.newContext();
  const pageOne = await contextOne.newPage();
  const pageTwo = await contextTwo.newPage();
  const message = `Realtime message ${Date.now()}`;

  try {
    await installRelayBridge(pageOne, "tyler");
    await installRelayBridge(pageTwo, "alice");

    await pageOne.goto("/");
    await pageTwo.goto("/");

    await pageOne.getByTestId("channel-general").click();
    await pageTwo.getByTestId("channel-general").click();
    await expect(pageOne.getByTestId("chat-title")).toHaveText("general");
    await expect(pageTwo.getByTestId("chat-title")).toHaveText("general");

    await pageOne.getByTestId("message-input").fill(message);
    await pageOne.getByTestId("send-message").click();

    await expect(pageTwo.getByTestId("message-timeline")).toContainText(
      message,
    );
  } finally {
    await contextOne.close();
    await contextTwo.close();
  }
});
