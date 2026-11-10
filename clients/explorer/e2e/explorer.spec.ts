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

/** Where a point is drawn near the centre of the view: out from the centre until the pointer is
 * over a point (its label begins with its id), not over Earth at the focus or a structure's name. */
async function pointNearCentre(page: Page): Promise<{ x: number; y: number }> {
  const canvas = await centreOf(page.locator("canvas"));
  const tooltip = page.getByRole("tooltip");
  for (let r = 24; r <= 240; r += 6) {
    for (let a = 0; a < 2 * Math.PI; a += 6 / r) {
      const at = { x: canvas.x + r * Math.cos(a), y: canvas.y + r * Math.sin(a) };
      await page.mouse.move(at.x, at.y);
      // The label follows the pointer a frame later: what it names is read once it has.
      const names = async () => (await tooltip.isVisible()) && /^\d+ · /.test((await tooltip.textContent()) ?? "");
      if (await names()) {
        await page.waitForTimeout(50);
        if (await names()) return at;
      }
    }
  }
  throw new Error("no point is drawn near the centre");
}

/** A structure's name drawn where the view itself lies under it, on the screen and clear of the
 * panels: its id. The structures are read after the first points, so their names come a little
 * later. */
async function nameInTheView(page: Page): Promise<string> {
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
  return (await found.jsonValue()) ?? "";
}

/** How far Earth is from the centre of the view, in pixels: its mark's anchor, the top-left
 * corner of the element placed over it, is at the centre where Earth is the focus. */
async function earthFromCentre(page: Page): Promise<number> {
  const earth = await page.getByTitle(/^The Solar System barycentre/).boundingBox();
  const centre = await centreOf(page.locator("canvas"));
  return earth === null ? Number.POSITIVE_INFINITY : Math.hypot(earth.x - centre.x, earth.y - centre.y);
}

/** Past the time within which a second click near a click is part of a double-click. */
const PAST_A_DOUBLE_CLICK_MS = 600;

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
  const found = await pointNearCentre(page);
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

test("lets the chosen object go by a click on its point, making Earth the focus again, and keeps it chosen on a double-click", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await firstPoints(page);
  const found = await pointNearCentre(page);
  const panel = page.getByRole("region", { name: "The chosen object" });
  await page.mouse.click(found.x, found.y);
  await expect(panel.getByTestId("lede")).toBeVisible();
  // The point is now at the centre of the view, as the ring over it shows.
  const mark = page.getByTestId("chosen-mark").locator("span");
  await expect.poll(async () => (await mark.boundingBox()) !== null).toBe(true);
  const point = await centreOf(mark);
  await page.waitForTimeout(PAST_A_DOUBLE_CLICK_MS);
  await page.mouse.dblclick(point.x, point.y);
  await expect(panel.getByTestId("lede")).toBeVisible();
  await expect(page).toHaveURL(/\?object=\d+/);
  const distance = page.getByTestId("distance");
  const before = (await distance.textContent()) ?? "";
  const centre = await centreOf(mark);
  await page.waitForTimeout(PAST_A_DOUBLE_CLICK_MS);
  await page.mouse.click(centre.x, centre.y);
  await expect(panel).toHaveCount(0);
  await expect(mark).toBeHidden();
  await expect(page).not.toHaveURL(/object=/);
  await expect.poll(() => earthFromCentre(page)).toBeLessThan(3);
  await expect(distance).toHaveText(before);
});

test("closing a record with its close button lets go, making Earth the focus again", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  const distance = page.getByTestId("distance");
  // An object found is flown to; closing its record brings the focus home.
  await find(page, "canis MAJOR");
  await expect.poll(() => earthFromCentre(page)).toBeGreaterThan(3);
  let before = (await distance.textContent()) ?? "";
  await page.getByRole("button", { name: "Close the object's panel" }).click();
  await expect(page.getByRole("region", { name: "The chosen object" })).toHaveCount(0);
  await expect(page).not.toHaveURL(/object=/);
  await expect.poll(() => earthFromCentre(page)).toBeLessThan(3);
  await expect(distance).toHaveText(before);
  // A structure chosen from the list is flown to; closing its record brings the focus home.
  await page.getByRole("navigation", { name: "Panels" }).getByRole("button", { name: "Structures" }).click();
  await page.getByTestId("structures").getByRole("button", { name: /^Bootes Void/ }).click();
  await expect(page.getByRole("region", { name: "The chosen structure" })).toBeVisible();
  await expect.poll(() => earthFromCentre(page)).toBeGreaterThan(3);
  before = (await distance.textContent()) ?? "";
  await page.getByRole("button", { name: "Close the structure's panel" }).click();
  await expect(page.getByRole("region", { name: "The chosen structure" })).toHaveCount(0);
  await expect.poll(() => earthFromCentre(page)).toBeLessThan(3);
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
  // Chosen again from the list, it is let go.
  await page.getByTestId("structures").getByRole("button", { name: /^Bootes Void/ }).click();
  await expect(panel).toHaveCount(0);
  await expect(inside).toHaveCount(0);
});

