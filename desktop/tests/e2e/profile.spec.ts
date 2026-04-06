import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";
import { openProfileMenu, openSettings } from "../helpers/settings";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

test("updates the relay-backed profile from settings", async ({ page }) => {
  const stamp = Date.now();
  const displayName = `Tyler QA ${stamp}`;
  const avatarUrl = `https://example.com/avatar-${stamp}.png`;
  const about = `Coordinating relay profile setup ${stamp}`;
  await page.goto("/");

  await openSettings(page, "profile");
  await expect(page.getByTestId("settings-title")).toHaveText("Settings");

  await expect(page.getByTestId("profile-pubkey")).toContainText("deadbeef");
  await expect(page.getByTestId("profile-nip05")).toContainText("Not set");

  await page.getByTestId("profile-display-name").fill(displayName);
  await page.getByTestId("profile-avatar-url").fill(avatarUrl);
  await page.getByTestId("profile-about").fill(about);
  await page.getByTestId("profile-save").click();

  await expect(page.getByTestId("profile-display-name")).toHaveValue(
    displayName,
  );
  await expect(page.getByTestId("profile-nip05")).toContainText("Not set");
  await expect(page.getByTestId("profile-avatar-url")).toHaveValue(avatarUrl);
  await expect(page.getByTestId("profile-about")).toHaveValue(about);

  await page.getByTestId("settings-close").click();
  await expect(page.getByTestId("chat-title")).toHaveText("Home");
  await expect(page.getByTestId("open-settings")).toBeVisible();

  await openSettings(page, "profile");
  await expect(page.getByTestId("profile-display-name")).toHaveValue(
    displayName,
  );
  await expect(page.getByTestId("profile-nip05")).toContainText("Not set");
  await expect(page.getByTestId("profile-avatar-url")).toHaveValue(avatarUrl);
  await expect(page.getByTestId("profile-about")).toHaveValue(about);
});

test("updates presence from the profile menu", async ({ page }) => {
  await page.goto("/");

  await openProfileMenu(page);
  await expect(
    page.getByTestId("profile-popover-current-status"),
  ).toContainText("Offline");

  await page.getByTestId("profile-popover-status-away").click();
  await openProfileMenu(page);
  await expect(
    page.getByTestId("profile-popover-current-status"),
  ).toContainText("Away");

  await page.getByTestId("profile-popover-status-offline").click();
  await openProfileMenu(page);
  await expect(
    page.getByTestId("profile-popover-current-status"),
  ).toContainText("Offline");
});

