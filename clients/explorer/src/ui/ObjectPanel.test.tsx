import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ANDROMEDA } from "../test/objects.ts";
import { ObjectPanel } from "./ObjectPanel.tsx";

const close = () => undefined;

describe("the chosen object's panel", () => {
  it("gives the object's names and designations, each with its source", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: ANDROMEDA }} onChoose={close} onClose={close} />);
    const names = screen.getByTestId("names").textContent ?? "";
    expect(names).toContain("2557 (cosmicflows-4-galaxies)");
    expect(names).toContain("MESSIER031 (updated-nearby-galaxies)");
  });

  it("gives the preferred distance with its uncertainty, kind, source and rule", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: ANDROMEDA }} onChoose={close} onClose={close} />);
    const preferred = screen.getByTestId("preferred").textContent ?? "";
    expect(preferred).toContain("747 kpc (2.44 million ly) ± 2.1 %");
    expect(preferred).toContain("measured · cosmicflows-4-galaxies");
    expect(preferred).toContain("measured redshifts first then indicators over uncertain redshifts");
  });

  it("lists each other distance measurement beside the preferred one", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: ANDROMEDA }} onChoose={close} onClose={close} />);
    expect(screen.getByTestId("other-distances").textContent).toContain("783 kiloparsec (distance, measured, mcconnachie-2012)");
  });

  it("gives the frame and epoch of the object's position", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: ANDROMEDA }} onChoose={close} onClose={close} />);
    expect(screen.getByTestId("position-frame").textContent).toBe(
      "Position: ICRS equatorial Cartesian, about the Solar System barycentre, epoch 2000",
    );
  });

  it("lists the objects that replaced an id that was split, each to choose", async () => {
    const chosen = vi.fn();
    const choices = { kind: "choices" as const, query: "12", objects: [{ id: 3, names: [] }, { id: 4, names: [] }] };
    render(<ObjectPanel chosen={{ kind: "choices", choices }} onChoose={chosen} onClose={close} />);
    await userEvent.click(screen.getByRole("button", { name: "4" }));
    expect(chosen).toHaveBeenCalledWith(4);
  });

  it("gives the status and title Univ Server answered with", () => {
    render(<ObjectPanel chosen={{ kind: "failed", id: 9, status: 404, title: "No such object" }} onChoose={close} onClose={close} />);
    expect(screen.getByRole("alert").textContent).toBe("Univ Server answered 404: No such object");
  });
});
