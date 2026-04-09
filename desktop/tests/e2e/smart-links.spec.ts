import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

test("GitHub PR URL renders as an inline smart chip", async ({ page }) => {
  const prUrl = "https://github.com/block/goose2/pull/125";
  const message = `Check out ${prUrl} for the fix`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill(message);
  await page.getByTestId("send-message").click();

  const lastRow = page.getByTestId("message-row").last();

  // The PR link should render as a styled chip with the repo and PR number
  const prChip = lastRow.locator("a", { hasText: "block/goose2#125" });
  await expect(prChip).toBeVisible();
  await expect(prChip).toHaveAttribute("href", prUrl);

  // Should contain the GitPullRequest icon (rendered as an SVG)
  await expect(prChip.locator("svg")).toBeVisible();
});

test("GitHub PR chip links open in new tab", async ({ page }) => {
  const prUrl = "https://github.com/block/sprout/pull/42";

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill(prUrl);
  await page.getByTestId("send-message").click();

  const lastRow = page.getByTestId("message-row").last();
  const prChip = lastRow.locator("a", { hasText: "block/sprout#42" });
  await expect(prChip).toHaveAttribute("target", "_blank");
  await expect(prChip).toHaveAttribute("rel", "noreferrer");
});

test("non-PR GitHub links render as regular links", async ({ page }) => {
  const repoUrl = "https://github.com/block/sprout";
  const message = `Check out ${repoUrl}`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill(message);
  await page.getByTestId("send-message").click();

  const lastRow = page.getByTestId("message-row").last();

  // Should render as a normal underlined link, not a chip
  const link = lastRow.locator("a", { hasText: repoUrl });
  await expect(link).toBeVisible();
  // Regular links have underline styling, not the chip background
  await expect(link.locator("svg")).not.toBeVisible();
});
