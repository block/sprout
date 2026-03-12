import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

/** Locator scoped to the mention autocomplete dropdown inside the composer. */
function autocomplete(page: import("@playwright/test").Page) {
  return page
    .getByTestId("message-composer")
    .locator(".rounded-xl.border.bg-popover");
}

test("@ trigger shows autocomplete dropdown with channel members", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill("@");

  const dropdown = autocomplete(page);
  await expect(dropdown).toBeVisible();
  await expect(dropdown.getByText("alice")).toBeVisible();
  await expect(dropdown.getByText("bob")).toBeVisible();
});

test("autocomplete filters suggestions as user types", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill("@ali");

  const dropdown = autocomplete(page);
  await expect(dropdown.getByText("alice")).toBeVisible();
  await expect(dropdown.getByText("bob")).not.toBeVisible();
});

test("selecting a mention inserts @Name into input", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill("Hey @ali");

  const dropdown = autocomplete(page);
  await dropdown.getByText("alice").click();

  await expect(input).toHaveValue("Hey @alice ");
});

test("keyboard navigation selects mention with Enter", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill("@ali");

  const dropdown = autocomplete(page);
  await expect(dropdown.getByText("alice")).toBeVisible();

  // Press Enter to select the first (and only) suggestion
  await input.press("Enter");

  // Should insert @alice and NOT send the message
  await expect(input).toHaveValue("@alice ");
});

test("Escape dismisses autocomplete dropdown", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill("@");

  const dropdown = autocomplete(page);
  await expect(dropdown).toBeVisible();

  await input.press("Escape");

  await expect(dropdown).not.toBeVisible();
});

test("mention text is highlighted in sent messages", async ({ page }) => {
  const message = `Hey @alice check this out ${Date.now()}`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill(message);
  await page.getByTestId("send-message").click();

  // The mention should render inside a styled span with the mention class
  const mentionSpan = page
    .getByTestId("message-row")
    .last()
    .locator("span.text-primary", { hasText: "@alice" });
  await expect(mentionSpan).toBeVisible();
});

test("clicking author name opens user profile popover", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  // The seed message in general is from the mock identity (npub1mock...)
  const firstMessage = page.getByTestId("message-row").first();
  const authorButton = firstMessage.locator("button", {
    hasText: "npub1mock...",
  });
  await authorButton.click();

  const popover = page.locator("[data-radix-popper-content-wrapper]");
  await expect(popover).toBeVisible();
  await expect(popover).toContainText("deadbeef");
});

test("clicking avatar opens user profile popover", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  // Click the avatar button on the first message
  const firstMessage = page.getByTestId("message-row").first();
  const avatarButton = firstMessage.locator("button").first();
  await avatarButton.click();

  await expect(
    page.locator("[data-radix-popper-content-wrapper]"),
  ).toBeVisible();
});
