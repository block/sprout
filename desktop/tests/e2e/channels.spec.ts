import { expect, test } from "@playwright/test";

import { TEST_IDENTITIES, installMockBridge } from "../helpers/bridge";

const MOCK_IDENTITY_PUBKEY = "deadbeef".repeat(8);

async function openChannelManagement(
  page: import("@playwright/test").Page,
  channelName: string,
) {
  await page.getByTestId(`channel-${channelName}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
}

async function closeChannelManagement(page: import("@playwright/test").Page) {
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("channel-management-sheet")).not.toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

test("sidebar shows all channel types", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByTestId("app-sidebar")).toBeVisible();

  // Streams
  const streamList = page.getByTestId("stream-list");
  await expect(streamList).toContainText("general");
  await expect(streamList).toContainText("random");
  await expect(streamList).toContainText("engineering");
  await expect(streamList).toContainText("agents");

  // Forums
  const forumList = page.getByTestId("forum-list");
  await expect(forumList).toContainText("watercooler");
  await expect(forumList).toContainText("announcements");

  // DMs
  const dmList = page.getByTestId("dm-list");
  await expect(dmList).toContainText("alice-tyler");
  await expect(dmList).toContainText("bob-tyler");
});

test("shows presence in sidebar, DM header, and member list", async ({
  page,
}) => {
  await page.goto("/");

  await expect(page.getByTestId("sidebar-profile-card")).toBeVisible();
  await expect(page.getByTestId("self-presence-badge")).toHaveAttribute(
    "aria-label",
    "Offline",
  );
  await expect(page.getByTestId("channel-presence-alice-tyler")).toBeVisible();

  await page.getByTestId("channel-alice-tyler").click();
  await expect(page.getByTestId("chat-title")).toHaveText("alice-tyler");
  await expect(page.getByTestId("chat-presence-badge")).toContainText("Online");

  await openChannelManagement(page, "general");
  await expect(
    page.getByTestId(`member-presence-${TEST_IDENTITIES.alice.pubkey}`),
  ).toContainText("Online");
  await expect(
    page.getByTestId(`member-presence-${TEST_IDENTITIES.bob.pubkey}`),
  ).toContainText("Away");
  await closeChannelManagement(page);
});

test("create stream with name and description", async ({ page }) => {
  const channelName = `my-new-stream-${Date.now()}`;

  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page
    .getByTestId("create-stream-description")
    .fill("A stream for testing channel creation");
  await page.getByRole("button", { name: "Create" }).click();

  await expect(page.getByTestId("stream-list")).toContainText(channelName);
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
});

test("create stream with special characters", async ({ page }) => {
  const channelName = `dev ops-${Date.now()}`;

  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page
    .getByTestId("create-stream-description")
    .fill("Stream with spaces and hyphens");
  await page.getByRole("button", { name: "Create" }).click();

  await expect(page.getByTestId("stream-list")).toContainText(channelName);
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
});

test("switch between streams", async ({ page }) => {
  await page.goto("/");

  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");

  await page.getByTestId("channel-engineering").click();
  await expect(page.getByTestId("chat-title")).toHaveText("engineering");
});

test("switch between channel types", async ({ page }) => {
  await page.goto("/");

  // Stream
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  // Forum
  await page.getByTestId("channel-watercooler").click();
  await expect(page.getByTestId("chat-title")).toHaveText("watercooler");

  // DM
  await page.getByTestId("channel-alice-tyler").click();
  await expect(page.getByTestId("chat-title")).toHaveText("alice-tyler");
});

test("empty channel shows empty state", async ({ page }) => {
  await page.goto("/");

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");
  await expect(page.getByTestId("message-empty")).toBeVisible();
});

test("channel with messages shows content", async ({ page }) => {
  await page.goto("/");

  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("message-timeline")).toContainText(
    "Welcome to #general",
  );
});

test("sidebar shows unread indicator for newly active channels", async ({
  page,
}) => {
  await page.goto("/");

  await expect(page.getByTestId("channel-unread-random")).toHaveCount(0);

  await page.evaluate(() => {
    window.__SPROUT_E2E_EMIT_MOCK_MESSAGE__?.({
      channelName: "random",
      content: "Unread update for #random",
    });
  });

  await expect(page.getByTestId("channel-unread-random")).toBeVisible();

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");
  await expect(page.getByTestId("message-timeline")).toContainText(
    "Unread update for #random",
  );
  await expect(page.getByTestId("channel-unread-random")).toHaveCount(0);
});

test("sidebar persists after channel switch", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByTestId("app-sidebar")).toBeVisible();

  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("app-sidebar")).toBeVisible();

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");
  await expect(page.getByTestId("app-sidebar")).toBeVisible();

  await page.getByTestId("channel-watercooler").click();
  await expect(page.getByTestId("chat-title")).toHaveText("watercooler");
  await expect(page.getByTestId("app-sidebar")).toBeVisible();
});

test("manage channel updates details and context", async ({ page }) => {
  const stamp = Date.now();
  const newName = `release-hub-${stamp}`;
  const newDescription = `Release coordination ${stamp}`;
  const newTopic = `Launch plan ${stamp}`;
  const newPurpose = `Track blockers and owners ${stamp}`;

  await page.goto("/");
  await openChannelManagement(page, "general");

  await page.getByTestId("channel-management-name").fill(newName);
  await page.getByTestId("channel-management-description").fill(newDescription);
  await page.getByTestId("channel-management-save-details").click();

  await expect(page.getByTestId("chat-title")).toHaveText(newName);
  await expect(page.getByTestId("stream-list")).toContainText(newName);

  const saveTopicButton = page.getByTestId("channel-management-save-topic");
  const savePurposeButton = page.getByTestId("channel-management-save-purpose");

  await page.getByTestId("channel-management-topic").fill(newTopic);
  await saveTopicButton.click();
  await expect(saveTopicButton).toHaveText("Save topic");
  await expect(page.getByTestId("channel-management-topic")).toHaveValue(
    newTopic,
  );

  await page.getByTestId("channel-management-purpose").fill(newPurpose);
  await savePurposeButton.click();
  await expect(savePurposeButton).toHaveText("Save purpose");
  await expect(page.getByTestId("channel-management-purpose")).toHaveValue(
    newPurpose,
  );

  await closeChannelManagement(page);

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");

  await page.getByTestId("stream-list").getByText(newName).click();
  await expect(page.getByTestId("chat-title")).toHaveText(newName);
  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();

  await expect(page.getByTestId("channel-management-name")).toHaveValue(
    newName,
  );
  await expect(page.getByTestId("channel-management-description")).toHaveValue(
    newDescription,
  );
  await expect(page.getByTestId("channel-management-topic")).toHaveValue(
    newTopic,
  );
  await expect(page.getByTestId("channel-management-purpose")).toHaveValue(
    newPurpose,
  );
});

test("manage channel can invite and remove members", async ({ page }) => {
  await page.goto("/");
  await openChannelManagement(page, "general");

  await page
    .getByTestId("channel-management-add-pubkeys")
    .fill(TEST_IDENTITIES.charlie.pubkey);
  await page.getByTestId("channel-management-add-role").selectOption("admin");
  await page.getByTestId("channel-management-add-members").click();

  await expect(page.getByTestId("channel-management-add-pubkeys")).toHaveValue(
    "",
  );
  await expect(
    page.getByTestId(`channel-member-${TEST_IDENTITIES.charlie.pubkey}`),
  ).toContainText("charlie");
  await expect(
    page.getByTestId(`channel-member-${TEST_IDENTITIES.charlie.pubkey}`),
  ).toContainText("admin");

  await page
    .getByTestId(`remove-member-${TEST_IDENTITIES.charlie.pubkey}`)
    .click();

  await expect(
    page.getByTestId(`channel-member-${TEST_IDENTITIES.charlie.pubkey}`),
  ).toHaveCount(0);
});

test("open channel management supports join and leave", async ({ page }) => {
  await page.goto("/");
  await openChannelManagement(page, "random");

  await expect(page.getByTestId("channel-management-join")).toBeVisible();
  await expect(page.getByTestId("channel-management-leave")).toHaveCount(0);

  await page.getByTestId("channel-management-join").click();

  await expect(page.getByTestId("channel-management-join")).toHaveCount(0);
  await expect(page.getByTestId("channel-management-leave")).toBeVisible();
  await expect(
    page.getByTestId(`channel-member-${MOCK_IDENTITY_PUBKEY}`),
  ).toContainText("You");

  await page.getByTestId("channel-management-leave").click();
  await expect(page.getByTestId("channel-management-sheet")).not.toBeVisible();

  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
  await expect(page.getByTestId("channel-management-join")).toBeVisible();
  await expect(
    page.getByTestId(`channel-member-${MOCK_IDENTITY_PUBKEY}`),
  ).toHaveCount(0);
});

test("manage channel can archive and unarchive a stream", async ({ page }) => {
  await page.goto("/");
  await openChannelManagement(page, "general");

  await page.getByTestId("channel-management-archive").click();
  await expect(page.getByTestId("channel-management-unarchive")).toBeVisible();

  await closeChannelManagement(page);
  await expect(page.getByTestId("message-input")).toBeDisabled();
  await expect(page.getByTestId("send-message")).toBeDisabled();

  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
  await page.getByTestId("channel-management-unarchive").click();
  await expect(page.getByTestId("channel-management-archive")).toBeVisible();

  await closeChannelManagement(page);
  await expect(page.getByTestId("message-input")).toBeEnabled();
});

test("manage channel can delete an owned stream", async ({ page }) => {
  const channelName = `delete-me-${Date.now()}`;

  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page.getByRole("button", { name: "Create" }).click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);

  page.once("dialog", (dialog) => dialog.accept());

  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
  await page.getByTestId("channel-management-delete").click();

  await expect(page.getByTestId("chat-title")).toHaveText("Home");
  await expect(page.getByTestId("stream-list")).not.toContainText(channelName);
});
