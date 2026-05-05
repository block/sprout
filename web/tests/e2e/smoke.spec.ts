import { expect, test } from "@playwright/test";

test("home page loads with Sprout heading", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("header")).toContainText("Sprout");
});

test("home page shows repositories section", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Repositories")).toBeVisible();
});
