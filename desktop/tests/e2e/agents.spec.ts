import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.beforeEach(async ({ page }) => {
  await installMockBridge(page);
});

async function gotoApp(page: import("@playwright/test").Page) {
  let lastError: unknown = null;

  for (const attempt of [0, 1]) {
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await waitForInvokeBridge(page);

    try {
      await expect(page.getByTestId("open-agents-view")).toBeVisible({
        timeout: 10_000,
      });
      return;
    } catch (error) {
      lastError = error;
      if (attempt === 1) {
        throw error;
      }
    }
  }

  throw lastError;
}

async function getCatalogOrder(page: import("@playwright/test").Page) {
  return page
    .locator('[data-testid^="persona-catalog-card-target-"]')
    .evaluateAll((elements) =>
      elements.map((element) => element.getAttribute("data-testid") ?? ""),
    );
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

test("built-in personas are chosen from the dialog and can be selected", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 420 });
  await gotoApp(page);
  await page.getByTestId("open-agents-view").click();

  await expect(page.getByTestId("agents-library-personas")).toContainText(
    "No agents yet",
  );
  await expect(page.getByTestId("agents-persona-catalog")).toHaveCount(0);
  await page.getByTestId("open-persona-catalog").click();
  await expect(page.getByTestId("persona-catalog-dialog")).toContainText(
    "Reviewer",
  );
  await expect(page.getByTestId("persona-catalog-dialog-header")).toBeVisible();
  await expect(
    page.getByTestId("persona-catalog-dialog-scroll-area"),
  ).toBeVisible();
  await expect(
    page.getByTestId("persona-catalog-dialog-scroll-area"),
  ).toHaveCSS("overflow-y", "auto");
  expect(
    await page
      .getByTestId("persona-catalog-dialog-scroll-area")
      .evaluate(
        (element) =>
          element.scrollHeight > element.clientHeight &&
          element.clientHeight > 0,
      ),
  ).toBe(true);
  await expect(page.getByTestId("persona-catalog-dialog-footer")).toBeVisible();
  await expect(page.getByRole("tooltip")).toHaveCount(0);
  const initialCatalogOrder = await getCatalogOrder(page);

  await page
    .getByTestId("persona-catalog-card-target-builtin:reviewer")
    .click();
  await expect(
    page.getByTestId("persona-catalog-feedback-notice"),
  ).toContainText("Selected Reviewer for My Agents.");

  await expect(page.getByTestId("agents-library-personas")).toContainText(
    "Reviewer",
  );
  await expect(
    page.getByTestId("persona-catalog-card-target-builtin:reviewer"),
  ).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => getCatalogOrder(page)).toEqual(initialCatalogOrder);

  await page
    .getByTestId("persona-catalog-card-target-builtin:reviewer")
    .click();
  await expect(
    page.getByTestId("persona-catalog-feedback-notice"),
  ).toContainText("Deselected Reviewer from My Agents.");
  await expect(page.getByTestId("agents-library-personas")).not.toContainText(
    "Reviewer",
  );
  await expect.poll(() => getCatalogOrder(page)).toEqual(initialCatalogOrder);
});

test("persona catalog can reopen from the populated library header", async ({
  page,
}) => {
  await gotoApp(page);
  await page.getByTestId("open-agents-view").click();
  await page.getByTestId("open-persona-catalog").click();

  await page
    .getByTestId("persona-catalog-card-target-builtin:reviewer")
    .click();
  await expect(page.getByTestId("agents-library-personas")).toContainText(
    "Reviewer",
  );

  await page.getByTestId("persona-catalog-dialog-done").click();
  await page.getByTestId("open-persona-catalog").click();

  await expect(page.getByTestId("persona-catalog-dialog")).toBeVisible();
  await expect(
    page.getByTestId("persona-catalog-card-target-builtin:reviewer"),
  ).toHaveAttribute("aria-pressed", "true");
});

test("persona catalog chooser order stays stable when selection changes", async ({
  page,
}) => {
  await gotoApp(page);
  await page.getByTestId("open-agents-view").click();
  await page.getByTestId("open-persona-catalog").click();

  const before = await getCatalogOrder(page);

  await page.getByTestId("persona-catalog-card-target-builtin:solo").click();
  await expect(
    page.getByTestId("persona-catalog-feedback-notice"),
  ).toContainText("Selected Solo for My Agents.");

  expect(await getCatalogOrder(page)).toEqual(before);
});

test("catalog details sheet shows the full persona details", async ({
  page,
}) => {
  await gotoApp(page);
  await page.getByTestId("open-agents-view").click();
  await page.getByTestId("open-persona-catalog").click();

  await page.getByTestId("persona-catalog-details-builtin:reviewer").click();
  const detailSelectionTarget = page.getByTestId(
    "persona-catalog-detail-selection-target-builtin:reviewer",
  );

  await expect(page.getByTestId("persona-catalog-details-sheet")).toContainText(
    "Reviewer",
  );
  await expect(page.getByTestId("persona-catalog-details-sheet")).toContainText(
    "You are Reviewer.",
  );
  await expect(
    page.getByTestId("persona-catalog-detail-selection-title"),
  ).toHaveText("Available in Persona Catalog");
  await expect(detailSelectionTarget).toHaveAttribute(
    "aria-label",
    "Select Reviewer in My Agents",
  );
  await expect(detailSelectionTarget).toHaveAttribute("aria-pressed", "false");

  await detailSelectionTarget.click();
  await expect(
    page.getByTestId("persona-catalog-detail-selection-title"),
  ).toHaveText("Selected for My Agents");
  await expect(detailSelectionTarget).toHaveAttribute(
    "aria-label",
    "Deselect Reviewer in My Agents",
  );
  await expect(detailSelectionTarget).toHaveAttribute("aria-pressed", "true");
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
  await page.getByTestId("open-persona-catalog").click();
  await page
    .getByTestId("persona-catalog-card-target-builtin:reviewer")
    .click();

  await invokeTauri(page, "create_team", {
    input: {
      name: "Reviewers",
      personaIds: ["builtin:reviewer"],
    },
  });

  await page
    .getByTestId("persona-catalog-card-target-builtin:reviewer")
    .click();

  await expect(
    page.getByTestId("persona-catalog-feedback-error"),
  ).toContainText("Reviewer is still referenced by a team.");
});

test("channel quick add falls back to added personas when defaults are absent", async ({
  page,
}) => {
  await gotoApp(page);
  await page.getByTestId("open-agents-view").click();
  await page.getByTestId("open-persona-catalog").click();
  await page
    .getByTestId("persona-catalog-card-target-builtin:reviewer")
    .click();
  await page.getByTestId("persona-catalog-dialog-done").click();

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
