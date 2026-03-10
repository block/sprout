import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

test("send a message and see it in timeline", async ({ page }) => {
  const message = `Hello timeline ${Date.now()}`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.getByTestId("message-input").fill(message);
  await page.getByTestId("send-message").click();

  await expect(page.getByTestId("message-timeline")).toContainText(message);
});

test("send multiple messages in sequence", async ({ page }) => {
  const ts = Date.now();
  const messages = [
    `First message ${ts}`,
    `Second message ${ts}`,
    `Third message ${ts}`,
  ];
  const input = page.getByTestId("message-input");
  const sendButton = page.getByTestId("send-message");

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  for (const message of messages) {
    await input.fill(message);
    await sendButton.click();
    await expect(page.getByTestId("message-timeline")).toContainText(message);
  }

  const timeline = page.getByTestId("message-timeline");
  for (const message of messages) {
    await expect(timeline).toContainText(message);
  }
});

test("message input clears after send", async ({ page }) => {
  const message = `Clear after send ${Date.now()}`;
  const input = page.getByTestId("message-input");

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await input.fill(message);
  await expect(input).toHaveValue(message);
  await page.getByTestId("send-message").click();

  await expect(page.getByTestId("message-timeline")).toContainText(message);
  await expect(input).toHaveValue("");
});

test("empty message cannot be sent", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const sendButton = page.getByTestId("send-message");
  await expect(sendButton).toBeDisabled();
});

test("send message with Enter key", async ({ page }) => {
  const message = `Enter key send ${Date.now()}`;
  const input = page.getByTestId("message-input");

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await input.fill(message);
  await input.press("Enter");

  await expect(page.getByTestId("message-timeline")).toContainText(message);
});

test("messages persist across channel switches", async ({ page }) => {
  const message = `Persist across switch ${Date.now()}`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.getByTestId("message-input").fill(message);
  await page.getByTestId("send-message").click();
  await expect(page.getByTestId("message-timeline")).toContainText(message);

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");

  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("message-timeline")).toContainText(message);
});

test("different channels have independent messages", async ({ page }) => {
  const ts = Date.now();
  const generalMessage = `General only ${ts}`;
  const randomMessage = `Random only ${ts}`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await page.getByTestId("message-input").fill(generalMessage);
  await page.getByTestId("send-message").click();
  await expect(page.getByTestId("message-timeline")).toContainText(
    generalMessage,
  );

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");
  await expect(page.getByTestId("message-timeline")).not.toContainText(
    generalMessage,
  );

  await page.getByTestId("message-input").fill(randomMessage);
  await page.getByTestId("send-message").click();
  await expect(page.getByTestId("message-timeline")).toContainText(
    randomMessage,
  );

  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("message-timeline")).toContainText(
    generalMessage,
  );
  await expect(page.getByTestId("message-timeline")).not.toContainText(
    randomMessage,
  );
});

test("day divider appears in timeline", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await expect(page.getByTestId("message-timeline")).toContainText(
    "Welcome to #general",
  );
  await expect(page.getByTestId("message-timeline-day-divider")).toBeVisible();
});

test("send message to DM channel", async ({ page }) => {
  const message = `DM message ${Date.now()}`;

  await page.goto("/");
  await page.getByTestId("channel-alice-tyler").click();
  await expect(page.getByTestId("chat-title")).toHaveText("alice-tyler");

  await page.getByTestId("message-input").fill(message);
  await page.getByTestId("send-message").click();

  await expect(page.getByTestId("message-timeline")).toContainText(message);
});
