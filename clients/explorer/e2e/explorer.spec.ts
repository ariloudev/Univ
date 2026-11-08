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
  await expect(page.getByRole("region", { name: "The chosen object" }).getByTestId("lede")).toBeVisible();
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
  await page.getByText("Credits", { exact: true }).click();
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

test("centres the view on a point clicked, keeping the distance, with the grid and the distance rings drawn over it", async ({ page }) => {
  // The view moves at once, so that where things are drawn can be read without waiting.
  await page.emulateMedia({ reducedMotion: "reduce" });
  await firstPoints(page);
  // The grid takes no pointer events: the points under it are named and chosen as without it.
  await page.getByRole("switch", { name: "Grid" }).check();
  await expect(page.getByTestId("grid")).toBeVisible();
  await page.getByRole("switch", { name: "Rings" }).check();
  await expect(page.getByTestId("ring-labels").locator("span:visible").first()).toBeVisible();
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
  await expect(page.getByRole("region", { name: "The chosen object" }).getByTestId("lede")).toBeVisible();
  // The ring drawn over the chosen object, centred on it (its box's corner is at the object), at
  // the centre of the view as it now stands, narrowed by the record opened beside it.
  const mark = page.getByTestId("chosen-mark").locator("span");
  // The ring is placed on the frame after the object is read: until then it is not drawn.
  await expect
    .poll(async () => {
      const box = await mark.boundingBox();
      const centre = await centreOf(page.locator("canvas"));
      return box === null ? Number.POSITIVE_INFINITY : Math.hypot(box.x + box.width / 2 - centre.x, box.y + box.height / 2 - centre.y);
    })
    .toBeLessThan(3);
  await expect(distance).toHaveText(before);
});

test("finds an object by designation, and says when nothing is found", async ({ page }) => {
  const search = page.getByRole("textbox", { name: /Find an object/ });
  await search.fill("mcconnachie-2012:Sagittarius dSph");
  await page.getByRole("button", { name: "Find" }).click();
  await expect(page.getByRole("region", { name: "The chosen object" }).getByRole("heading").first()).toContainText("Sagittarius dSph");
  await search.fill("No such thing anywhere");
  await page.getByRole("button", { name: "Find" }).click();
  await expect(page.getByText("Nothing found for “No such thing anywhere”.")).toBeVisible();
});

test("lists the release's structures and reads one, culling the view to it from the top bar", async ({ page }) => {
  await page.getByRole("button", { name: "Structures" }).click();
  await page.getByTestId("structures").getByRole("button", { name: /^Bootes Void/ }).click();
  const panel = page.getByRole("region", { name: "The chosen structure" });
  await expect(panel.getByTestId("structure-kind")).toBeVisible();
  await expect(panel.getByTestId("lede")).toBeVisible();
  const inside = page.getByRole("banner").getByRole("checkbox", { name: "Only what lies inside" });
  await expect(inside).not.toBeChecked();
  await expect(panel.getByTestId("structure-inside")).toHaveCount(0);
  await inside.check();
  await expect(panel.getByTestId("structure-inside")).toBeVisible();
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
  await page.getByRole("button", { name: "Show", exact: true }).click();
  await page.getByLabel("Stars", { exact: true }).uncheck();
  await expect(page).toHaveURL(/hide=domain%3Astars/);
  await expect(page.getByTestId("show-summary")).toContainText("Points: 2 of 3 domains");
  await expect(stars).toContainText(/(^|\D)0 stars in the colour/);
  await page.reload();
  await page.getByRole("button", { name: "Show", exact: true }).click();
  await expect(page.getByLabel("Stars", { exact: true })).not.toBeChecked();
});

test("shows only one catalogue from its line, every other one off, and everything again from the summary", async ({ page }) => {
  await firstPoints(page);
  await page.getByRole("button", { name: "Show", exact: true }).click();
  const drawer = page.getByRole("complementary", { name: "Show" });
  const hipparcos = drawer.getByLabel("Hipparcos 2007", { exact: true });
  await hipparcos.hover();
  await drawer.getByRole("button", { name: "Only Hipparcos 2007" }).click();
  await expect(hipparcos).toBeChecked();
  await expect(drawer.getByLabel("Researched distances", { exact: true })).not.toBeChecked();
  await expect(page).toHaveURL(/source%3Aresearched-star-distances/);
  await expect(page).not.toHaveURL(/source%3Ahipparcos-2007/);
  await expect(drawer.getByTestId("show-summary")).toContainText(/1 of \d+ catalogues/);
  await drawer.getByRole("button", { name: "Show everything" }).click();
  await expect(page).not.toHaveURL(/hide=/);
});

