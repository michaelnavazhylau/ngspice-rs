const PREFIXES: [number, string][] = [
  [1e12, "T"],
  [1e9, "G"],
  [1e6, "M"],
  [1e3, "k"],
  [1, ""],
  [1e-3, "m"],
  [1e-6, "µ"],
  [1e-9, "n"],
  [1e-12, "p"],
  [1e-15, "f"],
];

/** Format with an SI prefix and `digits` significant digits: 0.0025 -> "2.5m". */
export function formatSI(value: number, digits = 3, unit = ""): string {
  if (value === null || Number.isNaN(value)) return "NaN";
  if (!Number.isFinite(value)) return (value < 0 ? "-" : "") + "∞" + unit;
  if (value === 0) return "0" + (unit ? ` ${unit}` : "");
  const abs = Math.abs(value);
  let chosen = PREFIXES[PREFIXES.length - 1]!;
  for (const p of PREFIXES) {
    if (abs >= p[0] * 0.9995) {
      chosen = p;
      break;
    }
  }
  if (abs < 1e-15 * 0.9995 || abs >= 1e15) {
    return value.toExponential(digits - 1).replace(/\.?0+e/, "e") + (unit ? ` ${unit}` : "");
  }
  const scaled = value / chosen[0];
  let text = Number(scaled.toPrecision(digits)).toString();
  // toPrecision can round 999.9 up to 1000: bump to next prefix
  if (Math.abs(Number(text)) >= 1000 && chosen[0] < 1e12) {
    const idx = PREFIXES.indexOf(chosen);
    const up = PREFIXES[idx - 1];
    if (up) {
      chosen = up;
      text = Number((value / chosen[0]).toPrecision(digits)).toString();
    }
  }
  return `${text}${unit ? " " : ""}${chosen[1]}${unit}`;
}

const SUFFIX: Record<string, number> = {
  t: 1e12,
  g: 1e9,
  meg: 1e6,
  k: 1e3,
  m: 1e-3,
  u: 1e-6,
  µ: 1e-6,
  n: 1e-9,
  p: 1e-12,
  f: 1e-15,
  mil: 25.4e-6,
};

/** Parse a SPICE number with suffix ("1k", "2.2n", "1meg", "10uF"). Returns null if invalid. */
export function parseSI(text: string): number | null {
  const m = /^\s*([+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?)\s*([a-zµ]*)\s*$/i.exec(text);
  if (!m) return null;
  const base = Number(m[1]);
  const rest = m[2]!.toLowerCase();
  if (!rest) return base;
  if (rest.startsWith("meg")) return base * 1e6;
  if (rest.startsWith("mil")) return base * 25.4e-6;
  const s = SUFFIX[rest[0]!];
  return s === undefined ? base : base * s;
}
