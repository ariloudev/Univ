import AxeBuilder from "@axe-core/playwright";
import { type Locator, type Page, expect, test } from "@playwright/test";

import { firstPointsMs } from "./browser.ts";

/** The first points drawn: the page has drawn some. */
const firstPoints = (page: Page) => firstPointsMs(page);

/** The centre of what `locator` finds, on the page. */
async function centreOf(locator: Locator): Promise<{ x: number; y: number }> {
  const box = await locator.boundingBox();
  if (box === null) throw new Error("not drawn");
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/** Finds `name` with the search box, and waits for its record. */
async function find(page: Page, name: string): Promise<void> {
  await page.getByRole("textbox", { name: /Find an object/ }).fill(name);
  await page.getByRole("button", { name: "Find" }).click();
  await expect(page.getByRole("region", { name: "The chosen object" }).getByTestId("names")).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText(/^Drawing release \d+/)).toBeVisible();
});

test("draws the latest release's points around Earth, stars by colour and the rest by kind, credited", async ({ page }) => {
  // How soon is measured by `cargo xtask bench first-points`, on the real release.
  expect(await firstPoints(page)).toBeGreaterThan(0);
  const stars = page.getByTestId("star-key");
  await expect(stars).toContainText("30,000 K");
  await expect(stars).toContainText(/[1-9][\d,]* stars in the colour of the temperature their colour index gives/);
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
  await find(page, "canis MAJOR");
  const panel = page.getByRole("region", { name: "The chosen object" });
  await expect(page).toHaveURL(/\?object=\d+/);
  const heading = await panel.getByRole("heading").first().textContent();
  await page.reload();
  await expect(page.getByRole("region", { name: "The chosen object" }).getByRole("heading").first()).toHaveText(heading ?? "");
});

test("centres the view on a point clicked, keeping the distance", async ({ page }) => {
  // The view moves at once, so that where things are drawn can be read without waiting.
  await page.emulateMedia({ reducedMotion: "reduce" });
  await firstPoints(page);
  const canvas = await centreOf(page.locator("canvas"));
  const tooltip = page.getByRole("tooltip");
  // Out from the centre until the pointer is over a point (its label begins with its id), not
  // over Earth at the focus or a structure's name.
  let found: { x: number; y: number } | undefined;
  for (let r = 24; r <= 240 && found === undefined; r += 6) {
    for (let a = 0; a < 2 * Math.PI && found === undefined; a += 6 / r) {
      const at = { x: canvas.x + r * Math.cos(a), y: canvas.y + r * Math.sin(a) };
      await page.mouse.move(at.x, at.y);
      // The label follows the pointer a frame later: what it names is read once it has.
      const names = async () => (await tooltip.isVisible()) && /^\d+ · /.test((await tooltip.textContent()) ?? "");
      if (await names()) {
        await page.waitForTimeout(50);
        if (await names()) found = at;
      }
    }
  }
  if (found === undefined) throw new Error("no point is drawn near the centre");
  const distance = page.getByTestId("distance");
  const before = (await distance.textContent()) ?? "";
  await page.mouse.click(found.x, found.y);
  await expect(page.getByRole("region", { name: "The chosen object" }).getByTestId("names")).toBeVisible();
  // The ring drawn over the chosen object, centred on it (its box's corner is at the object).
  const mark = page.getByTestId("chosen-mark").locator("span");
  // The ring is placed on the frame after the object is read: until then it is not drawn.
  await expect
    .poll(async () => {
      const box = await mark.boundingBox();
      return box === null ? Number.POSITIVE_INFINITY : Math.hypot(box.x + box.width / 2 - canvas.x, box.y + box.height / 2 - canvas.y);
    })
    .toBeLessThan(3);
  await expect(distance).toHaveText(before);
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
  // A name drawn where the view itself lies under it: on the screen and clear of the panels.
  // Its inner element is the text, centred on the structure's place.
  // The structures are read after the first points, so their names come a little later.
  const found = await page.waitForFunction(() => {
    const canvas = document.querySelector("canvas");
    for (const text of document.querySelectorAll("[data-structure] > span")) {
      const box = text.getBoundingClientRect();
      if (box.width > 0 && document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2) === canvas) {
        return text.parentElement?.dataset["structure"];
      }
    }
    return undefined;
  });
  const id = (await found.jsonValue()) ?? "";
  const text = page.locator(`[data-structure="${id}"] > span`);
  const centre = () => centreOf(text);
  const distance = page.getByTestId("distance");
  const before = (await distance.textContent()) ?? "";
  let at = await centre();
  await page.mouse.move(at.x, at.y);
  await expect(page.getByRole("tooltip")).toContainText("Click to read");
  await page.mouse.wheel(0, -300);
  await expect(distance).not.toHaveText(before);
  // As far back out: each step scales the distance about the focus by one factor, so the view,
  // and the name in it, come back to where they were.
  await page.mouse.wheel(0, 300);
  await expect(distance).toHaveText(before);
  await expect(text).toBeVisible();
  at = await centre();
  await page.mouse.click(at.x, at.y);
  await expect(page.getByRole("region", { name: "The chosen structure" })).toBeVisible();
});

test("turns a scale domain off, counts none of its points in the key, and keeps it off across a reload", async ({ page }) => {
  await firstPoints(page);
  const stars = page.getByTestId("star-key");
  await expect(stars).toContainText(/[1-9][\d,]* stars in the colour/);
  await page.getByText("Show", { exact: true }).click();
  await page.getByLabel("stars", { exact: true }).uncheck();
  await expect(page).toHaveURL(/hide=domain%3Astars/);
  await expect(stars).toContainText(/(^|\D)0 stars in the colour/);
  await page.reload();
  await page.getByText("Show", { exact: true }).click();
  await expect(page.getByLabel("stars", { exact: true })).not.toBeChecked();
});

test("has no serious or critical accessibility violations", async ({ page }) => {
  await firstPoints(page);
  await page.getByText(/^Structures \(\d+\)/).click();
  await page.getByText("Show", { exact: true }).click();
  const results = await new AxeBuilder({ page }).analyze();
  const serious = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(serious.map((v) => `${v.id}: ${v.help}`)).toEqual([]);
});
