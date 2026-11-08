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
  render(<ShowPanel choices={choices} hidden={hiddenInAddress(search)} onApply={onApply} />);
  return onApply;
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
    drawer();
    expect(screen.getByTestId("show-summary").textContent).toBe("Points: 3 of 3 domains, 6 of 6 catalogues, 4 of 4 kinds. Structures: 2 of 2 kinds.");
    expect(screen.queryByRole("button", { name: "Show everything" })).toBeNull();
    const onApply = drawer("?hide=domain:stars,structure:void");
    expect(screen.getAllByTestId("show-summary")[1]?.textContent).toContain("Points: 2 of 3 domains");
    await userEvent.click(screen.getByRole("button", { name: "Show everything" }));
    expect(onApply).toHaveBeenCalledWith([
      { group: "domain", name: "stars", shown: true },
      { group: "structure", name: "void", shown: true },
    ]);
  });

  it("heads each domain's sources with its counts, names each on one line by its short name with its title on hover, and marks one in two domains", () => {
    drawer();
    const nearby = screen.getByTestId("show-domain-nearby_galaxies");
    expect(nearby.textContent).toContain("9 · 3 catalogues");
    const row = within(nearby).getByLabelText("Cosmicflows-4 groups").closest("label");
    expect(row?.getAttribute("title")).toBe("Cosmicflows-4 group distances");
    expect(within(nearby).getByRole("img", { name: "Also in cosmic web" })).toBeTruthy();
    expect(nearby.textContent).toContain("↔ also in cosmic web, and switched there too.");
    expect(within(screen.getByTestId("show-domain-stars")).queryByRole("img")).toBeNull();
  });

  it("folds a domain's sources under its heading and opens them again", async () => {
    drawer();
    const fold = screen.getByRole("button", { name: "Stars's catalogues" });
    expect(fold.getAttribute("aria-expanded")).toBe("true");
    await userEvent.click(fold);
    expect(screen.queryByLabelText("Hipparcos 2007")).toBeNull();
    await userEvent.click(fold);
    expect(screen.getByLabelText("Hipparcos 2007")).toBeTruthy();
  });

  it("shows a domain's switch mixed while some of its sources are off, and turns it on with all of them, or off alone", async () => {
    const onApply = drawer("?hide=source:mcconnachie-2012");
    expect(box("Nearby galaxies").indeterminate).toBe(true);
    expect(box("Stars").indeterminate).toBe(false);
    await userEvent.click(box("Nearby galaxies"));
    expect(onApply).toHaveBeenLastCalledWith([
      { group: "domain", name: "nearby_galaxies", shown: true },
      { group: "source", name: "cosmicflows-4", shown: true },
      { group: "source", name: "mcconnachie-2012", shown: true },
      { group: "source", name: "updated-nearby-galaxies", shown: true },
    ]);
    await userEvent.click(box("Stars"));
    expect(onApply).toHaveBeenLastCalledWith([{ group: "domain", name: "stars", shown: false }]);
  });

  it("offers only one source or kind of structure on its line, reachable by the keyboard", async () => {
    const onApply = drawer();
    const only = screen.getByRole("button", { name: "Only 2MRS" });
    only.focus();
    expect(document.activeElement).toBe(only);
    fireEvent.click(only);
    expect(onApply).toHaveBeenLastCalledWith(expect.arrayContaining([{ group: "domain", name: "cosmic_web", shown: true }, { group: "source", name: "twomrs", shown: true }, { group: "source", name: "hipparcos-2007", shown: false }]));
    await userEvent.click(screen.getByRole("button", { name: "Only void" }));
    expect(onApply).toHaveBeenLastCalledWith([
      { group: "structure", name: "supercluster", shown: false },
      { group: "structure", name: "void", shown: true },
    ]);
  });

  it("marks each kind of value as the key does, hollow and not pressed while off", async () => {
    const onApply = drawer("?hide=kind:estimated");
    expect(screen.getByTestId("show-kinds").textContent).toBe("●measured◐estimated○inferred◌reconstructed");
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
    const onApply = drawer();
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
