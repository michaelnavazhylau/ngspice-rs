# WebAssembly feasibility spike (2026-10-07, synced to main 3379cb2 on 2026-10-08)

Question: can the Rust port run a circuit simulator inside a web page?

Short answer: **yes, the port runs in WASM at about native speed, and its output
is byte-identical to native for every supported deck except AC sweeps, which
differ only in floating-point rounding.** The blockers for a real web product
are the port's current scope (no subcircuits or controlled sources yet) and two
O(n²) hot spots that also slow native runs. WASM itself is not a blocker.

## What the spike adds

* `spice_netlist::SourceProvider` (`crates/spice-netlist/src/sources.rs`): the
  include resolver reads deck and `.include`/`.lib` text through this trait.
  `FileSystem` is `std::fs` (what `Parser::parse_file*` uses, unchanged
  behaviour). `MemorySources` is an in-memory file map with lexical path
  normalization under `/`, and is what the browser uses.
  `Parser::parse_file_with_sources(path, &dyn SourceProvider, limits)` keeps
  every resolution rule: source-relative paths, canonical file/section cycle
  detection, and depth/file/byte/card budgets.
* Elaboration (`Circuit::from_netlist_with_context`) now accepts **resolved**
  includes, whose content is already inlined in order. An unresolved directive
  from the syntax-only `parse_deck` is still an explicit error, so an include is
  never simulated as an empty file. Subcircuits are still rejected.
  `crates/spice-analysis/tests/memory_sources.rs` checks that an included deck
  (nested relative includes, `.param`, a `.lib` section) gives a plot equal to
  the inline deck, and covers missing files, cycles and the byte budget.
* `crates/spice-wasm`: `run(deck, files)` returns the plots; `to_json` and
  `simulate_rawfile` render them. On wasm32 it exports
  `simulate(deck, file_names, file_contents) -> JSON` and `version()` through
  `wasm-bindgen`. It does not inherit the workspace lints, because
  `#[wasm_bindgen]` expands to `unsafe` glue and the workspace sets
  `unsafe_code = "forbid"`. The crate's own code contains no `unsafe`.
* `examples/bench.rs` (native) and `web/bench.mjs` (Node/V8) print the median
  time and an FNV-1a hash of each rawfile.
* `web/`: React + Tailwind + Bun schematic editor that runs the module in a Web
  Worker (see `web/README.md`).
* Workspace: `faer` is now `default-features = false, features = ["std",
  "sparse-linalg"]`, the same feature set diffsol requests. Native diffsol still
  turns on `rayon`, so the native feature set is unchanged: 571 tests pass, and
  clippy and fmt are clean. On wasm32 the old defaults pulled in faer's
  `spindle` thread pool, whose `atomic-wait` dependency does not build there.
* `.cargo/config.toml`: `--cfg getrandom_backend="wasm_js"` for
  `wasm32-unknown-unknown`. faer -> rand -> getrandom 0.3 needs both this cfg
  and the `wasm_js` feature, even though no randomness is used at runtime.

## Build and size (`wasm32-unknown-unknown`, wasm-bindgen 0.2.129, binaryen wasm-opt)

| build | raw | gzip -9 | brotli 11 |
|---|---|---|---|
| plain `--release` (after bindgen) | 2.02 MB | 576 KB | - |
| LTO fat, cgu=1, panic=abort, opt 3, `wasm-opt -O3` | 1.37 MB | 509 KB | 385 KB |
| same with opt `s`, `wasm-opt -Oz` | 1.08 MB | 436 KB | 340 KB |

Instantiation takes about 2 ms in Node 26. The module contains `v128` code, so
`wasm-opt` needs `--enable-simd`. Every current browser supports WASM SIMD.
Building with `-C target-feature=+simd128` made no measurable difference.

## Speed and output parity (Apple Silicon, median of 5; WASM run in Node 26/V8)

The hash column compares rawfile bytes between the two builds.

| deck | unknowns | native ms | wasm ms | ratio | output |
|---|---|---|---|---|---|
| conformance tran decks (12) | 3-6 | 0.2-8.6 | 0.3-7.4 | 0.9-1.3 | identical |
| rc_lowpass_ac / rlc_series_ac | 3-4 | 0.02-0.6 | 0.03-0.8 | 1.2-1.3 | identical |
| ladder100_tran | 201 | 270 | 265 | 1.0 | identical |
| ladder1000_tran | 2001 | 18 762 | (not run) | - | - |
| ladder200_ac (801 points) | 401 | 1 014 | 4 050 | **4.0** | max rel diff 2.9e-10 |
| mesh30_op | 901 | 82 | 90 | 1.1 | identical |
| mesh70_op | 4901 | 2 495 | 2 736 | 1.1 | identical |

