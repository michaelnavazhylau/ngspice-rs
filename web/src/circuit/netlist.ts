import { onSegmentInterior, pinNames, pinPositions, pointKey } from "./geometry.ts";
import type { Analysis, Component, Doc, ModelSpec, Point, SourceSpec } from "./model.ts";

export interface Warning {
  severity: "warning" | "error";
  message: string;
}

export interface NetlistResult {
  deck: string;
  warnings: Warning[];
  /** "<componentId>:<pinIndex>" -> net name */
  pinNet: Map<string, string>;
  /** wire id -> net name */
  wireNet: Map<string, string>;
  /** net name -> point where annotations are drawn */
  netAnchors: Map<string, Point>;
  /** all net names in order of appearance, `0` first when present */
  nets: string[];
  hasErrors: boolean;
}

class UnionFind {
  private parent = new Map<string, string>();
  find(k: string): string {
    let p = this.parent.get(k);
    if (p === undefined) {
      this.parent.set(k, k);
      return k;
    }
    let root = k;
    while (p !== root) {
      root = p;
      p = this.parent.get(root) ?? root;
    }
    // path compression
    let cur = k;
    while (cur !== root) {
      const next = this.parent.get(cur) ?? root;
      this.parent.set(cur, root);
      cur = next;
    }
    return root;
  }
  union(a: string, b: string): void {
    const ra = this.find(a);
    const rb = this.find(b);
    if (ra !== rb) this.parent.set(ra, rb);
  }
}

/** Lower-case, SPICE-safe node name. Returns "" when nothing usable remains. */
export function sanitizeNode(raw: string): string {
  return raw
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]/g, "_");
}

const GROUND_NAMES = new Set(["0", "gnd"]);

export const pinId = (c: Component, i: number): string => `${c.id}:${i}`;

/** Component terminals that take part in the electrical netlist. */
const isDevice = (c: Component): boolean => c.kind !== "GND" && c.kind !== "LABEL";

export function sourceSpec(s: SourceSpec): string {
  const parts: string[] = [];
  const t = (v: string, d = "0") => v.trim().replace(/\s+/g, "") || d;
  if (s.wave === "dc") parts.push(`dc ${t(s.dc)}`);
  if (s.wave === "pulse") {
    const p = s.pulse;
    parts.push(`pulse(${[p.v1, p.v2, p.td, p.tr, p.tf, p.pw, p.per].map((v) => t(v)).join(" ")})`);
  }
  if (s.wave === "pwl") {
    const nums = s.pwl.split(/[\s,]+/).filter(Boolean);
    parts.push(`pwl(${nums.join(" ")})`);
  }
  if (s.ac.trim()) parts.push(`ac ${t(s.ac, "1")}`);
  return parts.join(" ");
}

/** Pin nets in SPICE node order: D anode cathode; Q collector base emitter;
 * M drain gate source bulk (bulk = source). Pins are top, base/gate, bottom,
 * and PNP/PMOS put the emitter/source on top. */
export function spiceTerminals(c: Pick<Component, "kind" | "model">, nets: string[]): string[] {
  const [top, mid, bottom] = nets as [string, string, string];
  const flipped = c.model?.type === "pnp" || c.model?.type === "pmos";
  if (c.kind === "Q") return flipped ? [bottom, mid, top] : [top, mid, bottom];
  if (c.kind === "M") return flipped ? [bottom, mid, top, top] : [top, mid, bottom, bottom];
  return nets;
}

/** Model parameter text as written inside `.model name type( … )`. */
export const modelParams = (m: ModelSpec): string => m.params.trim().replace(/\s+/g, " ");

export function analysisCard(a: Analysis): string[] {
  const out: string[] = [];
  const ic = a.ic.trim();
  if (ic) out.push(`.ic ${ic.split(/[\s,]+/).filter(Boolean).join(" ")}`);
  switch (a.kind) {
    case "op":
      out.push(".op");
      break;
    case "dc":
      out.push(`.dc ${[a.dc.src, a.dc.start, a.dc.stop, a.dc.step].map((s) => s.trim()).join(" ")}`);
      break;
    case "ac":
      out.push(
        `.ac ${a.ac.sweep} ${[a.ac.points, a.ac.fstart, a.ac.fstop].map((s) => s.trim()).join(" ")}`,
      );
      break;
    case "tran": {
      const t = a.tran;
      const fields = [t.tstep.trim(), t.tstop.trim()];
      const tstart = t.tstart.trim();
      const tmax = t.tmax.trim();
      if (tstart || tmax) fields.push(tstart || "0");
      if (tmax) fields.push(tmax);
      if (t.uic) fields.push("uic");
      out.push(`.tran ${fields.join(" ")}`);
      break;
    }
  }
  return out;
}

