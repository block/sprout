import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

const ENGINEERING_CHANNEL_ID = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const WATERCOLOR_CHANNEL_ID = "a27e1ee9-76a6-5bdf-a5d5-1d85610dad11";
const FORUM_POST_ID = "mock-forum-release-thread";
const FORUM_REPLY_ID = "mock-forum-release-reply";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

async function navigateToWorkflows(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.getByTestId("open-workflows-view").click();
  await expect(page).toHaveURL(/#\/workflows$/);
  await expect(page.getByTestId("workflows-view")).toBeVisible();
}

async function createWorkflow(
  page: import("@playwright/test").Page,
  name: string,
) {
  await page.getByRole("button", { name: "Create Workflow" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Workflow name").fill(name);
  await dialog.getByRole("button", { name: "Add step" }).click();
  await dialog.getByRole("button", { name: "Create" }).click();
  await expect(dialog).not.toBeVisible();
}

test("global back and forward move across channel routes", async ({ page }) => {
  await page.goto("/");

  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");

  await page.getByTestId("global-back").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  await page.getByTestId("global-forward").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");
});

test("direct forum thread links close back to the forum route", async ({
  page,
}) => {
  await page.goto(
    `/#/channels/${WATERCOLOR_CHANNEL_ID}/posts/${FORUM_POST_ID}`,
  );

  await expect(page.getByTestId("chat-title")).toHaveText("watercooler");
  await expect(
    page.getByRole("button", { name: "Back to posts" }),
  ).toBeVisible();

  await page.getByRole("button", { name: "Back to posts" }).click();

  await expect(page).toHaveURL(
    /#\/channels\/a27e1ee9-76a6-5bdf-a5d5-1d85610dad11$/,
  );
  await expect(
    page.getByText("Release checklist: async feedback thread."),
  ).toBeVisible();
});

test("direct workflow detail links close back to workflows", async ({
  page,
}) => {
  const workflowName = `workflow_nav_${Date.now()}`;

  await navigateToWorkflows(page);
  await createWorkflow(page, workflowName);

  const workflowCard = page
    .locator('[data-testid^="workflow-card-"]')
    .filter({ hasText: workflowName })
    .first();
  const workflowTestId = await workflowCard.getAttribute("data-testid");
  const workflowId = workflowTestId?.replace("workflow-card-", "");

  expect(workflowId).toBeTruthy();

  await page.goto(`/#/workflows/${workflowId}`);

  await expect(page.getByTestId("workflow-detail-panel")).toBeVisible();
  await page.getByRole("button", { name: "Close detail panel" }).click();

  await expect(page).toHaveURL(/#\/workflows$/);
  await expect(page.getByTestId("workflows-view")).toBeVisible();
});

test("forum reply deep links survive reload", async ({ page }) => {
  await page.goto(
    `/#/channels/${WATERCOLOR_CHANNEL_ID}/posts/${FORUM_POST_ID}?replyId=${FORUM_REPLY_ID}`,
  );

  await expect(page.getByTestId("chat-title")).toHaveText("watercooler");
  await expect(
    page.getByText("Looks good to me. We should ship it."),
  ).toBeVisible();

  await page.reload();

  await expect(page.getByTestId("chat-title")).toHaveText("watercooler");
  await expect(
    page.getByText("Looks good to me. We should ship it."),
  ).toBeVisible();
});

test("message deep links survive reload", async ({ page }) => {
  await page.goto(
    `/#/channels/${ENGINEERING_CHANNEL_ID}?messageId=mock-engineering-shipped`,
  );

  await expect(page.getByTestId("chat-title")).toHaveText("engineering");
  await expect(page.getByTestId("message-timeline")).toContainText(
    "Engineering shipped the desktop build.",
  );

  await page.reload();

  await expect(page.getByTestId("chat-title")).toHaveText("engineering");
  await expect(page.getByTestId("message-timeline")).toContainText(
    "Engineering shipped the desktop build.",
  );
});
