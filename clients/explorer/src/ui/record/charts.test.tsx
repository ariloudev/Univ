import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { KEY_TEMPERATURES } from "../../scene/starColour.ts";
import { ColourScale, DepthBand, DotPlot, LengthBars, TickStrip, keyPosition } from "./charts.tsx";

describe("the record's charts", () => {
  it("draw distances as dots on one axis, the preferred in the accent, and name each in words", () => {
    render(
      <DotPlot
        unit="pc"
        rows={[
          { label: "Bailer-Jones 2021", value: 297.8, low: 295.7, high: 299.8, preferred: true, text: "297.8 pc" },
          { label: "Hipparcos 2007, 1/ϖ", value: 211.4, low: 176.7, high: 263.2, text: "211 pc" },
        ]}
      />,
    );
    const plot = screen.getByRole("img", { name: /Bailer-Jones 2021: 297.8 pc; Hipparcos 2007, 1\/ϖ: 211 pc/ });
    const dots = plot.querySelectorAll("circle");
    expect(dots).toHaveLength(2);
    expect(dots[0]?.getAttribute("fill")).toBe("var(--color-accent)");
    expect(dots[1]?.getAttribute("fill")).toBe("var(--color-dim)");
  });

  it("draw many values as ticks, a band between two ends with its marks, and lengths as bars, a lower limit open", () => {
    render(<TickStrip values={[-594, -300, -33]} marks={[{ value: -300, label: "−300" }]} unit="km/s" label="224 spectra" />);
    expect(screen.getByRole("img", { name: "224 spectra" }).querySelectorAll("line[stroke-opacity]")).toHaveLength(3);
    render(<DepthBand from={233.5} to={348} marks={[{ value: 262.3, label: "W1" }]} unit="Mpc" label="Depth" />);
    expect(screen.getByRole("img", { name: "Depth" }).querySelectorAll("circle")).toHaveLength(1);
    render(<LengthBars unit="Mpc" rows={[{ label: "Long", value: 119, open: true, text: "≥ 119", kind: "estimated" }]} />);
    expect(screen.getByRole("img", { name: "Long: ≥ 119 Mpc" }).querySelector("rect")?.getAttribute("fill")).toMatch(/^url\(#open-Mpc\)$/);
  });

  it("place a star's BP−RP on the key's gradient by the temperature it gives", () => {
    expect(keyPosition(KEY_TEMPERATURES[0] ?? 0)).toBe(0);
    expect(keyPosition(KEY_TEMPERATURES.at(-1) ?? 0)).toBe(1);
    expect(keyPosition(1e6)).toBe(1);
    expect(keyPosition(100)).toBe(0);
    render(<ColourScale index={0.251} />);
    expect(screen.getByRole("img", { name: /^BP−RP 0.251: about [\d,]+ K, [A-F]\d/ }).dataset["testid"]).toBe("colour-scale");
  });
});
