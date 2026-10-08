# Spice Web

Browser schematic editor and simulator front end for the Rust ngspice port
(`crates/spice-wasm`). Stack: Bun 1.3, React 19, TypeScript (strict), Tailwind CSS v4.
All schematic symbols, wires and plots are hand-written SVG; the only runtime
dependencies are `react`, `react-dom` and `clsx`.

## Commands (run from `web/`)

| command | what it does |
|---|---|
| `bun install` | install dependencies |
| `bun run build:wasm` | `scripts/build-wasm.ts`: checks that the `wasm-bindgen` CLI matches the version in `Cargo.lock` (prints the `cargo install wasm-bindgen-cli --version … --locked` command otherwise) and that the wasm32 target is installed, then compiles `spice-wasm` and generates bindings into `src/wasm/pkg/` (gitignored; needed before typecheck, the engine tests, dev and build) |
| `bun run dev` | dev server on http://localhost:3000 (`PORT=...` to change) |
| `bun run build` | production bundle into `dist/` (page, CSS, `worker.js`, `.wasm`) |
| `bun run preview` | serve `dist/` (`PORT=...`) |
| `bun run typecheck` | `tsc --noEmit` |
| `bun test` | unit tests (netlist, SI formatting, ticks) plus the wasm engine tests when `src/wasm/pkg` exists |

## How the bundling works

`bunfig.toml` registers `bun-plugin-tailwind` for Bun's HTML-import dev server and
`build.ts` passes it to `Bun.build`. Bun's bundler does not bundle `new Worker(new URL(...))`
targets from an HTML entry, so the simulator worker is built separately by
`build-worker.ts` (`Bun.build` of `src/sim/worker.ts`; the `.wasm` becomes a file asset
referenced relatively). The dev server (`server.ts`) serves `/worker.js` and the wasm from
memory next to the HTML-import route; `build.ts` writes both into `dist/`. The page
loads the worker with `new Worker(new URL("worker.js", document.baseURI), {type:"module"})`,
so `dist/` can be hosted from any sub-path.

Note: Bun's dev-server client (1.3.x) calls `new URL(link.href)` on every `<head>`
stylesheet link at startup, and extensions such as Dark Reader insert `<link>` tags
without an `href`. That threw `Invalid URL` and left `bun run dev` blank. A small inline
script at the top of `index.html` makes href-less links report the page URL until
`DOMContentLoaded`; the client ignores that URL. Remove the script once Bun skips such
links itself.

## Layout

```
src/circuit/   model.ts (types), geometry.ts (pins, rotation, junctions), netlist.ts
               (union-find connectivity, naming, warnings, deck), symbols.tsx (SVG symbols),
               si.ts (format/parse SI), examples.ts
src/editor/    Canvas.tsx (pan/zoom/place/move/wire/probe/box-select), store.ts (undo/redo
               history with edit coalescing), ops.ts (pure document edits)
src/plot/      LineChart.tsx (SVG chart), ticks.ts (linear/log ticks), results.ts (dB/phase, colours)
src/sim/       worker.ts (wasm in a Web Worker), engine.ts (typed client, cancel = terminate +
               respawn, "/deck.cir:N" -> "line N" error prettifier), types.ts (result contract)
src/panels/    Palette, Inspector, AnalysisPanel, NetlistPanel, FilesPanel, Results
src/storage.ts localStorage (project, virtual files, UI prefs), all wrapped in try/catch
tests/         bun tests
```

## Behaviour notes

* Connectivity: pins and wire endpoints at the same grid point connect; an endpoint or
  pin landing on another wire's body connects (T junction); crossings do not. Ground
  symbols (and labels named `0`/`gnd`) are node `0`; equal labels are one net; other nets are
  `n1`, `n2`, ... in component order. Problems (floating terminal, shorted part, no ground,
  duplicate/invalid names, empty values) are listed in the Results header; errors block Run
  in schematic mode.
* The generated deck puts `.include` lines directly after the title (so `.param` values
  are defined before use), then devices, `.ic`, the analysis card and `.end`.
* "Edit as text" copies the generated deck into an editable box; the text is run instead of
  the schematic until "Back to schematic".
* Probe tool (P): click a wire or pin to toggle its `v(net)` trace. `.op` results are shown
  in a table and as labels on the schematic nets.
* Shortcuts: S select, W wire, P probe, R rotate, Del delete, Ctrl/Cmd+Z / Shift+Z undo/redo,
  Ctrl/Cmd+A select all, Ctrl/Cmd+Enter run, space+drag or middle-drag pan, wheel zoom.

## Known limitations

* Engine: R, C, L, independent V/I (dc, ac, pulse, pwl), model-backed diodes,
  Ebers-Moll BJTs and level-1 MOSFETs (`.op`/`.dc`/`.ac`/`.tran`), and since M5
  `.subckt`/`X` (in "Edit as text" or from virtual include files). Controlled
  sources (E/F/G/H) are not ported.
* The engine follows `spice-rs simulate`: exactly one analysis card per run;
  `.save`/`.print` select the plotted vectors (`.print` vectors are saved too, as
  in C); `.measure` and `.four` run over the full plot and appear as extra
  Results tabs (Measurements, Fourier with THD, and the `.print` table).
* Semiconductors (palette: Diode, NPN/PNP BJT, NMOS/PMOS MOSFET). Each part carries a
  `.model` card (name, type, parameters). Parts with the same model name share one
  card: editing parameters in the Inspector updates all of them, and two different
  definitions under one name are an error. Instance parameters go in the value field
  (area factor for D/Q, `w=.. l=..` for M). Pins are top / base or gate / bottom:
  NPN collector and NMOS drain on top; PNP emitter and PMOS source on top, drawn
  mirrored. MOSFETs are 3-terminal with the bulk tied to the source. Pin tooltips
  name each terminal.
* Moving a component drags attached wire ends; wires that become diagonal are re-routed as
  an L, which can leave small detours. Wires are not split automatically when a part is
  dropped onto the middle of one (it simply connects to the wire).
* No copy/paste, no wire dragging, no plot zoom; the AC phase is unwrapped along the sweep.
* The Cancel button terminates and recreates the worker; it was not exercised against a
  genuinely long simulation.
* `.tran` output may end slightly before `tstop` (e.g. 9.999999999999999e-5).
