import { clsx } from "clsx";
import type { Analysis, AnalysisKind, VFile } from "../circuit/model.ts";

interface Props {
  analysis: Analysis;
  setAnalysis: (a: Analysis) => void;
  files: VFile[];
  onRun: () => void;
  running: boolean;
  disabledReason: string | null;
}

function F({ label, value, onChange, wide }: { label: string; value: string; onChange: (v: string) => void; wide?: boolean }) {
  return (
    <label className={clsx("block", wide && "col-span-2")}>
      <span className="label">{label}</span>
      <input className="field" value={value} spellCheck={false} onChange={(e) => onChange(e.target.value)} />
    </label>
  );
}

const KINDS: [AnalysisKind, string][] = [
  ["op", ".op"],
  ["dc", ".dc"],
  ["ac", ".ac"],
  ["tran", ".tran"],
];

export function AnalysisPanel({ analysis: a, setAnalysis, files, onRun, running, disabledReason }: Props) {
  const upd = (patch: Partial<Analysis>) => setAnalysis({ ...a, ...patch });
  return (
    <div className="space-y-3 p-3">
      <div className="flex gap-1">
        {KINDS.map(([k, label]) => (
          <button key={k} className={clsx("btn flex-1 font-mono", a.kind === k && "btn-on")} onClick={() => upd({ kind: k })}>
            {label}
          </button>
        ))}
      </div>
      <div className="grid grid-cols-2 gap-2">
        {a.kind === "op" && <p className="col-span-2 text-xs text-muted">DC operating point: one value per node and source current.</p>}
        {a.kind === "dc" && (
          <>
            <F label="Source name" value={a.dc.src} onChange={(v) => upd({ dc: { ...a.dc, src: v } })} wide />
            <F label="Start" value={a.dc.start} onChange={(v) => upd({ dc: { ...a.dc, start: v } })} />
            <F label="Stop" value={a.dc.stop} onChange={(v) => upd({ dc: { ...a.dc, stop: v } })} />
            <F label="Step" value={a.dc.step} onChange={(v) => upd({ dc: { ...a.dc, step: v } })} />
          </>
        )}
        {a.kind === "ac" && (
          <>
            <label className="block">
              <span className="label">Sweep</span>
              <select className="field" value={a.ac.sweep} onChange={(e) => upd({ ac: { ...a.ac, sweep: e.target.value as Analysis["ac"]["sweep"] } })}>
                <option value="dec">dec (per decade)</option>
                <option value="oct">oct (per octave)</option>
                <option value="lin">lin (total points)</option>
              </select>
            </label>
            <F label="Points" value={a.ac.points} onChange={(v) => upd({ ac: { ...a.ac, points: v } })} />
            <F label="Start freq (Hz)" value={a.ac.fstart} onChange={(v) => upd({ ac: { ...a.ac, fstart: v } })} />
            <F label="Stop freq (Hz)" value={a.ac.fstop} onChange={(v) => upd({ ac: { ...a.ac, fstop: v } })} />
          </>
        )}
        {a.kind === "tran" && (
          <>
            <F label="Step (tstep)" value={a.tran.tstep} onChange={(v) => upd({ tran: { ...a.tran, tstep: v } })} />
            <F label="Stop (tstop)" value={a.tran.tstop} onChange={(v) => upd({ tran: { ...a.tran, tstop: v } })} />
            <F label="Start (tstart)" value={a.tran.tstart} onChange={(v) => upd({ tran: { ...a.tran, tstart: v } })} />
            <F label="Max step (tmax)" value={a.tran.tmax} onChange={(v) => upd({ tran: { ...a.tran, tmax: v } })} />
            <label className="col-span-2 flex items-center gap-2 text-xs">
              <input type="checkbox" checked={a.tran.uic} onChange={(e) => upd({ tran: { ...a.tran, uic: e.target.checked } })} />
              uic – use initial conditions (skip the DC operating point)
            </label>
          </>
        )}
        {a.kind !== "op" && <></>}
        <F label="Initial conditions (.ic), e.g. v(out)=0" value={a.ic} onChange={(v) => upd({ ic: v })} wide />
      </div>
      <div>
        <span className="label">Include virtual files (.include)</span>
        {files.length === 0 && <p className="text-xs text-muted">No files. Add some in the Files tab.</p>}
        {files.map((f) => (
          <label key={f.name} className="flex items-center gap-2 py-0.5 font-mono text-xs">
            <input
              type="checkbox"
              checked={a.includes.includes(f.name)}
              onChange={(e) => upd({ includes: e.target.checked ? [...a.includes, f.name] : a.includes.filter((n) => n !== f.name) })}
            />
            {f.name}
          </label>
        ))}
      </div>
      <button className="btn btn-primary w-full py-1.5" onClick={onRun} disabled={running || !!disabledReason} title={disabledReason ?? "Run (Ctrl/Cmd+Enter)"}>
        {running ? "Running…" : "Run  (Ctrl+Enter)"}
      </button>
      {disabledReason && <p className="text-[11px] text-danger">{disabledReason}</p>}
    </div>
  );
}
