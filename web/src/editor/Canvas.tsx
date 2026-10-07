import { clsx } from "clsx";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { docBounds, GRID, junctionPoints, pinNames, pinPositions, snap } from "../circuit/geometry.ts";
import type { Component, Doc, Kind, ModelType, Point, Rot } from "../circuit/model.ts";
import { KIND_LABEL, makeComponent, MODEL_TYPE_LABEL } from "../circuit/model.ts";
import { pinId, type NetlistResult } from "../circuit/netlist.ts";
import { localBox, SymbolGlyph } from "../circuit/symbols.tsx";
import {
  addWirePath,
  deleteIds,
  isConnectionPoint,
  moveComponents,
  moveWires,
  rotateIds,
} from "./ops.ts";

export type Tool =
  | { type: "select" }
  | { type: "wire" }
  | { type: "probe" }
  | { type: "place"; kind: Kind; modelType?: ModelType };

interface View {
  x: number;
  y: number;
  k: number;
}

interface Props {
  doc: Doc;
  commit: (d: Doc) => void;
  undo: () => void;
  redo: () => void;
  selection: ReadonlySet<string>;
  setSelection: (s: Set<string>) => void;
  tool: Tool;
  setTool: (t: Tool) => void;
  netlist: NetlistResult;
  /** net name -> text drawn at the net (e.g. ".op" voltages) */
  annotations: Map<string, string> | null;
  /** net name -> trace colour for probed nets */
  probed: Map<string, string>;
  onProbe: (net: string) => void;
  fitKey: number;
}

type Drag =
  | { type: "pan"; sx: number; sy: number; vx: number; vy: number }
  | { type: "move"; start: Point; dx: number; dy: number; moved: boolean }
  | { type: "box"; start: Point; cur: Point; additive: boolean };

const isTyping = (t: EventTarget | null): boolean => {
  const el = t as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
};

