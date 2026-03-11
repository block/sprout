import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

test("updates the relay-backed profile from settings", async ({ page }) => {
  const stamp = Date.now();
  const displayName = `Tyler QA ${stamp}`;
  const avatarUrl = `https://example.com/avatar-${stamp}.png`;
  const about = `Coordinating relay profile setup ${stamp}`;
  const nip05Handle = `tyler-${stamp}@localhost`;

  await page.goto("/");

  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("settings-view")).toBeVisible();
  await expect(page.getByTestId("chat-title")).toHaveText("Settings");
  await expect(page.getByTestId("open-settings")).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  await expect(page.getByTestId("profile-pubkey")).toContainText("deadbeef");
  await expect(page.getByTestId("profile-nip05")).toContainText("Not set");

  await page.getByTestId("profile-display-name").fill(displayName);
  await page.getByTestId("profile-nip05-input").fill(nip05Handle);
  await page.getByTestId("profile-avatar-url").fill(avatarUrl);
  await page.getByTestId("profile-about").fill(about);
  await page.getByTestId("profile-save").click();

  await expect(page.getByTestId("profile-display-name")).toHaveValue(
    displayName,
  );
  await expect(page.getByTestId("profile-nip05")).toContainText(nip05Handle);
  await expect(page.getByTestId("profile-nip05-input")).toHaveValue(
    nip05Handle,
  );
  await expect(page.getByTestId("profile-avatar-url")).toHaveValue(avatarUrl);
  await expect(page.getByTestId("profile-about")).toHaveValue(about);

  await page.getByRole("button", { name: "Home" }).click();
  await expect(page.getByTestId("chat-title")).toHaveText("Home");
  await expect(page.getByTestId("open-settings")).toHaveAttribute(
    "aria-pressed",
    "false",
  );

  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("settings-view")).toBeVisible();
  await expect(page.getByTestId("profile-display-name")).toHaveValue(
    displayName,
  );
  await expect(page.getByTestId("profile-nip05")).toContainText(nip05Handle);
  await expect(page.getByTestId("profile-nip05-input")).toHaveValue(
    nip05Handle,
  );
  await expect(page.getByTestId("profile-avatar-url")).toHaveValue(avatarUrl);
  await expect(page.getByTestId("profile-about")).toHaveValue(about);
});

test("updates presence from settings", async ({ page }) => {
  await page.goto("/");

  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("settings-view")).toBeVisible();
  await expect(page.getByTestId("presence-current-status")).toContainText(
    "Offline",
  );

  await page.getByTestId("presence-option-away").click();
  await expect(page.getByTestId("presence-current-status")).toContainText(
    "Away",
  );

  await page.getByRole("button", { name: "Home" }).click();
  await expect(page.getByTestId("chat-title")).toHaveText("Home");

  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("presence-current-status")).toContainText(
    "Away",
  );

  await page.getByTestId("presence-option-offline").click();
  await expect(page.getByTestId("presence-current-status")).toContainText(
    "Offline",
  );
});

test("opens settings with the keyboard shortcut and updates theme", async ({
  page,
}) => {
  await page.goto("/");

  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+," : "Control+,",
  );

  await expect(page.getByTestId("settings-view")).toBeVisible();
  await page.getByTestId("theme-option-dark").click();

  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.classList.contains("dark")),
    )
    .toBe(true);
});
