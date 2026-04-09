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

test("selecting a PR chip copies the full URL, not the chip label", async ({
  page,
}) => {
  const prUrl = "https://github.com/block/goose2/pull/125";

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill(prUrl);
  await page.getByTestId("send-message").click();

  const lastRow = page.getByTestId("message-row").last();
  const prChip = lastRow.locator("a", { hasText: "block/goose2#125" });
  await expect(prChip).toBeVisible();

  // The hidden span should contain the full URL for selection/copy
  const hiddenUrl = prChip.locator("span.overflow-hidden");
  await expect(hiddenUrl).toHaveText(prUrl);

  // The visible label should not be selectable
  const visibleLabel = prChip.locator("span.select-none");
  await expect(visibleLabel).toBeVisible();
});

test("GitHub issue URL renders as an inline smart chip", async ({ page }) => {
  const issueUrl = "https://github.com/block/sprout/issues/99";
  const message = `See ${issueUrl} for context`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill(message);
  await page.getByTestId("send-message").click();

  const lastRow = page.getByTestId("message-row").last();

  const issueChip = lastRow.locator("a", { hasText: "block/sprout#99" });
  await expect(issueChip).toBeVisible();
  await expect(issueChip).toHaveAttribute("href", issueUrl);
  await expect(issueChip.locator("svg")).toBeVisible();
});

test("GitHub commit URL renders as an inline smart chip", async ({ page }) => {
  const commitUrl =
    "https://github.com/block/sprout/commit/abc1234def5678901234567890abcdef12345678";
  const message = `Reverted in ${commitUrl}`;

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  const input = page.getByTestId("message-input");
  await input.fill(message);
  await page.getByTestId("send-message").click();

  const lastRow = page.getByTestId("message-row").last();

  // Should show short SHA
  const commitChip = lastRow.locator("a", {
    hasText: "block/sprout@abc1234",
  });
  await expect(commitChip).toBeVisible();
  await expect(commitChip).toHaveAttribute("href", commitUrl);
  await expect(commitChip.locator("svg")).toBeVisible();
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
