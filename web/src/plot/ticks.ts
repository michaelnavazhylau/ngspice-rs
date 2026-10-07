import { formatSI } from "../circuit/si.ts";

/** Round `x` to a "nice" 1/2/5 x 10^k number. */
export function niceStep(raw: number): number {
  if (!(raw > 0) || !Number.isFinite(raw)) return 1;
  const exp = Math.floor(Math.log10(raw));
  const f = raw / 10 ** exp;
  const nf = f < 1.5 ? 1 : f < 3.5 ? 2 : f < 7.5 ? 5 : 10;
  return nf * 10 ** exp;
}

export interface LinearTicks {
  ticks: number[];
  step: number;
  min: number;
  max: number;
}

/** About `target` evenly spaced ticks covering [min, max] (ticks lie inside the range). */
export function linearTicks(min: number, max: number, target = 6): LinearTicks {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return { ticks: [], step: 1, min: 0, max: 1 };
  if (min === max) {
    const d = Math.abs(min) * 0.5 || 1;
    min -= d;
    max += d;
  }
  if (min > max) [min, max] = [max, min];
  const step = niceStep((max - min) / Math.max(1, target));
  const first = Math.ceil(min / step - 1e-9);
  const last = Math.floor(max / step + 1e-9);
  const ticks: number[] = [];
  for (let i = first; i <= last; i++) ticks.push(Number((i * step).toPrecision(12)));
  return { ticks, step, min, max };
}

/** Expand a data range to nice outer bounds (for y axes). */
export function niceBounds(min: number, max: number, target = 6): [number, number] {
  if (min === max) {
    const d = Math.abs(min) * 0.1 || 1;
    min -= d;
    max += d;
  }
  const step = niceStep((max - min) / target);
  return [Math.floor(min / step + 1e-9) * step, Math.ceil(max / step - 1e-9) * step];
}

export interface LogTicks {
  major: number[];
  minor: number[];
}

/** Decade ticks for a log axis; adds 2..9 minor ticks. Range must be positive. */
export function logTicks(min: number, max: number): LogTicks {
  const major: number[] = [];
  const minor: number[] = [];
  if (!(min > 0) || !(max > min)) return { major, minor };
  const e0 = Math.floor(Math.log10(min) + 1e-12);
  const e1 = Math.ceil(Math.log10(max) - 1e-12);
  for (let e = e0; e <= e1; e++) {
    const base = 10 ** e;
    if (base >= min * (1 - 1e-9) && base <= max * (1 + 1e-9)) major.push(Number(base.toPrecision(12)));
    for (let m = 2; m <= 9; m++) {
      const v = base * m;
      if (v >= min && v <= max) minor.push(Number(v.toPrecision(12)));
    }
  }
  return { major, minor };
}

/** Tick label with SI prefix; `unit` is appended ("1.5 kHz" style handled by caller). */
export function formatTick(v: number): string {
  if (v === 0) return "0";
  return formatSI(Number(v.toPrecision(10)), 4);
}
