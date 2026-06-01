import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

// Exercises the generic file-attachment UI contract end-to-end through the
// mock Tauri bridge: paperclip upload → composer chip → send → FileCard in the
// timeline. This guards the frontend wiring (the riskiest, previously
// untested path). It does NOT prove the real relay store/serve round-trip —
// that lives in the Rust media + relay tests.

test.beforeEach(async ({ page }) => {
  await installMockBridge(page, {
    uploadDescriptors: [
      {
        url: `https://mock.relay/media/${"a".repeat(64)}.pdf`,
        sha256: "a".repeat(64),
        size: 12345,
        type: "application/pdf",
        uploaded: Math.floor(Date.now() / 1000),
        filename: "quarterly-report.pdf",
      },
    ],
  });
});

test("upload a file and see a FileCard in the timeline", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");

  // Paperclip → mocked pick_and_upload_media returns the PDF descriptor.
  await page.getByRole("button", { name: "Attach image" }).click();

  // The composer shows a chip with the original filename.
  await expect(page.getByTestId("message-composer")).toContainText(
    "quarterly-report.pdf",
  );

  // Send the (attachment-only) message.
  await page.getByTestId("send-message").click();

  // A FileCard renders in the timeline: a download link carrying the filename
  // and pointing at the blob URL.
  const card = page.getByTestId("file-card");
  await expect(card).toBeVisible();
  await expect(card).toContainText("quarterly-report.pdf");
  await expect(card).toHaveAttribute(
    "href",
    `https://mock.relay/media/${"a".repeat(64)}.pdf`,
  );
  await expect(card).toHaveAttribute("download", "quarterly-report.pdf");
});
