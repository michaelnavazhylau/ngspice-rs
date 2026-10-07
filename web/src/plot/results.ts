import type { Plot, Variable } from "../sim/types.ts";
import type { Series } from "./LineChart.tsx";

/** Categorical palette readable on both light and dark backgrounds. */
export const PALETTE = [
  "#3b82f6",
  "#f97316",
  "#10b981",
  "#e11d48",
  "#a855f7",
  "#eab308",
  "#06b6d4",
  "#84cc16",
  "#ec4899",
  "#64748b",
];

export const colorFor = (index: number): string => PALETTE[index % PALETTE.length]!;

export const isScaleless = (p: Plot): boolean => p.type === "Operating Point";

/** Magnitude in dB. A node with no AC response (e.g. a supply rail) has
 * magnitude exactly 0, which has no dB value; it is left out (null) rather than
 * plotted at a huge negative number that would flatten every other trace. */
export function magDb(re: number | null, im: number | null): number | null {
  if (re == null || im == null) return null;
  const m = Math.hypot(re, im);
  return m > 0 ? 20 * Math.log10(m) : null;
}

/** Phase in degrees, unwrapped along the sweep. */
export function phaseDeg(re: (number | null)[], im: (number | null)[]): (number | null)[] {
  const out: (number | null)[] = [];
  let prev: number | null = null;
  let offset = 0;
  for (let i = 0; i < re.length; i++) {
    const r = re[i];
    const m = im[i];
    // Phase is undefined at zero magnitude.
    if (r == null || m == null || (r === 0 && m === 0)) {
      out.push(null);
      continue;
    }
    let d = (Math.atan2(m, r) * 180) / Math.PI;
    if (prev != null) {
      while (d + offset - prev > 180) offset -= 360;
      while (d + offset - prev < -180) offset += 360;
    }
    d += offset;
    prev = d;
    out.push(d);
  }
  return out;
}

export const num = (a: (number | null)[]): number[] => a.map((v) => (v == null ? NaN : v));

export type TraceColors = Map<string, string>;

/** Stable colour per variable name within a plot. */
export function traceColors(vars: Variable[]): TraceColors {
  const m = new Map<string, string>();
  vars.slice(1).forEach((v, i) => m.set(v.name, colorFor(i)));
  return m;
}

export function buildSeries(
  plot: Plot,
  mode: "re" | "mag" | "phase",
  shown: ReadonlySet<string>,
  colors: TraceColors,
): Series[] {
  const [scale, ...rest] = plot.variables;
  if (!scale) return [];
  const x = num(scale.re);
  return rest.map((v) => {
    let y: (number | null)[];
    if (plot.complex && v.im) {
      y = mode === "phase" ? phaseDeg(v.re, v.im) : v.re.map((r, i) => magDb(r, v.im![i] ?? null));
    } else y = v.re;
    return { name: v.name, color: colors.get(v.name) ?? "#888", x, y, visible: shown.has(v.name) };
  });
}

/** True when an AC variable is zero at every frequency (no AC excitation reaches it). */
export const noAcResponse = (v: Variable): boolean =>
  !!v.im && v.re.every((r, i) => (r ?? 0) === 0 && (v.im![i] ?? 0) === 0);

/** Default visible traces: node voltages (max 8), else everything. AC traces
 * without any response are skipped. */
export function defaultShown(plot: Plot): Set<string> {
  const vars = plot.variables
    .slice(isScaleless(plot) ? 0 : 1)
    .filter((v) => !(plot.complex && noAcResponse(v)));
  const v = vars.filter((x) => x.name.startsWith("v("));
  return new Set((v.length ? v : vars).slice(0, 8).map((x) => x.name));
}
