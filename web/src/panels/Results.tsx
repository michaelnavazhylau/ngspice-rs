import { clsx } from "clsx";
import { useEffect, useMemo, useState } from "react";
import type { Warning } from "../circuit/netlist.ts";
import { formatSI } from "../circuit/si.ts";
import { LineChart } from "../plot/LineChart.tsx";
import { buildSeries, isScaleless, traceColors } from "../plot/results.ts";
import type { RunOutcome } from "../sim/engine.ts";
import type { Plot, Variable } from "../sim/types.ts";

export type RunState =
  | { status: "idle" }
  | { status: "running"; startedAt: number }
  | { status: "done"; outcome: RunOutcome }
  | { status: "error"; message: string };

export function unitSymbol(v: Variable): string {
  const u = v.unit.toLowerCase();
  if (u.startsWith("volt")) return "V";
  if (u.startsWith("curr")) return "A";
  if (u === "time") return "s";
  if (u.startsWith("freq")) return "Hz";
  if (v.name.startsWith("v(")) return "V";
  if (v.name.startsWith("i(")) return "A";
  return v.unit === "none" ? "" : v.unit;
}

interface Props {
  state: RunState;
  warnings: Warning[];
  shown: ReadonlySet<string>;
  toggle: (name: string) => void;
}

export function Results({ state, warnings, shown, toggle }: Props) {
  const [plotIdx, setPlotIdx] = useState(0);
  const [showProblems, setShowProblems] = useState(true);
  const plots = state.status === "done" ? state.outcome.result.plots : [];
  const plot = plots[Math.min(plotIdx, plots.length - 1)] ?? null;
  useEffect(() => setPlotIdx(0), [state]);

  return (
    <div className="flex h-full min-h-0 flex-col bg-panel">
      <div className="flex items-center gap-2 border-b border-line px-2 py-1 text-xs">
        <span className="font-semibold">Results</span>
        {plots.length > 1 &&
          plots.map((p, i) => (
            <button key={p.name} className={clsx("btn", i === plotIdx && "btn-on")} onClick={() => setPlotIdx(i)}>
              {p.name}
            </button>
          ))}
        {plot && <span className="text-muted">{plot.type}</span>}
        {warnings.length > 0 && (
          <button
            className={clsx("btn", warnings.some((w) => w.severity === "error") ? "text-danger" : "text-warn")}
            onClick={() => setShowProblems(!showProblems)}
          >
            {warnings.length} problem{warnings.length > 1 ? "s" : ""} {showProblems ? "▾" : "▸"}
          </button>
        )}
        {state.status === "done" && (
          <span className="ml-auto text-muted">
            {plot ? `${plot.variables[0]?.re.length ?? 0} point${plot.variables[0]?.re.length === 1 ? "" : "s"} · ` : ""}engine {state.outcome.engineMs.toFixed(1)} ms · wall {state.outcome.wallMs.toFixed(0)} ms
          </span>
        )}
      </div>
      {showProblems && warnings.length > 0 && (
        <ul className="max-h-20 shrink-0 space-y-0.5 overflow-y-auto border-b border-line bg-canvas px-3 py-1 text-xs">
          {warnings.map((w, i) => (
            <li key={i} className={w.severity === "error" ? "text-danger" : "text-warn"}>
              {w.severity === "error" ? "✖" : "⚠"} {w.message}
            </li>
          ))}
        </ul>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        {state.status === "idle" && <p className="p-4 text-xs text-muted">Press Run (Ctrl/Cmd+Enter) to simulate. Results appear here.</p>}
        {state.status === "running" && <Elapsed since={state.startedAt} />}
        {state.status === "error" && (
          <pre className="m-3 rounded border border-danger/50 bg-danger/10 p-3 font-mono text-xs whitespace-pre-wrap text-danger">{state.message}</pre>
        )}
        {plot && <PlotView plot={plot} shown={shown} toggle={toggle} />}
      </div>
    </div>
  );
}

function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(performance.now());
  useEffect(() => {
    const t = setInterval(() => setNow(performance.now()), 100);
    return () => clearInterval(t);
  }, []);
  return <p className="p-4 text-xs text-muted">Simulating… {((now - since) / 1000).toFixed(1)} s (use Cancel to stop)</p>;
}

function PlotView({ plot, shown, toggle }: { plot: Plot; shown: ReadonlySet<string>; toggle: (n: string) => void }) {
  const colors = useMemo(() => traceColors(plot.variables), [plot]);
  if (isScaleless(plot) || plot.variables.length === 0) return <OpTable plot={plot} />;
  const scale = plot.variables[0]!;
  const rest = plot.variables.slice(1);
  const xUnit = unitSymbol(scale);
  const yUnit = rest.every((v) => unitSymbol(v) === unitSymbol(rest[0]!)) ? unitSymbol(rest[0]!) : "";
  if (plot.complex) {
    const mag = buildSeries(plot, "mag", shown, colors);
    const ph = buildSeries(plot, "phase", shown, colors);
    return (
      <div className="grid h-full min-h-[240px] grid-rows-2 gap-1 p-1">
        <LineChart series={mag} xScale="log" xLabel="frequency" xUnit="Hz" yLabel="magnitude" yUnit="dB" onToggle={toggle} />
        <LineChart series={ph} xScale="log" xLabel="frequency" xUnit="Hz" yLabel="phase" yUnit="°" legend={false} />
      </div>
    );
  }
  const series = buildSeries(plot, "re", shown, colors);
  return (
    <div className="flex h-full min-h-[160px] flex-col p-1">
      <LineChart className="flex-1" series={series} xLabel={scale.name} xUnit={xUnit} yLabel={yUnit === "V" ? "voltage" : yUnit === "A" ? "current" : "value"} yUnit={yUnit} onToggle={toggle} />
    </div>
  );
}

function OpTable({ plot }: { plot: Plot }) {
  return (
    <table className="w-full max-w-md text-xs">
      <thead>
        <tr className="border-b border-line text-left text-muted">
          <th className="px-3 py-1 font-normal">Variable</th>
          <th className="px-3 py-1 font-normal">Value</th>
        </tr>
      </thead>
      <tbody>
        {plot.variables.map((v) => (
          <tr key={v.name} className="border-b border-line/50">
            <td className="px-3 py-0.5 font-mono">{v.name}</td>
            <td className="px-3 py-0.5 font-mono">{v.re[0] == null ? "–" : formatSI(v.re[0], 6, unitSymbol(v))}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
