import { expect, test, type Browser } from "@playwright/test";

import { installRelayBridge } from "../helpers/bridge";
import { assertRelaySeeded } from "../helpers/seed";

async function createStream(
  page: import("@playwright/test").Page,
  channelName: string,
  description?: string,
) {
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  if (description !== undefined) {
    await page.getByTestId("create-stream-description").fill(description);
  }
  await page.getByRole("button", { name: "Create" }).click();

  await expect(page.getByTestId("stream-list")).toContainText(channelName);
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
}

async function openChannelManagement(page: import("@playwright/test").Page) {
  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
}

async function closeChannelManagement(page: import("@playwright/test").Page) {
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("channel-management-sheet")).not.toBeVisible();
}

test.beforeAll(async () => {
  await assertRelaySeeded();
});

test("create channel and verify in sidebar", async ({ page }) => {
  const channelName = `integration-e2e-${Date.now()}`;

  await installRelayBridge(page, "tyler");
  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page.getByRole("button", { name: "Create" }).click();

  await expect(page.getByTestId("stream-list")).toContainText(channelName);
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
});

test("two users see the same channel", async ({
  browser,
}: {
  browser: Browser;
}) => {
  const channelName = `shared-channel-${Date.now()}`;
  const contextOne = await browser.newContext();
  const contextTwo = await browser.newContext();
  const pageOne = await contextOne.newPage();
  const pageTwo = await contextTwo.newPage();

  try {
    await installRelayBridge(pageOne, "tyler");
    await installRelayBridge(pageTwo, "alice");

    await pageOne.goto("/");
    await pageOne.getByRole("button", { name: "Create a stream" }).click();
    await pageOne.getByTestId("create-stream-name").fill(channelName);
    await pageOne.getByRole("button", { name: "Create" }).click();
    await expect(pageOne.getByTestId("stream-list")).toContainText(channelName);

    await pageTwo.goto("/");
    await pageTwo.getByTestId("browse-channels").click();
    await expect(pageTwo.getByTestId("channel-browser-dialog")).toBeVisible();
    await pageTwo
      .getByTestId(`browse-channel-${channelName}`)
      .getByRole("button", { name: "Join" })
      .click();
    await expect(pageTwo.getByTestId("stream-list")).toContainText(channelName);
  } finally {
    await contextOne.close();
    await contextTwo.close();
  }
});

test("message delivery across users", async ({
  browser,
}: {
  browser: Browser;
}) => {
  const message = `Cross-user message ${Date.now()}`;
  const contextOne = await browser.newContext();
  const contextTwo = await browser.newContext();
  const pageOne = await contextOne.newPage();
  const pageTwo = await contextTwo.newPage();

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

test("DM channel appears in sidebar", async ({ page }) => {
  await installRelayBridge(page, "tyler");
  await page.goto("/");

  await expect(page.getByTestId("dm-list")).toContainText("alice-tyler");
});

test("send message to DM", async ({ page }) => {
  const message = `DM message ${Date.now()}`;

  await installRelayBridge(page, "tyler");
  await page.goto("/");
  await page.getByTestId("channel-alice-tyler").click();
  await expect(page.getByTestId("chat-title")).toHaveText("alice-tyler");

  await page.getByTestId("message-input").fill(message);
  await page.getByTestId("send-message").click();

  await expect(page.getByTestId("message-timeline")).toContainText(message);
});

test("forum channel appears in sidebar", async ({ page }) => {
  await installRelayBridge(page, "tyler");
  await page.goto("/");

  await expect(page.getByTestId("forum-list")).toContainText("watercooler");
});

test("create channel with description", async ({ page }) => {
  const channelName = `desc-channel-${Date.now()}`;
  const description = `Description for ${channelName}`;

  await installRelayBridge(page, "tyler");
  await page.goto("/");
  await createStream(page, channelName, description);

  await expect(page.getByTestId("chat-description")).toContainText(description);
});

test("multiple channels independent", async ({ page }) => {
  const channelA = `channel-a-${Date.now()}`;
  const channelB = `channel-b-${Date.now()}`;
  const messageA = `Message in A ${Date.now()}`;

  await installRelayBridge(page, "tyler");
  await page.goto("/");

  // Create channel A
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelA);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelA);

  // Create channel B
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelB);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelB);

  // Navigate to channel A and send a message
  await page.getByTestId(`channel-${channelA}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelA);
  await page.getByTestId("message-input").fill(messageA);
  await page.getByTestId("send-message").click();
  await expect(page.getByTestId("message-timeline")).toContainText(messageA);

  // Switch to channel B — message from A should not appear
  await page.getByTestId(`channel-${channelB}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelB);
  await expect(page.getByTestId("message-timeline")).not.toContainText(
    messageA,
  );
});

