import { clsx } from "clsx";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { formatSI } from "../circuit/si.ts";
import { formatTick, linearTicks, logTicks, niceBounds } from "./ticks.ts";

export interface Series {
  name: string;
  color: string;
  x: number[];
  y: (number | null)[];
  visible: boolean;
}

interface Props {
  series: Series[];
  xScale?: "linear" | "log";
  xLabel: string;
  xUnit?: string;
  yLabel: string;
  yUnit?: string;
  /** Show legend with toggles; `onToggle` called with the series name. */
  legend?: boolean;
  onToggle?: (name: string) => void;
  /** Fixed y range expansion e.g. to keep 0 visible */
  className?: string;
}

const M = { l: 58, r: 12, t: 8, b: 30 };

export function LineChart({
  series,
  xScale = "linear",
  xLabel,
  xUnit = "",
  yLabel,
  yUnit = "",
  legend = true,
  onToggle,
  className,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const cid = "clip" + useId().replace(/[^a-zA-Z0-9]/g, "");
  const [size, setSize] = useState({ w: 600, h: 200 });
  const [hover, setHover] = useState<number | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  const vis = series.filter((s) => s.visible);
  const log = xScale === "log";

  const dom = useMemo(() => {
    let x0 = Infinity,
      x1 = -Infinity,
      y0 = Infinity,
      y1 = -Infinity;
    for (const s of vis) {
      for (let i = 0; i < s.x.length; i++) {
        const x = s.x[i]!;
        const y = s.y[i];
        if (!Number.isFinite(x) || (log && x <= 0)) continue;
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y != null && Number.isFinite(y)) {
          if (y < y0) y0 = y;
          if (y > y1) y1 = y;
        }
      }
    }
    if (!Number.isFinite(x0)) {
      x0 = log ? 1 : 0;
      x1 = log ? 10 : 1;
    }
    if (x0 === x1) x1 = log ? x0 * 10 : x0 + 1;
    if (!Number.isFinite(y0)) {
      y0 = 0;
      y1 = 1;
    }
    const [ya, yb] = niceBounds(y0, y1, 5);
    return { x0, x1, y0: ya, y1: yb };
  }, [vis, log]);

  const pw = Math.max(10, size.w - M.l - M.r);
  const ph = Math.max(10, size.h - M.t - M.b);
  const fx = (x: number) =>
    log
      ? M.l + ((Math.log10(x) - Math.log10(dom.x0)) / (Math.log10(dom.x1) - Math.log10(dom.x0))) * pw
      : M.l + ((x - dom.x0) / (dom.x1 - dom.x0)) * pw;
  const fy = (y: number) => M.t + ph - ((y - dom.y0) / (dom.y1 - dom.y0)) * ph;
  const invX = (px: number) => {
    const t = (px - M.l) / pw;
    return log
      ? 10 ** (Math.log10(dom.x0) + t * (Math.log10(dom.x1) - Math.log10(dom.x0)))
      : dom.x0 + t * (dom.x1 - dom.x0);
  };

  const xt = log ? logTicks(dom.x0, dom.x1) : { major: linearTicks(dom.x0, dom.x1, Math.max(3, Math.floor(pw / 90))).ticks, minor: [] };
  const yt = linearTicks(dom.y0, dom.y1, Math.max(3, Math.floor(ph / 36))).ticks;

  const paths = useMemo(
    () =>
      vis.map((s) => {
        let d = "";
        let pen = false;
        for (let i = 0; i < s.x.length; i++) {
          const y = s.y[i];
          const x = s.x[i]!;
          if (y == null || !Number.isFinite(y) || !Number.isFinite(x) || (log && x <= 0)) {
            pen = false;
            continue;
          }
          d += `${pen ? "L" : "M"}${fx(x).toFixed(1)} ${fy(y).toFixed(1)}`;
          pen = true;
        }
        return { s, d };
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [vis, dom, pw, ph, log],
  );

  // hover: nearest sample by x of the first visible series
  const ref0 = vis[0];
  let hoverIdx: number | null = null;
  if (hover != null && ref0 && ref0.x.length) {
    const target = invX(hover);
    let lo = 0,
      hi = ref0.x.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (ref0.x[mid]! < target) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0 && Math.abs(ref0.x[lo - 1]! - target) < Math.abs(ref0.x[lo]! - target)) lo--;
    hoverIdx = lo;
  }
  const hx = ref0 && hoverIdx != null ? ref0.x[hoverIdx]! : null;

  return (
    <div className={clsx("flex min-h-0 flex-col", className)}>
      {legend && (
        <div className="flex flex-wrap gap-x-3 gap-y-0.5 px-2 pb-1 text-xs">
          {series.map((s) => (
            <button
              key={s.name}
              type="button"
              onClick={() => onToggle?.(s.name)}
              className={clsx("flex items-center gap-1 rounded px-1 hover:bg-hover", !s.visible && "opacity-40")}
              title={s.visible ? "Hide trace" : "Show trace"}
            >
              <span className="inline-block h-0.5 w-4" style={{ background: s.color, height: 3 }} />
              <span className="font-mono">{s.name}</span>
            </button>
          ))}
        </div>
      )}
      <div ref={ref} className="relative min-h-0 flex-1">
        <svg
          width={size.w}
          height={size.h}
          className="absolute inset-0 select-none"
          onPointerMove={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            const px = e.clientX - r.left;
            setHover(px >= M.l && px <= M.l + pw ? px : null);
          }}
          onPointerLeave={() => setHover(null)}
        >
          <rect x={M.l} y={M.t} width={pw} height={ph} className="fill-plot stroke-line" />
          {xt.minor.map((v) => (
            <line key={`xm${v}`} x1={fx(v)} x2={fx(v)} y1={M.t} y2={M.t + ph} className="stroke-grid" strokeOpacity={0.4} />
          ))}
          {xt.major.map((v) => (
            <g key={`x${v}`}>
              <line x1={fx(v)} x2={fx(v)} y1={M.t} y2={M.t + ph} className="stroke-grid" />
              <text x={fx(v)} y={M.t + ph + 13} textAnchor="middle" className="fill-muted text-[10px]">
                {formatTick(v)}
              </text>
            </g>
          ))}
          {yt.map((v) => (
            <g key={`y${v}`}>
              <line x1={M.l} x2={M.l + pw} y1={fy(v)} y2={fy(v)} className="stroke-grid" />
              <text x={M.l - 5} y={fy(v) + 3} textAnchor="end" className="fill-muted text-[10px]">
                {formatTick(v)}
              </text>
            </g>
          ))}
          <text x={M.l + pw / 2} y={size.h - 3} textAnchor="middle" className="fill-muted text-[10px]">
            {xLabel}
            {xUnit ? ` (${xUnit})` : ""}
          </text>
          <text
            transform={`translate(10 ${M.t + ph / 2}) rotate(-90)`}
            textAnchor="middle"
            className="fill-muted text-[10px]"
          >
            {yLabel}
            {yUnit ? ` (${yUnit})` : ""}
          </text>
          <clipPath id={cid}>
            <rect x={M.l} y={M.t} width={pw} height={ph} />
          </clipPath>
          <g clipPath={`url(#${cid})`}>
            {paths.map(({ s, d }) => (
              <path key={s.name} d={d} fill="none" stroke={s.color} strokeWidth={1.6} strokeLinejoin="round" />
            ))}
          </g>
          {hx != null && hoverIdx != null && (
            <g>
              <line x1={fx(hx)} x2={fx(hx)} y1={M.t} y2={M.t + ph} className="stroke-muted" strokeDasharray="3 3" />
              {vis.map((s) => {
                const y = s.y[hoverIdx!];
                return y == null ? null : <circle key={s.name} cx={fx(hx)} cy={fy(y)} r={3.5} fill={s.color} />;
              })}
              {(() => {
                const rows = vis.map((s) => ({ s, y: s.y[hoverIdx!] }));
                const boxW = 150;
                const boxH = 16 + rows.length * 13;
                const bx = fx(hx) + 10 + boxW > M.l + pw ? fx(hx) - 10 - boxW : fx(hx) + 10;
                return (
                  <g transform={`translate(${bx} ${M.t + 6})`}>
                    <rect width={boxW} height={boxH} rx={4} className="fill-panel stroke-line" opacity={0.95} />
                    <text x={6} y={12} className="fill-fg font-mono text-[10px] font-semibold">
                      {xLabel} = {formatSI(hx, 5, xUnit)}
                    </text>
                    {rows.map(({ s, y }, i) => (
                      <text key={s.name} x={6} y={25 + i * 13} className="font-mono text-[10px]" fill={s.color}>
                        {s.name}: {y == null ? "–" : formatSI(y, 4, yUnit)}
                      </text>
                    ))}
                  </g>
                );
              })()}
            </g>
          )}
        </svg>
      </div>
    </div>
  );
}
