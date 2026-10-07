import type { Component, Doc, Kind, Point, Rot, Wire } from "./model.ts";

export const GRID = 10;
export const snap = (v: number, g = GRID): number => Math.round(v / g) * g;
export const snapPoint = (p: Point): Point => ({ x: snap(p.x), y: snap(p.y) });

/** Pin positions in the component's local (unrotated) frame. */
export const LOCAL_PINS: Record<Kind, Point[]> = {
  R: [
    { x: -30, y: 0 },
    { x: 30, y: 0 },
  ],
  C: [
    { x: -30, y: 0 },
    { x: 30, y: 0 },
  ],
  L: [
    { x: -30, y: 0 },
    { x: 30, y: 0 },
  ],
  // pin 0 is the positive terminal
  V: [
    { x: 0, y: -30 },
    { x: 0, y: 30 },
  ],
  I: [
    { x: 0, y: -30 },
    { x: 0, y: 30 },
  ],
  // anode, cathode
  D: [
    { x: -30, y: 0 },
    { x: 30, y: 0 },
  ],
  // top, base, bottom: collector/emitter for NPN, emitter/collector for PNP
  Q: [
    { x: 10, y: -30 },
    { x: -30, y: 0 },
    { x: 10, y: 30 },
  ],
  // top, gate, bottom: drain/source for NMOS, source/drain for PMOS; the bulk
  // is tied to the source in the netlist
  M: [
    { x: 10, y: -30 },
    { x: -30, y: 0 },
    { x: 10, y: 30 },
  ],
  GND: [{ x: 0, y: 0 }],
  LABEL: [{ x: 0, y: 0 }],
};

/** Terminal names, in pin order, for tooltips and warnings. */
export function pinNames(c: Pick<Component, "kind" | "model">): string[] {
  if (c.model?.type === "pnp") return ["emitter", "base", "collector"];
  if (c.model?.type === "pmos") return ["source/bulk", "gate", "drain"];
  return PIN_NAMES[c.kind];
}

const PIN_NAMES: Record<Kind, string[]> = {
  R: ["1", "2"],
  C: ["1", "2"],
  L: ["1", "2"],
  V: ["+", "−"],
  I: ["n+ (arrow tail)", "n− (arrow head)"],
  D: ["anode", "cathode"],
  Q: ["collector", "base", "emitter"],
  M: ["drain", "gate", "source/bulk"],
  GND: ["ground"],
  LABEL: ["net"],
};

/** Rotate by `rot` quarter turns clockwise (screen coordinates, y down). */
export function rotate(p: Point, rot: Rot): Point {
  switch (rot) {
    case 0:
      return { x: p.x, y: p.y };
    case 1:
      return { x: -p.y || 0, y: p.x };
    case 2:
      return { x: -p.x || 0, y: -p.y || 0 };
    case 3:
      return { x: p.y, y: -p.x || 0 };
  }
}

export function pinPositions(c: Pick<Component, "kind" | "x" | "y" | "rot">): Point[] {
  return LOCAL_PINS[c.kind].map((p) => {
    const r = rotate(p, c.rot);
    return { x: c.x + r.x, y: c.y + r.y };
  });
}

export const samePoint = (a: Point, b: Point): boolean => a.x === b.x && a.y === b.y;
export const pointKey = (p: Point): string => `${p.x},${p.y}`;

/** True when p lies on the segment a-b (inclusive of endpoints). Segments are axis aligned. */
export function onSegment(p: Point, a: Point, b: Point): boolean {
  const minX = Math.min(a.x, b.x);
  const maxX = Math.max(a.x, b.x);
  const minY = Math.min(a.y, b.y);
  const maxY = Math.max(a.y, b.y);
  if (p.x < minX || p.x > maxX || p.y < minY || p.y > maxY) return false;
  if (a.x === b.x || a.y === b.y) return true;
  // diagonal fallback (should not occur for editor output)
  return Math.abs((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) < 1e-9;
}

/** Strictly inside the segment (not an endpoint). */
export const onSegmentInterior = (p: Point, a: Point, b: Point): boolean =>
  onSegment(p, a, b) && !samePoint(p, a) && !samePoint(p, b);

export const wireLength = (w: Wire): number => Math.abs(w.a.x - w.b.x) + Math.abs(w.a.y - w.b.y);

/** Points where a junction dot is drawn (three or more connections meet). */
export function junctionPoints(doc: Doc): Point[] {
  const degree = new Map<string, { p: Point; n: number }>();
  const bump = (p: Point, n: number) => {
    const k = pointKey(p);
    const e = degree.get(k);
    if (e) e.n += n;
    else degree.set(k, { p, n });
  };
  const pins: Point[] = doc.components.flatMap((c) => (c.kind === "LABEL" ? [] : pinPositions(c)));
  const wires = doc.wires.filter((w) => wireLength(w) > 0);
  for (const w of wires) {
    bump(w.a, 1);
    bump(w.b, 1);
  }
  for (const p of pins) {
    if (degree.has(pointKey(p))) bump(p, 1);
  }
  // wires passing through a point that already has connections
  for (const w of wires) {
    for (const e of degree.values()) {
      if (onSegmentInterior(e.p, w.a, w.b)) e.n += 2;
    }
  }
  return [...degree.values()].filter((e) => e.n >= 3).map((e) => e.p);
}

/** Axis-aligned L-shaped route from a to b (horizontal leg first unless `vertFirst`). */
export function route(a: Point, b: Point, vertFirst: boolean): Point[] {
  if (a.x === b.x || a.y === b.y) return [a, b];
  const corner = vertFirst ? { x: a.x, y: b.y } : { x: b.x, y: a.y };
  return [a, corner, b];
}

export interface Bounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

export function docBounds(doc: Doc): Bounds | null {
  const pts: Point[] = [];
  for (const c of doc.components) pts.push({ x: c.x - 40, y: c.y - 40 }, { x: c.x + 40, y: c.y + 40 });
  for (const w of doc.wires) pts.push(w.a, w.b);
  if (!pts.length) return null;
  return {
    minX: Math.min(...pts.map((p) => p.x)),
    minY: Math.min(...pts.map((p) => p.y)),
    maxX: Math.max(...pts.map((p) => p.x)),
    maxY: Math.max(...pts.map((p) => p.y)),
  };
}
