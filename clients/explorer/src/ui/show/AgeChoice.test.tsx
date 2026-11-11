import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { manifestOf } from "../../test/fixtures.ts";
import type { AgeCut } from "../age.ts";
import { NOTHING_HIDDEN } from "../hidden.ts";
import { createStore } from "../store.ts";
import type { AgeState } from "./AgeChoice.tsx";
import { choicesOf } from "./choices.ts";
import { ShowPanel } from "./ShowPanel.tsx";

/** The choices of the fixture domains, the Galaxy's tracers among them where `tracers`. */
const choicesWith = (tracers: boolean) =>
  choicesOf(
    ["stars", ...(tracers ? ["galaxy_tracers"] : [])].map((name) => ({ name, manifest: manifestOf(name) })),
    [],
  );

function drawer(cut: AgeCut, tracers = true) {
  const choose = vi.fn();
  const onApply = vi.fn();
  const age: AgeState = { cut, choose, frame: createStore(0), held: () => ({ tracers: 10, aged: 3 }) };
  render(<ShowPanel choices={choicesWith(tracers)} hidden={NOTHING_HIDDEN} onApply={onApply} age={age} />);
  return { choose, onApply };
}

const pressed = (name: string) => screen.getByRole("button", { name }).getAttribute("aria-pressed");

describe("the Show drawer's age of the Galaxy's tracers", () => {
  it("offers any age, or younger than 100, 30 or 10 million years, any age pressed as the view opens", () => {
    drawer(undefined);
    expect(pressed("Tracers of any age")).toBe("true");
    for (const cut of [100, 30, 10]) expect(pressed(`Tracers younger than ${cut} million years`)).toBe("false");
  });

  it("names the share of the tracers held that carry an age, which it acts on", () => {
    drawer(undefined);
    expect(screen.getByTestId("show-age-share").textContent).toContain("3 of the 10 tracers held carry an age");
  });

  it("chooses an age", async () => {
    const { choose } = drawer(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Tracers younger than 30 million years" }));
    expect(choose).toHaveBeenCalledWith(30);
  });

  it("says the age chosen in its summary, and shows tracers of any age from All and from Show everything", async () => {
    const { choose, onApply } = drawer(30);
    expect(pressed("Tracers younger than 30 million years")).toBe("true");
    expect(screen.getByTestId("show-summary").textContent).toContain("tracers younger than 30 million years");
    await userEvent.click(screen.getByRole("button", { name: "Show everything" }));
    expect(choose).toHaveBeenLastCalledWith(undefined);
    choose.mockClear();
    await userEvent.click(screen.getByRole("button", { name: "All points" }));
    expect(choose).toHaveBeenLastCalledWith(undefined);
    expect(onApply).toHaveBeenCalled();
  });

  it("is not offered where the release has no Galaxy tracers", () => {
    drawer(undefined, false);
    expect(screen.queryByTestId("show-age")).toBeNull();
  });
});
