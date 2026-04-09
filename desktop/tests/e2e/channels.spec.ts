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

async function openMembersSidebar(
  page: import("@playwright/test").Page,
  channelName: string,
) {
  await page.getByTestId(`channel-${channelName}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
  await page.getByTestId("channel-members-trigger").click();
  await expect(page.getByTestId("members-sidebar")).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

test("sidebar shows all channel types", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByTestId("app-sidebar")).toBeVisible();
  await expect(page.getByTestId("sidebar-agents-count")).toHaveText("0");

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

  await openMembersSidebar(page, "general");
  await expect(
    page.getByTestId(`sidebar-member-presence-${TEST_IDENTITIES.alice.pubkey}`),
  ).toContainText("Online");
  await expect(
    page.getByTestId(`sidebar-member-presence-${TEST_IDENTITIES.bob.pubkey}`),
  ).toContainText("Away");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("members-sidebar")).not.toBeVisible();
});

test("start a new direct message from the sidebar", async ({ page }) => {
  await page.goto("/");

  await page.getByTestId("new-dm-trigger").click();
  await expect(page.getByTestId("new-dm-dialog")).toBeVisible();

  await page.getByTestId("new-dm-search").fill("charlie");
  await page
    .getByTestId(`new-dm-result-${TEST_IDENTITIES.charlie.pubkey}`)
    .click();
  await expect(
    page.getByTestId(`new-dm-selected-${TEST_IDENTITIES.charlie.pubkey}`),
  ).toBeVisible();

  await page.getByTestId("new-dm-submit").click();

  await expect(page.getByTestId("dm-list")).toContainText("charlie");
  await expect(page.getByTestId("chat-title")).toHaveText("charlie");
});

test("create stream with name and description", async ({ page }) => {
  const channelName = `my-new-stream-${Date.now()}`;

  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page
    .getByTestId("create-stream-description")
    .fill("A stream for testing channel creation");
  await page
    .getByTestId("create-stream-form")
    .getByRole("button", { name: "Create" })
    .click();

  await expect(page.getByTestId("stream-list")).toContainText(channelName);
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
});

test("create ephemeral stream shows sidebar and header affordances", async ({
  page,
}) => {
  const channelName = `ephemeral-stream-${Date.now()}`;

  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page
    .getByTestId("create-stream-description")
    .fill("Auto-cleaned test stream");
  await page
    .getByTestId("create-stream-form")
    .getByLabel("Ephemeral — auto-archives after 1 day of inactivity")
    .click();
  await page
    .getByTestId("create-stream-form")
    .getByRole("button", { name: "Create" })
    .click();

  await expect(page.getByTestId("stream-list")).toContainText(channelName);
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
  await expect(
    page.getByTestId(`channel-ephemeral-${channelName}`),
  ).toContainText("Ephemeral");
  await expect(page.getByTestId("chat-ephemeral-badge")).toHaveText(
    /Ephemeral.+left/,
  );
});

test("create stream with special characters", async ({ page }) => {
  const channelName = `dev ops-${Date.now()}`;

  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page
    .getByTestId("create-stream-description")
    .fill("Stream with spaces and hyphens");
  await page
    .getByTestId("create-stream-form")
    .getByRole("button", { name: "Create" })
    .click();

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

test("shows and clears typing indicators for active channel bots", async ({
  page,
}) => {
  await page.goto("/");

  await page.getByTestId("channel-agents").click();
  await expect(page.getByTestId("chat-title")).toHaveText("agents");
  await page.waitForTimeout(300);

  await page.evaluate((pubkey) => {
    window.__SPROUT_E2E_EMIT_MOCK_TYPING__?.({
      channelName: "agents",
      pubkey,
    });
  }, TEST_IDENTITIES.charlie.pubkey);

  await expect(page.getByTestId("message-typing-indicator")).toBeVisible();
  await expect(
    page.getByTestId("message-typing-indicator-label"),
  ).toContainText("charlie is typing");

  await page.evaluate((pubkey) => {
    window.__SPROUT_E2E_EMIT_MOCK_MESSAGE__?.({
      channelName: "agents",
      content: "Done.",
      pubkey,
    });
  }, TEST_IDENTITIES.charlie.pubkey);

  await expect(page.getByTestId("message-timeline")).toContainText("Done.");
  await expect(page.getByTestId("message-typing-indicator")).toHaveCount(0);

  await page.waitForTimeout(1_200);
  await page.evaluate((pubkey) => {
    window.__SPROUT_E2E_EMIT_MOCK_TYPING__?.({
      channelName: "agents",
      pubkey,
    });
  }, TEST_IDENTITIES.charlie.pubkey);

  await expect(page.getByTestId("message-typing-indicator")).toHaveCount(0);
});

test("typing indicator shows avatars and maintains stable name order", async ({
  page,
}) => {
  await page.goto("/");

  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await page.waitForTimeout(300);

  // Alice starts typing first
  await page.evaluate((pubkey) => {
    window.__SPROUT_E2E_EMIT_MOCK_TYPING__?.({
      channelName: "general",
      pubkey,
    });
  }, TEST_IDENTITIES.alice.pubkey);

  await expect(page.getByTestId("message-typing-indicator")).toBeVisible();
  await expect(
    page.getByTestId("message-typing-indicator-label"),
  ).toContainText("alice is typing");

  // Verify avatar is rendered for the typing user
  const avatars = page
    .getByTestId("message-typing-indicator")
    .locator("[data-testid='message-typing-avatar']");
  await expect(avatars).toHaveCount(1);

  // Bob starts typing second
  await page.evaluate((pubkey) => {
    window.__SPROUT_E2E_EMIT_MOCK_TYPING__?.({
      channelName: "general",
      pubkey,
    });
  }, TEST_IDENTITIES.bob.pubkey);

  await expect(
    page.getByTestId("message-typing-indicator-label"),
  ).toContainText("alice and bob are typing");
  await expect(avatars).toHaveCount(2);

  // Alice re-broadcasts — order should stay "alice and bob", not flip
  await page.evaluate((pubkey) => {
    window.__SPROUT_E2E_EMIT_MOCK_TYPING__?.({
      channelName: "general",
      pubkey,
    });
  }, TEST_IDENTITIES.alice.pubkey);

  await expect(
    page.getByTestId("message-typing-indicator-label"),
  ).toContainText("alice and bob are typing");

  // Bob re-broadcasts — order should still stay "alice and bob"
  await page.evaluate((pubkey) => {
    window.__SPROUT_E2E_EMIT_MOCK_TYPING__?.({
      channelName: "general",
      pubkey,
    });
  }, TEST_IDENTITIES.bob.pubkey);

  await expect(
    page.getByTestId("message-typing-indicator-label"),
  ).toContainText("alice and bob are typing");
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

test("sidebar clears unread indicator after opening a DM", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByTestId("channel-unread-alice-tyler")).toHaveCount(0);

  await page.evaluate((pubkey) => {
    window.__SPROUT_E2E_EMIT_MOCK_MESSAGE__?.({
      channelName: "alice-tyler",
      content: "Unread update for the DM",
      pubkey,
    });
  }, TEST_IDENTITIES.alice.pubkey);

  await expect(page.getByTestId("channel-unread-alice-tyler")).toBeVisible();

  await page.getByTestId("channel-alice-tyler").click();
  await expect(page.getByTestId("chat-title")).toHaveText("alice-tyler");
  await expect(page.getByTestId("message-timeline")).toContainText(
    "Unread update for the DM",
  );
  await expect(page.getByTestId("channel-unread-alice-tyler")).toHaveCount(0);
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

test("manage channel keeps canvas near the top of the sheet", async ({
  page,
}) => {
  await page.goto("/");
  await openChannelManagement(page, "general");

  const sectionHeadings = await page
    .getByTestId("channel-management-sheet")
    .locator("section h2")
    .allTextContents();

  expect(sectionHeadings).toEqual([
    "Access",
    "Canvas",
    "Context",
    "Details",
    "Channel state",
    "Danger zone",
  ]);
});

test("members sidebar can invite and remove members", async ({ page }) => {
  await page.goto("/");
  await openMembersSidebar(page, "general");
  await expect(page.getByTestId("channel-members-trigger")).toContainText("3");
  await expect(page.getByTestId("channel-management-add-pubkeys")).toHaveCount(
    0,
  );

  await page.getByTestId("channel-management-search-users").fill("char");
  await expect(
    page.getByTestId(
      `channel-user-search-result-${TEST_IDENTITIES.charlie.pubkey}`,
    ),
  ).toBeVisible();
  await page
    .getByTestId(`channel-user-search-result-${TEST_IDENTITIES.charlie.pubkey}`)
    .click();
  await expect(
    page.getByTestId(`selected-invitee-${TEST_IDENTITIES.charlie.pubkey}`),
  ).toContainText("charlie");

  await page.getByTestId("channel-management-add-role").selectOption("admin");
  await page.getByTestId("channel-management-add-members").click();

  await expect(
    page.getByTestId(`selected-invitee-${TEST_IDENTITIES.charlie.pubkey}`),
  ).toHaveCount(0);
  await expect(page.getByTestId("channel-management-search-users")).toHaveValue(
    "",
  );
  await expect(
    page.getByTestId(`sidebar-member-${TEST_IDENTITIES.charlie.pubkey}`),
  ).toContainText("charlie");
  await expect(page.getByTestId("channel-members-trigger")).toContainText("4");

  await page
    .getByTestId(`sidebar-remove-member-${TEST_IDENTITIES.charlie.pubkey}`)
    .click();

  await expect(
    page.getByTestId(`sidebar-member-${TEST_IDENTITIES.charlie.pubkey}`),
  ).toHaveCount(0);
  await expect(page.getByTestId("channel-members-trigger")).toContainText("3");
});

test("members sidebar keeps direct pubkey entry behind a toggle", async ({
  page,
}) => {
  await page.goto("/");
  await openMembersSidebar(page, "general");

  await expect(page.getByTestId("channel-management-add-pubkeys")).toHaveCount(
    0,
  );

  await page.getByTestId("channel-management-toggle-direct-pubkeys").click();
  await expect(
    page.getByTestId("channel-management-add-pubkeys"),
  ).toBeVisible();

  await page
    .getByTestId("channel-management-add-pubkeys")
    .fill(TEST_IDENTITIES.outsider.pubkey);
  await page.getByTestId("channel-management-add-members").click();

  await expect(
    page.getByTestId(`sidebar-member-${TEST_IDENTITIES.outsider.pubkey}`),
  ).toContainText("outsider");
  await expect(page.getByTestId("channel-members-trigger")).toContainText("4");
});

test("open-channel members can add agents from the header", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 420 });
  await page.goto("/");

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");

  const addAgentTrigger = page.getByTestId("channel-add-bot-trigger");
  await expect(addAgentTrigger).toBeEnabled();

  await addAgentTrigger.click();
  await expect(page.getByRole("heading", { name: "Add agents" })).toBeVisible();
  await expect(page.getByTestId("add-channel-bot-dialog-header")).toBeVisible();
  await expect(
    page.getByTestId("add-channel-bot-dialog-scroll-area"),
  ).toBeVisible();
  await expect(
    page.getByTestId("add-channel-bot-dialog-scroll-area"),
  ).toHaveCSS("overflow-y", "auto");
  expect(
    await page
      .getByTestId("add-channel-bot-dialog-scroll-area")
      .evaluate(
        (element) =>
          element.scrollHeight > element.clientHeight &&
          element.clientHeight > 0,
      ),
  ).toBe(true);
  await expect(page.getByTestId("add-channel-bot-dialog-footer")).toBeVisible();
});

test("removing a channel-scoped agent also cleans up the managed agent record", async ({
  page,
}) => {
  const agentName = `cleanup-agent-${Date.now()}`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.getByTestId("channel-add-bot-trigger").click();
  await expect(page.getByRole("heading", { name: "Add agents" })).toBeVisible();

  await page.getByRole("button", { name: "Generic" }).click();
  await page.locator("#channel-generic-name").fill(agentName);
  await page
    .locator("#channel-generic-prompt")
    .fill("Watch the channel and help when asked.");
  await page.getByRole("button", { name: "Add agent" }).click();
  await expect(page.getByRole("heading", { name: "Add agents" })).toHaveCount(
    0,
  );

  await page.getByTestId("open-agents-view").click();
  const managedAgentRow = page
    .locator('[data-testid^="managed-agent-"]')
    .filter({ hasText: agentName });
  await expect(managedAgentRow).toHaveCount(1);

  const managedAgentTestId = await managedAgentRow
    .first()
    .getAttribute("data-testid");
  if (!managedAgentTestId) {
    throw new Error("Managed agent row test id missing.");
  }
  const agentPubkey = managedAgentTestId.replace("managed-agent-", "");

  await page.getByTestId("channel-general").click();
  await openMembersSidebar(page, "general");

  const removeButton = page.getByTestId(`sidebar-remove-member-${agentPubkey}`);
  await expect(removeButton).toBeVisible();
  await removeButton.click();
  await expect(removeButton).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("members-sidebar")).not.toBeVisible();

  await page.getByTestId("open-agents-view").click();
  await expect(page.getByTestId(`managed-agent-${agentPubkey}`)).toHaveCount(0);

  const commands = await page.evaluate(() => {
    return (
      (
        window as Window & {
          __SPROUT_E2E_COMMANDS__?: string[];
        }
      ).__SPROUT_E2E_COMMANDS__ ?? []
    );
  });
  expect(commands).toContain("delete_managed_agent");
});

test("open channel management supports join and leave", async ({ page }) => {
  await page.goto("/");

  // Navigate to "design" (an unjoined channel) via the channel browser
  await page.getByTestId("browse-channels").click();
  await expect(page.getByTestId("channel-browser-dialog")).toBeVisible();
  await page
    .getByTestId("browse-channel-design")
    .getByRole("button", { name: "Join" })
    .click();
  await expect(page.getByTestId("chat-title")).toHaveText("design");

  // Open members sidebar — should show current user after joining
  await page.getByTestId("channel-members-trigger").click();
  await expect(page.getByTestId("members-sidebar")).toBeVisible();
  await expect(
    page.getByTestId(`sidebar-member-${MOCK_IDENTITY_PUBKEY}`),
  ).toContainText("You");
  await page.keyboard.press("Escape");

  // Open channel management — should show Leave since we just joined
  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
  await expect(page.getByTestId("channel-management-join")).toHaveCount(0);
  await expect(page.getByTestId("channel-management-leave")).toBeVisible();

  // Leave the channel
  await page.getByTestId("channel-management-leave").click();
  await expect(page.getByTestId("channel-management-sheet")).not.toBeVisible();

  // After leaving, the app navigates away — re-open browser and find design
  await page.getByTestId("browse-channels").click();
  await expect(page.getByTestId("channel-browser-dialog")).toBeVisible();

  // "design" should be back in the unjoined section with a Join button
  await expect(
    page
      .getByTestId("browse-channel-design")
      .getByRole("button", { name: "Join" }),
  ).toBeVisible();
});

test("manage channel can archive and unarchive a stream", async ({ page }) => {
  await page.goto("/");
  await openChannelManagement(page, "general");

  await page.getByTestId("channel-management-archive").click();
  await expect(page.getByTestId("channel-management-unarchive")).toBeVisible();

  await closeChannelManagement(page);
  await expect(page.getByTestId("stream-list")).not.toContainText("general");
  await expect(page.getByTestId("message-input")).toBeDisabled();
  await expect(page.getByTestId("send-message")).toBeDisabled();

  await page.getByTestId("browse-channels").click();
  await expect(page.getByTestId("channel-browser-dialog")).toBeVisible();
  await expect(page.getByTestId("browse-channel-general")).toContainText(
    "archived",
  );
  await page.getByTestId("browse-channel-general").click();
  await expect(page.getByTestId("channel-browser-dialog")).not.toBeVisible();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
  await page.getByTestId("channel-management-unarchive").click();
  await expect(page.getByTestId("channel-management-archive")).toBeVisible();

  await closeChannelManagement(page);
  await expect(page.getByTestId("stream-list")).toContainText("general");
  await expect(page.getByTestId("message-input")).toBeEnabled();
});

test("manage channel can delete an owned stream", async ({ page }) => {
  const channelName = `delete-me-${Date.now()}`;

  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page
    .getByTestId("create-stream-form")
    .getByRole("button", { name: "Create" })
    .click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);

  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
  await page.getByTestId("channel-management-delete").click();
  await expect(
    page.getByTestId("channel-delete-confirmation-dialog"),
  ).toBeVisible();
  await page.getByTestId("channel-delete-confirm").click();

  await expect(page.getByTestId("chat-title")).toHaveText("Home");
  await expect(page.getByTestId("stream-list")).not.toContainText(channelName);
});

test("canceling channel deletion keeps the owned stream", async ({ page }) => {
  const channelName = `keep-me-${Date.now()}`;

  await page.goto("/");
  await page.getByRole("button", { name: "Create a stream" }).click();
  await page.getByTestId("create-stream-name").fill(channelName);
  await page
    .getByTestId("create-stream-form")
    .getByRole("button", { name: "Create" })
    .click();
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);

  await page.getByTestId("channel-management-trigger").click();
  await expect(page.getByTestId("channel-management-sheet")).toBeVisible();
  await page.getByTestId("channel-management-delete").click();
  await expect(
    page.getByTestId("channel-delete-confirmation-dialog"),
  ).toBeVisible();
  await page.getByTestId("channel-delete-cancel").click();

  await expect(
    page.getByTestId("channel-delete-confirmation-dialog"),
  ).not.toBeVisible();
  await expect(page.getByTestId("chat-title")).toHaveText(channelName);
  await expect(page.getByTestId("stream-list")).toContainText(channelName);
});
