export type Kind = "R" | "C" | "L" | "V" | "I" | "D" | "Q" | "M" | "GND" | "LABEL";
export type Rot = 0 | 1 | 2 | 3;
export interface Point {
  x: number;
  y: number;
}

export type Waveform = "dc" | "pulse" | "pwl";
export interface SourceSpec {
  wave: Waveform;
  dc: string;
  ac: string;
  pulse: { v1: string; v2: string; td: string; tr: string; tf: string; pw: string; per: string };
  pwl: string;
}

/** SPICE `.model` type of a semiconductor device. */
export type ModelType = "d" | "npn" | "pnp" | "nmos" | "pmos";

/** A `.model name type(params)` card owned by a D/Q/M component. Components that
 * use the same model name share one card (see `generateNetlist`). */
export interface ModelSpec {
  name: string;
  type: ModelType;
  /** Parameter list written inside the parentheses, e.g. `is=1e-14 n=1`. */
  params: string;
}

export interface Component {
  id: string;
  kind: Kind;
  x: number;
  y: number;
  rot: Rot;
  /** Reference designator (R1, C2 ...). For LABEL it is unused. */
  name: string;
  /** Value for R/C/L, node name for LABEL, optional instance parameters for
   * D/Q/M (area factor for D/Q, `w=.. l=..` for M). */
  value: string;
  /** Source description for V and I. */
  src?: SourceSpec;
  /** Model card for D, Q and M. */
  model?: ModelSpec;
}

/** A single straight orthogonal wire segment. */
export interface Wire {
  id: string;
  a: Point;
  b: Point;
}

export interface Doc {
  components: Component[];
  wires: Wire[];
}

export type AnalysisKind = "op" | "dc" | "ac" | "tran";
export interface Analysis {
  kind: AnalysisKind;
  dc: { src: string; start: string; stop: string; step: string };
  ac: { sweep: "dec" | "oct" | "lin"; points: string; fstart: string; fstop: string };
  tran: { tstep: string; tstop: string; tstart: string; tmax: string; uic: boolean };
  /** Initial conditions, e.g. "v(out)=0 v(x)=1" */
  ic: string;
  /** Virtual files emitted as .include lines. */
  includes: string[];
}

export interface VFile {
  name: string;
  content: string;
}

export const emptyDoc = (): Doc => ({ components: [], wires: [] });

export const defaultSource = (): SourceSpec => ({
  wave: "dc",
  dc: "1",
  ac: "",
  pulse: { v1: "0", v2: "1", td: "0", tr: "1n", tf: "1n", pw: "1m", per: "2m" },
  pwl: "0 0 1m 1",
});

export const defaultAnalysis = (): Analysis => ({
  kind: "op",
  dc: { src: "v1", start: "0", stop: "5", step: "0.1" },
  ac: { sweep: "dec", points: "10", fstart: "10", fstop: "1meg" },
  tran: { tstep: "10u", tstop: "5m", tstart: "", tmax: "", uic: false },
  ic: "",
  includes: [],
});

let counter = 0;
export const newId = (prefix = "e"): string =>
  `${prefix}${Date.now().toString(36)}${(counter++).toString(36)}${Math.floor(Math.random() * 1296).toString(36)}`;

export const KIND_LABEL: Record<Kind, string> = {
  R: "Resistor",
  C: "Capacitor",
  L: "Inductor",
  V: "Voltage source",
  I: "Current source",
  D: "Diode",
  Q: "BJT",
  M: "MOSFET",
  GND: "Ground",
  LABEL: "Net label",
};

export const DEFAULT_VALUE: Record<Kind, string> = {
  R: "1k",
  C: "100n",
  L: "1m",
  V: "",
  I: "",
  D: "",
  Q: "",
  M: "w=10u l=1u",
  GND: "",
  LABEL: "net",
};

export const MODEL_TYPE_LABEL: Record<ModelType, string> = {
  d: "Diode",
  npn: "NPN",
  pnp: "PNP",
  nmos: "NMOS",
  pmos: "PMOS",
};

/** The model types a kind can use; the first is the default. */
export const MODEL_TYPES: Partial<Record<Kind, ModelType[]>> = {
  D: ["d"],
  Q: ["npn", "pnp"],
  M: ["nmos", "pmos"],
};

/** Default model per type. Parameters are ones the engine's M4 models accept
 * (diode with junction/transit charge, Ebers-Moll BJT, MOS level 1). */
export const DEFAULT_MODEL: Record<ModelType, ModelSpec> = {
  d: { name: "dmod", type: "d", params: "is=1e-14 n=1 rs=10 cjo=20p vj=0.7 m=0.5 tt=1n" },
  npn: { name: "qnpn", type: "npn", params: "is=1e-14 bf=100 cje=20p cjc=5p tf=1n" },
  pnp: { name: "qpnp", type: "pnp", params: "is=1e-14 bf=100 cje=20p cjc=5p tf=1n" },
  nmos: {
    name: "nmos1",
    type: "nmos",
    params: "level=1 vto=1 kp=1e-4 cbd=10p cbs=5p cgso=4e-7 cgdo=2e-7",
  },
  pmos: {
    name: "pmos1",
    type: "pmos",
    params: "level=1 vto=-1 kp=5e-5 cbd=10p cbs=5p cgso=4e-7 cgdo=2e-7",
  },
};

/** A model for `type`: an existing card of that type with the default name is
 * reused so new parts share it; otherwise the default card. */
export function modelFor(doc: Doc, type: ModelType): ModelSpec {
  const def = DEFAULT_MODEL[type];
  const existing = doc.components.find(
    (c) => c.model && c.model.type === type && c.model.name.toLowerCase() === def.name,
  );
  return { ...(existing?.model ?? def) };
}

/** Next free designator such as R3. */
export function nextName(doc: Doc, kind: Kind): string {
  if (kind === "GND" || kind === "LABEL") return "";
  const used = new Set(doc.components.map((c) => c.name.toLowerCase()));
  for (let i = 1; ; i++) {
    const n = `${kind}${i}`;
    if (!used.has(n.toLowerCase())) return n;
  }
}

export function makeComponent(
  doc: Doc,
  kind: Kind,
  x: number,
  y: number,
  rot: Rot = 0,
  modelType?: ModelType,
): Component {
  const c: Component = {
    id: newId("c"),
    kind,
    x,
    y,
    rot,
    name: nextName(doc, kind),
    value: DEFAULT_VALUE[kind],
  };
  if (kind === "V" || kind === "I") c.src = defaultSource();
  const types = MODEL_TYPES[kind];
  if (types) c.model = modelFor(doc, modelType && types.includes(modelType) ? modelType : types[0]!);
  if (kind === "LABEL") c.value = nextLabel(doc);
  return c;
}

function nextLabel(doc: Doc): string {
  const used = new Set(doc.components.filter((c) => c.kind === "LABEL").map((c) => c.value));
  for (let i = 1; ; i++) if (!used.has(`net${i}`)) return `net${i}`;
}