test("notification settings drive the Home badge and desktop alerts", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByTestId("sidebar-home-count")).toHaveCount(0);

  await openSettings(page, "notifications");
  await expect(page.getByTestId("settings-notifications")).toBeVisible();
  await expect(page.getByTestId("notifications-desktop-state")).toContainText(
    "Off",
  );

  await page.getByTestId("notifications-desktop-toggle").click();
  await expect(page.getByTestId("notifications-desktop-state")).toContainText(
    "On",
  );
  await page.getByTestId("settings-close").click();
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.evaluate(() => {
    const win = window as Window & {
      __SPROUT_E2E_PUSH_MOCK_FEED_ITEM__?: (item: {
        category: "mention" | "needs_action" | "activity" | "agent_activity";
        channel_id: string | null;
        channel_name: string;
        content: string;
        created_at: number;
        id: string;
        kind: number;
        pubkey: string;
        tags: string[][];
      }) => unknown;
    };

    win.__SPROUT_E2E_PUSH_MOCK_FEED_ITEM__?.({
      category: "mention",
      channel_id: "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9",
      channel_name: "engineering",
      content: "Please review the rollout checklist.",
      created_at: Math.floor(Date.now() / 1000) + 5,
      id: `mock-feed-notification-${Date.now()}`,
      kind: 9,
      pubkey:
        "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260",
      tags: [
        ["e", "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9"],
        [
          "p",
          "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        ],
      ],
    });
  });

  await expect(page.getByTestId("sidebar-home-count")).toHaveText("1");

  await expect
    .poll(() =>
      page.evaluate(() => {
        const win = window as Window & {
          __SPROUT_E2E_NOTIFICATIONS__?: Array<{
            body: string | null;
            title: string;
          }>;
        };

        return win.__SPROUT_E2E_NOTIFICATIONS__?.length ?? 0;
      }),
    )
    .toBe(1);

  const notifications = await page.evaluate(() => {
    const win = window as Window & {
      __SPROUT_E2E_NOTIFICATIONS__?: Array<{
        body: string | null;
        title: string;
      }>;
    };

    return win.__SPROUT_E2E_NOTIFICATIONS__ ?? [];
  });

  expect(notifications).toEqual([
    {
      body: "Please review the rollout checklist.",
      title: "@Mention in #engineering",
    },
  ]);

  await openSettings(page, "notifications");
  await page.getByTestId("notifications-home-badge-toggle").click();
  await page.getByTestId("settings-close").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("sidebar-home-count")).toHaveCount(0);

  await openSettings(page, "notifications");
  await page.getByTestId("notifications-home-badge-toggle").click();
  await page.getByTestId("settings-close").click();
  await expect(page.getByTestId("sidebar-home-count")).toHaveText("1");

  await page.getByRole("button", { name: "Home" }).click();
  await expect(page.getByTestId("chat-title")).toHaveText("Home");
  await expect(page.getByTestId("sidebar-home-count")).toHaveCount(0);
});

test("opens settings with the keyboard shortcut and updates theme", async ({
  page,
}) => {
  await page.goto("/");

  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+," : "Control+,",
  );

  await expect(page.getByTestId("settings-view")).toBeVisible();
  await page.getByTestId("settings-nav-appearance").click();
  await page.getByTestId("theme-option-dracula").click();

  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.classList.contains("dark")),
    )
    .toBe(true);

  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+," : "Control+,",
  );
  await expect(page.getByTestId("settings-view")).toHaveCount(0);
  await expect(page.getByTestId("chat-title")).toHaveText("Home");
});

test("supports webview zoom keyboard shortcuts", async ({ page }) => {
  await page.goto("/");

  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+Shift+Equal" : "Control+Shift+Equal",
  );

  await expect
    .poll(() => page.evaluate(() => window.__SPROUT_E2E_WEBVIEW_ZOOM__))
    .toBe(1.2);

  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+Minus" : "Control+Minus",
  );

  await expect
    .poll(() => page.evaluate(() => window.__SPROUT_E2E_WEBVIEW_ZOOM__))
    .toBe(1);

  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+Shift+Equal" : "Control+Shift+Equal",
  );
  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+Shift+Equal" : "Control+Shift+Equal",
  );

  await expect
    .poll(() => page.evaluate(() => window.__SPROUT_E2E_WEBVIEW_ZOOM__))
    .toBe(1.4);

  await page.keyboard.press(
    process.platform === "darwin" ? "Meta+Digit0" : "Control+Digit0",
  );

  await expect
    .poll(() => page.evaluate(() => window.__SPROUT_E2E_WEBVIEW_ZOOM__))
    .toBe(1);
});

test("shows doctor checks for local sprout tooling", async ({ page }) => {
  await page.goto("/");

  await openSettings(page, "doctor");

  await expect(page.getByTestId("settings-doctor")).toBeVisible();
  await expect(page.getByTestId("doctor-check-admin")).toContainText(
    "sprout-admin",
  );
  await expect(page.getByTestId("doctor-check-acp")).toContainText(
    "sprout-acp",
  );
  await expect(page.getByTestId("doctor-check-mcp")).toContainText(
    "sprout-mcp-server",
  );
  await expect(page.getByTestId("doctor-provider-goose")).toContainText(
    "Goose",
  );
});
