import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { DepthRow } from "./depth.ts";
import { DepthIndex, FIND_KEY, ORDER_KEY } from "./DepthIndex.tsx";

const row = (id: string, name: string, kind: string, distance: number | undefined, known: DepthRow["known"] = "observed"): DepthRow => ({ id, name, kind, colour: "#9e73ff", known, distance });

const ROWS: DepthRow[] = [
  row("sloan", "Sloan Great Wall", "wall", 307),
  row("local-void", "Local Void", "void", 36, "reconstructed"),
  row("giant-arc", "Giant Arc", "arc", 2820, "statistical_claim"),
  row("sa68", "SA 68 Supercluster", "supercluster", undefined),
  row("bootes", "Bootes Void", "void", 226),
  row("perseus", "Perseus arm (Reid 2019 model)", "spiral_arm", 0.00412, "model"),
];

const none = () => undefined;

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});
const names = (): string[] => within(screen.getByTestId("structures")).getAllByRole("button").map((b) => (b.getAttribute("aria-label") ?? "").split(",")[0] ?? "");

describe("the Structures drawer as a depth index (see-the-structures)", () => {
  it("lists each structure by name and kind, to choose", async () => {
    const chosen = vi.fn();
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={chosen} />);
    const bootes = screen.getByRole("button", { name: "Bootes Void, void, observed, 226 Mpc" });
    expect(within(bootes).getByText("Bootes Void").className).toContain("italic");
    expect(within(bootes).getByText("void")).toBeTruthy();
    await userEvent.click(bootes);
    expect(chosen).toHaveBeenCalledWith("bootes");
  });

  it("lists the structures nearest first, under headings with their counts, those not placed last", () => {
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    expect(names()).toEqual(["Perseus arm (Reid 2019 model)", "Local Void", "Bootes Void", "Sloan Great Wall", "Giant Arc", "SA 68 Supercluster"]);
    const headings = screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
    expect(headings).toEqual(["Within 30 kpc1", "30 kpc to 100 Mpc1", "100 Mpc to 1 Gpc2", "1 to 10 Gpc1", "Not placed1"]);
  });

  it("gives each structure's distance, and draws a dot on the shared axis for each one placed", () => {
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    expect(within(screen.getByRole("button", { name: /^Giant Arc/ })).getByText("2.82 Gpc")).toBeTruthy();
    expect(within(screen.getByRole("button", { name: /^SA 68/ })).getByText("not placed")).toBeTruthy();
    expect(within(screen.getByRole("button", { name: /^Perseus arm/ })).getByText("4.12 kpc")).toBeTruthy();
    const dots = screen.getAllByTestId("depth-track").map((track) => track.querySelectorAll("circle").length);
    expect(dots).toEqual([1, 1, 1, 1, 1, 0]);
    expect(screen.getAllByText(/^(1 kpc|100 kpc|10 Mpc|1 Gpc)$/).map((t) => t.textContent)).toEqual(["1 kpc", "100 kpc", "10 Mpc", "1 Gpc"]);
  });

  it("marks how each structure is known on its dot, as the legend says", () => {
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    expect(screen.getByTestId("depth-legend").textContent).toBe("observedstatistical claimreconstructedmodel");
    const dot = (name: RegExp) => screen.getByRole("button", { name }).querySelector("circle");
    const known = (name: RegExp) => screen.getByRole("button", { name }).querySelector("[data-known]")?.getAttribute("data-known");
    expect(known(/^Bootes Void/)).toBe("observed");
    expect(dot(/^Bootes Void/)?.getAttribute("fill")).toBe("#9e73ff");
    expect(known(/^Giant Arc/)).toBe("statistical_claim");
    expect(dot(/^Giant Arc/)?.getAttribute("fill")).toBe("var(--color-panel)");
    expect(known(/^Local Void/)).toBe("reconstructed");
    expect(dot(/^Local Void/)?.getAttribute("stroke-dasharray")).toBe("1.8 1.8");
    expect(screen.getByRole("button", { name: /^Giant Arc/ }).getAttribute("aria-label")).toContain("statistical claim");
    // A model's dot is half filled, in the legend too.
    expect(known(/^Perseus arm/)).toBe("model");
    expect(dot(/^Perseus arm/)?.getAttribute("fill")).toBe("var(--color-panel)");
    expect(screen.getByRole("button", { name: /^Perseus arm/ }).querySelector("[data-half] path")?.getAttribute("fill")).toBe("#9e73ff");
    expect(screen.getByTestId("depth-legend").querySelectorAll("[data-half]")).toHaveLength(1);
  });

  it("marks a structure known in no way in the dim, and says nothing of it", () => {
    render(<DepthIndex rows={[{ ...row("x", "Unread Wall", "wall", 300), known: undefined }]} chosen={undefined} onChoose={none} />);
    const button = screen.getByRole("button", { name: "Unread Wall, wall, 300 Mpc" });
    expect(button.querySelector("circle")?.getAttribute("fill")).toBe("var(--color-dim)");
    expect(button.querySelector("[data-known]")).toBeNull();
  });

  it("orders the list by name instead from its switch, without headings, keeping each dot", async () => {
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    await userEvent.click(screen.getByRole("button", { name: "A to Z" }));
    expect(screen.getByRole("button", { name: "A to Z" }).getAttribute("aria-pressed")).toBe("true");
    expect(names()).toEqual(["Bootes Void", "Giant Arc", "Local Void", "Perseus arm (Reid 2019 model)", "SA 68 Supercluster", "Sloan Great Wall"]);
    expect(screen.queryAllByRole("heading", { level: 3 })).toEqual([]);
    expect(screen.getAllByTestId("depth-track")).toHaveLength(6);
  });

  it("narrows the list to the structures whose name or kind holds the text typed, counting them out of all", async () => {
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    expect(screen.getByTestId("structures-count").textContent).toBe("6");
    await userEvent.type(screen.getByRole("searchbox", { name: "Find a structure" }), "VOID");
    expect(names()).toEqual(["Local Void", "Bootes Void"]);
    expect(screen.getByTestId("structures-count").textContent).toBe("2 of 6");
    await userEvent.clear(screen.getByRole("searchbox", { name: "Find a structure" }));
    await userEvent.type(screen.getByRole("searchbox", { name: "Find a structure" }), "wall");
    expect(names()).toEqual(["Sloan Great Wall"]);
  });

  it("says so where no structure's name or kind holds the text typed", async () => {
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    await userEvent.type(screen.getByRole("searchbox", { name: "Find a structure" }), "Andromeda");
    expect(screen.getByTestId("structures").textContent).toBe("No structure's name or kind holds “Andromeda”. Try part of a name, or a kind such as void.");
    expect(screen.getByTestId("structures-count").textContent).toBe("0 of 6");
  });

  it("says where no structure is drawn to list", () => {
    render(<DepthIndex rows={[]} chosen={undefined} onChoose={none} />);
    expect(screen.getByTestId("structures").textContent).toBe("No structures are drawn. Turn their kinds on in the Show drawer.");
    expect(screen.getByTestId("structures-count").textContent).toBe("0");
  });

  it("keeps the order and the find text across visits", async () => {
    const { unmount } = render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    await userEvent.click(screen.getByRole("button", { name: "A to Z" }));
    await userEvent.type(screen.getByRole("searchbox", { name: "Find a structure" }), "void");
    expect(localStorage.getItem(ORDER_KEY)).toBe("name");
    expect(localStorage.getItem(FIND_KEY)).toBe("void");
    unmount();
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    expect(screen.getByRole("button", { name: "A to Z" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole<HTMLInputElement>("searchbox", { name: "Find a structure" }).value).toBe("void");
    expect(names()).toEqual(["Bootes Void", "Local Void"]);
  });

  it("forgets the order and the find text once they are back to nearest first and empty", async () => {
    localStorage.setItem(ORDER_KEY, "name");
    localStorage.setItem(FIND_KEY, "void");
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    await userEvent.click(screen.getByRole("button", { name: "Nearest first" }));
    await userEvent.clear(screen.getByRole("searchbox", { name: "Find a structure" }));
    expect(localStorage.getItem(ORDER_KEY)).toBeNull();
    expect(localStorage.getItem(FIND_KEY)).toBeNull();
  });

  it("opens nearest first with an empty find box where nothing is kept, or local storage is refused", () => {
    const { unmount } = render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    expect(screen.getByRole("button", { name: "Nearest first" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole<HTMLInputElement>("searchbox", { name: "Find a structure" }).value).toBe("");
    unmount();
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("The operation is insecure.", "SecurityError");
    });
    render(<DepthIndex rows={ROWS} chosen={undefined} onChoose={none} />);
    expect(screen.getByRole("button", { name: "Nearest first" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole<HTMLInputElement>("searchbox", { name: "Find a structure" }).value).toBe("");
    expect(names()).toHaveLength(6);
  });

  it("marks the chosen structure's row as chosen, and no other", () => {
    render(<DepthIndex rows={ROWS} chosen="bootes" onChoose={none} />);
    const pressed = within(screen.getByTestId("structures"))
      .getAllByRole("button")
      .filter((b) => b.getAttribute("aria-pressed") === "true");
    expect(pressed.map((b) => b.getAttribute("aria-label"))).toEqual(["Bootes Void, void, observed, 226 Mpc"]);
    expect(pressed[0]?.className).toContain("bg-raised");
  });
});
