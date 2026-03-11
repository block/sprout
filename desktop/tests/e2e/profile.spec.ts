import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

test("updates the relay-backed profile from the sidebar", async ({ page }) => {
  const stamp = Date.now();
  const displayName = `Tyler QA ${stamp}`;
  const avatarUrl = `https://example.com/avatar-${stamp}.png`;
  const about = `Coordinating relay profile setup ${stamp}`;

  await page.goto("/");

  await page.getByTestId("open-profile").click();
  await expect(page.getByTestId("profile-sheet")).toBeVisible();

  await expect(page.getByTestId("profile-pubkey")).toContainText("deadbeef");
  await expect(page.getByTestId("profile-nip05")).toContainText("Not set");

  await page.getByTestId("profile-display-name").fill(displayName);
  await page.getByTestId("profile-avatar-url").fill(avatarUrl);
  await page.getByTestId("profile-about").fill(about);
  await page.getByTestId("profile-save").click();

  await expect(page.getByTestId("profile-display-name")).toHaveValue(
    displayName,
  );
  await expect(page.getByTestId("profile-avatar-url")).toHaveValue(avatarUrl);
  await expect(page.getByTestId("profile-about")).toHaveValue(about);

  await page.keyboard.press("Escape");
  await expect(page.getByTestId("profile-sheet")).not.toBeVisible();

  await page.getByTestId("open-profile").click();
  await expect(page.getByTestId("profile-sheet")).toBeVisible();
  await expect(page.getByTestId("profile-display-name")).toHaveValue(
    displayName,
  );
  await expect(page.getByTestId("profile-avatar-url")).toHaveValue(avatarUrl);
  await expect(page.getByTestId("profile-about")).toHaveValue(about);
});
