import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ANDROMEDA, BETELGEUSE, UNTYPED_STAR } from "../test/objects.ts";
import { ANDROMEDA_GROUPED, HIP_32754, HIP_32754_SOURCES, M42 } from "../test/records.ts";
import { manifestOf } from "../test/fixtures.ts";
import { NO_CONTEXT, type RecordContext } from "./ObjectPanel.tsx";
import { ObjectPanel } from "./ObjectPanel.tsx";

const close = () => undefined;

describe("the chosen object's panel", () => {
  it("is headed by the name the object was found by, with its id beside it", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: ANDROMEDA, as: "Andromeda" }} onChoose={close} onClose={close} />);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Andromeda #856254");
  });

  it("is headed, when opened from the view, by the first proper name its sources give, or else its first designation in its catalogue's form", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: ANDROMEDA_GROUPED }} onChoose={close} onClose={close} />);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Andromeda Galaxy #856254");
    // HIP 32754: no proper name, and the Hipparcos number its Gaia entry gives as a second name
    // heads nothing.
    render(<ObjectPanel chosen={{ kind: "ready", record: HIP_32754 }} onChoose={close} onClose={close} />);
    expect(screen.getAllByRole("heading", { level: 2 })[1]?.textContent).toBe("HIP 32754 #17");
    // Where no form is known, the identifier with its source.
    render(<ObjectPanel chosen={{ kind: "ready", record: ANDROMEDA }} onChoose={close} onClose={close} />);
    expect(screen.getAllByRole("heading", { level: 2 })[2]?.textContent).toBe("2557 (cosmicflows-4-galaxies) #856254");
  });

  it("names a galaxy group as such, and the scale domain of every object, not \"body\"", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: ANDROMEDA }} onChoose={close} onClose={close} />);
    expect(screen.getByTestId("nature").textContent).toBe("In the nearby galaxies domain");
    render(<ObjectPanel chosen={{ kind: "ready", record: { ...ANDROMEDA, kind: "galaxy_group" } }} onChoose={close} onClose={close} />);
    expect(screen.getAllByTestId("nature")[1]?.textContent).toBe("A galaxy group, in the nearby galaxies domain");
    render(<ObjectPanel chosen={{ kind: "ready", record: { ...ANDROMEDA, position: null } }} onChoose={close} onClose={close} />);
    expect(screen.getAllByTestId("nature")[2]?.textContent).toBe("Not placed, so in no scale domain");
  });

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
    expect(preferred).toContain("measured, cosmicflows-4-galaxies; chosen by the rule");
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

  it("gives each published spectral type with its source and kind of value, and each colour index with its bands", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: BETELGEUSE }} onChoose={close} onClose={close} />);
    const star = screen.getByTestId("star").textContent ?? "";
    expect(star).toContain("M2Ib (measured, hipparcos-1997)");
    expect(star).toContain("B−V 1.5 ± 0.51 magnitude (measured, hipparcos-2007)");
    expect(star).not.toContain("Estimated class");
  });

  it("gives the class a colour index suggests, labelled as estimated and cited, where no source gives a type", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: UNTYPED_STAR }} onChoose={close} onClose={close} />);
    const star = screen.getByTestId("star").textContent ?? "";
    expect(star).toContain("Estimated class G2V, about 5,800 K");
    expect(star).toContain("as for a main-sequence star");
    expect(star).toContain("Pecaut & Mamajek 2013");
    expect(star).not.toContain("unknown");
  });

  it("estimates the class from the colour index the star is drawn by, and marks a derived index", () => {
    const derived = {
      entry: 0,
      source: "gcns",
      quantity: "colour_index",
      n: 0,
      kind_of_value: "measured",
      native: { value: 0.9, unit: "magnitude", uncertainty: { form: "none_reported" } },
      band: "BP-RP",
      derived_from: ["BPmag", "RPmag"],
    };
    const published = { ...derived, source: "hipparcos-2007", native: { ...derived.native, value: 0.65 }, band: "B-V", derived_from: null };
    const record = { ...UNTYPED_STAR, measurements: [derived, published] };
    render(<ObjectPanel chosen={{ kind: "ready", record }} onChoose={close} onClose={close} />);
    const star = screen.getByTestId("star").textContent ?? "";
    expect(star).toContain("Estimated class G2V");
    expect(star).toContain("from B−V 0.65 (hipparcos-2007)");
    expect(star).toContain("not corrected for interstellar reddening");
    expect(star).toContain("(measured, gcns, derived from BPmag − RPmag)");
  });

  it("names the papers of a researched entry", () => {
    render(<ObjectPanel chosen={{ kind: "ready", record: BETELGEUSE }} onChoose={close} onClose={close} />);
    const papers = screen.getByTestId("papers").textContent ?? "";
    expect(papers).toContain("Joyce, Leung, Molnár, Ireland, Kobayashi & Nomoto 2020, ApJ 902, 63");
    expect(papers).toContain("2020ApJ...902...63J");
    expect(screen.getByTestId("preferred").textContent).toContain("estimated, researched-star-distances");
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

const TITLES = new Map([
  ["hipparcos-2007", "Hipparcos, the new reduction"],
  ["hipparcos-1997", "The Hipparcos Catalogue"],
  ["gaia-dr3-bright", "Gaia DR3, bright stars"],
  ["gaia-dr3-hipparcos2", "Gaia DR3 best neighbours in Hipparcos-2"],
  ["bailer-jones-2021", "Bailer-Jones et al. 2021 distances"],
  ["cosmicflows-4", "Cosmicflows-4 groups"],
  ["openngc", "OpenNGC"],
]);

function context(overrides: Partial<RecordContext> = {}): RecordContext {
  return {
    ...NO_CONTEXT,
    sources: { titles: TITLES, details: new Map(HIP_32754_SOURCES.map((d) => [d.id, d])) },
    toGalactic: manifestOf("stars").rotations.to_galactic,
    ...overrides,
  };
}

const ready = (record: typeof HIP_32754, given = context()) =>
  render(<ObjectPanel chosen={{ kind: "ready", record }} onChoose={close} onClose={close} context={given} />);

describe("the chosen object's record, as a gazetteer entry", () => {
  it("gives each designation in its catalogue's form once, with the titles of every source that gives it", () => {
    ready(HIP_32754);
    const items = [...screen.getByTestId("names").querySelectorAll("li")].map((li) => li.textContent);
    expect(items).toContain("HIP 32754 (Hipparcos, the new reduction; The Hipparcos Catalogue)");
    expect(items).toContain("Gaia DR3 1000166358456525568 (Gaia DR3, bright stars; Gaia DR3 best neighbours in Hipparcos-2)");
    expect(items.filter((t) => t?.startsWith("HIP 32754"))).toHaveLength(1);
    // The Hipparcos number its Gaia entry gives as a second name is among its designations.
    expect(items.some((t) => t?.startsWith("32754 "))).toBe(false);
  });

  it("gives the proper names apart, and an entry's names only where they are not among them", () => {
    ready(M42);
    const items = [...screen.getByTestId("names").querySelectorAll("li")].map((li) => li.textContent);
    expect(items.slice(0, 2)).toEqual(["Great Orion Nebula (OpenNGC)", "Orion Nebula (OpenNGC)"]);
    expect(items.join("|")).not.toContain("Great Orion Nebula,Orion Nebula");
    expect(items).toContain("M 42 (OpenNGC)");
  });

  it("gives the sky position as right ascension and declination and as galactic longitude and latitude, each with frame and epoch", () => {
    ready(HIP_32754);
    const sky = screen.getByTestId("sky").textContent ?? "";
    expect(sky).toContain("RA 06h 49m 49.73s, Dec +55° 52′ 52.9″ (ICRS, epoch J2016, ± 0.010 mas)");
    // l and b as the ESA 1997 pole (α 192.85948°, δ +27.12825°, l of the celestial pole 122.93192°)
    // gives them for that position, worked out apart from the explorer.
    expect(sky).toContain("l 160.1451°, b +21.9622° (galactic, from that position, epoch J2016)");
    expect(sky).toContain("From Gaia DR3, bright stars");
  });

  it("gives each magnitude with its band, uncertainty, kind of value and source", () => {
    ready(HIP_32754);
    const brightness = screen.getByTestId("brightness").textContent ?? "";
    expect(brightness).toContain("Hpmag 10.5 magnitude, no uncertainty reported (measured, Hipparcos, the new reduction)");
    expect(brightness).toContain("phot_g_mean_mag 9.879 magnitude");
  });

  it("gives the proper motion, radial velocity and redshift with uncertainty, kind of value and source", () => {
    ready(HIP_32754);
    const motion = screen.getByTestId("motion").textContent ?? "";
    expect(motion).toContain("Proper motion μα* -15.5 milliarcsecond per year, no uncertainty reported (measured, Gaia DR3, bright stars)");
    expect(motion).toContain("Proper motion μδ -221.1 milliarcsecond per year");
    expect(motion).toContain("Radial velocity 27.25 ± 0.1954 kilometre per second (measured, Gaia DR3, bright stars)");
    ready(ANDROMEDA_GROUPED);
    expect(screen.getAllByTestId("motion")[1]?.textContent).toContain("Redshift -585 kilometre per second, no uncertainty reported, z = -0.001951");
  });

  it("names the group the object belongs to, to choose", async () => {
    const chosen = vi.fn();
    ready(ANDROMEDA_GROUPED, context({ onChooseDesignation: chosen }));
    expect(screen.getByTestId("groups").textContent).toBe("GroupsThe dominant galaxy of the group 2557 (Cosmicflows-4 groups)");
    await userEvent.click(screen.getByRole("button", { name: "2557" }));
    expect(chosen).toHaveBeenCalledWith("cosmicflows-4", "2557");
  });

  it("names each structure whose volume holds the object, to choose", async () => {
    const chosen = vi.fn();
    const holding = vi.fn(() => [{ id: "local-void", name: "Local Void" }]);
    ready(HIP_32754, context({ holding, onChooseStructure: chosen }));
    expect(holding).toHaveBeenCalledWith([-4.1965e-6, 18.9965e-6, 28.714e-6].map((v) => expect.closeTo(v, 12)));
    await userEvent.click(screen.getByRole("button", { name: "Local Void" }));
    expect(chosen).toHaveBeenCalledWith("local-void");
    ready(M42, context({ holding }));
    expect(screen.getAllByTestId("catalogues")).toHaveLength(2);
    expect(holding).toHaveBeenCalledTimes(1);
  });

  it("gives each flag of each entry with the meaning its source gives that value, or the value alone", () => {
    ready(HIP_32754);
    const catalogues = screen.getByTestId("catalogues").textContent ?? "";
    expect(catalogues).toContain("Sn 5: 5-parameter solution; single star (d = 0, s = 5).");
    expect(catalogues).toContain("xm_flag 8: The Gaia object has a five parameters astrometric solution.");
    expect(catalogues).toContain("flag 10033");
    expect(catalogues).not.toContain("flag 10033:");
  });

  it("marks each value with the symbol of its kind of value that the key shows", () => {
    ready(HIP_32754);
    const marks = (testid: string) => [...screen.getByTestId(testid).querySelectorAll("[data-kind]")].map((m) => [m.getAttribute("data-kind"), m.textContent]);
    expect(marks("preferred")).toEqual([["estimated", "◐"]]);
    expect(marks("brightness")).toEqual([
      ["measured", "●"],
      ["measured", "●"],
    ]);
    expect(marks("motion").every(([kind, mark]) => kind === "measured" && mark === "●")).toBe(true);
    // Every value line of the record carries one.
    for (const testid of ["brightness", "motion"]) {
      for (const li of screen.getByTestId(testid).querySelectorAll("li")) expect(li.querySelector("[data-kind]")).not.toBeNull();
    }
  });

  it("says an object of a sky position only is not placed, and why", () => {
    ready(M42);
    expect(screen.getByTestId("preferred").textContent).toContain("No distance: the object has a sky position only (no distance), so it is not drawn.");
    expect(screen.getByTestId("sky").textContent).toContain("RA 05h 35m 16.48s, Dec −05° 23′ 22.8″");
  });
});
