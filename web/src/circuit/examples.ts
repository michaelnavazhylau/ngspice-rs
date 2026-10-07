import { pinPositions } from "./geometry.ts";
import {
  defaultAnalysis,
  defaultSource,
  emptyDoc,
  makeComponent,
  newId,
  type Analysis,
  type Component,
  type Doc,
  type Kind,
  type ModelType,
  type Point,
  type Rot,
  type SourceSpec,
  type VFile,
} from "./model.ts";

class Builder {
  doc: Doc = emptyDoc();
  add(
    kind: Kind,
    x: number,
    y: number,
    rot: Rot,
    value?: string,
    src?: Partial<SourceSpec>,
    modelType?: ModelType,
  ): Component {
    const c = makeComponent(this.doc, kind, x, y, rot, modelType);
    if (value !== undefined) c.value = value;
    if (src && c.src) c.src = { ...c.src, ...src };
    this.doc = { ...this.doc, components: [...this.doc.components, c] };
    return c;
  }
  pin(c: Component, i: number): Point {
    return pinPositions(c)[i]!;
  }
  /** Polyline through the points, inserting horizontal-first corners where needed. */
  wire(...pts: Point[]): void {
    const wires = [...this.doc.wires];
    for (let i = 0; i + 1 < pts.length; i++) {
      const a = pts[i]!;
      const b = pts[i + 1]!;
      const seq = a.x === b.x || a.y === b.y ? [a, b] : [a, { x: b.x, y: a.y }, b];
      for (let j = 0; j + 1 < seq.length; j++) wires.push({ id: newId("w"), a: seq[j]!, b: seq[j + 1]! });
    }
    this.doc = { ...this.doc, wires };
  }
}

export interface Example {
  id: string;
  title: string;
  description: string;
  doc: Doc;
  analysis: Analysis;
  files: VFile[];
}

export const PARAMS_FILE: VFile = {
  name: "params.inc",
  content: "* shared parameters\n.param rload=2k\n",
};

const p = (x: number, y: number): Point => ({ x, y });

function rcLowpass(src: Partial<SourceSpec>): Doc {
  const b = new Builder();
  const v1 = b.add("V", 100, 100, 0, undefined, src);
  const r1 = b.add("R", 200, 70, 0, "1k");
  const c1 = b.add("C", 300, 100, 1, "100n");
  b.add("GND", 100, 160, 0);
  const out = b.add("LABEL", 340, 70, 0, "out");
  b.add("LABEL", 130, 70, 0, "in");
  b.wire(b.pin(v1, 0), b.pin(r1, 0));
  b.wire(b.pin(r1, 1), b.pin(c1, 0), b.pin(out, 0));
  b.wire(b.pin(v1, 1), p(100, 160), p(300, 160), b.pin(c1, 1));
  return b.doc;
}

function rlcSeries(): Doc {
  const b = new Builder();
  const v1 = b.add("V", 100, 100, 0, undefined, {
    wave: "pulse",
    pulse: { v1: "0", v2: "1", td: "50u", tr: "20u", tf: "20u", pw: "400u", per: "1m" },
  });
  const r1 = b.add("R", 190, 70, 0, "10");
  const l1 = b.add("L", 280, 70, 0, "1m");
  const c1 = b.add("C", 340, 100, 1, "1u");
  b.add("GND", 100, 160, 0);
  const out = b.add("LABEL", 380, 70, 0, "out");
  b.wire(b.pin(v1, 0), b.pin(r1, 0));
  b.wire(b.pin(r1, 1), b.pin(l1, 0));
  b.wire(b.pin(l1, 1), b.pin(c1, 0), b.pin(out, 0));
  b.wire(b.pin(v1, 1), p(100, 160), p(340, 160), b.pin(c1, 1));
  return b.doc;
}

