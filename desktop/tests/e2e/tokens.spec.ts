import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

const GENERAL_CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const DESIGN_CHANNEL_ID = "b5e2f8a1-3c44-5912-9e67-4a8d1f2b3c4e";

test("creates a channel-scoped token from settings and can revoke it", async ({
  page,
}) => {
  await installMockBridge(page);
  await page.goto("/");

  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("settings-view")).toBeVisible();
  await page.getByTestId("settings-nav-tokens").click();

  const tokenCard = page.getByTestId("settings-tokens");
  await tokenCard.getByRole("button", { name: "Create token" }).click();

  const dialog = page.getByTestId("create-token-dialog");
  await expect(dialog).toBeVisible();

  await page.getByTestId("token-name-input").fill("qa-selected-channels");
  await page.getByTestId("token-scope-messages-read").click();
  await page.getByTestId("token-scope-channels-read").click();
  await page.getByTestId("token-channel-access-selected").click();

  await expect(
    dialog.getByText(
      /Only channels where you are a member can be added to a scoped token\.( \d+ accessible channels? hidden because you are not a member\.)?/,
    ),
  ).toBeVisible();
  await expect(
    page.getByTestId(`token-channel-${GENERAL_CHANNEL_ID}`),
  ).toBeVisible();
  await expect(
    page.getByTestId(`token-channel-${DESIGN_CHANNEL_ID}`),
  ).toHaveCount(0);

  await page.getByTestId(`token-channel-${GENERAL_CHANNEL_ID}`).click();
  await page.getByTestId("token-expiry-7").click();
  await page.getByTestId("confirm-create-token").click();

  const createdDialog = page.getByTestId("token-created-dialog");
  await expect(createdDialog).toBeVisible();
  await expect(createdDialog).toContainText("Token created");
  await expect(createdDialog).toContainText("spr_tok_mock_");
  await page.getByTestId("token-created-done").click();

  await expect(tokenCard).toContainText("qa-selected-channels");
  await expect(tokenCard).toContainText("Scoped to 1 channel");
  await expect(tokenCard).toContainText("general");
  await tokenCard.locator('[data-testid^="revoke-token-"]').click();
  await expect(tokenCard).toContainText("revoked");
});

test("surfaces token mint errors in the dialog", async ({ page }) => {
  await installMockBridge(page, {
    mintTokenError:
      "relay returned 403 Forbidden: not a member of channel: 8f321c1d-f77e-4952-881c-f6e7bfb94c6b",
  });
  await page.goto("/");

  await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("settings-view")).toBeVisible();
  await page.getByTestId("settings-nav-tokens").click();

  await page
    .getByTestId("settings-tokens")
    .getByRole("button", { name: "Create token" })
    .click();

  await page.getByTestId("token-name-input").fill("qa-failing-token");
  await page.getByTestId("token-scope-messages-read").click();
  await page.getByTestId("confirm-create-token").click();

  const dialog = page.getByTestId("create-token-dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText(
    "relay returned 403 Forbidden: not a member of channel: 8f321c1d-f77e-4952-881c-f6e7bfb94c6b",
  );
  await expect(page.getByTestId("confirm-create-token")).toHaveText(
    "Create token",
  );
});
