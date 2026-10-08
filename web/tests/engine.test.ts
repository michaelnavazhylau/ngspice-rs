import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { EXAMPLES, PARAMS_FILE } from "../src/circuit/examples.ts";
import { generateNetlist } from "../src/circuit/netlist.ts";
import type { SimResult } from "../src/sim/types.ts";

const wasm = new URL("../src/wasm/pkg/spice_wasm_bg.wasm", import.meta.url).pathname;
const have = existsSync(wasm);

describe.skipIf(!have)("wasm engine (needs `bun run build:wasm`)", async () => {
  const pkg = have ? await import("../src/wasm/pkg/spice_wasm.js") : null!;
  if (have) pkg.initSync({ module: await Bun.file(wasm).arrayBuffer() });

  test("version", () => {
    expect(pkg.version().length).toBeGreaterThan(0);
  });

  for (const ex of EXAMPLES) {
    test(`example ${ex.id}`, () => {
      const deck = generateNetlist(ex.doc, ex.analysis).deck;
      const files = ex.files.length ? ex.files : [PARAMS_FILE];
      const res = JSON.parse(pkg.simulate(deck, files.map((f) => f.name), files.map((f) => f.content))) as SimResult;
      expect(res.plots.length).toBe(1);
      const plot = res.plots[0]!;
      expect(plot.variables.length).toBeGreaterThan(1);
      if (ex.analysis.kind !== "op") expect(plot.variables[0]!.re.length).toBeGreaterThan(5);
      if (ex.id === "divider-op") {
        const out = plot.variables.find((v) => v.name === "v(out)")!;
        expect(out.re[0]).toBeCloseTo(10 * (2000 / 3000), 6);
      }
      if (ex.analysis.kind === "ac") expect(plot.variables[1]!.im).toBeDefined();
    });
  }

  test(".measure, .four, .print and .save come back with the plot", () => {
    const deck =
      "rc lowpass\nv1 in 0 pulse(0 1 0 1n 1n 0.5m 1m)\nr1 in out 1k\nc1 out 0 1u\n" +
      ".tran 1u 5m\n.save v(out)\n.meas tran vmax max v(out)\n.four 1k v(in)\n.end\n";
    const r = JSON.parse(pkg.simulate(deck, [], [])) as SimResult;
    // .save keeps v(out) only; .four still transforms v(in) from the full plot.
    expect(r.plots[0]!.variables.map((v) => v.name)).toEqual(["time", "v(out)"]);
    expect(r.printed).toBeNull();
    expect(r.measurements!.map((m) => m.name)).toEqual(["vmax"]);
    expect(r.measurements![0]!.unit).toBe("voltage");
    const four = r.fourier![0]!;
    expect(four.vector).toBe("v(in)");
    expect(four.dc!).toBeCloseTo(0.5, 2);
    expect(four.harmonics[0]!.amplitude!).toBeCloseTo(2 / Math.PI, 2);
  });

  test("subcircuits from virtual include files elaborate", () => {
    const deck = 's\n.include "parts/div.inc"\nv1 in 0 dc 10\nx1 in out div\nr2 out 0 1k\n.op\n.end\n';
    const r = JSON.parse(pkg.simulate(deck, ["parts/div.inc"], [".subckt div a b\nr1 a b 1k\n.ends div\n"])) as SimResult;
    expect(r.plots[0]!.variables.find((v) => v.name === "v(out)")!.re[0]).toBe(5);
  });

  test("errors are thrown with a message", () => {
    // A missing virtual include stays an error whatever devices are ported.
    expect(() =>
      pkg.simulate('t\n.include "missing.inc"\nr1 a 0 1k\n.op\n.end\n', [], []),
    ).toThrow(/cannot resolve source '\/missing\.inc'/);
  });
});
