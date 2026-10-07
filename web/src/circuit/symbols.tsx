import type { ReactNode } from "react";
import { LOCAL_PINS } from "./geometry.ts";
import type { Component, Kind, ModelType, Point } from "./model.ts";

const stroke = { fill: "none", strokeWidth: 1.8, strokeLinecap: "round", strokeLinejoin: "round" } as const;

/** Body of a symbol in its local, unrotated frame (pins excluded). `modelType`
 * picks the BJT/MOSFET polarity (arrow direction). */
export function symbolBody(kind: Kind, modelType?: ModelType): ReactNode {
  switch (kind) {
    case "R":
      return (
        <polyline
          {...stroke}
          points="-30,0 -18,0 -15,-7 -9,7 -3,-7 3,7 9,-7 15,7 18,0 30,0"
        />
      );
    case "C":
      return (
        <g {...stroke}>
          <path d="M-30 0H-4M4 0H30" />
          <path d="M-4 -11V11M4 -11V11" />
        </g>
      );
    case "L":
      return (
        <path
          {...stroke}
          d="M-30 0H-20a5 5 0 0 1 10 0a5 5 0 0 1 10 0a5 5 0 0 1 10 0a5 5 0 0 1 10 0H30"
        />
      );
    case "V":
      return (
        <g {...stroke}>
          <path d="M0 -30V-14M0 14V30" />
          <circle cx={0} cy={0} r={14} />
          <path d="M0 -10V-4M-3 -7H3" strokeWidth={1.4} />
          <path d="M-3 7H3" strokeWidth={1.4} />
        </g>
      );
    case "I":
      return (
        <g {...stroke}>
          <path d="M0 -30V-14M0 14V30" />
          <circle cx={0} cy={0} r={14} />
          <path d="M0 -8V7M-4 2L0 8L4 2" />
        </g>
      );
    case "D":
      return (
        <g {...stroke}>
          <path d="M-30 0H-8M8 0H30M8 -9V9" />
          <path d="M-8 -9L8 0L-8 9Z" className="sym-fill" />
        </g>
      );
    case "Q": {
      // NPN: collector on top, emitter (bottom leg, (-8,6) to (10,18)) with the
      // arrow pointing away from the base. PNP is drawn mirrored so its emitter
      // is on top, with the arrow pointing into the base.
      const pnp = modelType === "pnp";
      const arrow = pnp ? "M-2.6 9.6L-0.4 15.3L3.5 9.5Z" : "M7 16L0.1 15.6L3.9 9.8Z";
      return (
        <g {...stroke} transform={pnp ? "scale(1 -1)" : undefined}>
          <path d="M-30 0H-8" />
          <path d="M-8 -12V12" strokeWidth={3} />
          <path d="M-8 -6L10 -18V-30M-8 6L10 18V30" />
          <path d={arrow} className="sym-fill" strokeWidth={1} />
        </g>
      );
    }
    case "M": {
      // Enhancement MOSFET, 3 terminals, body tied to the source. NMOS: drain
      // on top, source below, arrow into the channel. PMOS is mirrored (source
      // on top) with the arrow pointing out.
      const pmos = modelType === "pmos";
      const arrow = pmos ? "M8 0L2 -3.5V3.5Z" : "M-5 0L1 -3.5V3.5Z";
      return (
        <g {...stroke} transform={pmos ? "scale(1 -1)" : undefined}>
          <path d="M-30 0H-12M-12 -12V12" />
          <path d="M-6 -14V-6M-6 -4V4M-6 6V14" strokeWidth={2.4} />
          <path d="M-6 -10H10V-30M-6 10H10V30M-6 0H10V10" />
          <path d={arrow} className="sym-fill" strokeWidth={1} />
        </g>
      );
    }
    case "GND":
      return (
        <g {...stroke}>
          <path d="M0 0V8M-12 8H12M-8 13H8M-4 18H4" />
        </g>
      );
    case "LABEL":
      return <path {...stroke} d="M0 0H5" />;
  }
}

/** Is the symbol drawn vertically after rotation? */
export function isVertical(c: Pick<Component, "kind" | "rot">): boolean {
  const baseVertical = c.kind === "V" || c.kind === "I" || c.kind === "Q" || c.kind === "M";
  return baseVertical !== (c.rot % 2 === 1);
}

export interface TextSpot {
  x: number;
  y: number;
  anchor: "start" | "middle" | "end";
}

/** Where the name and value text go (in world coordinates relative to the component origin). */
export function textSpots(c: Component): { name: TextSpot; value: TextSpot } {
  if (isVertical(c)) {
    return {
      name: { x: 20, y: -3, anchor: "start" },
      value: { x: 20, y: 10, anchor: "start" },
    };
  }
  return {
    name: { x: 0, y: -14, anchor: "middle" },
    value: { x: 0, y: 24, anchor: "middle" },
  };
}

export function sourceSummary(c: Component): string {
  const s = c.src;
  if (!s) return "";
  const parts: string[] = [];
  if (s.wave === "dc") parts.push(`DC ${s.dc || "0"}`);
  if (s.wave === "pulse") parts.push(`PULSE ${s.pulse.v1}→${s.pulse.v2}`);
  if (s.wave === "pwl") parts.push("PWL");
  if (s.ac.trim()) parts.push(`AC ${s.ac}`);
  return parts.join(" ");
}

export function displayValue(c: Component): string {
  if (c.kind === "V" || c.kind === "I") return sourceSummary(c);
  if (c.model) return [c.model.name, c.value.trim()].filter(Boolean).join(" ");
  return c.value;
}

/** Complete symbol in world coordinates (rotation applied), used by canvas and palette. */
export function SymbolGlyph({ c, withText = true }: { c: Component; withText?: boolean }) {
  const spots = textSpots(c);
  return (
    <g>
      <g transform={`translate(${c.x} ${c.y}) rotate(${c.rot * 90})`} className="sym">
        {symbolBody(c.kind, c.model?.type)}
      </g>
      {withText && c.kind === "LABEL" && (
        <text
          x={c.x + (c.rot === 2 ? -8 : 8)}
          y={c.y - 7}
          textAnchor={c.rot === 2 ? "end" : "start"}
          className="fill-accent font-mono text-[12px] font-semibold"
        >
          {c.value}
        </text>
      )}
      {withText && c.kind !== "LABEL" && c.kind !== "GND" && (
        <g className="font-mono text-[11px]">
          <text x={c.x + spots.name.x} y={c.y + spots.name.y} textAnchor={spots.name.anchor} className="fill-fg font-semibold">
            {c.name}
          </text>
          <text x={c.x + spots.value.x} y={c.y + spots.value.y} textAnchor={spots.value.anchor} className="fill-muted">
            {displayValue(c)}
          </text>
        </g>
      )}
    </g>
  );
}

/** Approximate selection/hit box in local coordinates. */
export function localBox(kind: Kind): { x0: number; y0: number; x1: number; y1: number } {
  switch (kind) {
    case "R":
    case "C":
    case "L":
      return { x0: -30, y0: -12, x1: 30, y1: 12 };
    case "V":
    case "I":
      return { x0: -14, y0: -30, x1: 14, y1: 30 };
    case "D":
      return { x0: -30, y0: -10, x1: 30, y1: 10 };
    case "Q":
    case "M":
      return { x0: -30, y0: -30, x1: 14, y1: 30 };
    case "GND":
      return { x0: -12, y0: 0, x1: 12, y1: 20 };
    case "LABEL":
      return { x0: 0, y0: -8, x1: 40, y1: 8 };
  }
}

export const PIN_COUNT = (k: Kind): number => LOCAL_PINS[k].length;

export type { Point };