function divider(r2: string, extraLoad: boolean, srcDc: string): Doc {
  const b = new Builder();
  const v1 = b.add("V", 100, 100, 0, undefined, { wave: "dc", dc: srcDc });
  const r1 = b.add("R", 200, 70, 0, "1k");
  const r2c = b.add("R", 260, 100, 1, r2);
  b.add("GND", 100, 160, 0);
  const out = b.add("LABEL", 300, 70, 0, "out");
  b.wire(b.pin(v1, 0), b.pin(r1, 0));
  b.wire(b.pin(r1, 1), b.pin(r2c, 0));
  b.wire(b.pin(r2c, 0), b.pin(out, 0));
  let right = 260;
  if (extraLoad) {
    const r3 = b.add("R", 360, 100, 1, "2k");
    right = 360;
    b.wire(b.pin(out, 0), b.pin(r3, 0));
    b.wire(b.pin(r3, 1), p(360, 160));
  }
  b.wire(b.pin(v1, 1), p(100, 160), p(right, 160));
  b.wire(b.pin(r2c, 1), p(260, 160));
  return b.doc;
}

function rectifier(): Doc {
  const b = new Builder();
  // ±5 V triangle wave (2 ms period) into a diode with an RC load.
  const v1 = b.add("V", 100, 100, 0, undefined, {
    wave: "pulse",
    pulse: { v1: "-5", v2: "5", td: "0", tr: "1m", tf: "1m", pw: "0", per: "2m" },
  });
  const d1 = b.add("D", 190, 70, 0);
  const r1 = b.add("R", 260, 100, 1, "1k");
  const c1 = b.add("C", 330, 100, 1, "10u");
  b.add("GND", 100, 160, 0);
  b.add("LABEL", 130, 70, 0, "in");
  const out = b.add("LABEL", 370, 70, 0, "out");
  b.wire(b.pin(v1, 0), b.pin(d1, 0));
  b.wire(b.pin(d1, 1), b.pin(r1, 0), b.pin(c1, 0), b.pin(out, 0));
  b.wire(b.pin(v1, 1), p(100, 160), p(330, 160), b.pin(c1, 1));
  b.wire(b.pin(r1, 1), p(260, 160));
  return b.doc;
}

function commonEmitter(): Doc {
  const b = new Builder();
  b.add("V", 40, 160, 0, undefined, { wave: "dc", dc: "12" }); // V1 = Vcc
  b.add("LABEL", 40, 130, 0, "vcc");
  b.add("GND", 40, 190, 0);
  const vin = b.add("V", 110, 190, 0, undefined, { wave: "dc", dc: "0", ac: "1" }); // V2
  b.add("GND", 110, 220, 0);
  const cin = b.add("C", 170, 160, 0, "10u");
  const r1 = b.add("R", 230, 100, 1, "100k");
  const r2 = b.add("R", 230, 220, 1, "20k");
  b.add("LABEL", 230, 70, 0, "vcc");
  b.add("GND", 230, 250, 0);
  const q1 = b.add("Q", 300, 160, 0, undefined, undefined, "npn");
  // RC's lower pin and RE's upper pin sit on the collector/emitter pins.
  b.add("R", 310, 100, 1, "4.7k");
  b.add("R", 310, 220, 1, "1k");
  b.add("LABEL", 310, 70, 0, "vcc");
  b.add("GND", 310, 250, 0);
  const out = b.add("LABEL", 350, 130, 0, "out");
  b.wire(b.pin(vin, 0), b.pin(cin, 0));
  b.wire(b.pin(cin, 1), p(230, 160));
  b.wire(b.pin(r1, 1), p(230, 160), b.pin(q1, 1));
  b.wire(p(230, 160), b.pin(r2, 0));
  b.wire(b.pin(q1, 0), b.pin(out, 0));
  return b.doc;
}