test("lists the structures outward from Earth, under headings, each on one axis, and narrows them from the find box", async ({ page }) => {
  await page.getByRole("navigation", { name: "Panels" }).getByRole("button", { name: "Structures" }).click();
  const list = page.getByTestId("structures");
  const rows = list.getByRole("button");
  await expect(rows.first()).toBeVisible();
  const total = await rows.count();
  await expect(page.getByTestId("structures-count")).toHaveText(String(total));
  // Nearest first: each placed row's distance, in megaparsecs, no less than the one before.
  const mpc = (label: string): number | undefined => {
    const m = /, (around us|[\d,.]+ (Mpc|Gpc))$/.exec(label);
    if (m === null) return undefined;
    if (m[1] === "around us") return 0;
    const value = Number((m[1] ?? "").replace(/,/g, "").split(" ")[0]);
    return m[2] === "Gpc" ? value * 1000 : value;
  };
  const labels = await rows.evaluateAll((buttons) => buttons.map((b) => b.getAttribute("aria-label") ?? ""));
  const distances = labels.map(mpc).filter((d) => d !== undefined);
  expect(distances.length).toBeGreaterThan(0);
  expect(distances).toEqual([...distances].sort((a, b) => a - b));
  await expect(list.getByRole("heading", { level: 3 }).first()).toHaveText(/^(Within 100 Mpc|100 Mpc to 1 Gpc|1 to 10 Gpc|Not placed)\d+$/);
  expect(await page.getByTestId("depth-track").count()).toBe(total);
  // From the find box: only the voids, counted out of all.
  await page.getByRole("searchbox", { name: "Find a structure" }).fill("void");
  const voids = await rows.count();
  expect(voids).toBeGreaterThan(0);
  expect(voids).toBeLessThanOrEqual(total);
  await expect(page.getByTestId("structures-count")).toHaveText(`${voids} of ${total}`);
  await page.getByRole("searchbox", { name: "Find a structure" }).fill("no such structure");
  await expect(list).toHaveText("No structure's name or kind holds “no such structure”. Try part of a name, or a kind such as void.");
  // By name, A to Z, without headings.
  await page.getByRole("searchbox", { name: "Find a structure" }).fill("");
  await page.getByRole("button", { name: "A to Z" }).click();
  await expect(list.getByRole("heading", { level: 3 })).toHaveCount(0);
  const byName = await rows.evaluateAll((buttons) => buttons.map((b) => (b.getAttribute("aria-label") ?? "").split(", ")[0] ?? ""));
  expect(byName).toEqual([...byName].sort((a, b) => a.localeCompare(b)));
});

test("takes the structures' outlines away from their switch, still naming each and choosing it by its name, and keeps it off across a reload", async ({ page }) => {
  await firstPoints(page);
  const outlines = page.getByRole("banner").getByRole("switch", { name: "Outlines" });
  await expect(outlines).toBeChecked();
  await outlines.uncheck();
  await expect(page).toHaveURL(/outlines=off/);
  // A name still drawn where the view itself lies under it, as with the outlines on.
  const at = await centreOf(page.locator(`[data-structure="${await nameInTheView(page)}"] > span`));
  await page.mouse.click(at.x, at.y);
  await expect(page.getByRole("region", { name: "The chosen structure" })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("banner").getByRole("switch", { name: "Outlines" })).not.toBeChecked();
});

test("chooses a structure by the name it shows in the view, and still zooms over it", async ({ page }) => {
  await firstPoints(page);
  // A name's inner element is the text, centred on the structure's place.
  const id = await nameInTheView(page);
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

test("lets the chosen structure go by a click on its name, making Earth the focus again, and keeps it chosen on a double-click", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await firstPoints(page);
  const name = page.locator(`[data-structure="${await nameInTheView(page)}"]`);
  const text = name.locator("> span");
  const panel = page.getByRole("region", { name: "The chosen structure" });
  let at = await centreOf(text);
  await page.mouse.click(at.x, at.y);
  await expect(panel).toBeVisible();
  // Flown to at once, its name is placed on the next frame.
  await page.waitForTimeout(PAST_A_DOUBLE_CLICK_MS);
  at = await centreOf(text);
  await page.mouse.dblclick(at.x, at.y);
  await expect(panel).toBeVisible();
  await expect(name).toHaveClass(/font-medium/);
  const distance = page.getByTestId("distance");
  await page.waitForTimeout(PAST_A_DOUBLE_CLICK_MS);
  const before = (await distance.textContent()) ?? "";
  at = await centreOf(text);
  await page.mouse.click(at.x, at.y);
  await expect(panel).toHaveCount(0);
  await expect(name).not.toHaveClass(/font-medium/);
  await expect.poll(() => earthFromCentre(page)).toBeLessThan(3);
  await expect(distance).toHaveText(before);
});

test("keeps the drawer open across a reload, and none once it is closed", async ({ page }) => {
  const show = page.getByRole("navigation", { name: "Panels" }).getByRole("button", { name: "Show", exact: true });
  const drawer = page.locator("#drawer-show");
  await show.click();
  await expect(drawer).toBeVisible();
  await page.reload();
  await expect(drawer).toBeVisible();
  await expect(show).toHaveAttribute("aria-expanded", "true");
  await show.click();
  await expect(drawer).toHaveCount(0);
  await page.reload();
  await expect(page.getByText(/^Drawing release \d+/)).toBeVisible();
  await expect(drawer).toHaveCount(0);
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
  // The Show drawer is kept open across the reload.
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
