import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { structureCss } from "../../scene/kinds.ts";
import { DOMAINS, manifestOf } from "../../test/fixtures.ts";
import { NOTHING_HIDDEN, hiddenInAddress } from "../hidden.ts";
import { choicesOf } from "./choices.ts";
import { ShowPanel } from "./ShowPanel.tsx";

const choices = choicesOf(
  DOMAINS.map((name) => ({ name, manifest: manifestOf(name) })),
  [{ kind: "void" }, { kind: "supercluster" }, { kind: "void" }],
);

function drawer(search = "") {
  const onApply = vi.fn();
  const view = render(<ShowPanel choices={choices} hidden={hiddenInAddress(search)} onApply={onApply} />);
  return { onApply, view };
}

const box = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
const pressed = (name: string) => screen.getByRole("button", { name }).getAttribute("aria-pressed");

describe("the Show drawer", () => {
  it("offers every scale domain, each source of its points, the four kinds of value and each kind of structure, all on", () => {
    render(<ShowPanel choices={choices} hidden={NOTHING_HIDDEN} onApply={() => undefined} />);
    for (const label of ["Stars", "Nearby galaxies", "Cosmic web", "Hipparcos 2007", "Researched distances", "2MRS"]) expect(box(label).checked).toBe(true);
    for (const name of ["measured", "estimated", "inferred", "reconstructed", "void, 2 structures", "supercluster, 1 structure"]) expect(pressed(name)).toBe("true");
  });

  it("sums up what is on, and offers to show everything only while a group is off", async () => {
    const first = drawer();
    expect(screen.getByTestId("show-summary").textContent).toBe("Points: 3 of 3 domains, 6 of 6 catalogues, 4 of 4 kinds. Structures: 2 of 2 kinds.");
    expect(screen.queryByRole("button", { name: "Show everything" })).toBeNull();
    first.view.unmount();
    const { onApply } = drawer("?hide=domain:stars,structure:void");
    expect(screen.getByTestId("show-summary").textContent).toContain("Points: 2 of 3 domains");
    await userEvent.click(screen.getByRole("button", { name: "Show everything" }));
    expect(onApply).toHaveBeenCalledWith([
      { group: "domain", name: "stars", shown: true },
      { group: "structure", name: "void", shown: true },
    ]);
  });

  it("heads a column for each scale domain with its switch and its number of points", () => {
    drawer();
    const heads = within(screen.getByTestId("show-sources")).getAllByRole("columnheader");
    expect(heads.map((h) => h.textContent)).toEqual(["Catalogue", "Stars2", "Nearby9", "Web6"]);
    expect(heads[1]?.getAttribute("title")).toBe("Stars: 2 points");
  });

  it("lists each source once on one line by its short name, its title on hover, grouped by its smallest domain and marked in each domain it is in", () => {
    drawer();
    const rows = within(screen.getByTestId("show-sources")).getAllByRole("row").slice(1);
    expect(rows.map((r) => within(r).getByRole("checkbox").getAttribute("aria-label") ?? r.querySelector("label")?.textContent)).toEqual([
      "Researched distances",
      "Hipparcos 2007",
      "Cosmicflows-4 groups",
      "McConnachie 2012",
      "Karachentsev 2013",
      "2MRS",
    ]);
    expect(screen.getAllByLabelText("Cosmicflows-4 groups")).toHaveLength(1);
    const cf4 = rows[2];
    expect(cf4?.querySelector("label")?.getAttribute("title")).toBe("Cosmicflows-4 group distances");
    expect(within(cf4 as HTMLElement).getAllByRole("img").map((m) => m.getAttribute("aria-label"))).toEqual(["in nearby galaxies", "in cosmic web"]);
    expect(within(screen.getByTestId("show-sources")).getAllByRole("rowgroup")).toHaveLength(4);
  });

  it("turns a scale domain off or on alone, leaving every source's switch as it is", async () => {
    const { onApply } = drawer();
    await userEvent.click(box("Cosmic web"));
    expect(onApply).toHaveBeenLastCalledWith([{ group: "domain", name: "cosmic_web", shown: false }]);
  });

  it("offers only one source or kind of structure on its line, reachable by the keyboard", async () => {
    const { onApply } = drawer();
    const only = screen.getByRole("button", { name: "Only 2MRS" });
    only.focus();
    expect(document.activeElement).toBe(only);
    fireEvent.click(only);
    expect(onApply).toHaveBeenLastCalledWith(
      expect.arrayContaining([
        { group: "domain", name: "cosmic_web", shown: true },
        { group: "source", name: "twomrs", shown: true },
        { group: "source", name: "hipparcos-2007", shown: false },
      ]),
    );
    await userEvent.click(screen.getByRole("button", { name: "Only void" }));
    expect(onApply).toHaveBeenLastCalledWith([
      { group: "structure", name: "supercluster", shown: false },
      { group: "structure", name: "void", shown: true },
    ]);
  });

  it("marks each kind of value as the key does, hollow and not pressed while off", async () => {
    const { onApply } = drawer("?hide=kind:estimated");
    expect(screen.getByTestId("show-kinds").textContent).toBe("●measured◐estimated○inferred◌reconstructed");
    expect(within(screen.getByRole("button", { name: "measured" })).getByRole("img", { name: "measured" })).toBeTruthy();
    expect(pressed("estimated")).toBe("false");
    expect(screen.getByRole("button", { name: "estimated" }).className).toContain("border-dashed");
    await userEvent.click(screen.getByRole("button", { name: "estimated" }));
    expect(onApply).toHaveBeenCalledWith([{ group: "kind", name: "estimated", shown: true }]);
  });

  it("shows each kind of structure with its colour and count in its family, hollow and not pressed while off", () => {
    drawer("?hide=structure:void");
    const swatchOf = (name: string) => screen.getByRole("button", { name }).querySelector("span[aria-hidden]") as HTMLElement;
    // The browser writes the colour back with commas.
    expect(swatchOf("supercluster, 1 structure").style.background.replaceAll(",", "")).toBe(structureCss("supercluster"));
    expect(swatchOf("void, 2 structures").hasAttribute("data-hollow")).toBe(true);
    expect(pressed("void, 2 structures")).toBe("false");
    expect(screen.getByTestId("show-family-denser").textContent).toContain("Denser");
    expect(within(screen.getByTestId("show-family-emptier")).getByRole("button", { name: "void, 2 structures" })).toBeTruthy();
  });

  it("turns every group of the points or the structures on, or off, from their All and None", async () => {
    const { onApply } = drawer();
    await userEvent.click(screen.getByRole("button", { name: "No points" }));
    expect(onApply).toHaveBeenLastCalledWith(["stars", "nearby_galaxies", "cosmic_web"].map((name) => ({ group: "domain", name, shown: false })));
    await userEvent.click(screen.getByRole("button", { name: "All points" }));
    expect(onApply.mock.lastCall?.[0]).toHaveLength(3 + 6 + 4);
    await userEvent.click(screen.getByRole("button", { name: "No structures" }));
    expect(onApply).toHaveBeenLastCalledWith([
      { group: "structure", name: "supercluster", shown: false },
      { group: "structure", name: "void", shown: false },
    ]);
    await userEvent.click(screen.getByRole("button", { name: "All structures" }));
    expect(onApply.mock.lastCall?.[0]).toHaveLength(2);
  });
});