function cmosInverter(): Doc {
  const b = new Builder();
  b.add("V", 40, 160, 0, undefined, { wave: "dc", dc: "5" }); // V1 = Vdd
  b.add("LABEL", 40, 130, 0, "vdd");
  b.add("GND", 40, 190, 0);
  const vin = b.add("V", 120, 190, 0, undefined, { wave: "dc", dc: "0" }); // V2
  b.add("GND", 120, 220, 0);
  b.add("LABEL", 120, 160, 0, "in");
  const mp = b.add("M", 220, 90, 0, "w=20u l=1u", undefined, "pmos"); // source on top
  const mn = b.add("M", 220, 190, 0, "w=10u l=1u", undefined, "nmos"); // drain on top
  b.add("LABEL", 230, 60, 0, "vdd");
  b.add("GND", 230, 220, 0);
  const out = b.add("LABEL", 270, 140, 0, "out");
  b.wire(b.pin(mp, 2), p(230, 140), b.pin(mn, 0));
  b.wire(p(230, 140), b.pin(out, 0));
  b.wire(b.pin(mp, 1), p(160, 90), p(160, 190), b.pin(mn, 1));
  b.wire(b.pin(vin, 0), p(160, 160));
  return b.doc;
}

const an = (patch: Partial<Analysis>): Analysis => ({ ...defaultAnalysis(), ...patch });

export const EXAMPLES: Example[] = [
  {
    id: "rc-tran",
    title: "RC low-pass: pulse transient",
    description: "1 kΩ / 100 nF (τ = 100 µs) driven by a 1 ms pulse.",
    doc: rcLowpass({
      wave: "pulse",
      pulse: { v1: "0", v2: "1", td: "0.2m", tr: "1u", tf: "1u", pw: "1m", per: "2m" },
    }),
    analysis: an({ kind: "tran", tran: { tstep: "5u", tstop: "4m", tstart: "", tmax: "", uic: false } }),
    files: [],
  },
  {
    id: "rc-ac",
    title: "RC low-pass: AC sweep",
    description: "Bode plot of the same filter (fc ≈ 1.59 kHz).",
    doc: rcLowpass({ wave: "dc", dc: "0", ac: "1" }),
    analysis: an({ kind: "ac", ac: { sweep: "dec", points: "20", fstart: "10", fstop: "1meg" } }),
    files: [],
  },
  {
    id: "rlc",
    title: "Series RLC ringing",
    description: "R = 10 Ω, L = 1 mH, C = 1 µF excited by a pulse; underdamped ringing.",
    doc: rlcSeries(),
    analysis: an({ kind: "tran", tran: { tstep: "1u", tstop: "1m", tstart: "", tmax: "", uic: false } }),
    files: [],
  },
  {
    id: "divider-op",
    title: "Resistor divider (.op, params.inc)",
    description: "R2 = {rload} comes from params.inc via .include.",
    doc: divider("{rload}", false, "10"),
    analysis: an({ kind: "op", includes: ["params.inc"] }),
    files: [PARAMS_FILE],
  },
  {
    id: "dc-sweep",
    title: "DC sweep",
    description: "Sweep V1 from 0 to 10 V across a loaded divider.",
    doc: divider("1k", true, "5"),
    analysis: an({ kind: "dc", dc: { src: "v1", start: "0", stop: "10", step: "0.25" } }),
    files: [],
  },
  {
    id: "rectifier",
    title: "Diode half-wave rectifier",
    description: "±5 V triangle into a diode with a 1 kΩ / 10 µF load (peak detector with ripple).",
    doc: rectifier(),
    analysis: an({ kind: "tran", tran: { tstep: "10u", tstop: "10m", tstart: "", tmax: "", uic: false } }),
    files: [],
  },
  {
    id: "ce-amp",
    title: "BJT common-emitter amplifier (AC)",
    description: "NPN with a 100k/20k divider bias, RC = 4.7 kΩ, RE = 1 kΩ: midband gain ≈ −RC/RE.",
    doc: commonEmitter(),
    analysis: an({ kind: "ac", ac: { sweep: "dec", points: "20", fstart: "1", fstop: "100meg" } }),
    files: [],
  },
  {
    id: "cmos-inverter",
    title: "CMOS inverter transfer curve",
    description: "Level-1 PMOS/NMOS pair; V2 swept 0–5 V.",
    doc: cmosInverter(),
    analysis: an({ kind: "dc", dc: { src: "v2", start: "0", stop: "5", step: "0.05" } }),
    files: [],
  },
];

export { defaultSource };
