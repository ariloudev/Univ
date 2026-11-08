import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { StructureDetail } from "../api/structures.ts";
import { R12_HUGE_LQG, R12_KBC_VOID, R12_VELA } from "../test/records/index.ts";
import { shapeOf } from "../test/structures.ts";
import { StructureRecordPanel } from "./record/StructureRecordPanel.tsx";
import { matchedText } from "./record/structureText.ts";
import { StructureList } from "./StructuresPanel.tsx";

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
    expect(screen.getByRole("heading", { name: "Structures (1)" })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Local Void (void)" }));
    expect(chosen).toHaveBeenCalledWith("local-void");
  });

  it("give a structure's kind, scale domain, how it was placed, and the paper each number is cited from", () => {
    render(<StructureRecordPanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-kind").textContent).toBe("void · cosmic web domain");
    expect(screen.getByTestId("record-drawn").textContent).toContain("Drawn as a point in icrs.");
    fireEvent.click(screen.getByText("Show the record's notes, quotes and every value"));
    expect(screen.getByTestId("structure-matched").textContent).toBe("Placed by a point in icrs, derived from geometry Point.");
    const values = screen.getByTestId("structure-values").textContent ?? "";
    expect(values).toContain("density at deepest minimum: −1.89 dimensionless, no uncertainty reported");
    expect(values).toContain("reconstructed; Tully, Pomarède, Graziani, Courtois, Hoffman & Shaya 2019, ApJ 880, 24");
  });

  it("say, only with the cull on, that points lie at their preferred distance, and where the cull is to a box, that it is approximate", () => {
    const set = () => undefined;
    const { rerender } = render(<StructureRecordPanel detail={LOCAL_VOID} approximate={false} inside={{ on: false, approximate: false, set }} onClose={() => undefined} />);
    expect(screen.queryByTestId("structure-inside")).toBeNull();
    expect(screen.queryByRole("checkbox", { name: "Only what lies inside" })).toBeNull();
    rerender(<StructureRecordPanel detail={LOCAL_VOID} approximate={false} inside={{ on: true, approximate: false, set }} onClose={() => undefined} />);
    const exact = screen.getByTestId("structure-inside").textContent ?? "";
    expect(exact).toContain("Points are placed at their preferred distance, so members scattered past its faces are not drawn.");
    expect(exact).not.toContain("Approximate");
    rerender(<StructureRecordPanel detail={LOCAL_VOID} approximate={false} inside={{ on: true, approximate: true, set }} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-inside").textContent).toContain("Approximate: the cull is to a box about the structure, not its published shape.");
  });

  it("name a structure whose shape is a lower limit on its extent \"(at least)\" in the panel", () => {
    const vela = { ...LOCAL_VOID, name: "Vela Supercluster", derivation: { from: [], at_least: "/geometry/SkyRegion/at_least" } };
    const { unmount } = render(<StructureRecordPanel detail={vela} approximate={false} onClose={() => undefined} />);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Vela Supercluster (at least)");
    unmount();
    render(<StructureRecordPanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Local Void");
  });

  it("say a structure drawn by its extents has unknown bounds, and only then", () => {
    const axes: [[number, number, number], [number, number, number], [number, number, number]] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
    const { unmount } = render(<StructureRecordPanel detail={{ ...LOCAL_VOID, derivation: { from: [], extents: { frame: "supergalactic", axes } } }} approximate={false} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-extents").textContent).toBe(
      "Its bounds are unknown: the lines are its published extents about its position, which need not be its middle.",
    );
    unmount();
    render(<StructureRecordPanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
    expect(screen.queryByTestId("structure-extents")).toBeNull();
  });

  it("say an attractor's or a repeller's sphere is a symbol and which way its arrows point, and only for them", () => {
    const { unmount } = render(<StructureRecordPanel detail={{ ...LOCAL_VOID, kind: "attractor" }} approximate={false} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-flow").textContent).toBe(
      "Its sphere is a symbol of a fixed size on the view, not its extent; its arrows point in toward it, where its reconstructed flow converges.",
    );
    unmount();
    const again = render(<StructureRecordPanel detail={{ ...LOCAL_VOID, kind: "repeller" }} approximate={false} onClose={() => undefined} />);
    expect(screen.getByTestId("structure-flow").textContent).toContain("out from it, where its reconstructed flow diverges");
    again.unmount();
    render(<StructureRecordPanel detail={LOCAL_VOID} approximate={false} onClose={() => undefined} />);
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

describe("the chosen structure's record panel", () => {
  const PLANCK_2018 = { name: "Planck 2018 (Planck Collaboration 2020)", hubble_constant: 67.66, omega_matter: 0.3111 };
  const panel = (detail: StructureDetail) => render(<StructureRecordPanel detail={detail} approximate={false} cosmology={PLANCK_2018} onClose={() => undefined} />);

  it("leads with one sentence of what it is, and draws its depth as a band with its walls marked", () => {
    panel(R12_VELA);
    expect(screen.getByTestId("lede").textContent).toBe("A supercluster 234–348 Mpc away, spanning at least 124 Mpc.");
    expect(screen.getByRole("img", { name: "Depth from 233.5 Mpc to 348.0 Mpc; wall 1 at 262.3 Mpc; wall 2 at 319.5 Mpc" })).toBeTruthy();
  });

  it("draws its lengths as bars on one axis for each unit, a lower limit open, with what they were published as", () => {
    panel(R12_VELA);
    const bars = screen.getByRole("img", { name: "Long: ≥ 119 Mpc; Short: ≥ 93.1 Mpc; Wall 1 extent: ≥ 124 Mpc" });
    expect([...bars.querySelectorAll("rect")].map((r) => r.getAttribute("fill"))).toEqual(["url(#open-Mpc)", "url(#open-Mpc)", "url(#open-Mpc)"]);
    expect(screen.getByTestId("record-size").textContent).toContain("Published as 115, 90, 120 Mpc at H₀ = 70, drawn at H₀ = 67.66.");
  });

  it("draws each member of a structure placed by its members as a tick", () => {
    panel(R12_HUGE_LQG);
    const strip = screen.getByRole("img", { name: "73 members from 3813.0 Mpc to 4232.6 Mpc; mean z at 4017.3 Mpc" });
    expect(strip.querySelectorAll("line[stroke-opacity]")).toHaveLength(73);
  });

  it("says a disputed structure is disputed, above its values, naming each paper that disputes it", () => {
    panel(R12_KBC_VOID);
    const disputed = screen.getByTestId("disputed");
    expect(disputed.textContent).toBe("Disputed by Kenworthy et al. 2019.");
    expect(disputed.compareDocumentPosition(screen.getByTestId("record-depth")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("names its papers once by first author and year, and holds its notes, quotes and every value under one link", () => {
    panel(R12_VELA);
    expect(screen.getByTestId("record-papers").querySelector("li")?.textContent).toBe("Kraan-Korteweg et al. 2017, MNRAS Letters 466, L29");
    expect(screen.queryByTestId("structure-values")).toBeNull();
    fireEvent.click(screen.getByText("Show the record's notes, quotes and every value"));
    expect(screen.getByTestId("structure-notes").textContent).toContain("The region drawn is the one the paper measures the overdensity in");
    expect(screen.getByTestId("structure-values").textContent).toContain("The main wall extends over at least 120 h70 Mpc.");
  });
});

describe("a structure whose record the panel cannot read", () => {
  it("is still named and its drawing explained, and the panel says what it could not read", () => {
    render(<StructureRecordPanel detail={{ ...LOCAL_VOID, record: { disputes: [{}] } }} approximate={false} onClose={() => undefined} />);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Local Void");
    expect(screen.getByTestId("structure-unread").textContent).toBe(
      "Its record could not be read in full (record.disputes.0.cite: Invalid input: expected object, received undefined), so only how it is drawn is shown.",
    );
    expect(screen.getByTestId("record-drawn").textContent).toContain("Drawn as a point in icrs.");
  });
});
