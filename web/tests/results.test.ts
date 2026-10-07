import { describe, expect, test } from "bun:test";
import { defaultShown, magDb, phaseDeg } from "../src/plot/results.ts";
import type { Plot } from "../src/sim/types.ts";

describe("AC result helpers", () => {
  test("zero magnitude has no dB or phase value", () => {
    expect(magDb(0, 0)).toBeNull();
    expect(magDb(10, 0)).toBeCloseTo(20, 12);
    expect(magDb(null, 1)).toBeNull();
    expect(phaseDeg([0, 0, 1], [0, 1, 0])).toEqual([null, 90, 0]);
  });

  test("traces with no AC response are hidden by default", () => {
    const plot: Plot = {
      name: "ac1",
      type: "AC Analysis",
      complex: true,
      variables: [
        { name: "frequency", unit: "frequency", re: [1, 10], im: [0, 0] },
        { name: "v(vcc)", unit: "voltage", re: [0, 0], im: [0, 0] },
        { name: "v(out)", unit: "voltage", re: [-4, -4.5], im: [0.1, 0] },
      ],
    };
    expect([...defaultShown(plot)]).toEqual(["v(out)"]);
  });
});