test("manage sheet updates channel details and context through the relay", async ({
  page,
}) => {
  const stamp = Date.now();
  const initialName = `manage-integration-${stamp}`;
  const renamedChannel = `manage-renamed-${stamp}`;
  const initialDescription = `Initial description ${stamp}`;
  const updatedDescription = `Updated description ${stamp}`;
  const updatedTopic = `Updated topic ${stamp}`;
  const updatedPurpose = `Updated purpose ${stamp}`;

  await installRelayBridge(page, "tyler");
  await page.goto("/");
  await createStream(page, initialName, initialDescription);

  await openChannelManagement(page);
  await page.getByTestId("channel-management-name").fill(renamedChannel);
  await page
    .getByTestId("channel-management-description")
    .fill(updatedDescription);
  await page.getByTestId("channel-management-save-details").click();

  await expect(page.getByTestId("chat-title")).toHaveText(renamedChannel);
  await expect(page.getByTestId("stream-list")).toContainText(renamedChannel);

  const saveTopicButton = page.getByTestId("channel-management-save-topic");
  const savePurposeButton = page.getByTestId("channel-management-save-purpose");

  await page.getByTestId("channel-management-topic").fill(updatedTopic);
  await saveTopicButton.click();
  await expect(saveTopicButton).toHaveText("Save topic");
  await expect(page.getByTestId("channel-management-topic")).toHaveValue(
    updatedTopic,
  );

  await page.getByTestId("channel-management-purpose").fill(updatedPurpose);
  await savePurposeButton.click();
  await expect(savePurposeButton).toHaveText("Save purpose");
  await expect(page.getByTestId("channel-management-purpose")).toHaveValue(
    updatedPurpose,
  );

  await closeChannelManagement(page);
  await page.reload();

  await page.getByTestId(`channel-${renamedChannel}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(renamedChannel);
  await expect(page.getByTestId("chat-description")).toContainText(
    updatedTopic,
  );
  await expect(page.getByTestId("chat-description")).toContainText(
    updatedDescription,
  );
  await expect(page.getByTestId("chat-description")).toContainText(
    updatedPurpose,
  );

  await openChannelManagement(page);
  await expect(page.getByTestId("channel-management-name")).toHaveValue(
    renamedChannel,
  );
  await expect(page.getByTestId("channel-management-description")).toHaveValue(
    updatedDescription,
  );
  await expect(page.getByTestId("channel-management-topic")).toHaveValue(
    updatedTopic,
  );
  await expect(page.getByTestId("channel-management-purpose")).toHaveValue(
    updatedPurpose,
  );
});

test("manage sheet archive and unarchive survives a reload through the relay", async ({
  page,
}) => {
  const channelName = `archive-integration-${Date.now()}`;

  await installRelayBridge(page, "tyler");
  await page.goto("/");
  await createStream(page, channelName, "Archive integration channel");

  await openChannelManagement(page);
  await page.getByTestId("channel-management-archive").click();
  await expect(page.getByTestId("channel-management-unarchive")).toBeVisible();
  await closeChannelManagement(page);

  await expect(page.getByTestId("message-input")).toBeDisabled();
  await expect(page.getByTestId("send-message")).toBeDisabled();

  await page.reload();

  await page.getByTestId(`channel-${channelName}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
  await expect(page.getByTestId("message-input")).toBeDisabled();

  await openChannelManagement(page);
  await page.getByTestId("channel-management-unarchive").click();
  await expect(page.getByTestId("channel-management-archive")).toBeVisible();
  await closeChannelManagement(page);

  await expect(page.getByTestId("message-input")).toBeEnabled();
});
