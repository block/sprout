import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

async function navigateToWorkflows(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.getByTestId("open-workflows-view").click();
  await expect(page.getByTestId("workflows-view")).toBeVisible();
}

/** Matches UI humanizeWorkflowTitle for typical test_* snake_case names */
function displayWorkflowTitle(storedName: string): string {
  const name = storedName.trim();
  if (!name) {
    return name;
  }
  if (name.length > 100) {
    return `${name.slice(0, 99)}…`;
  }
  if (name.includes(" ") || !name.includes("_")) {
    return name;
  }
  return name
    .split("_")
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join(" ");
}

function defaultWorkflowPrompt(
  name: string,
  trigger: string | undefined,
): string {
  switch (trigger) {
    case "webhook":
      return "When an incoming webhook arrives, send a message to the channel.";
    case "diff_posted":
      return `When a diff mentions "${name}", send a message to the channel.`;
    case "reaction_added":
      return "When someone adds a :thumbsup: reaction, send a message.";
    case "schedule":
      return "Every hour, send a message to the channel.";
    default:
      return `When someone posts "${name}", send a message to the channel.`;
  }
}

async function createWorkflow(
  page: import("@playwright/test").Page,
  name: string,
  options?: {
    description?: string;
    enabled?: boolean;
    prompt?: string;
    trigger?: string;
    stepCondition?: string;
    stepName?: string;
    stepTimeoutSecs?: string;
  },
) {
  await page.getByRole("button", { name: "Create Workflow" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();

  await dialog
    .getByLabel("Describe the workflow")
    .fill(options?.prompt ?? defaultWorkflowPrompt(name, options?.trigger));
  await dialog.getByRole("button", { name: "Draft workflow" }).click();
  await expect(dialog.getByLabel("Workflow title")).toBeVisible();

  await dialog.getByLabel("Workflow title").clear();
  await dialog.getByLabel("Workflow title").fill(name);
  if (options?.description) {
    await dialog.getByLabel("Description (optional)").fill(options.description);
  }
  if (options?.enabled === false) {
    await dialog.getByLabel("Workflow is enabled").click();
  }
  if (options?.trigger) {
    await dialog.getByLabel("Trigger").selectOption(options.trigger);
  }

  if (options?.stepName) {
    await dialog.getByLabel("Step name (optional)").fill(options.stepName);
  }
  if (options?.stepCondition) {
    await dialog
      .getByLabel("Run condition (optional)")
      .fill(options.stepCondition);
  }
  if (options?.stepTimeoutSecs) {
    await dialog
      .getByLabel("Timeout seconds (optional)")
      .fill(options.stepTimeoutSecs);
  }

  await dialog.getByRole("button", { name: "Create" }).click();

  await expect(
    page.getByRole("heading", { name: "Create Workflow" }),
  ).not.toBeVisible();
}

test("navigates to workflows view and shows empty state", async ({ page }) => {
  await navigateToWorkflows(page);

  await expect(page.getByText("No workflows yet")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Create your first workflow" }),
  ).toBeVisible();
});

test("creates a workflow via the form builder", async ({ page }) => {
  const workflowName = `test_workflow_${Date.now()}`;

  await navigateToWorkflows(page);
  await createWorkflow(page, workflowName);

  // Verify workflow appears in the list
  await expect(page.getByTestId("workflows-view")).toContainText(
    displayWorkflowTitle(workflowName),
  );
});

test("starts workflow creation with a prompt before showing setup", async ({
  page,
}) => {
  await navigateToWorkflows(page);

  await page.getByRole("button", { name: "Create Workflow" }).click();
  const dialog = page.getByRole("dialog");

  await expect(dialog.getByLabel("Describe the workflow")).toBeVisible();
  await expect(dialog.getByLabel("Workflow title")).not.toBeVisible();

  await dialog
    .getByLabel("Describe the workflow")
    .fill('When someone posts "deploy", send a message.');
  await dialog.getByRole("button", { name: "Draft workflow" }).click();

  await expect(dialog.getByLabel("Workflow title")).toBeVisible();
  await expect(dialog.getByText("Drafted from your prompt")).toBeVisible();
});

test("disables autocapitalization in the workflow form", async ({ page }) => {
  await navigateToWorkflows(page);

  await page.getByRole("button", { name: "Create Workflow" }).click();
  const dialog = page.getByRole("dialog");

  await dialog
    .getByLabel("Describe the workflow")
    .fill('When someone posts "deploy", send a message.');
  await dialog.getByRole("button", { name: "Draft workflow" }).click();

  await expect(dialog.getByLabel("Workflow title")).toHaveAttribute(
    "autocapitalize",
    "off",
  );

  await expect(dialog.getByLabel("Step name (optional)")).toHaveAttribute(
    "autocapitalize",
    "off",
  );
});

test("captures disabled diff workflows in the list UI", async ({ page }) => {
  const workflowName = `diff_workflow_${Date.now()}`;
  const description = "Watches diff events for src/ changes";

  await navigateToWorkflows(page);
  await createWorkflow(page, workflowName, {
    description,
    enabled: false,
    trigger: "diff_posted",
    stepName: "Notify reviewers",
    stepCondition: 'str_contains(trigger_text, "src/")',
    stepTimeoutSecs: "45",
  });

  const card = page
    .locator('[data-testid^="workflow-card-"]')
    .filter({ hasText: displayWorkflowTitle(workflowName) })
    .first();
  await expect(card).toContainText(displayWorkflowTitle(workflowName));
  await expect(card).toContainText(description);
  await expect(card).toContainText("Diff Posted");
  await expect(card).toContainText("disabled");
});

test("shows the webhook secret dialog after saving a webhook workflow", async ({
  page,
}) => {
  const workflowName = `webhook_workflow_${Date.now()}`;

  await navigateToWorkflows(page);
  await createWorkflow(page, workflowName, {
    trigger: "webhook",
  });

  await expect(page.getByText("Webhook Ready")).toBeVisible();
  await expect(page.getByRole("button", { name: "Copy URL" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Copy Secret" })).toBeVisible();

  await page.getByRole("button", { name: "Close" }).click();
  await expect(page.getByText("Webhook Ready")).not.toBeVisible();
});

test("edits an existing workflow", async ({ page }) => {
  const originalName = `edit_test_${Date.now()}`;
  const updatedName = `${originalName}_updated`;

  await navigateToWorkflows(page);
  await createWorkflow(page, originalName);

  // Verify it exists
  await expect(page.getByTestId("workflows-view")).toContainText(
    displayWorkflowTitle(originalName),
  );

  // Open the dropdown menu and click Edit
  await page.getByRole("button", { name: "Workflow actions" }).first().click();
  await page.getByRole("menuitem", { name: "Edit" }).click();

  // Dialog should open in edit mode
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByText("Edit Workflow")).toBeVisible();

  // Change the name
  const nameInput = page.getByLabel("Workflow title");
  await nameInput.clear();
  await nameInput.fill(updatedName);

  // Save
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("dialog")).not.toBeVisible();

  // Verify the updated name appears
  await expect(page.getByTestId("workflows-view")).toContainText(
    displayWorkflowTitle(updatedName),
  );
});

test("duplicates a workflow", async ({ page }) => {
  const originalName = `dup_test_${Date.now()}`;

  await navigateToWorkflows(page);
  await createWorkflow(page, originalName);

  // Open the dropdown menu and click Duplicate
  await page.getByRole("button", { name: "Workflow actions" }).first().click();
  await page.getByRole("menuitem", { name: "Duplicate" }).click();

  // Dialog should open in duplicate mode with "(copy)" suffix
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByText("Duplicate Workflow")).toBeVisible();

  // Submit the duplicate
  await page.getByRole("button", { name: "Create Copy" }).click();
  await expect(page.getByRole("dialog")).not.toBeVisible();

  // Both the original and copy should exist
  await expect(page.getByTestId("workflows-view")).toContainText(
    displayWorkflowTitle(originalName),
  );
  await expect(page.getByTestId("workflows-view")).toContainText(
    `${originalName} (copy)`,
  );
});

test("deletes a workflow with confirmation", async ({ page }) => {
  const workflowName = `delete_test_${Date.now()}`;

  await navigateToWorkflows(page);
  await createWorkflow(page, workflowName);

  // Verify it exists
  await expect(page.getByTestId("workflows-view")).toContainText(
    displayWorkflowTitle(workflowName),
  );

  // Open the dropdown menu and click Delete
  await page.getByRole("button", { name: "Workflow actions" }).first().click();
  await page.getByRole("menuitem", { name: "Delete" }).click();

  // Confirmation dialog should appear with workflow name
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await expect(page.getByRole("alertdialog")).toContainText(
    displayWorkflowTitle(workflowName),
  );

  // Confirm deletion
  await page.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByRole("alertdialog")).not.toBeVisible();

  // Verify workflow is gone — back to empty state
  await expect(page.getByText("No workflows yet")).toBeVisible();
});

test("triggers a workflow from the detail panel", async ({ page }) => {
  const workflowName = `trigger_test_${Date.now()}`;

  await navigateToWorkflows(page);
  await createWorkflow(page, workflowName);

  // Click on the workflow card to open the detail panel
  await page
    .getByRole("button", { name: `View ${displayWorkflowTitle(workflowName)}` })
    .click();
  await expect(page.getByTestId("workflow-detail-panel")).toBeVisible();

  // Click the Trigger button
  await page
    .getByTestId("workflow-detail-panel")
    .getByRole("button", { name: "Trigger" })
    .click();

  // Wait for the trigger to complete (button text changes back from "Triggering...")
  await expect(
    page
      .getByTestId("workflow-detail-panel")
      .getByRole("button", { name: "Trigger" }),
  ).toBeVisible();

  await expect(
    page
      .getByTestId("workflow-detail-panel")
      .getByTestId("workflow-selected-run"),
  ).toBeVisible();
  await expect(
    page.getByTestId("workflow-detail-panel").getByTestId("workflow-run-trace"),
  ).toContainText("step_1");
});
