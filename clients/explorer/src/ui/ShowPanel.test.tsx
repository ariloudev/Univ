import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { manifestOf } from "../test/fixtures.ts";
import { NOTHING_HIDDEN, hiddenInAddress } from "./hidden.ts";
import { ShowPanel, choicesOf } from "./ShowPanel.tsx";

const DOMAINS = ["stars", "cosmic_web"].map((name) => ({ name, manifest: manifestOf(name) }));
const STRUCTURES = [{ kind: "void" }, { kind: "supercluster" }, { kind: "void" }];

describe("the Show panel", () => {
  it("offers every domain, every source once by its title, the four kinds and each kind of structure with its count", () => {
    const choices = choicesOf(DOMAINS, STRUCTURES);
    expect(choices.domains).toEqual(["stars", "cosmic_web"]);
    const ids = choices.sources.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(choices.structureKinds).toEqual([
      { kind: "void", count: 2 },
      { kind: "supercluster", count: 1 },
    ]);
    render(<ShowPanel choices={choices} hidden={NOTHING_HIDDEN} onToggle={() => undefined} />);
    const first = choices.sources[0];
    if (first === undefined) throw new Error("no source in the fixtures");
    for (const label of ["stars", "cosmic web", first.title, "measured", "reconstructed", "void (2)", "supercluster (1)"]) {
      expect((screen.getByLabelText(label) as HTMLInputElement).checked).toBe(true);
    }
  });

  it("shows a group turned off unchecked, and asks to turn a group on or off when its box is clicked", async () => {
    const onToggle = vi.fn();
    const choices = choicesOf(DOMAINS, STRUCTURES);
    render(<ShowPanel choices={choices} hidden={hiddenInAddress("?hide=kind:estimated")} onToggle={onToggle} />);
    expect((screen.getByLabelText("estimated") as HTMLInputElement).checked).toBe(false);
    await userEvent.click(screen.getByText("Show"));
    await userEvent.click(screen.getByLabelText("cosmic web"));
    expect(onToggle).toHaveBeenCalledWith("domain", "cosmic_web", false);
    await userEvent.click(screen.getByLabelText("estimated"));
    expect(onToggle).toHaveBeenCalledWith("kind", "estimated", true);
  });
});
