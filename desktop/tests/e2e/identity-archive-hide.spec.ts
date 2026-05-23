import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

// Guards the NIP-IA discovery-suppression contract (Dawn's table v2):
// - Members sidebar: archived members fold under "Archived (N)", not in the
//   active people/bots lists.
// - Mention autocomplete: archived members are filtered out of suggestions
//   (but their `members` entry still resolves historical @-mentions).
// - DM picker: archived users are omitted from search results.
// - History invariant: an archived user's existing channel message renders
//   normally (Tyler's "never hide messages").

const ALICE_PUBKEY =
  "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f";
const BOB_PUBKEY =
  "bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260";

test.describe("NIP-IA hide archived from discovery", () => {
  test("members sidebar: archived member folds under Archived section", async ({
    page,
  }) => {
    await installMockBridge(page, { archivedIdentities: [ALICE_PUBKEY] });
    await page.goto("/");
    await page.getByTestId("channel-general").click();
    await expect(page.getByTestId("chat-title")).toHaveText("general");
    await page.getByTestId("channel-members-trigger").click();
    await expect(page.getByTestId("members-sidebar")).toBeVisible();

    // Active People list should NOT include Alice.
    const peopleList = page.getByTestId("members-sidebar-people");
    await expect(peopleList).not.toContainText("alice");

    // Folded Archived section is visible with count = 1.
    await expect(page.getByTestId("members-sidebar-archived")).toBeVisible();
    await expect(page.getByTestId("members-sidebar-archived-count")).toHaveText(
      "1",
    );

    // Expanding it reveals Alice.
    await page.getByTestId("members-sidebar-archived").click();
    await expect(
      page.getByTestId("members-sidebar-archived-list"),
    ).toContainText("alice");
  });

  test("members sidebar: no Archived section when no archived members", async ({
    page,
  }) => {
    await installMockBridge(page, { archivedIdentities: [] });
    await page.goto("/");
    await page.getByTestId("channel-general").click();
    await page.getByTestId("channel-members-trigger").click();
    await expect(page.getByTestId("members-sidebar")).toBeVisible();
    await expect(page.getByTestId("members-sidebar-archived")).toHaveCount(0);
  });

  test("mention autocomplete: archived member is filtered out", async ({
    page,
  }) => {
    await installMockBridge(page, { archivedIdentities: [ALICE_PUBKEY] });
    await page.goto("/");
    await page.getByTestId("channel-general").click();
    await expect(page.getByTestId("chat-title")).toHaveText("general");

    // Type `@a` to trigger autocomplete. Without filtering Alice would match.
    const input = page.getByTestId("message-input");
    await input.click();
    await input.pressSequentially("@a");

    // Alice's suggestion testid does NOT appear.
    await expect(
      page.getByTestId(`mention-suggestion-${ALICE_PUBKEY}`),
    ).toHaveCount(0);

    // Sanity: Bob (also `@b...` candidate) is available when his query starts;
    // proves the autocomplete itself is functional, not just always-empty.
    await input.fill("@b");
    await expect(
      page.getByTestId(`mention-suggestion-${BOB_PUBKEY}`),
    ).toBeVisible();
  });

  test("DM picker: archived user is omitted from search results", async ({
    page,
  }) => {
    await installMockBridge(page, { archivedIdentities: [ALICE_PUBKEY] });
    await page.goto("/");
    await page.getByTestId("new-dm-trigger").click();
    await expect(page.getByTestId("new-dm-dialog")).toBeVisible();

    await page.getByTestId("new-dm-search").fill("alice");
    // Alice's result row does NOT appear, even though search would normally
    // surface her by display name.
    await expect(page.getByTestId(`new-dm-result-${ALICE_PUBKEY}`)).toHaveCount(
      0,
    );

    // Sanity: Bob is still searchable, confirming the dialog works.
    await page.getByTestId("new-dm-search").fill("bob");
    await expect(page.getByTestId(`new-dm-result-${BOB_PUBKEY}`)).toBeVisible();
  });

  test("history invariant: archived user's existing message still renders with their name", async ({
    page,
  }) => {
    // Alice has a seeded message in #general from the prior PR's e2e setup
    // (see e2eBridge.ts seed). Archiving her must not remove that message.
    await installMockBridge(page, { archivedIdentities: [ALICE_PUBKEY] });
    await page.goto("/");
    await page.getByTestId("channel-general").click();
    await expect(page.getByTestId("chat-title")).toHaveText("general");

    // Alice's seed message renders with her display name in the timeline.
    const aliceMessage = page.getByTestId("message-row").nth(1);
    await expect(aliceMessage).toContainText("alice");
    await expect(aliceMessage).toContainText("Hey team — checking in.");
  });
});
