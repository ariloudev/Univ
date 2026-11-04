import AxeBuilder from "@axe-core/playwright";
import { type Page, expect, test } from "@playwright/test";

import { firstPointsMs } from "./browser.ts";

/** The first points drawn: the page has drawn some. */
const firstPoints = (page: Page) => firstPointsMs(page);

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText(/^Drawing release \d+/)).toBeVisible();
});

test("draws the latest release's points around Earth, counted by kind and credited", async ({ page }) => {
  // How soon is measured by `cargo xtask bench first-points`, on the real release.
  expect(await firstPoints(page)).toBeGreaterThan(0);
  const key = page.getByTestId("key");
  await expect(key).toContainText(/measured: [\d,]+ points/);
  await page.getByText(/Credits: the sources of release \d+/).click();
  await expect(page.getByTestId("credits").locator("li").first()).toBeVisible();
  await expect(page.getByTitle("The Solar System barycentre, within about 1 AU of Earth")).toBeVisible();
});

test("zooms with the wheel, turns with a drag and returns to Earth", async ({ page }) => {
  await firstPoints(page);
  const distance = page.getByTestId("distance");
  const before = await distance.textContent();
  const canvas = page.locator("canvas");
  const box = await canvas.boundingBox();
  if (box === null) throw new Error("no canvas");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  for (let i = 0; i < 5; i += 1) await page.mouse.wheel(0, -100);
  await expect(distance).not.toHaveText(before ?? "");
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 200, box.y + box.height / 2, { steps: 8 });
  await page.mouse.up();
  await page.getByRole("button", { name: "Return to Earth" }).click();
  await expect(page.getByTitle("The Solar System barycentre, within about 1 AU of Earth")).toBeInViewport();
});

test("finds an object by name, reads it, and keeps it in the address across a reload", async ({ page }) => {
  await page.getByRole("textbox", { name: /Find an object/ }).fill("canis MAJOR");
  await page.getByRole("button", { name: "Find" }).click();
  const panel = page.getByRole("region", { name: "The chosen object" });
  await expect(panel.getByTestId("names")).toBeVisible();
  await expect(page).toHaveURL(/\?object=\d+/);
  const heading = await panel.getByRole("heading").first().textContent();
  await page.reload();
  await expect(page.getByRole("region", { name: "The chosen object" }).getByRole("heading").first()).toHaveText(heading ?? "");
});

test("finds an object by designation, and says when nothing is found", async ({ page }) => {
  const search = page.getByRole("textbox", { name: /Find an object/ });
  await search.fill("mcconnachie-2012:Sagittarius dSph");
  await page.getByRole("button", { name: "Find" }).click();
  await expect(page.getByRole("region", { name: "The chosen object" }).getByTestId("names")).toContainText("Sagittarius dSph");
  await search.fill("No such thing anywhere");
  await page.getByRole("button", { name: "Find" }).click();
  await expect(page.getByText("Nothing found for “No such thing anywhere”.")).toBeVisible();
});

test("lists the release's structures and reads one", async ({ page }) => {
  await page.getByText(/^Structures \(\d+\)/).click();
  await page.getByTestId("structures").getByRole("button").first().click();
  await expect(page.getByRole("region", { name: "The chosen structure" }).getByTestId("structure-kind")).toBeVisible();
});

test("chooses a structure by the name it shows in the view, and still zooms over it", async ({ page }) => {
  await firstPoints(page);
  const name = page.locator("[data-structure]").filter({ visible: true }).first();
  await expect(name).toBeVisible();
  const box = await name.boundingBox();
  if (box === null) throw new Error("no name drawn");
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  const distance = page.getByTestId("distance");
  const before = await distance.textContent();
  await page.mouse.move(x, y);
  await expect(page.getByRole("tooltip")).toContainText("Click to read");
  await page.mouse.wheel(0, -300);
  await expect(distance).not.toHaveText(before ?? "");
  await page.getByRole("button", { name: "Return to Earth" }).click();
  await page.mouse.click(x, y);
  await expect(page.getByRole("region", { name: "The chosen structure" })).toBeVisible();
});

test("has no serious or critical accessibility violations", async ({ page }) => {
  await firstPoints(page);
  await page.getByText(/^Structures \(\d+\)/).click();
  const results = await new AxeBuilder({ page }).analyze();
  const serious = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(serious.map((v) => `${v.id}: ${v.help}`)).toEqual([]);
});