export function generateNetlist(doc: Doc, analysis: Analysis, title = "Schematic"): NetlistResult {
  const warnings: Warning[] = [];
  const uf = new UnionFind();
  const pt = (p: Point) => `p:${pointKey(p)}`;
  const wk = (id: string) => `w:${id}`;

  // 1. wires join their endpoints
  for (const w of doc.wires) {
    uf.union(wk(w.id), pt(w.a));
    uf.union(wk(w.id), pt(w.b));
  }
  // 2. any pin or wire endpoint lying on the interior of a wire joins that wire.
  //    Crossings without an endpoint there are deliberately not connected.
  const candidates: Point[] = [];
  for (const c of doc.components) candidates.push(...pinPositions(c));
  for (const w of doc.wires) candidates.push(w.a, w.b);
  for (const w of doc.wires) {
    for (const p of candidates) if (onSegmentInterior(p, w.a, w.b)) uf.union(pt(p), wk(w.id));
  }
  // 3. ground symbols and labels
  const labelPins: { c: Component; name: string }[] = [];
  for (const c of doc.components) {
    const pins = pinPositions(c);
    if (c.kind === "GND") uf.union(pt(pins[0]!), "L:0");
    if (c.kind === "LABEL") {
      let name = sanitizeNode(c.value);
      if (!name) {
        warnings.push({ severity: "warning", message: "A net label has no name and is ignored." });
        continue;
      }
      if (name !== c.value.trim()) {
        warnings.push({
          severity: "warning",
          message: `Net label "${c.value}" was renamed to "${name}" (lowercase letters, digits and _ only).`,
        });
      }
      if (GROUND_NAMES.has(name)) name = "0";
      labelPins.push({ c, name });
      uf.union(pt(pins[0]!), `L:${name}`);
    }
  }

  // 4. name nets
  const rootNames = new Map<string, string>();
  const groundRoot = uf.find("L:0");
  const labelsByRoot = new Map<string, Set<string>>();
  for (const { c, name } of labelPins) {
    const root = uf.find(pt(pinPositions(c)[0]!));
    let set = labelsByRoot.get(root);
    if (!set) labelsByRoot.set(root, (set = new Set()));
    set.add(name);
  }
  const hasGround = doc.components.some((c) => c.kind === "GND") || labelPins.some((l) => l.name === "0");
  if (hasGround) rootNames.set(groundRoot, "0");
  const usedNames = new Set<string>(["0"]);
  for (const [root, names] of labelsByRoot) {
    if (root === groundRoot && hasGround) continue;
    const sorted = [...names].sort();
    if (sorted.length > 1) {
      warnings.push({
        severity: "warning",
        message: `Labels ${sorted.map((n) => `"${n}"`).join(" and ")} are on the same net; using "${sorted[0]}".`,
      });
    }
    rootNames.set(root, sorted[0]!);
    for (const n of sorted) usedNames.add(n);
  }
  if (hasGround) {
    const g = labelsByRoot.get(groundRoot);
    if (g && [...g].some((n) => n !== "0")) {
      warnings.push({ severity: "warning", message: "A net label shares the ground net; it is named 0." });
    }
  }

  let auto = 0;
  const netOf = (p: Point): string => {
    const root = uf.find(pt(p));
    let n = rootNames.get(root);
    if (!n) {
      do {
        auto++;
        n = `n${auto}`;
      } while (usedNames.has(n));
      usedNames.add(n);
      rootNames.set(root, n);
    }
    return n;
  };

  // Deterministic naming: walk components in document order.
  const pinNet = new Map<string, string>();
  const netAnchors = new Map<string, Point>();
  const nets: string[] = [];
  const noteNet = (n: string, p: Point) => {
    if (!netAnchors.has(n)) {
      netAnchors.set(n, p);
      nets.push(n);
    }
  };
  for (const c of doc.components) {
    const pins = pinPositions(c);
    pins.forEach((p, i) => {
      const n = netOf(p);
      pinNet.set(pinId(c, i), n);
      noteNet(n, p);
    });
  }
  // Labeled nets are annotated at the label itself.
  for (const { c, name } of labelPins) {
    const n = pinNet.get(pinId(c, 0));
    if (n && name !== "0") netAnchors.set(n, pinPositions(c)[0]!);
  }
  const wireNet = new Map<string, string>();
  for (const w of doc.wires) {
    const root = uf.find(wk(w.id));
    const n = rootNames.get(root);
    if (n) wireNet.set(w.id, n);
    else {
      // a wire that touches no component pin: still give it a name
      const nn = netOf(w.a);
      wireNet.set(w.id, nn);
      noteNet(nn, w.a);
    }
  }

  // 5. checks
  const devices = doc.components.filter(isDevice);
  if (!doc.components.length) warnings.push({ severity: "warning", message: "The schematic is empty." });
  else if (!hasGround) {
    warnings.push({
      severity: "error",
      message: "No ground: add a Ground symbol (or a net label named 0) to define the reference node.",
    });
  }
  const seen = new Map<string, number>();
  for (const c of devices) {
    const k = c.name.trim().toLowerCase();
    seen.set(k, (seen.get(k) ?? 0) + 1);
    if (!c.name.trim()) warnings.push({ severity: "error", message: "A component has no name." });
    else if (!c.name.trim().toLowerCase().startsWith(c.kind.toLowerCase())) {
      warnings.push({ severity: "error", message: `${c.name} must start with the letter ${c.kind}.` });
    } else if (/\s/.test(c.name.trim())) {
      warnings.push({ severity: "error", message: `${c.name}: names cannot contain spaces.` });
    }
  }
  for (const [name, n] of seen) {
    if (n > 1) warnings.push({ severity: "error", message: `Duplicate component name "${name}".` });
  }
  const pinCount = new Map<string, number>();
  const netsOf = (c: Component): string[] => pinPositions(c).map((_, i) => pinNet.get(pinId(c, i))!);
  for (const c of devices) {
    for (const n of netsOf(c)) pinCount.set(n, (pinCount.get(n) ?? 0) + 1);
  }
  for (const c of devices) {
    const nets = netsOf(c);
    for (const [i, n] of nets.entries()) {
      if (n !== "0" && pinCount.get(n) === 1) {
        warnings.push({
          severity: "warning",
          message: `${c.name || c.kind}: ${pinNames(c)[i] ?? `terminal ${i + 1}`} (net ${n}) is not connected to anything else.`,
        });
      }
    }
    if (nets.every((n) => n === nets[0])) {
      warnings.push({ severity: "warning", message: `${c.name || c.kind}: all terminals are on net ${nets[0]} (shorted).` });
    }
    if (c.kind === "R" || c.kind === "C" || c.kind === "L") {
      const v = c.value.trim();
      if (!v) warnings.push({ severity: "error", message: `${c.name}: value is empty.` });
      else if (/\s/.test(v)) warnings.push({ severity: "error", message: `${c.name}: value "${v}" contains spaces.` });
    }
    if (c.model && /[()\n]/.test(c.value)) {
      warnings.push({ severity: "error", message: `${c.name}: instance parameters cannot contain parentheses or line breaks.` });
    }
  }

  // Model cards: one per name (case-insensitive); every user must agree on it.
  const models = new Map<string, { model: ModelSpec; owner: string }>();
  for (const c of devices) {
    const m = c.model;
    if (!m) continue;
    const name = m.name.trim();
    if (!/^[a-z_][a-z0-9_]*$/i.test(name)) {
      warnings.push({
        severity: "error",
        message: `${c.name}: model name "${m.name}" must start with a letter and use only letters, digits and _.`,
      });
      continue;
    }
    if (/[()\n]/.test(m.params)) {
      warnings.push({ severity: "error", message: `${c.name}: model parameters cannot contain parentheses or line breaks.` });
      continue;
    }
    const key = name.toLowerCase();
    const seenModel = models.get(key);
    if (!seenModel) models.set(key, { model: { ...m, name }, owner: c.name });
    else if (seenModel.model.type !== m.type || modelParams(seenModel.model) !== modelParams(m)) {
      warnings.push({
        severity: "error",
        message: `Model "${name}" is defined differently by ${seenModel.owner} and ${c.name}; give one of them another model name.`,
      });
    }
  }

  // 6. deck
  const lines: string[] = [`* ${title}`];
  for (const f of analysis.includes) lines.push(`.include "${f}"`);
  for (const c of devices) {
    const nets = netsOf(c);
    const name = c.name.trim();
    const extra = c.value.trim().replace(/\s+/g, " ");
    if (c.kind === "V" || c.kind === "I") {
      const spec = c.src ? sourceSpec(c.src) : "dc 0";
      lines.push(`${name} ${nets.join(" ")} ${spec}`.trimEnd());
    } else if (c.model) {
      const terminals = spiceTerminals(c, nets);
      lines.push(`${name} ${terminals.join(" ")} ${c.model.name.trim()} ${extra}`.trimEnd());
    } else {
      lines.push(`${name} ${nets.join(" ")} ${extra}`);
    }
  }
  for (const { model } of models.values()) {
    lines.push(`.model ${model.name} ${model.type}(${modelParams(model)})`);
  }
  lines.push(...analysisCard(analysis));
  lines.push(".end");

  const hasErrors = warnings.some((w) => w.severity === "error");
  return { deck: lines.join("\n") + "\n", warnings, pinNet, wireNet, netAnchors, nets, hasErrors };
}
