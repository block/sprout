import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";
import { openSettings } from "../helpers/settings";

// Use real-looking provider keys so the detector matches.
const FAKE_ANTHROPIC_KEY = `sk-ant-api03-${"A".repeat(91)}aaAA`;
// OpenAI legacy/proj key shape. Built via concat so GitHub's secret scanner
// does not regex-match an inline OpenAI-shaped key.
const OPENAI_INFIX = "T3" + "BlbkFJ";
const FAKE_OPENAI_KEY = `sk-proj-${"a".repeat(40)}${OPENAI_INFIX}${"b".repeat(40)}`;

const SCREENSHOT_DIR = "screenshots/agent-provider";
const ACTIVE_PUBKEY = "deadbeef".repeat(8);

type ProfileSeed = {
  id: string;
  label: string;
  provider: "anthropic" | "openai";
  model: string;
  baseUrl: string;
  detectedProviderId: string;
  apiKeyPreview: string | null;
};

function seedProfile(p: ProfileSeed) {
  return {
    id: p.id,
    label: p.label,
    createdAt: 1_700_000_000,
    updatedAt: 1_700_000_000,
    view: {
      provider: p.provider,
      model: p.model,
      baseUrl: p.baseUrl,
      anthropicApiVersion: null,
      systemPrompt: null,
      maxRounds: null,
      maxOutputTokens: null,
      llmTimeoutSecs: null,
      toolTimeoutSecs: null,
      maxHistoryBytes: null,
      detectedProviderId: p.detectedProviderId,
      detectionOverridden: false,
      apiKeyPresent: true,
      apiKeyPreview: p.apiKeyPreview,
    },
  };
}

const ANTHROPIC_WORK = seedProfile({
  id: "11111111-1111-4111-8111-111111111111",
  label: "Anthropic (work)",
  provider: "anthropic",
  model: "claude-sonnet-4-5",
  baseUrl: "https://api.anthropic.com",
  detectedProviderId: "anthropic",
  apiKeyPreview: "wQrk",
});

const OPENAI_PERSONAL = seedProfile({
  id: "22222222-2222-4222-8222-222222222222",
  label: "OpenAI (personal)",
  provider: "openai",
  model: "gpt-5",
  baseUrl: "https://api.openai.com/v1",
  detectedProviderId: "openai",
  apiKeyPreview: "p3rs",
});