test("has no serious or critical accessibility violations", async ({ page }) => {
  await firstPoints(page);
  await page.getByRole("button", { name: "Structures" }).click();
  await find(page, "canis MAJOR");
  await page.getByText("Key", { exact: true }).click();
  await page.getByRole("button", { name: "Show", exact: true }).click();
  const results = await new AxeBuilder({ page }).analyze();
  const serious = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(serious.map((v) => `${v.id}: ${v.help}`)).toEqual([]);
});

/** Whether two boxes overlap by more than a pixel. */
function overlap(a: { x: number; y: number; width: number; height: number }, b: { x: number; y: number; width: number; height: number }): boolean {
  return a.x + a.width - 1 > b.x && b.x + b.width - 1 > a.x && a.y + a.height - 1 > b.y && b.y + b.height - 1 > a.y;
}

async function boxOf(locator: Locator) {
  const box = await locator.boundingBox();
  if (box === null) throw new Error("not drawn");
  return box;
}

test("lays the view out as an atlas: bars, rail, drawer and record beside it at 1,280 pixels, never over it", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await firstPoints(page);
  await page.getByRole("button", { name: "Show", exact: true }).click();
  await find(page, "canis MAJOR");
  const view = await boxOf(page.getByRole("region", { name: "The view" }));
  const top = await boxOf(page.getByRole("banner"));
  const bottom = await boxOf(page.getByRole("contentinfo"));
  expect(top.height).toBeLessThanOrEqual(40);
  expect(bottom.height).toBeLessThanOrEqual(30);
  const rail = await boxOf(page.getByRole("navigation", { name: "Panels" }));
  expect(rail.width).toBeLessThanOrEqual(52);
  for (const part of [top, bottom, rail, await boxOf(page.getByRole("complementary", { name: "Show" })), await boxOf(page.getByRole("complementary", { name: "The record" }))]) {
    expect(overlap(part, view)).toBe(false);
  }
  // The edges carry their coordinates, and the bottom bar the scale.
  await expect(page.locator('[data-edge="top"]')).toHaveAttribute("data-coordinate", "galactic longitude");
  await expect(page.locator('[data-edge="top"] text').first()).toHaveText(/^\d+°$/);
  await expect(page.getByTestId("scale")).toContainText(/(pc|kpc|Mpc)$/);
  // The record folds to the window's edge, and the view widens.
  await page.getByRole("button", { name: "Fold the record" }).click();
  const widened = await boxOf(page.getByRole("region", { name: "The view" }));
  expect(widened.width).toBeGreaterThan(view.width + 300);
});

test("draws the grid across the view from its switch beside the axes, and keeps it across a reload", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await firstPoints(page);
  const toggle = page.getByRole("banner").getByRole("switch", { name: "Grid" });
  await expect(toggle).not.toBeChecked();
  await expect(page.getByTestId("grid")).toHaveCount(0);
  await toggle.check();
  await expect(page).toHaveURL(/grid=on/);
  const lines = page.getByTestId("grid-lines");
  await expect(lines).toHaveAttribute("d", /^M/);
  // The top bar keeps to one line with the switch in it.
  expect((await boxOf(page.getByRole("banner"))).height).toBeLessThanOrEqual(40);
  // A turn of the view turns the grid; a zoom leaves it still.
  const before = await lines.getAttribute("d");
  const distance = page.getByTestId("distance");
  const far = await distance.textContent();
  const view = await centreOf(page.locator("canvas"));
  await page.mouse.move(view.x, view.y);
  for (let i = 0; i < 5; i += 1) await page.mouse.wheel(0, -100);
  await expect(distance).not.toHaveText(far ?? "");
  expect(await lines.getAttribute("d")).toBe(before);
  await page.mouse.down();
  await page.mouse.move(view.x + 120, view.y + 40, { steps: 6 });
  await page.mouse.up();
  await expect.poll(() => lines.getAttribute("d")).not.toBe(before);
  await page.reload();
  await expect(page.getByRole("switch", { name: "Grid" })).toBeChecked();
  await expect(page.getByTestId("grid-lines")).toHaveAttribute("d", /^M/);
});

