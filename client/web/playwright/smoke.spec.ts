// Browser smoke test for the React UI against a real (isolated) vstimd --null.
// Covers the paths no other test exercises: the app boots, connects, renders,
// creates a stimulus, and the canvas drag drives a position change (the manual
// receptive-field mapping interaction).

import { expect, test } from "@playwright/test";
import { Connection } from "../src/index.js";

// Backend web port from playwright.config.ts. Reset the scene before each test
// (the --null server persists across tests) using the same client, node-side.
const BACKEND = "ws://127.0.0.1:8138";

test.beforeEach(async () => {
  const conn = await Connection.connect(BACKEND);
  await conn.system.deleteAll();
  conn.close();
});

test("boots and connects", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("connected")).toBeVisible();
  // Server info header (resolution/refresh/version) is populated under --null.
  await expect(page.getByText(/\d+×\d+ @ \d+ Hz/)).toBeVisible();
});

test("creates a stimulus", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("connected")).toBeVisible();

  await page.getByRole("button", { name: "+ Rect" }).click();

  // The new stimulus appears in the panel at the origin.
  const row = page.locator("table tbody tr").first();
  await expect(row).toContainText("rect");
  await expect(row).toContainText("0, 0");
});

test("drag on the map moves the stimulus (RF mapping)", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("connected")).toBeVisible();

  await page.getByRole("button", { name: "+ Circle" }).click();
  const row = page.locator("table tbody tr").first();
  await expect(row).toContainText("0, 0");

  // The stimulus is created at (0,0) → rendered at the canvas centre. Drag it.
  const canvas = page.locator("canvas");
  const box = (await canvas.boundingBox())!;
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;

  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 120, cy + 80, { steps: 8 });
  await page.mouse.up();

  // Position should no longer be the origin; the next snapshot reconciles it.
  await expect(row).not.toContainText("0, 0");
});
