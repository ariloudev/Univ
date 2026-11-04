import AxeBuilder from "@axe-core/playwright";
import { type Page, expect, test } from "@playwright/test";

/** Milliseconds from the page's start to the first points drawn. */
async function firstPoints(page: Page): Promise<number> {
  await page.waitForFunction(() => performance.getEntriesByName("univ:first-points").length > 0, null, { timeout: 30_000 });
  return page.evaluate(() => performance.getEntriesByName("univ:first-points")[0]?.startTime ?? Number.NaN);
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText(/^Drawing release \d+/)).toBeVisible();
});

test("draws the latest release's points around Earth, counted by kind and credited", async ({ page }) => {
  expect(await firstPoints(page)).toBeLessThan(3000);
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

test("lists the release's structures, reads one, and still turns the view over its name", async ({ page }) => {
  await firstPoints(page);
  await page.getByText(/^Structures \(\d+\)/).click();
  await page.getByTestId("structures").getByRole("button").first().click();
  const panel = page.getByRole("region", { name: "The chosen structure" });
  await expect(panel.getByTestId("structure-kind")).toBeVisible();
  // Flown to, the structure's name lies at the centre of the view: the wheel there still zooms,
  // and a click there chooses the structure by its name.
  await page.waitForTimeout(1500);
  const distance = page.getByTestId("distance");
  const before = await distance.textContent();
  const box = await page.locator("canvas").boundingBox();
  if (box === null) throw new Error("no canvas");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, -300);
  await expect(distance).not.toHaveText(before ?? "");
  await panel.getByRole("button", { name: "Close the structure's panel" }).click();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(panel).toBeVisible();
});

test("has no serious or critical accessibility violations", async ({ page }) => {
  await firstPoints(page);
  await page.getByText(/^Structures \(\d+\)/).click();
  const results = await new AxeBuilder({ page }).analyze();
  const serious = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(serious.map((v) => `${v.id}: ${v.help}`)).toEqual([]);
});
