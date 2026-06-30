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
  for (const a of await conn.animations.list()) await conn.animations.delete(a.handle);
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

test("shows a VTL line and toggles its level", async ({ page }) => {
  // Register a named input line server-side, then load the UI.
  const conn = await Connection.connect(BACKEND);
  await conn.vtl.setName(0, 1, "input", "trig");
  await conn.vtl.setInput("trig", false); // known starting level
  conn.close();

  await page.goto("/");
  await expect(page.getByText("connected")).toBeVisible();

  // The named line appears in the Trigger Lines panel, starting low.
  const row = page.locator("tr", { hasText: "trig" });
  await expect(row).toBeVisible();
  const indicator = row.locator("span[title]");
  await expect(indicator).toHaveAttribute("title", "low");

  // The toggle button drives the line high (reconciled via the next snapshot).
  await row.getByRole("button", { name: "toggle" }).click();
  await expect(indicator).toHaveAttribute("title", "high");
});

test("lists an animation and arms it", async ({ page }) => {
  // Create a stimulus + a flash animation server-side, then load the UI.
  const conn = await Connection.connect(BACKEND);
  const h = await conn.stimuli.shapes.createRect({ name: "anim-rect" });
  await conn.animations.flash(h, { durationFrames: 30, name: "fl" });
  conn.close();

  await page.goto("/");
  await expect(page.getByText("connected")).toBeVisible();

  // The animation appears in the panel (polled) with its canonical type tag.
  const row = page.locator("tr", { hasText: "fl" });
  await expect(row).toContainText("FlashForNFrames");
  await expect(row).toContainText("idle");

  // Arming starts it; the polled state leaves idle (armed → running → done).
  await row.getByRole("button", { name: "arm", exact: true }).click();
  await expect(row).not.toContainText("idle");
});

test("system: Hide all disables every stimulus", async ({ page }) => {
  const conn = await Connection.connect(BACKEND);
  await conn.stimuli.shapes.createRect({ name: "sys-rect" });
  conn.close();

  await page.goto("/");
  await expect(page.getByText("connected")).toBeVisible();

  const checkbox = page.locator("tr", { hasText: "sys-rect" }).locator("input[type=checkbox]");
  await expect(checkbox).toBeChecked();

  await page.getByRole("button", { name: "Hide all" }).click();
  await expect(checkbox).not.toBeChecked(); // reconciled via the next snapshot
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
