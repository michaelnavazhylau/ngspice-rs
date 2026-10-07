import { clsx } from "clsx";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { EXAMPLES, PARAMS_FILE } from "./circuit/examples.ts";
import type { Analysis, Doc, VFile } from "./circuit/model.ts";
import { defaultAnalysis, emptyDoc } from "./circuit/model.ts";
import { generateNetlist } from "./circuit/netlist.ts";
import { formatSI } from "./circuit/si.ts";
import { Canvas, type Tool } from "./editor/Canvas.tsx";
import { deleteIds, rotateIds } from "./editor/ops.ts";
import { useEditorHistory } from "./editor/store.ts";
import { AnalysisPanel } from "./panels/AnalysisPanel.tsx";
import { FilesPanel } from "./panels/FilesPanel.tsx";
import { Inspector } from "./panels/Inspector.tsx";
import { NetlistPanel } from "./panels/NetlistPanel.tsx";
import { Palette } from "./panels/Palette.tsx";
import { Results, type RunState } from "./panels/Results.tsx";
import { colorFor, defaultShown, isScaleless, traceColors } from "./plot/results.ts";
import { prettyError, SimCancelled, SimEngine } from "./sim/engine.ts";
import { loadFiles, loadProject, loadUi, parseProject, saveFiles, saveProject, saveUi } from "./storage.ts";

type Tab = "inspect" | "analysis" | "netlist" | "files";

