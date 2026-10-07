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

  test("errors are thrown with a message", () => {
    // A missing virtual include stays an error whatever devices are ported.
    expect(() =>
      pkg.simulate('t\n.include "missing.inc"\nr1 a 0 1k\n.op\n.end\n', [], []),
    ).toThrow(/cannot resolve source '\/missing\.inc'/);
  });
});
