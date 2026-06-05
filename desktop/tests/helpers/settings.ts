import { expect, type Page } from "@playwright/test";

type SettingsSection =
  | "profile"
  | "notifications"
  | "agents"
  | "channel-templates"
  | "compute"
  | "appearance"
  | "shortcuts"
  | "tokens"
  | "relay-members"
  | "mobile"
  | "updates"
  | "doctor";

export async function openProfileMenu(page: Page) {
  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("profile-popover")).toBeVisible();
}

export async function openSettings(page: Page, section?: SettingsSection) {
  await openProfileMenu(page);
  if (section === "profile") {
    await page.getByTestId("profile-popover-profile").click();
  } else {
    await page.getByTestId("profile-popover-settings").click();
  }
  await expect(page.getByTestId("settings-view")).toBeVisible();

  if (section && section !== "profile") {
    await page.getByTestId(`settings-nav-${section}`).click();
  }
}