test.describe("Agent Provider settings — multi-profile list", () => {
  test("empty state shows shell-env hint and Add profile button", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderEnvPresence: { anthropicApiKey: true },
    });
    await page.goto("/");

    await openSettings(page, "agent-provider");
    const card = page.getByTestId("settings-agent-provider");
    await expect(card).toBeVisible();

    // Empty state: shell-env hint + Add button visible; no list, no banners.
    await expect(
      page.getByTestId("agent-provider-shell-env-hint"),
    ).toBeVisible();
    await expect(page.getByTestId("agent-provider-add")).toBeEnabled();
    await expect(page.getByTestId("agent-provider-profile-list")).toHaveCount(
      0,
    );
    await expect(
      page.getByTestId("agent-provider-rotation-warning"),
    ).toHaveCount(0);

    await page.screenshot({
      path: `${SCREENSHOT_DIR}/01-empty-with-shell-env-hint.png`,
      fullPage: false,
    });
  });

  test("add-profile flow: detection, auto-default, list refresh", async ({
    page,
  }) => {
    await installMockBridge(page, {});
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await page.getByTestId("agent-provider-add").click();
    const dialog = page.getByTestId("agent-provider-profile-dialog");
    await expect(dialog).toBeVisible();

    await page.getByTestId("agent-provider-profile-label").fill("Work key");
    await page.getByTestId("agent-provider-api-key").fill(FAKE_ANTHROPIC_KEY);

    // Auto-detection wires provider + base URL + model.
    await expect(
      page.getByTestId("agent-provider-detected-badge"),
    ).toContainText(/anthropic/i);
    await expect(page.getByTestId("agent-provider-base-url")).toHaveValue(
      "https://api.anthropic.com",
    );
    await expect(page.getByTestId("agent-provider-model")).not.toHaveValue("");

    await page.screenshot({
      path: `${SCREENSHOT_DIR}/02-add-dialog-detected-anthropic.png`,
      fullPage: false,
    });

    await page.getByTestId("agent-provider-profile-save").click();
    // The first profile auto-becomes default → toast says so.
    await expect(page.getByText(/set as default/i).first()).toBeVisible({
      timeout: 5000,
    });

    // List re-renders with one row and a default pill.
    const list = page.getByTestId("agent-provider-profile-list");
    await expect(list).toBeVisible();
    await expect(list.locator("li")).toHaveCount(1);
    await expect(
      page.locator('[data-testid^="agent-provider-profile-default-pill-"]'),
    ).toBeVisible();

    await page.screenshot({
      path: `${SCREENSHOT_DIR}/03-list-after-first-save.png`,
      fullPage: false,
    });
  });

  test("two-profile list: set default and edit", async ({ page }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK, OPENAI_PERSONAL],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    // Both rows render; Anthropic shows default pill.
    await expect(
      page.getByTestId(`agent-provider-profile-row-${ANTHROPIC_WORK.id}`),
    ).toBeVisible();
    await expect(
      page.getByTestId(`agent-provider-profile-row-${OPENAI_PERSONAL.id}`),
    ).toBeVisible();
    await expect(
      page.getByTestId(
        `agent-provider-profile-default-pill-${ANTHROPIC_WORK.id}`,
      ),
    ).toBeVisible();
    await expect(
      page.getByTestId(
        `agent-provider-profile-default-pill-${OPENAI_PERSONAL.id}`,
      ),
    ).toHaveCount(0);

    await page.screenshot({
      path: `${SCREENSHOT_DIR}/04-two-profile-list.png`,
      fullPage: false,
    });

    // Promote OpenAI to default.
    await page
      .getByTestId(`agent-provider-profile-set-default-${OPENAI_PERSONAL.id}`)
      .click();
    await expect(
      page.getByTestId(
        `agent-provider-profile-default-pill-${OPENAI_PERSONAL.id}`,
      ),
    ).toBeVisible();
    await expect(
      page.getByTestId(
        `agent-provider-profile-default-pill-${ANTHROPIC_WORK.id}`,
      ),
    ).toHaveCount(0);

    // Open edit dialog on the (newly-default) OpenAI row; label hydrates,
    // saved-key preview is opaque server-side.
    await page
      .getByTestId(`agent-provider-profile-edit-${OPENAI_PERSONAL.id}`)
      .click();
    const dialog = page.getByTestId("agent-provider-profile-dialog");
    await expect(dialog).toBeVisible();
    await expect(page.getByTestId("agent-provider-profile-label")).toHaveValue(
      OPENAI_PERSONAL.label,
    );
    await expect(page.getByTestId("agent-provider-api-key")).toHaveValue("");
    await expect(page.getByTestId("agent-provider-model")).toHaveValue("gpt-5");
    await page.getByTestId("agent-provider-profile-cancel").click();
    await expect(dialog).toHaveCount(0);
  });

  test("delete-row: arming click marks red, second click deletes", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK, OPENAI_PERSONAL],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    const deleteBtn = page.getByTestId(
      `agent-provider-profile-delete-${OPENAI_PERSONAL.id}`,
    );
    // First click arms; row still present.
    await deleteBtn.click();
    await expect(
      page.getByTestId(`agent-provider-profile-row-${OPENAI_PERSONAL.id}`),
    ).toBeVisible();
    // Second click commits.
    await deleteBtn.click();
    await expect(
      page.getByTestId(`agent-provider-profile-row-${OPENAI_PERSONAL.id}`),
    ).toHaveCount(0);
    // Anthropic still the default; survives.
    await expect(
      page.getByTestId(
        `agent-provider-profile-default-pill-${ANTHROPIC_WORK.id}`,
      ),
    ).toBeVisible();
  });

  test("edit dialog: in-dialog Delete arms then deletes the profile", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK, OPENAI_PERSONAL],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await page
      .getByTestId(`agent-provider-profile-edit-${OPENAI_PERSONAL.id}`)
      .click();

    const deleteBtn = page.getByTestId("agent-provider-profile-delete");
    await expect(deleteBtn).toHaveText("Delete");
    await deleteBtn.click();
    await expect(deleteBtn).toHaveText("Click to confirm");
    await deleteBtn.click();

    // Dialog closes and the row is gone.
    await expect(page.getByTestId("agent-provider-profile-dialog")).toHaveCount(
      0,
    );
    await expect(
      page.getByTestId(`agent-provider-profile-row-${OPENAI_PERSONAL.id}`),
    ).toHaveCount(0);
    // Anthropic (default) survives.
    await expect(
      page.getByTestId(`agent-provider-profile-row-${ANTHROPIC_WORK.id}`),
    ).toBeVisible();
  });

  test("edit dialog: in-dialog Delete disarms after Cancel → reopen", async ({
    page,
  }) => {
    // Two-click destructive confirmation must reset on close. Otherwise
    // a user who armed delete, hit Cancel for any reason, and came back
    // would commit deletion on a single click. Catches the regression
    // codex R13 flagged in the scope-marker pattern.
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK, OPENAI_PERSONAL],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await page
      .getByTestId(`agent-provider-profile-edit-${OPENAI_PERSONAL.id}`)
      .click();

    const deleteBtn = page.getByTestId("agent-provider-profile-delete");
    await expect(deleteBtn).toHaveText("Delete");
    await deleteBtn.click();
    await expect(deleteBtn).toHaveText("Click to confirm");

    // Cancel the dialog without confirming the delete.
    await page.getByTestId("agent-provider-profile-cancel").click();
    await expect(page.getByTestId("agent-provider-profile-dialog")).toHaveCount(
      0,
    );

    // Reopen the same profile. Delete must be back to its safe label.
    await page
      .getByTestId(`agent-provider-profile-edit-${OPENAI_PERSONAL.id}`)
      .click();
    await expect(deleteBtn).toBeVisible();
    await expect(deleteBtn).toHaveText("Delete");

    // Row must still exist (no premature delete).
    await expect(
      page.getByTestId("agent-provider-profile-cancel"),
    ).toBeVisible();
    await page.getByTestId("agent-provider-profile-cancel").click();
    await expect(
      page.getByTestId(`agent-provider-profile-row-${OPENAI_PERSONAL.id}`),
    ).toBeVisible();
  });

  test("add dialog: no in-dialog Delete button", async ({ page }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");
    await page.getByTestId("agent-provider-add").click();
    await expect(
      page.getByTestId("agent-provider-profile-dialog"),
    ).toBeVisible();
    await expect(page.getByTestId("agent-provider-profile-delete")).toHaveCount(
      0,
    );
  });

  test("deleting the default surfaces the no-default banner", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK, OPENAI_PERSONAL],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    const deleteBtn = page.getByTestId(
      `agent-provider-profile-delete-${ANTHROPIC_WORK.id}`,
    );
    await deleteBtn.click();
    await deleteBtn.click();
    await expect(
      page.getByTestId(`agent-provider-profile-row-${ANTHROPIC_WORK.id}`),
    ).toHaveCount(0);
    // OpenAI row remains, but no default pill anywhere and the warning
    // banner is now showing.
    await expect(
      page.locator('[data-testid^="agent-provider-profile-default-pill-"]'),
    ).toHaveCount(0);
    await expect(
      page.getByTestId("agent-provider-no-default-banner"),
    ).toBeVisible();

    await page.screenshot({
      path: `${SCREENSHOT_DIR}/05-no-default-banner.png`,
      fullPage: false,
    });
  });

  test("identity-rotation banner appears when stored pubkey differs", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey:
          "1111111111111111111111111111111111111111111111111111111111111111",
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await expect(
      page.getByTestId("agent-provider-rotation-warning"),
    ).toBeVisible();
    // Add button is disabled while another identity owns the record.
    await expect(page.getByTestId("agent-provider-add")).toBeDisabled();

    await page.screenshot({
      path: `${SCREENSHOT_DIR}/06-rotation-banner.png`,
      fullPage: false,
    });
  });

  test("load-error banner surfaces when stored envelope is unreadable", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettingsLoadError:
        "parse stored settings: expected value at line 1 column 1",
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");
    await expect(page.getByTestId("agent-provider-load-error")).toBeVisible();
    await expect(page.getByTestId("agent-provider-load-error")).toContainText(
      /parse stored settings/i,
    );
    // Clear-all must remain reachable in the corrupt-envelope state so the
    // user has an escape hatch (R7 LOW). The button is also present (but
    // not exercised here) in the identity-mismatch and ok-empty cases.
    await expect(page.getByTestId("agent-provider-clear")).toBeVisible();
  });

  test("clear-all flow wipes the list and returns the empty state", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await page.getByTestId("agent-provider-clear").click();
    const confirm = page.getByTestId("agent-provider-clear-dialog");
    await expect(confirm).toBeVisible();
    await expect(confirm).toHaveAttribute("role", "alertdialog");
    await page.getByTestId("agent-provider-clear-confirm").click();

    await expect(page.getByTestId("agent-provider-profile-list")).toHaveCount(
      0,
    );
    await expect(
      page.getByTestId("agent-provider-rotation-warning"),
    ).toHaveCount(0);
    await expect(page.getByTestId("agent-provider-load-error")).toHaveCount(0);
  });

  test("clear-all dialog: Escape cancels without deleting", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await page.getByTestId("agent-provider-clear").click();
    const confirm = page.getByTestId("agent-provider-clear-dialog");
    await expect(confirm).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(confirm).toHaveCount(0);
    // List unchanged.
    await expect(
      page.getByTestId(`agent-provider-profile-row-${ANTHROPIC_WORK.id}`),
    ).toBeVisible();
  });
});