test("draws the distance rings from their switch beside the grid's, labelled, low over the plane without a WebGL error, and keeps them across a reload", async ({ page }) => {
  const errors: string[] = [];
  page.on("console", (message) => {
    // Chromium reports WebGL's errors, such as INVALID_OPERATION, as warnings.
    if (message.type() === "error" || /INVALID_|GL_INVALID|GL ERROR/.test(message.text())) errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1280, height: 800 });
  await firstPoints(page);
  const toggle = page.getByRole("banner").getByRole("switch", { name: "Rings" });
  await expect(toggle).not.toBeChecked();
  await expect(page.getByTestId("ring-labels").locator("span:visible")).toHaveCount(0);
  await toggle.check();
  await expect(page).toHaveURL(/rings=on/);
  const labels = page.getByTestId("ring-labels").locator("span:visible");
  await expect(labels.first()).toHaveText(/^[\d.,]+ (pc|kpc|Mpc|Gpc)$/);
  // The top bar keeps to one line with both switches in it.
  expect((await boxOf(page.getByRole("banner"))).height).toBeLessThanOrEqual(40);
  // About 4 degrees over the plane, where the far rings crowd and fade (the overlay's tests check
  // which labels go), the near rings stay labelled and the line shader draws without an error:
  // the view opens 20 degrees up, and each pixel dragged turns it by 0.005 radians.
  const view = await boxOf(page.getByRole("region", { name: "The view" }));
  await page.mouse.move(view.x + view.width / 2, view.y + view.height / 2);
  await page.mouse.down();
  await page.mouse.move(view.x + view.width / 2, view.y + view.height / 2 - 56, { steps: 6 });
  await page.mouse.up();
  await expect(labels.first()).toBeVisible();
  expect(errors).toEqual([]);
  await page.reload();
  await expect(page.getByRole("switch", { name: "Rings" })).toBeChecked();
  await expect(page.getByTestId("ring-labels").locator("span:visible").first()).toBeVisible();
});

test("opens a drawer and the record over the view below 1,024 pixels, and closes them again", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await firstPoints(page);
  const viewBefore = await boxOf(page.getByRole("region", { name: "The view" }));
  const structures = page.getByRole("button", { name: "Structures" });
  await structures.click();
  const drawer = page.getByRole("complementary", { name: "Structures" });
  expect(overlap(await boxOf(drawer), viewBefore)).toBe(true);
  expect((await boxOf(page.getByRole("region", { name: "The view" }))).width).toBe(viewBefore.width);
  await structures.click();
  await expect(drawer).toBeHidden();
  await find(page, "canis MAJOR");
  const record = page.getByRole("complementary", { name: "The record" });
  expect(overlap(await boxOf(record), viewBefore)).toBe(true);
  await page.getByRole("button", { name: "Close the object's panel" }).click();
  await expect(record).toBeHidden();
});

test("reads an object's record as a gazetteer entry: on the sky, its catalogues, and each value marked with its kind", async ({ page }) => {
  await find(page, "canis MAJOR");
  const panel = page.getByRole("region", { name: "The chosen object" });
  await expect(panel.getByTestId("record-position")).toContainText(/\d{2}h \d{2}m [\d.]+s, [+−]\d{2}° \d{2}′ [\d.]+″/);
  await expect(panel.getByTestId("record-position")).toContainText(/Galacticl [\d.]+°, b [+−][\d.]+°/);
  await expect(panel.getByTestId("catalogues-summary")).toBeVisible();
  await expect(panel.getByTestId("record-distance").locator("[data-kind]").first()).toBeVisible();
  await panel.getByText("Show every name, value and flag, with the papers").click();
  await expect(panel.getByTestId("catalogues")).toBeVisible();
  await expect(panel.getByTestId("sky")).toContainText(/RA \d{2}h \d{2}m [\d.]+s, Dec [+−]\d{2}° \d{2}′ [\d.]+″/);
});
