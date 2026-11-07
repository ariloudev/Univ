import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { shapeOf } from "../test/structures.ts";
import { StructureList, StructurePanel, matchedText } from "./StructuresPanel.tsx";

const LOCAL_VOID: StructureDetail = {
  id: "local-void",
  name: "Local Void",
  kind: "void",
  scale_domain: "cosmic_web",
  centre: { x: 1, y: 2, z: 3, unit: "megaparsec" },
  bounds: { min: [0, 0, 0], max: [2, 4, 6] },
  shape: { frame: "icrs", unit: "megaparsec", geometry: { kind: "point", at: [1, 2, 3] } },
  derivation: { from: ["/geometry/Point"] },
  record: {
    papers: [{ id: "tully-2019", reference: "Tully, Pomarède, Graziani, Courtois, Hoffman & Shaya 2019, ApJ 880, 24" }],
    values: {
      density_at_deepest_minimum: {
        kind: "reconstructed",
        quantity: { value: -1.89, unit: "dimensionless", uncertainty: { form: "none_reported" } },
        cite: { paper: "tully-2019" },
      },
    },
  },
};

describe("the structures", () => {
  it("are listed by name and kind, each to choose", async () => {
    const chosen = vi.fn();
    render(<StructureList list={[{ id: "local-void", name: "Local Void", kind: "void", scale_domain: "cosmic_web" }]} onChoose={chosen} />);
    await userEvent.click(screen.getByText("Structures (1)"));
    await userEvent.click(screen.getByRole("button", { name: "Local Void (void)" }));
    expect(chosen).toHaveBeenCalledWith("local-void");
  });

  it("give a structure's kind, scale domain, how it was placed, and the paper each number is cited from", () => {
    render(<StructurePanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-kind").textContent).toBe("void · cosmic web");
    expect(screen.getByTestId("structure-matched").textContent).toBe("Placed by a point in icrs, derived from geometry Point.");
    const values = screen.getByTestId("structure-values").textContent ?? "";
    expect(values).toContain("density at deepest minimum: -1.89 dimensionless, no uncertainty reported");
    expect(values).toContain("reconstructed; Tully, Pomarède, Graziani, Courtois, Hoffman & Shaya 2019, ApJ 880, 24");
  });

  it("offer the cull to a structure's volume only where it has one, cleared until it is turned on", async () => {
    const set = vi.fn();
    const { rerender } = render(<StructurePanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
    expect(screen.queryByRole("checkbox", { name: "Only what lies inside" })).toBeNull();
    rerender(<StructurePanel detail={LOCAL_VOID} approximate={false} inside={{ on: false, approximate: false, set }} onClose={() => undefined} />);
    const box = screen.getByRole<HTMLInputElement>("checkbox", { name: "Only what lies inside" });
    expect(box.checked).toBe(false);
    expect(screen.queryByTestId("structure-inside")).toBeNull();
    await userEvent.click(box);
    expect(set).toHaveBeenCalledWith(true);
  });

  it("say, with the cull on, that points lie at their preferred distance, and where the cull is to a box, that it is approximate", () => {
    const set = () => undefined;
    const { rerender } = render(<StructurePanel detail={LOCAL_VOID} approximate={false} inside={{ on: true, approximate: false, set }} onClose={() => undefined} />);
    expect(screen.getByRole<HTMLInputElement>("checkbox", { name: "Only what lies inside" }).checked).toBe(true);
    const exact = screen.getByTestId("structure-inside").textContent ?? "";
    expect(exact).toContain("Points are placed at their preferred distance, so members scattered past its faces are not drawn.");
    expect(exact).not.toContain("Approximate");
    rerender(<StructurePanel detail={LOCAL_VOID} approximate={false} inside={{ on: true, approximate: true, set }} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-inside").textContent).toContain("Approximate: the cull is to a box about the structure, not its published shape.");
  });

  it("name a structure whose shape is a lower limit on its extent \"(at least)\" in the panel", () => {
    const vela = { ...LOCAL_VOID, name: "Vela Supercluster", derivation: { from: [], at_least: "/geometry/SkyRegion/at_least" } };
    const { unmount } = render(<StructurePanel detail={vela} approximate={false} onClose={() => undefined} />);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Vela Supercluster (at least)");
    unmount();
    render(<StructurePanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Local Void");
  });

  it("say a structure drawn by its extents has unknown bounds, and only then", () => {
    const axes: [[number, number, number], [number, number, number], [number, number, number]] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
    const { unmount } = render(<StructurePanel detail={{ ...LOCAL_VOID, derivation: { from: [], extents: { frame: "supergalactic", axes } } }} approximate={false} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-extents").textContent).toBe(
      "Its bounds are unknown: the lines are its published extents about its position, which need not be its middle.",
    );
    unmount();
    render(<StructurePanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
    expect(screen.queryByTestId("structure-extents")).toBeNull();
  });

  it("say an attractor's or a repeller's sphere is a symbol and which way its arrows point, and only for them", () => {
    const { unmount } = render(<StructurePanel detail={{ ...LOCAL_VOID, kind: "attractor" }} approximate={false} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-flow").textContent).toBe(
      "Its sphere is a symbol of a fixed size on the view, not its extent; its arrows point in toward it, where its reconstructed flow converges.",
    );
    unmount();
    const again = render(<StructurePanel detail={{ ...LOCAL_VOID, kind: "repeller" }} approximate={false} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-flow").textContent).toContain("out from it, where its reconstructed flow diverges");
    again.unmount();
    render(<StructurePanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
    expect(screen.queryByTestId("structure-flow")).toBeNull();
  });

  it("name a primitive shape by its form", () => {
    const sphere = shapeOf({ frame: "icrs", unit: "megaparsec", geometry: { kind: "primitive", primitive: { form: "sphere", centre: [0, 0, 0], radius: 1 } } });
    expect(matchedText({ ...LOCAL_VOID, shape: sphere, derivation: null }, false)).toBe("a sphere in icrs");
  });

  it("say when a structure is drawn by its bounding box, or not placed at all", () => {
    expect(matchedText({ ...LOCAL_VOID, shape: null, derivation: null }, true)).toBe("no placed shape, drawn as its bounding box (approximate)");
    expect(matchedText({ ...LOCAL_VOID, shape: null, bounds: null }, false)).toBe(
      "not placed: no shape and no bounding box, so it is listed but not drawn",
    );
  });
});