export function Canvas(p: Props) {
  const { doc, commit, selection, setSelection, tool, setTool, netlist } = p;
  const svgRef = useRef<SVGSVGElement>(null);
  const [view, setView] = useState<View>({ x: 40, y: 40, k: 1 });
  const [cursor, setCursor] = useState<Point | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [ghostRot, setGhostRot] = useState<Rot>(0);
  const [chain, setChain] = useState<Point[] | null>(null);
  const [space, setSpace] = useState(false);
  const [flash, setFlash] = useState<string | null>(null);

  const toWorld = useCallback(
    (e: { clientX: number; clientY: number }): Point => {
      const r = svgRef.current!.getBoundingClientRect();
      return { x: (e.clientX - r.left - view.x) / view.k, y: (e.clientY - r.top - view.y) / view.k };
    },
    [view],
  );
  const snapped = (pt: Point): Point => ({ x: snap(pt.x), y: snap(pt.y) });

  // fit to content when requested
  const fit = useCallback(() => {
    const el = svgRef.current;
    if (!el) return;
    const b = docBounds(doc);
    const w = el.clientWidth;
    const h = el.clientHeight;
    if (!b) return setView({ x: w / 2, y: h / 2, k: 1 });
    const pad = 30;
    const k = Math.min(2, Math.max(0.3, Math.min((w - 2 * pad) / (b.maxX - b.minX), (h - 2 * pad) / (b.maxY - b.minY))));
    setView({
      k,
      x: (w - (b.maxX - b.minX) * k) / 2 - b.minX * k,
      y: (h - (b.maxY - b.minY) * k) / 2 - b.minY * k,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc]);
  useEffect(() => {
    fit();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.fitKey]);

  // wheel zoom (non-passive)
  useEffect(() => {
    const el = svgRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      const mx = e.clientX - r.left;
      const my = e.clientY - r.top;
      setView((v) => {
        const k = Math.min(4, Math.max(0.2, v.k * Math.exp(-e.deltaY * 0.0015)));
        return { k, x: mx - ((mx - v.x) / v.k) * k, y: my - ((my - v.y) / v.k) * k };
      });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  const finishChain = useCallback(
    (pts: Point[] | null) => {
      if (pts && pts.length > 1) commit(addWirePath(doc, pts));
      setChain(null);
    },
    [commit, doc],
  );

  // keyboard
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (isTyping(e.target)) return;
      const mod = e.metaKey || e.ctrlKey;
      if (e.code === "Space") {
        setSpace(true);
        e.preventDefault();
        return;
      }
      if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) p.redo();
        else p.undo();
        return;
      }
      if (mod && e.key.toLowerCase() === "y") {
        e.preventDefault();
        p.redo();
        return;
      }
      if (mod && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setSelection(new Set([...doc.components.map((c) => c.id), ...doc.wires.map((w) => w.id)]));
        return;
      }
      if (mod) return;
      switch (e.key) {
        case "Delete":
        case "Backspace":
          if (selection.size) {
            e.preventDefault();
            commit(deleteIds(doc, selection));
            setSelection(new Set());
          }
          break;
        case "r":
        case "R":
          if (tool.type === "place") setGhostRot(((ghostRot + 1) % 4) as Rot);
          else if (selection.size) commit(rotateIds(doc, selection));
          break;
        case "w":
        case "W":
          setTool({ type: "wire" });
          break;
        case "s":
        case "S":
          setChain(null);
          setTool({ type: "select" });
          break;
        case "p":
        case "P":
          setTool({ type: "probe" });
          break;
        case "Enter":
          if (chain) finishChain(chain);
          break;
        case "Escape":
          if (chain) setChain(null);
          else if (tool.type !== "select") setTool({ type: "select" });
          else setSelection(new Set());
          break;
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.code === "Space") setSpace(false);
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  });

  // Live document while dragging a selection.
  const live = useMemo(() => {
    if (drag?.type !== "move" || !drag.moved) return doc;
    const compIds = new Set(doc.components.filter((c) => selection.has(c.id)).map((c) => c.id));
    const wireIds = new Set(doc.wires.filter((w) => selection.has(w.id)).map((w) => w.id));
    return moveWires(moveComponents(doc, compIds, drag.dx, drag.dy), wireIds, drag.dx, drag.dy);
  }, [doc, drag, selection]);

  const junctions = useMemo(() => junctionPoints(live), [live]);

  const hitId = (t: EventTarget): { cid?: string; wid?: string; pin?: string } => {
    const el = (t as Element).closest?.("[data-cid],[data-wid]") as HTMLElement | null;
    return { cid: el?.dataset.cid, wid: el?.dataset.wid, pin: (t as HTMLElement).dataset?.pin };
  };

  const probeAt = (e: React.PointerEvent) => {
    const { cid, wid, pin } = hitId(e.target);
    let net: string | undefined;
    if (pin) net = netlist.pinNet.get(pin);
    else if (wid) net = netlist.wireNet.get(wid);
    else if (cid) {
      const c = doc.components.find((x) => x.id === cid);
      if (c) net = netlist.pinNet.get(pinId(c, 0));
    }
    if (!net) return;
    if (net === "0") {
      setFlash("Ground (0) is the reference: no trace");
      setTimeout(() => setFlash(null), 1800);
      return;
    }
    p.onProbe(net);
  };

  const onPointerDown = (e: React.PointerEvent<SVGSVGElement>) => {
    const w = toWorld(e);
    const sw = snapped(w);
    if (e.button === 1 || (e.button === 0 && space)) {
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      setDrag({ type: "pan", sx: e.clientX, sy: e.clientY, vx: view.x, vy: view.y });
      return;
    }
    if (e.button === 2) {
      // right button cancels the current tool / wire
      if (chain) finishChain(chain);
      else if (tool.type !== "select") setTool({ type: "select" });
      return;
    }
    if (e.button !== 0) return;
    if (tool.type === "place") {
      const c = makeComponent(doc, tool.kind, sw.x, sw.y, ghostRot, tool.modelType);
      commit({ ...doc, components: [...doc.components, c] });
      setSelection(new Set([c.id]));
      return;
    }
    if (tool.type === "wire") {
      if (!chain) {
        setChain([sw]);
        return;
      }
      const last = chain[chain.length - 1]!;
      const dx = Math.abs(sw.x - last.x);
      const dy = Math.abs(sw.y - last.y);
      if (dx === 0 && dy === 0) return finishChain(chain);
      const corner = dy > dx ? { x: last.x, y: sw.y } : { x: sw.x, y: last.y };
      const pts = dx === 0 || dy === 0 ? [...chain, sw] : [...chain, corner, sw];
      if (isConnectionPoint(doc, sw)) finishChain(pts);
      else setChain(pts);
      return;
    }
    if (tool.type === "probe") return probeAt(e);
    // select tool
    const { cid, wid } = hitId(e.target);
    const id = cid ?? wid;
    if (id) {
      e.currentTarget.setPointerCapture(e.pointerId);
      let sel: Set<string>;
      if (e.shiftKey) {
        sel = new Set(selection);
        if (sel.has(id)) sel.delete(id);
        else sel.add(id);
      } else sel = selection.has(id) ? new Set(selection) : new Set([id]);
      setSelection(sel);
      if (sel.has(id)) setDrag({ type: "move", start: sw, dx: 0, dy: 0, moved: false });
    } else {
      e.currentTarget.setPointerCapture(e.pointerId);
      setDrag({ type: "box", start: w, cur: w, additive: e.shiftKey });
      if (!e.shiftKey) setSelection(new Set());
    }
  };

  const onPointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const w = toWorld(e);
    const sw = snapped(w);
    setCursor(sw);
    if (!drag) return;
    if (drag.type === "pan") {
      setView({ ...view, x: drag.vx + e.clientX - drag.sx, y: drag.vy + e.clientY - drag.sy });
    } else if (drag.type === "move") {
      const dx = sw.x - drag.start.x;
      const dy = sw.y - drag.start.y;
      if (dx !== drag.dx || dy !== drag.dy) setDrag({ ...drag, dx, dy, moved: drag.moved || dx !== 0 || dy !== 0 });
    } else if (drag.type === "box") {
      setDrag({ ...drag, cur: w });
    }
  };

  const onPointerUp = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!drag) return;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
    if (drag.type === "move" && drag.moved) commit(live);
    if (drag.type === "box") {
      const x0 = Math.min(drag.start.x, drag.cur.x);
      const x1 = Math.max(drag.start.x, drag.cur.x);
      const y0 = Math.min(drag.start.y, drag.cur.y);
      const y1 = Math.max(drag.start.y, drag.cur.y);
      if (x1 - x0 > 3 || y1 - y0 > 3) {
        const inside = (q: Point) => q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1;
        const sel = new Set(drag.additive ? selection : []);
        for (const c of doc.components) if (inside(c)) sel.add(c.id);
        for (const w of doc.wires) if (inside(w.a) && inside(w.b)) sel.add(w.id);
        setSelection(sel);
      }
    }
    setDrag(null);
  };

  const onDoubleClick = () => {
    if (tool.type === "wire" && chain) finishChain(chain);
  };

  // wire preview
  const preview: Point[] | null = (() => {
    if (tool.type !== "wire" || !chain || !cursor) return null;
    const last = chain[chain.length - 1]!;
    const dx = Math.abs(cursor.x - last.x);
    const dy = Math.abs(cursor.y - last.y);
    if (dx === 0 && dy === 0) return chain;
    const corner = dy > dx ? { x: last.x, y: cursor.y } : { x: cursor.x, y: last.y };
    return dx === 0 || dy === 0 ? [...chain, cursor] : [...chain, corner, cursor];
  })();

  const ghost: Component | null =
    tool.type === "place" && cursor ? { ...makeComponent(doc, tool.kind, cursor.x, cursor.y, ghostRot, tool.modelType) } : null;

  const cursorClass =
    drag?.type === "pan" || space ? "cursor-grabbing" : tool.type === "select" ? "cursor-default" : "cursor-crosshair";

  return (
    <div className="relative h-full w-full overflow-hidden bg-canvas">
      <svg
        ref={svgRef}
        className={clsx("h-full w-full touch-none select-none", cursorClass)}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={() => setCursor(null)}
        onDoubleClick={onDoubleClick}
        onContextMenu={(e) => e.preventDefault()}
      >
        <defs>
          <pattern
            id="grid-dots"
            width={GRID * view.k}
            height={GRID * view.k}
            patternUnits="userSpaceOnUse"
            x={view.x}
            y={view.y}
          >
            <circle cx={0} cy={0} r={view.k > 0.6 ? 0.9 : 0.5} className="fill-grid" />
          </pattern>
        </defs>
        <rect width="100%" height="100%" fill="url(#grid-dots)" data-bg />
        <g transform={`translate(${view.x} ${view.y}) scale(${view.k})`}>
          {/* wires */}
          {live.wires.map((w) => {
            const net = netlist.wireNet.get(w.id);
            const color = net ? p.probed.get(net) : undefined;
            return (
              <g key={w.id} data-wid={w.id} className={clsx("wire", selection.has(w.id) && "sel")}>
                <line x1={w.a.x} y1={w.a.y} x2={w.b.x} y2={w.b.y} className="hit" />
                <line
                  x1={w.a.x}
                  y1={w.a.y}
                  x2={w.b.x}
                  y2={w.b.y}
                  className="vis"
                  style={color ? { stroke: color, strokeWidth: 3 } : undefined}
                />
              </g>
            );
          })}
          {junctions.map((j) => (
            <circle key={`${j.x},${j.y}`} cx={j.x} cy={j.y} r={3.5} className="fill-wire pointer-events-none" />
          ))}
          {/* components */}
          {live.components.map((c) => {
            const box = localBox(c.kind);
            const pins = pinPositions(c);
            return (
              <g key={c.id} data-cid={c.id} className={clsx("comp", selection.has(c.id) && "sel")}>
                <SymbolGlyph c={c} />
                <g transform={`translate(${c.x} ${c.y}) rotate(${c.rot * 90})`}>
                  <rect
                    x={box.x0 - 3}
                    y={box.y0 - 3}
                    width={box.x1 - box.x0 + 6}
                    height={box.y1 - box.y0 + 6}
                    className="hit-box"
                    rx={3}
                  />
                </g>
                {pins.map((q, i) => (
                  <circle
                    key={i}
                    cx={q.x}
                    cy={q.y}
                    r={3.2}
                    className="pin"
                    data-pin={pinId(c, i)}
                    data-cid={c.id}
                  >
                    <title>{c.kind === "GND" || c.kind === "LABEL" ? c.kind : `${c.name} ${pinNames(c)[i] ?? `pin ${i + 1}`}`}</title>
                  </circle>
                ))}
              </g>
            );
          })}
          {/* probed-net highlights and annotations */}
          {p.annotations &&
            [...netlist.netAnchors].map(([net, pt]) => {
              const text = p.annotations!.get(net);
              if (!text) return null;
              const wd = text.length * 6.2 + 8;
              return (
                <g key={net} transform={`translate(${pt.x + 6} ${pt.y - 22})`} className="pointer-events-none">
                  <rect width={wd} height={15} rx={3} className="fill-annot stroke-accent" strokeWidth={0.8} />
                  <text x={4} y={11} className="fill-fg font-mono text-[10px]">
                    {text}
                  </text>
                </g>
              );
            })}
          {/* placement ghost */}
          {ghost && (
            <g opacity={0.6} className="pointer-events-none comp sel">
              <SymbolGlyph c={ghost} />
            </g>
          )}
          {preview && (
            <polyline
              points={preview.map((q) => `${q.x},${q.y}`).join(" ")}
              fill="none"
              className="stroke-accent pointer-events-none"
              strokeWidth={2}
              strokeDasharray="5 3"
            />
          )}
          {tool.type === "wire" && cursor && (
            <circle cx={cursor.x} cy={cursor.y} r={4.5} className="fill-none stroke-accent pointer-events-none" />
          )}
          {drag?.type === "box" && (
            <rect
              x={Math.min(drag.start.x, drag.cur.x)}
              y={Math.min(drag.start.y, drag.cur.y)}
              width={Math.abs(drag.cur.x - drag.start.x)}
              height={Math.abs(drag.cur.y - drag.start.y)}
              className="fill-accent/10 stroke-accent pointer-events-none"
              strokeDasharray="4 3"
            />
          )}
        </g>
      </svg>
      <div className="pointer-events-none absolute bottom-2 left-2 rounded bg-panel/90 px-2 py-0.5 font-mono text-[11px] text-muted">
        {hintFor(tool, !!chain)} · {Math.round(view.k * 100)}%
        {cursor ? ` · ${cursor.x / GRID},${cursor.y / GRID}` : ""}
      </div>
      {flash && (
        <div className="pointer-events-none absolute top-2 left-1/2 -translate-x-1/2 rounded bg-panel px-3 py-1 text-xs shadow">
          {flash}
        </div>
      )}
      <div className="absolute right-2 bottom-2 flex gap-1">
        <button className="btn !px-2" title="Fit to content" onClick={fit}>
          Fit
        </button>
        <button className="btn !px-2" onClick={() => setView((v) => ({ ...v, k: Math.min(4, v.k * 1.25) }))}>
          +
        </button>
        <button className="btn !px-2" onClick={() => setView((v) => ({ ...v, k: Math.max(0.2, v.k / 1.25) }))}>
          −
        </button>
      </div>
    </div>
  );
}

function hintFor(tool: Tool, chain: boolean): string {
  switch (tool.type) {
    case "select":
      return "Select: click, shift-click, drag box · R rotate · Del delete · space/middle drag pans";
    case "wire":
      return chain ? "Wire: click to bend/end at a pin or wire · dbl-click/Enter finish · Esc cancel" : "Wire: click a start point";
    case "probe":
      return "Probe: click a net to toggle its v(net) trace";
    case "place":
      return `Place ${tool.modelType ? MODEL_TYPE_LABEL[tool.modelType] : KIND_LABEL[tool.kind]}: click to drop · R rotate · Esc stop`;
  }
}