function download(name: string, text: string, type = "text/plain") {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function App() {
  const initial = useMemo(() => loadProject(), []);
  const hist = useEditorHistory(initial?.doc ?? EXAMPLES[0]!.doc);
  const { doc, commit } = hist;
  const [analysis, setAnalysis] = useState<Analysis>(initial?.analysis ?? EXAMPLES[0]!.analysis);
  const [files, setFilesRaw] = useState<VFile[]>(() => loadFiles());
  const [override, setOverride] = useState<string | null>(initial?.deckOverride ?? null);
  const [selection, setSelection] = useState<Set<string>>(new Set());
  const [tool, setTool] = useState<Tool>({ type: "select" });
  const [tab, setTab] = useState<Tab>("analysis");
  const [run, setRun] = useState<RunState>({ status: "idle" });
  const [shown, setShown] = useState<Set<string>>(new Set());
  const [fitKey, setFitKey] = useState(0);
  const [resultsH, setResultsH] = useState(() => loadUi().resultsHeight);
  const [version, setVersion] = useState<string | null>(null);
  const [engineErr, setEngineErr] = useState<string | null>(null);
  const engine = useRef<SimEngine | null>(null);
  if (!engine.current && typeof Worker !== "undefined") engine.current = new SimEngine();

  useEffect(() => {
    const e = engine.current;
    if (!e) return;
    const sync = () => {
      setVersion(e.version);
      setEngineErr(e.fatal);
    };
    sync();
    return e.subscribe(sync);
  }, []);

  const netlist = useMemo(() => generateNetlist(doc, analysis), [doc, analysis]);
  const deck = override ?? netlist.deck;

  // persistence
  useEffect(() => {
    const t = setTimeout(() => saveProject({ version: 1, doc, analysis, deckOverride: override }), 300);
    return () => clearTimeout(t);
  }, [doc, analysis, override]);
  useEffect(() => saveFiles(files), [files]);
  useEffect(() => saveUi({ resultsHeight: resultsH }), [resultsH]);

  const setFiles = (next: VFile[]) => {
    // keep .include references in sync when a file is renamed
    if (next.length === files.length) {
      const renamed = next.findIndex((f, i) => f.name !== files[i]!.name);
      if (renamed >= 0 && next.every((f, i) => i === renamed || f.name === files[i]!.name)) {
        const old = files[renamed]!.name;
        setAnalysis((a) => ({ ...a, includes: a.includes.map((n) => (n === old ? next[renamed]!.name : n)) }));
      }
    } else if (next.length < files.length) {
      setAnalysis((a) => ({ ...a, includes: a.includes.filter((n) => next.some((f) => f.name === n)) }));
    }
    setFilesRaw(next);
  };

  const doRun = useCallback(async () => {
    const e = engine.current;
    if (!e || run.status === "running") return;
    if (override === null && netlist.hasErrors) {
      setRun({ status: "error", message: netlist.warnings.filter((w) => w.severity === "error").map((w) => w.message).join("\n") });
      return;
    }
    setRun({ status: "running", startedAt: performance.now() });
    try {
      const outcome = await e.run(deck, files);
      setRun({ status: "done", outcome });
      const plot = outcome.result.plots[0];
      if (plot) {
        setShown((prev) => {
          const names = new Set(plot.variables.map((v) => v.name));
          const keep = [...prev].filter((n) => names.has(n));
          return keep.length ? new Set(keep) : defaultShown(plot);
        });
      }
    } catch (err) {
      if (err instanceof SimCancelled) setRun({ status: "idle" });
      else setRun({ status: "error", message: prettyError(err instanceof Error ? err.message : String(err)) });
    }
  }, [deck, files, netlist, override, run.status]);

  const cancel = () => engine.current?.cancel();

  // Ctrl/Cmd+Enter
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault();
        void doRun();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [doRun]);

  // select component -> inspector
  const select = useCallback((s: Set<string>) => {
    setSelection(s);
    if (s.size === 1) setTab((t) => (t === "netlist" || t === "files" ? t : "inspect"));
  }, []);

  const loadDoc = (d: Doc, a: Analysis, f?: VFile[]) => {
    hist.reset(d);
    setAnalysis(a);
    setOverride(null);
    setSelection(new Set());
    setRun({ status: "idle" });
    setShown(new Set());
    setFitKey((k) => k + 1);
    if (f?.length) {
      setFilesRaw((cur) => {
        const out = [...cur];
        for (const nf of f) if (!out.some((x) => x.name === nf.name)) out.push(nf);
        return out;
      });
    }
  };

  const plot0 = run.status === "done" ? run.outcome.result.plots[0] : undefined;
  const colors = useMemo(() => (plot0 ? traceColors(plot0.variables) : new Map<string, string>()), [plot0]);
  const probed = useMemo(() => {
    const m = new Map<string, string>();
    if (!plot0 || isScaleless(plot0)) return m;
    for (const n of shown) {
      const mm = /^v\((.+)\)$/.exec(n);
      if (mm) m.set(mm[1]!, colors.get(n) ?? colorFor(0));
    }
    return m;
  }, [plot0, shown, colors]);
  const annotations = useMemo(() => {
    if (run.status !== "done" || override !== null) return null;
    const op = run.outcome.result.plots.find(isScaleless);
    if (!op) return null;
    const m = new Map<string, string>();
    for (const v of op.variables) {
      const mm = /^v\((.+)\)$/.exec(v.name);
      if (mm && v.re[0] != null) m.set(mm[1]!, formatSI(v.re[0], 4, "V"));
    }
    return m;
  }, [run, override]);

  const toggleTrace = useCallback((name: string) => {
    setShown((s) => {
      const n = new Set(s);
      if (n.has(name)) n.delete(name);
      else n.add(name);
      return n;
    });
  }, []);

  const importRef = useRef<HTMLInputElement>(null);
  const onImport = async (file: File | undefined) => {
    if (!file) return;
    try {
      const p = parseProject(JSON.parse(await file.text()));
      if (!p) throw new Error("not a schematic file");
      loadDoc(p.doc, p.analysis);
      setOverride(p.deckOverride ?? null);
    } catch (e) {
      alert(`Import failed: ${e instanceof Error ? e.message : String(e)}`);
    }
    if (importRef.current) importRef.current.value = "";
  };

  const running = run.status === "running";
  const errorsBlock = override === null && netlist.hasErrors;
  const disabledReason = engineErr ? `Engine failed to load: ${engineErr}` : null;
  const startResize = (e: React.PointerEvent) => {
    const y0 = e.clientY;
    const h0 = resultsH;
    const move = (ev: PointerEvent) => setResultsH(Math.min(window.innerHeight - 160, Math.max(100, h0 - (ev.clientY - y0))));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div className="flex h-full flex-col">
      {/* toolbar */}
      <header className="flex h-11 shrink-0 items-center gap-1.5 border-b border-line bg-panel px-2">
        <span className="mr-1 font-semibold tracking-tight">
          <span className="text-accent">∿</span> Spice Web
        </span>
        <select
          className="btn max-w-44 !py-1"
          value=""
          onChange={(e) => {
            const ex = EXAMPLES.find((x) => x.id === e.target.value);
            if (ex) loadDoc(structuredClone(ex.doc), structuredClone(ex.analysis), ex.files);
          }}
          title="Load an example"
        >
          <option value="">Examples…</option>
          {EXAMPLES.map((x) => (
            <option key={x.id} value={x.id} title={x.description}>
              {x.title}
            </option>
          ))}
        </select>
        <button className="btn" onClick={() => loadDoc(emptyDoc(), defaultAnalysis(), [PARAMS_FILE])} title="New empty schematic">
          New
        </button>
        <span className="mx-1 h-5 w-px bg-line" />
        <button className="btn" onClick={hist.undo} disabled={!hist.canUndo} title="Undo (Ctrl+Z)">
          ↶ Undo
        </button>
        <button className="btn" onClick={hist.redo} disabled={!hist.canRedo} title="Redo (Ctrl+Shift+Z)">
          ↷ Redo
        </button>
        <button className="btn" disabled={!selection.size} onClick={() => commit(rotateIds(doc, selection))} title="Rotate (R)">
          ⟳
        </button>
        <button
          className="btn"
          disabled={!selection.size}
          onClick={() => {
            commit(deleteIds(doc, selection));
            setSelection(new Set());
          }}
          title="Delete (Del)"
        >
          Delete
        </button>
        <span className="mx-1 h-5 w-px bg-line" />
        <button className="btn" onClick={() => download("schematic.json", JSON.stringify({ version: 1, doc, analysis }, null, 2), "application/json")}>
          Export
        </button>
        <button className="btn" onClick={() => importRef.current?.click()}>
          Import
        </button>
        <input ref={importRef} type="file" accept="application/json,.json" hidden onChange={(e) => void onImport(e.target.files?.[0])} />
        <button className="btn" onClick={() => download("circuit.cir", deck)}>
          Download .cir
        </button>
        <div className="ml-auto flex items-center gap-2">
          <span className="hidden text-[11px] text-muted md:inline" title="Engine version">
            {engineErr ? <span className="text-danger">engine error</span> : version ? `engine: ${version}` : "engine loading…"}
          </span>
          {running && (
            <button className="btn" onClick={cancel}>
              Cancel
            </button>
          )}
          <button
            className={clsx("btn btn-primary !px-4 !py-1.5", errorsBlock && "opacity-60")}
            onClick={() => void doRun()}
            disabled={running || !!disabledReason}
            title={disabledReason ?? (errorsBlock ? "Fix the listed problems first" : "Run (Ctrl/Cmd+Enter)")}
          >
            ▶ Run
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <Palette tool={tool} setTool={setTool} />
        <main className="flex min-w-0 flex-1 flex-col">
          <div className="relative min-h-0 flex-1">
            <Canvas
              doc={doc}
              commit={commit}
              undo={hist.undo}
              redo={hist.redo}
              selection={selection}
              setSelection={select}
              tool={tool}
              setTool={setTool}
              netlist={netlist}
              annotations={annotations}
              probed={probed}
              onProbe={(net) => toggleTrace(`v(${net})`)}
              fitKey={fitKey}
            />
            {override !== null && (
              <div className="pointer-events-none absolute top-2 left-2 rounded bg-warn/20 px-2 py-0.5 text-[11px] text-warn">
                Text mode: running the edited deck, not this schematic
              </div>
            )}
          </div>
          <div onPointerDown={startResize} className="h-1.5 shrink-0 cursor-row-resize border-y border-line bg-panel hover:bg-accent/40" title="Drag to resize" />
          <section style={{ height: resultsH }} className="shrink-0">
            <Results state={run} warnings={netlist.warnings} shown={shown} toggle={toggleTrace} />
          </section>
        </main>
        <aside className="flex w-72 shrink-0 flex-col border-l border-line bg-panel xl:w-80">
          <nav className="flex border-b border-line text-xs">
            {(
              [
                ["inspect", "Inspector"],
                ["analysis", "Analysis"],
                ["netlist", "Netlist"],
                ["files", "Files"],
              ] as const
            ).map(([k, label]) => (
              <button
                key={k}
                onClick={() => setTab(k)}
                className={clsx("flex-1 px-2 py-2", tab === k ? "border-b-2 border-accent font-semibold text-accent" : "text-muted hover:bg-hover")}
              >
                {label}
              </button>
            ))}
          </nav>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {tab === "inspect" && <Inspector doc={doc} selection={selection} commit={commit} />}
            {tab === "analysis" && (
              <AnalysisPanel analysis={analysis} setAnalysis={setAnalysis} files={files} onRun={() => void doRun()} running={running} disabledReason={disabledReason} />
            )}
            {tab === "netlist" && <NetlistPanel generated={netlist.deck} override={override} setOverride={setOverride} />}
            {tab === "files" && <FilesPanel files={files} setFiles={setFiles} />}
          </div>
        </aside>
      </div>
    </div>
  );
}
