import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";
import { openSettings } from "../helpers/settings";

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
  await expect(page.getByTestId("message-row").last()).toContainText(
    "npub1mock...",
  );
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

test("emoji picker inserts emoji into the draft and keeps focus in the composer", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill("Ship");

  await page.getByTestId("composer-emoji-button").click();

  // emoji-mart renders inside a Shadow DOM web component — use the search
  // input to find the rocket emoji, then click it.
  const pickerEl = page.locator("em-emoji-picker");
  const searchInput = pickerEl.locator("input[type='search']");
  await searchInput.fill("rocket");
  await pickerEl.locator("button[aria-label='🚀']").first().click();

  await expect(input).toHaveValue("Ship🚀");
  await expect(input).toBeFocused();

  await input.pressSequentially(" now");
  await expect(input).toHaveValue("Ship🚀 now");
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

test("draft is preserved when switching channels", async ({ page }) => {
  const draft = `Unsent draft ${Date.now()}`;
  const input = page.getByTestId("message-input");

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  // Type a draft but do not send it
  await input.fill(draft);
  await expect(input).toHaveValue(draft);

  // Switch to another channel — composer should be empty
  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");
  await expect(input).toHaveValue("");

  // Switch back — the draft should still be there
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(input).toHaveValue(draft);
});

test("sending a message clears the draft", async ({ page }) => {
  const message = `Sent message ${Date.now()}`;
  const input = page.getByTestId("message-input");

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  // Type and send a message
  await input.fill(message);
  await page.getByTestId("send-message").click();
  await expect(page.getByTestId("message-timeline")).toContainText(message);

  // Switch away and back — composer should be empty, not restored from draft
  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(input).toHaveValue("");
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

test("shows your avatar on your own message when profile avatar is set", async ({
  page,
}) => {
  const message = `Avatar message ${Date.now()}`;
  const avatarUrl =
    'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"%3E%3Crect width="16" height="16" rx="4" fill="%2300a36c"/%3E%3C/svg%3E';

  await page.goto("/");
  await openSettings(page, "profile");
  await page.getByTestId("profile-avatar-url").fill(avatarUrl);
  await page.getByTestId("profile-save").click();
  await page.getByTestId("settings-close").click();

  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.getByTestId("message-input").fill(message);
  await page.getByTestId("send-message").click();

  const lastMessage = page.getByTestId("message-row").last();
  await expect(lastMessage).toContainText(message);
  await expect(lastMessage.getByTestId("message-avatar-image")).toHaveAttribute(
    "src",
    avatarUrl,
  );
});

test("opens a branch-only thread panel from the reply action", async ({
  page,
}) => {
  const firstReply = `First threaded reply ${Date.now()}`;
  const siblingReply = `Sibling threaded reply ${Date.now()}`;
  const nestedReply = `Nested threaded reply ${Date.now()}`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("message-timeline")).toContainText(
    "Welcome to #general",
  );

  const timeline = page.getByTestId("message-timeline");
  const timelineRows = timeline.getByTestId("message-row");
  const threadPanel = page.getByTestId("message-thread-panel");
  const threadComposer = threadPanel.locator('[data-testid="message-input"]');
  const threadSendButton = threadPanel.getByTestId("send-message");
  const threadReplies = threadPanel.getByTestId("message-thread-replies");
  const rootMessage = timelineRows.first();

  await rootMessage.hover();
  await rootMessage.getByRole("button", { name: "Reply" }).click();
  await expect(threadPanel).toBeVisible();
  await expect(threadPanel.getByTestId("message-thread-head")).toContainText(
    "Welcome to #general",
  );

  await threadComposer.fill(firstReply);
  await threadSendButton.click();
  await expect(threadReplies).toContainText(firstReply);

  await threadComposer.fill(siblingReply);
  await threadSendButton.click();
  await expect(threadReplies).toContainText(siblingReply);

  await expect(
    timeline.getByTestId("message-row").filter({ hasText: firstReply }),
  ).toHaveCount(0);
  await expect(
    timeline.getByTestId("message-row").filter({ hasText: siblingReply }),
  ).toHaveCount(0);

  const rootSummaryRow = timeline.getByTestId("message-thread-summary").first();
  await expect(rootSummaryRow).toContainText("2 replies");

  await threadPanel.getByTestId("message-thread-close").click();
  await expect(threadPanel).toBeHidden();

  await rootSummaryRow.click();
  await expect(threadPanel).toBeVisible();
  await expect(threadPanel.getByTestId("message-thread-head")).toContainText(
    "Welcome to #general",
  );

  const firstReplyRow = threadReplies
    .getByTestId("message-row")
    .filter({ hasText: firstReply })
    .first();
  await firstReplyRow.hover();
  await firstReplyRow.getByRole("button", { name: "Reply" }).click();

  await expect(threadPanel.getByTestId("message-thread-back")).toBeVisible();
  await expect(threadPanel.getByTestId("message-thread-head")).toContainText(
    firstReply,
  );
  await expect(
    threadPanel.getByTestId("message-thread-head"),
  ).not.toContainText("Welcome to #general");
  await expect(threadReplies).not.toContainText(siblingReply);

  await threadComposer.fill(nestedReply);
  await threadSendButton.click();

  await expect(threadReplies).toContainText(nestedReply);
  await expect(threadReplies).not.toContainText(siblingReply);
  await expect(
    timeline.getByTestId("message-row").filter({ hasText: nestedReply }),
  ).toHaveCount(0);

  await threadPanel.getByTestId("message-thread-back").click();
  await expect(threadPanel.getByTestId("message-thread-head")).toContainText(
    "Welcome to #general",
  );
  await expect(
    threadReplies.getByTestId("message-row").filter({ hasText: nestedReply }),
  ).toHaveCount(0);

  const nestedSummaryRow = threadReplies.getByTestId("message-thread-summary");
  await expect(nestedSummaryRow).toContainText("1 reply");
  await nestedSummaryRow.click();

  await expect(threadPanel.getByTestId("message-thread-head")).toContainText(
    firstReply,
  );
  await expect(threadReplies).toContainText(nestedReply);
});