test.describe("Agent Provider settings — dialog form", () => {
  test("provider auto-detection: OpenAI key wins over default Anthropic base", async ({
    page,
  }) => {
    await installMockBridge(page, {});
    await page.goto("/");
    await openSettings(page, "agent-provider");
    await page.getByTestId("agent-provider-add").click();

    // Fresh dialog defaults to Anthropic; paste an OpenAI-shaped key.
    await page.getByTestId("agent-provider-api-key").fill(FAKE_OPENAI_KEY);
    await expect(
      page.getByTestId("agent-provider-detected-badge"),
    ).toContainText(/openai/i);
    await expect(
      page.getByTestId("agent-provider-provider-select"),
    ).toHaveValue("openai");
    await expect(page.getByTestId("agent-provider-base-url")).toHaveValue(
      "https://api.openai.com/v1",
    );
    await expect(page.getByTestId("agent-provider-model")).toHaveValue("gpt-5");
  });

  test("manual provider switch: model resets and Custom clears stale base URL", async ({
    page,
  }) => {
    await installMockBridge(page, {});
    await page.goto("/");
    await openSettings(page, "agent-provider");
    await page.getByTestId("agent-provider-add").click();

    const modelInput = page.getByTestId("agent-provider-model");
    const baseUrlInput = page.getByTestId("agent-provider-base-url");

    await expect(modelInput).toHaveValue("claude-sonnet-4-5");
    await expect(baseUrlInput).toHaveValue("https://api.anthropic.com");

    await page
      .getByTestId("agent-provider-provider-select")
      .selectOption("openai");
    await expect(modelInput).toHaveValue("gpt-5");
    await expect(baseUrlInput).toHaveValue("https://api.openai.com/v1");

    // Custom has no default base URL → must clear, not keep the OpenAI value.
    await page
      .getByTestId("agent-provider-provider-select")
      .selectOption("custom");
    await expect(baseUrlInput).toHaveValue("");
  });

  test("edit + switch to local provider keeps Save enabled (no key required)", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await page
      .getByTestId(`agent-provider-profile-edit-${ANTHROPIC_WORK.id}`)
      .click();
    await expect(
      page.getByTestId("agent-provider-profile-dialog"),
    ).toBeVisible();
    await page
      .getByTestId("agent-provider-provider-select")
      .selectOption("ollama");
    await expect(
      page.getByTestId("agent-provider-provider-change-warning"),
    ).toHaveCount(0);
    await expect(page.getByTestId("agent-provider-profile-save")).toBeEnabled();
  });

  test("edit + change issuer without re-entering key disables Save", async ({
    page,
  }) => {
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");
    await page
      .getByTestId(`agent-provider-profile-edit-${ANTHROPIC_WORK.id}`)
      .click();
    await expect(
      page.getByTestId("agent-provider-profile-dialog"),
    ).toBeVisible();
    // Wait for hydration to finish — the model field is prefilled from the
    // backend view. Without this, a fast `selectOption` below can race
    // hydration and the providerChangedWithoutKey check evaluates against
    // the still-blank form.
    await expect(page.getByTestId("agent-provider-model")).toHaveValue(
      "claude-sonnet-4-5",
    );

    // Switch to a different remote provider (OpenAI). The provider-change
    // warning must appear and Save must be disabled until the user enters
    // a new key.
    await page
      .getByTestId("agent-provider-provider-select")
      .selectOption("openai");
    await expect(
      page.getByTestId("agent-provider-provider-change-warning"),
    ).toBeVisible();
    await expect(
      page.getByTestId("agent-provider-profile-save"),
    ).toBeDisabled();

    await page.screenshot({
      path: `${SCREENSHOT_DIR}/07-provider-change-warning.png`,
      fullPage: false,
    });
  });

  test("edit dialog: Save is blocked until the target profile hydrates (no race)", async ({
    page,
  }) => {
    // Slow profile reads so the hydration race window is observable.
    // Without `hydrationStale`, a fast click during this window could
    // submit blank-default values (or worse, profile A's values after a
    // mid-open switch to B) under the wrong profile id.
    //
    // To prove that `hydrationStale` (not the existing `labelInvalid`
    // shape check) is doing the gating, the test fills a valid label
    // BEFORE the profile finishes loading. Without the hydration gate,
    // Save would now be enabled. With it, Save stays disabled until the
    // backend view lands.
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK, OPENAI_PERSONAL],
      },
      agentProviderProfileReadDelayMs: 1_500,
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await page
      .getByTestId(`agent-provider-profile-edit-${OPENAI_PERSONAL.id}`)
      .click();
    const dialog = page.getByTestId("agent-provider-profile-dialog");
    await expect(dialog).toBeVisible();

    // Type a label so labelInvalid is false. The blank-form defaults
    // (Anthropic model + base URL) already satisfy the model/baseUrl
    // checks. The only remaining gate is `hydrationStale`.
    await page
      .getByTestId("agent-provider-profile-label")
      .fill("Pre-hydration");

    await expect(
      page.getByTestId("agent-provider-profile-save"),
    ).toBeDisabled();

    // Once the OpenAI profile lands, hydration replaces the label and
    // form. Save then becomes enabled.
    await expect(page.getByTestId("agent-provider-profile-label")).toHaveValue(
      OPENAI_PERSONAL.label,
      { timeout: 5_000 },
    );
    await expect(page.getByTestId("agent-provider-model")).toHaveValue(
      OPENAI_PERSONAL.view.model,
    );
    await expect(page.getByTestId("agent-provider-profile-save")).toBeEnabled();
  });

  test("edit dialog: closing clears the apiKey input and reveal state", async ({
    page,
  }) => {
    // Open profile A's edit dialog, type a replacement key, reveal it,
    // then close. Reopen and confirm neither the plaintext key nor the
    // reveal toggle survived.
    await installMockBridge(page, {
      agentProviderSettings: {
        storedPubkey: ACTIVE_PUBKEY,
        defaultProfileId: ANTHROPIC_WORK.id,
        profiles: [ANTHROPIC_WORK],
      },
    });
    await page.goto("/");
    await openSettings(page, "agent-provider");

    await page
      .getByTestId(`agent-provider-profile-edit-${ANTHROPIC_WORK.id}`)
      .click();
    await expect(
      page.getByTestId("agent-provider-profile-dialog"),
    ).toBeVisible();
    // Wait for hydration.
    await expect(page.getByTestId("agent-provider-model")).toHaveValue(
      ANTHROPIC_WORK.view.model,
    );

    const apiKey = page.getByTestId("agent-provider-api-key");
    await apiKey.fill(FAKE_ANTHROPIC_KEY);
    await page.getByTestId("agent-provider-api-key-reveal").click();
    await expect(apiKey).toHaveAttribute("type", "text");

    await page.getByTestId("agent-provider-profile-cancel").click();
    await expect(page.getByTestId("agent-provider-profile-dialog")).toHaveCount(
      0,
    );

    // Reopen — the input must be empty and the field must be masked.
    await page
      .getByTestId(`agent-provider-profile-edit-${ANTHROPIC_WORK.id}`)
      .click();
    await expect(
      page.getByTestId("agent-provider-profile-dialog"),
    ).toBeVisible();
    await expect(page.getByTestId("agent-provider-model")).toHaveValue(
      ANTHROPIC_WORK.view.model,
    );
    await expect(page.getByTestId("agent-provider-api-key")).toHaveValue("");
    await expect(page.getByTestId("agent-provider-api-key")).toHaveAttribute(
      "type",
      "password",
    );
  });
});
