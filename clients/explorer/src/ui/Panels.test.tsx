import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import type { Controller } from "../scene/controller.ts";
import { POINT_CSS } from "../scene/kinds.ts";
import { fakeController } from "../test/world.ts";
import { manifestOf } from "../test/fixtures.ts";
import { KeyBar, type Tally, ViewDrawer, Whereabouts } from "./Panels.tsx";
import { Scene } from "./Scene.tsx";
import { createStore } from "./store.ts";

function controller(): Controller {
  const made = fakeController();
  made.setRotations(manifestOf("stars").rotations);
  return made;
}

const SOURCE = manifestOf("stars").sources[0];

function tally(failed = 0, unread = 0): Tally {
  return {
    kinds: [1200, 30, 4, 0],
    stars: { coloured: 98_500, grey: 1500 },
    failed,
    unread,
    credits: () => (SOURCE === undefined ? [] : [{ ...SOURCE, domain: "stars" }]),
  };
}

function show(catalogue: Tally, made = controller()) {
  const frame = createStore(0);
  render(
    <>
      <Whereabouts controller={made} frame={frame} />
      <ViewDrawer controller={made} frame={frame} cosmology="Planck 2018" />
      <KeyBar catalogue={() => catalogue} frame={frame} release={7} />
    </>,
  );
  return made;
}

describe("the bars and the view's drawer", () => {
  it("count the points of each kind of value under their one colour", () => {
    show(tally());
    const key = screen.getByTestId("key").textContent ?? "";
    expect(key).toContain("measured: 1,200 points");
    expect(key).toContain("estimated: 30 points");
    expect(key).toContain("inferred: 4 points");
    expect(key).toContain("reconstructed: 0 points");
    const swatch = screen.getByRole("heading", { name: /Other points/ }).querySelector("span") as HTMLElement;
    const probe = document.createElement("span");
    probe.style.background = POINT_CSS;
    expect(swatch.style.background).toBe(probe.style.background);
    expect(swatch.style.background).not.toBe("");
  });

  it("show the stars' colours by temperature and count the stars in colour and in grey", () => {
    show(tally());
    const key = screen.getByTestId("star-key").textContent ?? "";
    expect(key).toContain("2,500 K");
    expect(key).toContain("30,000 K");
    expect(key).toContain("98,500 stars in the colour of the temperature their colour index gives");
    expect(key).toContain("Pecaut & Mamajek 2013");
    expect(key).toContain("1,500 stars in grey, with no colour index");
  });

  it("credit each source of the drawn points with its title, citation and credit, naming the release", () => {
    show(tally());
    expect(screen.getByText(/The sources of release 7's points/)).toBeTruthy();
    const credits = screen.getByTestId("credits").textContent ?? "";
    expect(SOURCE).toBeDefined();
    expect(credits).toContain(SOURCE?.title);
    expect(credits).toContain(SOURCE?.citation);
    expect(credits).toContain(SOURCE?.credit);
  });

  it("state the tiles that failed their check, and those that could not be read", () => {
    show(tally(2, 1));
    expect(screen.getByText("2 tiles failed their check and are not drawn.")).toBeTruthy();
    expect(screen.getByText("1 tiles could not be read from Univ Server and are not drawn.")).toBeTruthy();
  });

  it("name the frame of the axes, the origin, the units and the cosmology", () => {
    show(tally());
    const frame = screen.getByTestId("frame").textContent ?? "";
    expect(frame).toContain("Axes galactic (Hipparcos");
    expect(frame).toContain("Solar System barycentre, within about 1 AU of Earth");
    expect(frame).toContain("megaparsecs");
    expect(frame).toContain("cosmology Planck 2018");
  });

  it("show the distance from the eye to the focus", () => {
    const made = controller();
    made.flyTo([30, 0, 0], 2);
    show(tally(), made);
    expect(screen.getByTestId("distance").textContent).toContain("2 Mpc (6.52 million ly)");
  });

  it("give each kind of value its mark, in the bar and in the key", () => {
    show(tally());
    expect(screen.getByTestId("key").textContent).toContain("●measured: 1,200 points");
    expect(screen.getByTestId("key").textContent).toContain("◌reconstructed: 0 points");
  });

  it("change the axes the view shows", async () => {
    const made = show(tally());
    await userEvent.selectOptions(screen.getByRole("combobox"), "supergalactic");
    expect(made.axes).toBe("supergalactic");
  });
});

describe("the view", () => {
  it("marks Earth as the Solar System barycentre, and says when it cannot draw", async () => {
    render(<Scene release={7} cosmology="Planck 2018" levelOfDetail bar={{ start: null, end: null }} />);
    expect(screen.getByTitle("The Solar System barycentre, within about 1 AU of Earth").textContent).toContain("Earth");
    expect((await screen.findByRole("alert")).textContent).toContain("cannot draw with WebGL2");
  });
});
