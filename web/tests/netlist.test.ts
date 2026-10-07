import { describe, expect, test } from "bun:test";
import { EXAMPLES } from "../src/circuit/examples.ts";
import { junctionPoints } from "../src/circuit/geometry.ts";
import { defaultAnalysis, emptyDoc, makeComponent, type Component, type Doc, type Kind, type Rot, type Wire } from "../src/circuit/model.ts";
import { generateNetlist, sanitizeNode } from "../src/circuit/netlist.ts";

let n = 0;
function comp(doc: Doc, kind: Kind, x: number, y: number, rot: Rot = 0, patch: Partial<Component> = {}): Component {
  const c = { ...makeComponent(doc, kind, x, y, rot), ...patch };
  doc.components.push(c);
  return c;
}
const wire = (doc: Doc, x1: number, y1: number, x2: number, y2: number): Wire => {
  const w = { id: `t${n++}`, a: { x: x1, y: y1 }, b: { x: x2, y: y2 } };
  doc.wires.push(w);
  return w;
};
const lines = (deck: string) => deck.trim().split("\n");

describe("netlist generation", () => {
  test("simple divider with ground and labels", () => {
    const d = emptyDoc();
    comp(d, "V", 0, 0, 0, { src: { ...makeComponent(d, "V", 0, 0).src!, dc: "5" } }); // pins (0,-30) (0,30)
    comp(d, "R", 60, -30, 0, { value: "1k" }); // pins (30,-30) (90,-30)
    comp(d, "R", 90, 0, 1, { value: "2k" }); // pins (90,-30) (90,30)
    comp(d, "GND", 0, 30);
    comp(d, "LABEL", 90, -30, 0, { value: "Out" });
    wire(d, 0, -30, 30, -30);
    wire(d, 0, 30, 90, 30);
    const r = generateNetlist(d, { ...defaultAnalysis(), kind: "op" });
    expect(r.warnings.filter((w) => w.severity === "error")).toEqual([]);
    expect(lines(r.deck)).toEqual([
      "* Schematic",
      "V1 n1 0 dc 5",
      "R1 n1 out 1k",
      "R2 out 0 2k",
      ".op",
      ".end",
    ]);
  });

  test("crossing wires do not connect, T junction does", () => {
    const d = emptyDoc();
    comp(d, "R", 0, 0, 0); // pins (-30,0)(30,0)
    comp(d, "R", 0, 100, 0); // (-30,100)(30,100)
    comp(d, "GND", 100, 200);
    // vertical wire from R1 right pin down, passing x=30 through y=100 (R2 right pin is at 30,100: endpoint-on-wire)
    wire(d, 30, 0, 30, 200);
    // horizontal wire crossing the vertical one at (30,50) without endpoint there
    wire(d, -50, 50, 80, 50);
    const r = generateNetlist(d, defaultAnalysis());
    const net = (name: string, pin: number) => r.pinNet.get(`${d.components.find((c) => c.name === name)!.id}:${pin}`);
    expect(net("R1", 1)).toBe(net("R2", 1)); // pin on wire body connects
    const crossing = d.wires[1]!;
    expect(r.wireNet.get(crossing.id)).not.toBe(net("R1", 1));
  });

  test("wire endpoint landing on another wire's segment connects", () => {
    const d = emptyDoc();
    const r1 = comp(d, "R", 0, 0, 0);
    const r2 = comp(d, "R", 0, 100, 0);
    wire(d, 30, 0, 30, 100); // vertical
    wire(d, 30, 50, 80, 50); // T from middle
    comp(d, "R", 110, 50, 0, { value: "1" }); // left pin (80,50)
    wire(d, -30, 0, -30, 100);
    const r = generateNetlist(d, defaultAnalysis());
    const r3 = d.components[2]!;
    expect(r.pinNet.get(`${r3.id}:0`)).toBe(r.pinNet.get(`${r1.id}:1`));
    expect(r.pinNet.get(`${r2.id}:0`)).toBe(r.pinNet.get(`${r1.id}:0`));
    expect(junctionPoints(d)).toContainEqual({ x: 30, y: 50 });
  });

  test("same label joins separate nets; different labels do not", () => {
    const d = emptyDoc();
    const a = comp(d, "R", 0, 0);
    const b = comp(d, "R", 0, 100);
    comp(d, "LABEL", 30, 0, 0, { value: "x" });
    comp(d, "LABEL", -30, 100, 0, { value: "x" });
    comp(d, "LABEL", -30, 0, 0, { value: "a" });
    comp(d, "LABEL", 30, 100, 0, { value: "b" });
    const r = generateNetlist(d, defaultAnalysis());
    expect(r.pinNet.get(`${a.id}:1`)).toBe("x");
    expect(r.pinNet.get(`${b.id}:0`)).toBe("x");
    expect(r.pinNet.get(`${a.id}:0`)).toBe("a");
    expect(r.pinNet.get(`${b.id}:1`)).toBe("b");
  });

  test("auto net names are deterministic and skip label names", () => {
    const d = emptyDoc();
    comp(d, "R", 0, 0);
    comp(d, "R", 100, 0);
    comp(d, "LABEL", -30, 0, 0, { value: "n1" });
    const a = generateNetlist(d, defaultAnalysis());
    const b = generateNetlist(structuredClone(d), defaultAnalysis());
    expect(a.deck).toBe(b.deck);
    expect(lines(a.deck)[1]).toBe("R1 n1 n2 1k");
    expect(lines(a.deck)[2]).toBe("R2 n3 n4 1k");
  });

  test("label 0 / gnd is ground", () => {
    const d = emptyDoc();
    comp(d, "R", 0, 0);
    comp(d, "LABEL", -30, 0, 0, { value: "GND" });
    comp(d, "GND", 30, 0);
    const r = generateNetlist(d, defaultAnalysis());
    expect(lines(r.deck)[1]).toBe("R1 0 0 1k");
    expect(r.warnings.some((w) => /shorted/.test(w.message))).toBe(true);
  });

  test("warnings: floating pin, no ground, duplicate names", () => {
    const d = emptyDoc();
    comp(d, "R", 0, 0);
    comp(d, "R", 0, 100, 0, { name: "R1" });
    const r = generateNetlist(d, defaultAnalysis());
    const msgs = r.warnings.map((w) => w.message).join("\n");
    expect(msgs).toMatch(/No ground/);
    expect(msgs).toMatch(/not connected to anything else/);
    expect(msgs).toMatch(/Duplicate/);
    expect(r.hasErrors).toBe(true);
  });

  test("analysis cards and includes", () => {
    const d = emptyDoc();
    comp(d, "R", 0, 0);
    comp(d, "GND", 30, 0);
    comp(d, "GND", -30, 0);
    const base = { ...defaultAnalysis(), includes: ["params.inc"], ic: "v(a)=1, v(b)=2" };
    expect(generateNetlist(d, { ...base, kind: "dc" }).deck).toContain(".dc v1 0 5 0.1");
    expect(generateNetlist(d, { ...base, kind: "ac" }).deck).toContain(".ac dec 10 10 1meg");
    const t = generateNetlist(d, { ...base, kind: "tran", tran: { tstep: "1u", tstop: "1m", tstart: "", tmax: "", uic: true } }).deck;
    expect(t).toContain(".tran 1u 1m uic");
    expect(t).toContain('.include "params.inc"');
    expect(t).toContain(".ic v(a)=1 v(b)=2");
    expect(lines(t).at(-1)).toBe(".end");
    expect(generateNetlist(d, { ...base, kind: "tran", tran: { tstep: "1u", tstop: "1m", tstart: "", tmax: "2u", uic: false } }).deck).toContain(".tran 1u 1m 0 2u");
  });

  test("source waveforms", () => {
    const d = emptyDoc();
    const v = comp(d, "V", 0, 0);
    v.src = { ...v.src!, wave: "pulse", ac: "1", pulse: { v1: "0", v2: "1", td: "50u", tr: "20u", tf: "20u", pw: "400u", per: "1m" } };
    const i = comp(d, "I", 100, 0);
    i.src = { ...i.src!, wave: "pwl", pwl: "0 0, 1m 1" };
    const deck = generateNetlist(d, defaultAnalysis()).deck;
    expect(deck).toContain("pulse(0 1 50u 20u 20u 400u 1m) ac 1");
    expect(deck).toContain("pwl(0 0 1m 1)");
  });

  test("semiconductors: SPICE terminal order follows polarity", () => {
    const d = emptyDoc();
    // Q/M pins: 0 top (x+10, y-30), 1 base/gate (x-30, y), 2 bottom (x+10, y+30).
    comp(d, "Q", 0, 0, 0, makeComponent(d, "Q", 0, 0, 0, "npn"));
    comp(d, "Q", 100, 0, 0, makeComponent(d, "Q", 100, 0, 0, "pnp"));
    comp(d, "M", 200, 0, 0, makeComponent(d, "M", 200, 0, 0, "nmos"));
    comp(d, "M", 300, 0, 0, makeComponent(d, "M", 300, 0, 0, "pmos"));
    comp(d, "D", 400, 0); // anode (370,0), cathode (430,0)
    for (const [x, y, name] of [
      [10, -30, "t1"], [-30, 0, "b1"], [10, 30, "u1"],
      [110, -30, "t2"], [70, 0, "b2"], [110, 30, "u2"],
      [210, -30, "t3"], [170, 0, "b3"], [210, 30, "u3"],
      [310, -30, "t4"], [270, 0, "b4"], [310, 30, "u4"],
      [370, 0, "a"], [430, 0, "0"],
    ] as const) {
      comp(d, "LABEL", x, y, 0, { value: name });
    }
    const deck = lines(generateNetlist(d, defaultAnalysis()).deck);
    expect(deck).toContain("Q1 t1 b1 u1 qnpn"); // collector on top
    expect(deck).toContain("Q2 u2 b2 t2 qpnp"); // PNP: emitter on top
    expect(deck).toContain("M1 t3 b3 u3 u3 nmos1 w=10u l=1u"); // bulk = source (bottom)
    expect(deck).toContain("M2 u4 b4 t4 t4 pmos1 w=10u l=1u"); // PMOS: source/bulk on top
    expect(deck).toContain("D1 a 0 dmod");
    for (const t of ["d", "npn", "pnp", "nmos", "pmos"]) {
      expect(deck.filter((l) => l.startsWith(".model ") && l.includes(` ${t}(`)).length, t).toBe(1);
    }
  });

  test("model cards are shared by name and conflicts are errors", () => {
    const d = emptyDoc();
    comp(d, "D", 0, 0);
    comp(d, "D", 0, 100, 0, { value: "2" });
    comp(d, "GND", 30, 0);
    comp(d, "GND", 30, 100);
    wire(d, -30, 0, -30, 100);
    let r = generateNetlist(d, defaultAnalysis());
    expect(r.deck.match(/^\.model dmod d\(/gm)?.length).toBe(1);
    expect(lines(r.deck)).toContain("D2 n1 0 dmod 2");
    expect(r.hasErrors).toBe(false);
    // Same name, different parameters: one card cannot represent both.
    d.components[1] = { ...d.components[1]!, model: { ...d.components[1]!.model!, params: "is=2e-14" } };
    r = generateNetlist(d, defaultAnalysis());
    expect(r.warnings.some((w) => w.severity === "error" && w.message.includes('Model "dmod" is defined differently'))).toBe(true);
    // Renamed, both cards are emitted.
    d.components[1] = { ...d.components[1]!, model: { ...d.components[1]!.model!, name: "dfast" } };
    r = generateNetlist(d, defaultAnalysis());
    expect(r.hasErrors).toBe(false);
    expect(lines(r.deck)).toContain(".model dfast d(is=2e-14)");
    // Invalid names and parentheses in parameters are caught before the engine.
    d.components[1] = { ...d.components[1]!, model: { ...d.components[1]!.model!, name: "2bad" } };
    expect(generateNetlist(d, defaultAnalysis()).warnings.some((w) => w.message.includes("must start with a letter"))).toBe(true);
    d.components[1] = { ...d.components[1]!, model: { name: "dx", type: "d", params: "is=(1)" } };
    expect(generateNetlist(d, defaultAnalysis()).warnings.some((w) => w.message.includes("parentheses"))).toBe(true);
  });

  test("new parts reuse the default model card already in the schematic", () => {
    const d = emptyDoc();
    const first = comp(d, "Q", 0, 0, 0, makeComponent(d, "Q", 0, 0, 0, "npn"));
    first.model = { ...first.model!, params: "is=1e-15 bf=200" };
    const second = makeComponent(d, "Q", 100, 0, 0, "npn");
    expect(second.model).toEqual(first.model);
    expect(makeComponent(d, "Q", 100, 0, 0, "pnp").model?.name).toBe("qpnp");
    expect(makeComponent(d, "Q", 100, 0, 0, "nmos").model?.type).toBe("npn"); // invalid type for Q
  });

  test("sanitizeNode", () => {
    expect(sanitizeNode(" Out-1 ")).toBe("out_1");
  });

  test("all examples generate error-free decks", () => {
    for (const ex of EXAMPLES) {
      const r = generateNetlist(ex.doc, ex.analysis);
      expect(r.warnings.filter((w) => w.severity !== "warning" || /connected|shorted/.test(w.message)), ex.id).toEqual([]);
      expect(r.deck.endsWith(".end\n")).toBe(true);
    }
  });
});
