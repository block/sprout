import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

async function gotoApp(page: import("@playwright/test").Page) {
  await page.goto("/");
  await expect(page.getByTestId("open-agents-view")).toBeVisible();
}

async function waitForInvokeBridge(page: import("@playwright/test").Page) {
  await page.waitForFunction(
    () => {
      const tauriWindow = window as Window & {
        __SPROUT_E2E_INVOKE_MOCK_COMMAND__?: unknown;
        __TAURI_INTERNALS__?: {
          invoke?: unknown;
        };
      };

      return (
        typeof tauriWindow.__SPROUT_E2E_INVOKE_MOCK_COMMAND__ === "function" ||
        typeof tauriWindow.__TAURI_INTERNALS__?.invoke === "function"
      );
    },
    null,
    { timeout: 5_000 },
  );
}

async function invokeTauri<T>(
  page: import("@playwright/test").Page,
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  await waitForInvokeBridge(page);

  return page.evaluate(
    async ({ command: targetCommand, payload: targetPayload }) => {
      const tauriWindow = window as Window & {
        __SPROUT_E2E_INVOKE_MOCK_COMMAND__?: (
          command: string,
          payload?: Record<string, unknown>,
        ) => Promise<unknown>;
        __TAURI_INTERNALS__?: {
          invoke?: (
            command: string,
            payload?: Record<string, unknown>,
          ) => Promise<unknown>;
        };
      };

      const invoke =
        tauriWindow.__SPROUT_E2E_INVOKE_MOCK_COMMAND__ ??
        tauriWindow.__TAURI_INTERNALS__?.invoke;
      if (!invoke) {
        throw new Error("Mock invoke bridge is unavailable.");
      }

      return (await invoke(targetCommand, targetPayload)) as T;
    },
    { command, payload },
  );
}

async function invokeTauriExpectError(
  page: import("@playwright/test").Page,
  command: string,
  payload?: Record<string, unknown>,
) {
  await waitForInvokeBridge(page);

  return page.evaluate(
    async ({ command: targetCommand, payload: targetPayload }) => {
      const tauriWindow = window as Window & {
        __SPROUT_E2E_INVOKE_MOCK_COMMAND__?: (
          command: string,
          payload?: Record<string, unknown>,
        ) => Promise<unknown>;
        __TAURI_INTERNALS__?: {
          invoke?: (
            command: string,
            payload?: Record<string, unknown>,
          ) => Promise<unknown>;
        };
      };

      const invoke =
        tauriWindow.__SPROUT_E2E_INVOKE_MOCK_COMMAND__ ??
        tauriWindow.__TAURI_INTERNALS__?.invoke;
      if (!invoke) {
        throw new Error("Mock invoke bridge is unavailable.");
      }

      try {
        await invoke(targetCommand, targetPayload);
        return null;
      } catch (error) {
        return error instanceof Error ? error.message : String(error);
      }
    },
    { command, payload },
  );
}

test("built-in personas stay visible in the catalog and can be selected", async ({
  page,
}) => {
  await gotoApp(page);
  await page.getByTestId("open-agents-view").click();

  await expect(page.getByTestId("agents-library-personas")).toContainText(
    "No agents yet",
  );
  await expect(page.getByTestId("agents-persona-catalog")).toContainText(
    "Reviewer",
  );
  await expect(page.getByRole("tooltip")).toHaveCount(0);

  await page.getByTestId("persona-catalog-toggle-builtin:reviewer").click();
  await expect(
    page.getByTestId("persona-catalog-feedback-notice"),
  ).toContainText("Selected Reviewer for My Agents.");

  await expect(page.getByTestId("agents-library-personas")).toContainText(
    "Reviewer",
  );
  await expect(
    page.getByTestId("persona-catalog-card-builtin:reviewer"),
  ).toContainText("Selected");

  await page.getByTestId("persona-catalog-toggle-builtin:reviewer").click();
  await expect(
    page.getByTestId("persona-catalog-feedback-notice"),
  ).toContainText("Deselected Reviewer from My Agents.");
  await expect(page.getByTestId("agents-library-personas")).not.toContainText(
    "Reviewer",
  );
});

test("catalog details sheet shows the full persona details", async ({
  page,
}) => {
  await gotoApp(page);
  await page.getByTestId("open-agents-view").click();

  await page.getByTestId("persona-catalog-details-builtin:reviewer").click();

  await expect(page.getByTestId("persona-catalog-details-sheet")).toContainText(
    "Reviewer",
  );
  await expect(page.getByTestId("persona-catalog-details-sheet")).toContainText(
    "You are Reviewer.",
  );

  await page
    .getByTestId("persona-catalog-detail-toggle-builtin:reviewer")
    .click();
  await expect(
    page.getByTestId("persona-catalog-detail-toggle-builtin:reviewer"),
  ).toHaveAttribute("data-state", "checked");
  await expect(page.getByTestId("agents-library-personas")).toContainText(
    "Reviewer",
  );
});

test("inactive built-ins cannot be used to create teams", async ({ page }) => {
  await gotoApp(page);

  const error = await invokeTauriExpectError(page, "create_team", {
    input: {
      name: "Reviewers",
      personaIds: ["builtin:reviewer"],
    },
  });

  expect(error).toBe(
    "Reviewer is not in My Agents. Choose it from Persona Catalog first.",
  );
});

test("built-in deselection failures show up in Persona Catalog", async ({
  page,
}) => {
  await gotoApp(page);

  await page.getByTestId("open-agents-view").click();
  await page.getByTestId("persona-catalog-toggle-builtin:reviewer").click();

  await invokeTauri(page, "create_team", {
    input: {
      name: "Reviewers",
      personaIds: ["builtin:reviewer"],
    },
  });

  await page.getByTestId("persona-catalog-toggle-builtin:reviewer").click();

  await expect(
    page.getByTestId("persona-catalog-feedback-error"),
  ).toContainText("Reviewer is still referenced by a team.");
});

test("channel quick add falls back to added personas when defaults are absent", async ({
  page,
}) => {
  await gotoApp(page);
  await page.getByTestId("open-agents-view").click();
  await page.getByTestId("persona-catalog-toggle-builtin:reviewer").click();

  await page.getByTestId("channel-random").click();
  await expect(page.getByTestId("chat-title")).toHaveText("random");
  await page.getByTestId("channel-add-bot-trigger").hover();

  await expect(
    page.getByRole("button", { name: "Add Reviewer" }),
  ).toBeVisible();
});

test("personas referenced by teams cannot be deleted", async ({ page }) => {
  await gotoApp(page);

  const persona = await invokeTauri<{ id: string }>(page, "create_persona", {
    input: {
      displayName: "Analyst",
      systemPrompt: "You are Analyst.",
    },
  });

  await invokeTauri(page, "create_team", {
    input: {
      name: "Analysts",
      personaIds: [persona.id],
    },
  });

  const error = await invokeTauriExpectError(page, "delete_persona", {
    id: persona.id,
  });

  expect(error).toBe(
    "Analyst is still referenced by a team. Remove it from those teams first.",
  );
});
