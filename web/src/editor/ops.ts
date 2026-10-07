import { onSegmentInterior, pinPositions, samePoint, wireLength } from "../circuit/geometry.ts";
import type { Component, Doc, Point, Rot, Wire } from "../circuit/model.ts";
import { newId } from "../circuit/model.ts";

export function deleteIds(doc: Doc, ids: ReadonlySet<string>): Doc {
  return {
    components: doc.components.filter((c) => !ids.has(c.id)),
    wires: doc.wires.filter((w) => !ids.has(w.id)),
  };
}

export function updateComponent(doc: Doc, id: string, patch: Partial<Component>): Doc {
  return { ...doc, components: doc.components.map((c) => (c.id === id ? { ...c, ...patch } : c)) };
}

export function rotateIds(doc: Doc, ids: ReadonlySet<string>): Doc {
  return {
    ...doc,
    components: doc.components.map((c) =>
      ids.has(c.id) ? { ...c, rot: (((c.rot + 1) % 4) as Rot) } : c,
    ),
  };
}

/** Add wire segments (skips zero-length ones). */
export function addWirePath(doc: Doc, pts: Point[]): Doc {
  const wires: Wire[] = [...doc.wires];
  for (let i = 0; i + 1 < pts.length; i++) {
    const a = pts[i]!;
    const b = pts[i + 1]!;
    if (samePoint(a, b)) continue;
    wires.push({ id: newId("w"), a, b });
  }
  return { ...doc, wires };
}

/**
 * Move components by (dx,dy). Wire endpoints sitting on a moved pin follow it;
 * wires that become diagonal are replaced by an L-shaped pair of segments.
 */
export function moveComponents(doc: Doc, ids: ReadonlySet<string>, dx: number, dy: number): Doc {
  if (dx === 0 && dy === 0) return doc;
  const movedPins = doc.components.filter((c) => ids.has(c.id)).flatMap((c) => pinPositions(c));
  const isMovedPin = (p: Point) => movedPins.some((q) => samePoint(p, q));
  const shift = (p: Point): Point => ({ x: p.x + dx, y: p.y + dy });
  const wires: Wire[] = [];
  for (const w of doc.wires) {
    // Segments whose interior holds a moved pin (pin on a wire body) are left alone.
    const ma = isMovedPin(w.a);
    const mb = isMovedPin(w.b);
    if (!ma && !mb) {
      wires.push(w);
      continue;
    }
    const a = ma ? shift(w.a) : w.a;
    const b = mb ? shift(w.b) : w.b;
    if (a.x === b.x || a.y === b.y) {
      wires.push({ ...w, a, b });
    } else {
      const fixedFirst = !ma; // keep the fixed end's leg straight
      const corner = fixedFirst ? { x: a.x, y: b.y } : { x: b.x, y: a.y };
      wires.push({ ...w, a, b: corner }, { id: newId("w"), a: corner, b });
    }
  }
  return {
    components: doc.components.map((c) => (ids.has(c.id) ? { ...c, x: c.x + dx, y: c.y + dy } : c)),
    wires: wires.filter((w) => wireLength(w) > 0),
  };
}

/** Move selected free-standing wires too (used when only wires are selected). */
export function moveWires(doc: Doc, ids: ReadonlySet<string>, dx: number, dy: number): Doc {
  return {
    ...doc,
    wires: doc.wires.map((w) =>
      ids.has(w.id) ? { ...w, a: { x: w.a.x + dx, y: w.a.y + dy }, b: { x: w.b.x + dx, y: w.b.y + dy } } : w,
    ),
  };
}

/** True when `p` is a pin or lies on a wire: a wire chain ends there. */
export function isConnectionPoint(doc: Doc, p: Point): boolean {
  for (const c of doc.components) if (pinPositions(c).some((q) => samePoint(q, p))) return true;
  for (const w of doc.wires) {
    if (samePoint(w.a, p) || samePoint(w.b, p) || onSegmentInterior(p, w.a, w.b)) return true;
  }
  return false;
}
