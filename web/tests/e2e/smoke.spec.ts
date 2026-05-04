import { expect, test } from "@playwright/test";

test("home page loads with Sprout heading", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("header")).toContainText("Sprout");
});

test("relay URL is visible", async ({ page }) => {
  await page.goto("/");
  const relayUrl = page.getByTestId("relay-url");
  await expect(relayUrl).toBeVisible();
  await expect(relayUrl).toContainText("ws://");
});