The AC differences are rounding differences in faer's complex kernels, well
inside `compare::TRAN` (1e-3).

### Where the time goes (affects native runs too)

1. **Rank and conditioning diagnostic in every LU factorization**
   (`spice-maths/src/linear.rs` `SparseLu::factor`, `complex.rs`
   `ComplexMatrix::factorize`). Each factorization runs n basis solves and
   allocates for each one: O(n * nnz(LU)) per factorization, for every Newton
   iteration, every timestep and every AC point. A throwaway experiment with
   the loop disabled (reverted, never committed) gave the same output hashes:

   | deck | native ms (with check -> without) | wasm ms (with check -> without) |
   |---|---|---|
   | ladder200_ac | 1 014 -> 124 | 4 050 -> 141 |
   | ladder100_tran | 270 -> 71 | 265 -> 75 |
   | ladder1000_tran | 18 762 -> 720 | - -> 764 |
   | mesh70_op | 2 495 -> 1 608 | 2 736 -> 1 829 |

   Without the diagnostic, WASM is 1.05-1.15x native on every deck. The 4x AC
   penalty came almost entirely from the diagnostic's allocation-heavy complex
   solves. A cheaper certificate (a condition *estimate* such as Hager/Higham
   1-norm, or a pivot-growth test) would keep the guardrail at O(nnz). That
   decision belongs to the solver guardrails, so the spike does not change it.
   Main has since audited the guard and **retained it unchanged** (#73,
   [SPARSE_RANK_DIAGNOSTICS.md](SPARSE_RANK_DIAGNOSTICS.md)): the n-solve cost
   stands, and the measurements above still apply. Re-run after syncing with
   main 3379cb2, the native benchmark outputs are byte-identical to the earlier
   runs and the timings are unchanged. Equilibration (#46,
   [EQUILIBRATION.md](EQUILIBRATION.md)) is opt-in and off by default, so it does
   not affect the WASM path.

2. **`Circuit::add_instance` clones the node table for every device**
   (`spice-devices/src/circuit.rs`, `let mut nodes = self.nodes.clone()`) so it
   can roll back on error. That is O(devices * nodes): the mesh70 profile is
   dominated by `BTreeMap<String, NodeId>` clone and drop. Rolling back only
   the nodes added by the failing instance would make it linear.

## Gaps for a web circuit-sim product

* **Scope, not WASM:** the WASM build runs everything the native port does:
  linear and M4 nonlinear devices (diode, Ebers-Moll BJT, MOS1), and since M5
  `.subckt`/`X`. `spice_wasm::run` follows the `spice-rs simulate` pipeline:
  exactly one analysis, `.save`/`.print` selection, and `.measure`/`.four` over
  the full plot, all returned in the JSON. E/F/G/H controlled sources are "not
  yet ported", and `.include` inside a `.subckt` body is `NotYetPorted`.
* **File system:** solved by `MemorySources`. The page supplies the file map,
  filling it from OPFS, IndexedDB, `fetch` or the user.
* **API shape:** the ASCII rawfile is 25 MB for ladder1000_tran. Return
  `Float64Array` columns, or stream points, instead.
* **Responsiveness:** `simulate` is synchronous with no progress or
  cancellation hook. Run it in a Web Worker, cancel with `worker.terminate()`,
  and add a step callback for progress and live plotting later. A panic traps
  the instance, which must then be re-instantiated.
* **Threads:** single-threaded only. The circuits a browser will see are small
  enough that this does not matter, and wasm threads need COOP/COEP headers.
* **Not measured:** Firefox/SpiderMonkey and Safari/JSC. Nonlinear workloads
  have not been benchmarked (the M4 fixtures each take 1-13 ms in WASM).

## Reproduce

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
CARGO_PROFILE_RELEASE_PANIC=abort CARGO_PROFILE_RELEASE_STRIP=true \
  cargo build --release --target wasm32-unknown-unknown -p spice-wasm
wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/spice_wasm.wasm
wasm-opt -O3 --enable-simd --enable-bulk-memory --enable-nontrapping-float-to-int \
  --enable-sign-ext --enable-mutable-globals --enable-reference-types \
  --enable-multivalue pkg/spice_wasm_bg.wasm -o pkg/opt.wasm
cargo run --release -p spice-wasm --example bench -- 5 conformance/netlists/*.cir
node crates/spice-wasm/web/bench.mjs pkg pkg/opt.wasm 5 conformance/netlists/*.cir
```
