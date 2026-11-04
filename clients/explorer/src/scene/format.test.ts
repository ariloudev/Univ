import { describe, expect, it } from "vitest";

import { formatDistance } from "./format.ts";

describe("distances", () => {
  it("are given in the parsec unit that suits them, with light-years beside", () => {
    expect(formatDistance(1.3e-6)).toBe("1.3 pc (4.24 ly)");
    expect(formatDistance(8.2e-3)).toBe("8.2 kpc (26.7 thousand ly)");
    expect(formatDistance(16.77)).toBe("16.8 Mpc (54.7 million ly)");
    expect(formatDistance(4200)).toBe("4.2 Gpc (13.7 billion ly)");
    expect(formatDistance(1e-7)).toBe("0.1 pc (0.326 ly)");
  });
});
