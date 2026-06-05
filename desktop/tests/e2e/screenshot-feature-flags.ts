import { expect, test } from "@playwright/test";
import { installMockBridge } from "../helpers/bridge";
import { openSettings } from "../helpers/settings";

const WATERCOLOR_CHANNEL_ID = "a27e1ee9-76a6-5bdf-a5d5-1d85610dad11";
const FORUM_POST_ID = "mock-forum-release-thread";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

// Helper: enable a feature via settings, then close settings
async function enableFeature(page: import("@playwright/test").Page, featureId: string) {
  await openSettings(page);
  await page.getByTestId("settings-nav-experimental").click();
  await expect(page.getByTestId("settings-experimental")).toBeVisible();
  await page.getByTestId(`feature-toggle-${featureId}`).click();
  await page.waitForTimeout(200);
  await page.getByTestId("settings-close").click();
  await page.waitForTimeout(300);
}

// --- Stable features (settings panels) ---

test("screenshot: Settings → Agents (managed-agents, stable)", async ({ page }) => {
  await page.goto("/");
  await openSettings(page, "agents");
  await page.waitForTimeout(500);
  const view = page.getByTestId("settings-view");
  await view.screenshot({ path: "tests/e2e/screenshots/settings-agents.png" });
});

test("screenshot: Settings → Templates (channel-templates, stable)", async ({ page }) => {
  await page.goto("/");
  await openSettings(page, "channel-templates");
  await page.waitForTimeout(500);
  const view = page.getByTestId("settings-view");
  await view.screenshot({ path: "tests/e2e/screenshots/settings-templates.png" });
});

test("screenshot: Settings → Custom Emoji (custom-emoji, stable)", async ({ page }) => {
  await page.goto("/");
  await openSettings(page);
  await page.getByTestId("settings-nav-custom-emoji").click();
  await page.waitForTimeout(500);
  const view = page.getByTestId("settings-view");
  await view.screenshot({ path: "tests/e2e/screenshots/settings-custom-emoji.png" });
});

test("screenshot: Settings → Doctor (doctor, stable)", async ({ page }) => {
  await page.goto("/");
  await openSettings(page, "doctor");
  await page.waitForTimeout(500);
  const view = page.getByTestId("settings-view");
  await view.screenshot({ path: "tests/e2e/screenshots/settings-doctor.png" });
});

// --- Preview features (navigated into their views) ---

test("screenshot: Workflows view (workflows, preview)", async ({ page }) => {
  await page.goto("/");
  await enableFeature(page, "workflows");
  await page.getByTestId("open-workflows-view").click();
  await page.waitForTimeout(500);
  await page.screenshot({ path: "tests/e2e/screenshots/view-workflows.png", fullPage: false });
});

test("screenshot: Projects view (projects, preview)", async ({ page }) => {
  await page.goto("/");
  await enableFeature(page, "projects");
  await page.getByTestId("open-projects-view").click();
  await page.waitForTimeout(500);
  await page.screenshot({ path: "tests/e2e/screenshots/view-projects.png", fullPage: false });
});

test("screenshot: Pulse view (pulse, preview)", async ({ page }) => {
  await page.goto("/");
  await enableFeature(page, "pulse");
  await page.getByTestId("open-pulse-view").click();
  await page.waitForTimeout(500);
  await page.screenshot({ path: "tests/e2e/screenshots/view-pulse.png", fullPage: false });
});

test("screenshot: Forum with active thread (forum, preview)", async ({ page }) => {
  await page.goto("/");
  await enableFeature(page, "forum");
  // Navigate directly into the forum channel and open a thread
  await page.goto(`/#/channels/${WATERCOLOR_CHANNEL_ID}/posts/${FORUM_POST_ID}`);
  await expect(page.getByTestId("chat-title")).toHaveText("watercooler");
  await page.waitForTimeout(600);
  await page.screenshot({ path: "tests/e2e/screenshots/view-forum-thread.png", fullPage: false });
});

test("screenshot: Forum post list (forum, preview)", async ({ page }) => {
  await page.goto("/");
  await enableFeature(page, "forum");
  // Navigate to the forum channel post list
  await page.goto(`/#/channels/${WATERCOLOR_CHANNEL_ID}`);
  await expect(page.getByText("Release checklist: async feedback thread.")).toBeVisible();
  await page.waitForTimeout(500);
  await page.screenshot({ path: "tests/e2e/screenshots/view-forum-posts.png", fullPage: false });
});

test("screenshot: Huddles in channel header (huddles, preview)", async ({ page }) => {
  await page.goto("/");
  await enableFeature(page, "huddles");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await page.waitForTimeout(500);
  await page.screenshot({ path: "tests/e2e/screenshots/view-huddles.png", fullPage: false });
});

// --- Unstable features (settings panels after enabling) ---

test("screenshot: Settings → Compute (mesh-compute, unstable)", async ({ page }) => {
  await page.goto("/");
  await openSettings(page);
  await page.getByTestId("settings-nav-experimental").click();
  await page.getByTestId("feature-toggle-mesh-compute").click();
  await page.waitForTimeout(300);
  await page.getByTestId("settings-nav-compute").click();
  await page.waitForTimeout(500);
  const view = page.getByTestId("settings-view");
  await view.screenshot({ path: "tests/e2e/screenshots/settings-compute.png" });
});

test("screenshot: Settings → Relay Members (relay-members, unstable)", async ({ page }) => {
  await page.goto("/");
  await openSettings(page);
  await page.getByTestId("settings-nav-experimental").click();
  await page.getByTestId("feature-toggle-relay-members").click();
  await page.waitForTimeout(300);
  await page.getByTestId("settings-nav-relay-members").click();
  await page.waitForTimeout(500);
  const view = page.getByTestId("settings-view");
  await view.screenshot({ path: "tests/e2e/screenshots/settings-relay-members.png" });
});

// --- Settings → Preview & Unstable panel ---

test("screenshot: Settings → Preview & Unstable (default)", async ({ page }) => {
  await page.goto("/");
  await openSettings(page);
  await page.getByTestId("settings-nav-experimental").click();
  await expect(page.getByTestId("settings-experimental")).toBeVisible();
  await page.waitForTimeout(500);
  const view = page.getByTestId("settings-view");
  await view.screenshot({ path: "tests/e2e/screenshots/settings-preview-unstable-default.png" });
});

test("screenshot: Settings → Preview & Unstable (all on)", async ({ page }) => {
  await page.goto("/");
  await openSettings(page);
  await page.getByTestId("settings-nav-experimental").click();
  await expect(page.getByTestId("settings-experimental")).toBeVisible();
  await page.getByTestId("feature-toggle-workflows").click();
  await page.getByTestId("feature-toggle-projects").click();
  await page.getByTestId("feature-toggle-pulse").click();
  await page.getByTestId("feature-toggle-forum").click();
  await page.getByTestId("feature-toggle-huddles").click();
  await page.getByTestId("feature-toggle-mesh-compute").click();
  await page.getByTestId("feature-toggle-identity-archive").click();
  await page.getByTestId("feature-toggle-relay-members").click();
  await page.waitForTimeout(500);
  const view = page.getByTestId("settings-view");
  await view.screenshot({ path: "tests/e2e/screenshots/settings-preview-unstable-all-on.png" });
});
