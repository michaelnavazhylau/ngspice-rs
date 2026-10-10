# C source tree → Rust module mapping

Paths in the tables below are paths **in the upstream ngspice tree**, relative to
its source root. The Rust port does not vendor the C sources; see
[NOTICE](../../NOTICE). “Ported” below means the stated bounded behavior, not a
line-by-line C translation or full SPICE parity. Current production APIs and
limits: [DIFFSOL_FAER_IMPLEMENTATION.md](DIFFSOL_FAER_IMPLEMENTATION.md).
Remaining work is tracked only in [TODO.md](../../TODO.md).

Line counts are real counts (`.c` + `.h`) from the C tree this worktree was
branched from (`pre-master-48`). They are ordering hints, not estimates of Rust
effort.

Total C tree: **723,230 lines** under `src/`, of which
`src/spicelib/devices/` alone is **464,108 lines (64%)**. Scope control is
therefore the central risk of this port — see
[`ROADMAP.md`](ROADMAP.md).

## Front end

| C | Lines | Rust module | Status |
| --- | --- | --- | --- |
| `src/spicelib/parser/inpeval.c` | 1,139 | `primitives::value` | numeric literals and scale factors ported |
| `src/frontend/inpcom.c` | 10,237 | `primitives::node` (`inp_fix_gnd_name`), `netlist::source` (`inp_stripcomments_line`), `netlist::card`, parser terminal canonicalization | ground aliasing, comment stripping, classification and bounded source-relative include/lib resolution ported; numparam preprocessing still missing |
| `src/frontend/inp.c` | 2,967 | `netlist::source` | title line, continuation folding ported |
| `src/frontend/subckt.c`, `inpcom.c` subcircuit preprocessing | — | `netlist::parser::{structure,scopes,resolution}`, `devices::subckt`, `netlist::eval::ParamScope::resolve_instance` | ordered nested definitions/X syntax, textual formals and source provenance; `X` expansion with hierarchical naming, per-instance parameters/models and `.global` ported (#18); see [SUBCIRCUITS.md](SUBCIRCUITS.md) |
| `src/frontend/parse-bison.y` | 180 | future front-end expression parser | **not ported**; this is an expression grammar, not the netlist deck grammar |
| `src/spicelib/parser/inp2*.c` (device and dot-card grammars) | 3,828 | `netlist::parser` | scalar/declared-model R/C/L, DC/AC/PULSE/PWL V/I and bounded D/Q/M flags/IC syntax; analysis arguments retained without validation; remaining grammars unported |
| `src/spicelib/parser/{inpmkmod,inpdomod,inpgmod,inpfindl,inpgval}.c` | — | `netlist::parser::model`, `devices::{models,schema}` | raw scalar cards retained; top-level first-wins resolution, family/level checks and bounded diode input schemas; `INPgetModBin`/`model_name_match` MOS binning in `devices::binning` (#109, see [MODEL_SCHEMAS.md](MODEL_SCHEMAS.md#model-binning-109)); advanced backends pending |
| `src/spicelib/parser/inppas*.c` (input passes: models, devices, IC/nodeset, shunts) | 667 | `netlist::parser`, later circuit elaboration | card dispatch and model-name indexing partially ported; model elaboration/IC/shunt passes unported; these are **not** `.param` evaluators |
| `src/frontend/numparam/{spicenum,xpressn}.c`, preprocessing in `inpcom.c` | — | `netlist::expr`, `parser/{expression,param}.rs` | bounded `.param`/expression **syntax** ported (`formula()` precedence, `fetchnumber()`, `fmathS` subset, multi-assignment split); top-level evaluation (`netlist::{eval,elaborate}`: `inp_sort_params` ordering/last-definition rule, `operate`/`mathfunction` semantics with finite-or-error) ported; per-instance subcircuit scoping ported (#18), the rest of numparam **not ported** |
| `src/spicelib/parser/ifeval.c` | 190 | future behavioural-device evaluator | **not ported**; evaluates IF parse trees, not numparam `.param` expressions |
| `src/spicelib/parser/inpsymt.c` | 305 | `netlist::symbols` (planned) | **not ported** |
| `src/frontend/circuits.c`, `define.c` | 483 | `devices::registry`, `primitives::node` | registry with working scalar R/C/L/V/I factories; other designators explicitly unavailable |
| `src/spicelib/parser/inp2dot.c` (dot-card grammar), `src/frontend/postcoms.c` (`com_print`) | 2,938 | `netlist::parser::save`, `analysis::selection` | bounded `.save`/`.print` output selection ported (#42): typed positioned requests, projection of the full plot into the written rawfile in C `dbs` order with first-wins dedup and a `.print` text table; `.plot` unported; the full C reference chain (`dotcards.c` `ft_dotsaves`, `breakp2.c` `dbs`, `outitf.c` `beginPlot`) is in [OUTPUT_SELECTION.md](OUTPUT_SELECTION.md) |
| `src/frontend/measure.c`, `src/frontend/com_measure2.c` | 3,246 | `netlist::parser::measure`, `analysis::measure` | bounded `.measure`/`.meas` measurements ported (#43): `FIND <operand> AT=`, `MIN`/`MAX`/`AVG`/`RMS`/`INTEG` (`/INTEGRAL`) and `TRIG … TARG …`; `WHEN`, `MIN_AT`/`MAX_AT`, `PP`, `DERIV`, `ERR*`, `TD=` and body-local cards unported; see [MEASURE.md](MEASURE.md) |
| `src/frontend/fourier.c`, `dotcards.c` (`.four`) | 373 (fourier.c) | `netlist::parser::fourier`, `analysis::fourier` | bounded final-period transient Fourier/THD ported (#44): physical-grid resampling, DC/peak amplitude/window-referenced phase, 1–100 harmonics; interactive commands and other interpolation/window settings unported; see [FOURIER.md](FOURIER.md) |
| `src/frontend/` (whole directory) | 88,452 | — | includes the command interpreter and interactive plotting; mostly deferred; the bounded `.measure`/`.meas` subset is ported ([MEASURE.md](MEASURE.md)) |

## Maths

| C | Lines | Rust module | Status |
| --- | --- | --- | --- |
| `src/maths/dense/` | 1,742 | `maths::dense` | row-major storage and owned faer pivoted LU with checked solves |
| `src/maths/sparse/` (SPARSE 1.3, MIT) | 10,465 | `maths::sparse` | triplet storage, petgraph row-coupling projection and owned faer sparse LU; finite/rank/residual checks and exact-pattern symbolic reuse |
| `src/maths/dense/`, `src/maths/sparse/` scaling (behavioral reference) | — | `maths::equilibration` | bounded opt-in dense/sparse/complex wrappers (#46); independent power-of-two policy, original-unit residuals, unchanged defaults ([EQUILIBRATION.md](EQUILIBRATION.md)) |
| `src/maths/sparse/` diagnostics (behavioral reference) | — | `maths::linear`, `complex` | #47 audits locked faer APIs and retains numerical guards; example-only batching/benchmarks, formal aggregate-proof caveat tracked in #68 ([SPARSE_RANK_DIAGNOSTICS.md](SPARSE_RANK_DIAGNOSTICS.md)) |
| `src/maths/KLU/` (LGPLv2) | 18,353 | behavioral reference only | **not translated or linked**; faer supplies real/complex LU, see licensing below |
| `src/maths/ni/` | 1,961 | `maths::integrator`, separate `maths::diffsol` | trap orders 1–2 and variable-step Gear orders 1–6 (#98) coefficients, `NIintegrate`/`NIpred`/`CKTterr` operations and accepted step history (six steps, seven state vectors); the `dctran.c` order policy never selects an order above 2; explicit adaptive BDF supports index-one DAEs including floating/coupled capacitor mass blocks (higher-index rejected), not ngspice trap/Gear parity |
| `src/maths/cmaths/` | 4,054 | `primitives::value::Complex` | arithmetic/magnitude/phase/conjugation ported; not the full C transcendental library |
| `src/maths/poly/`, `deriv/`, `fft/`, `misc/` | 6,358 | `maths` (planned modules) | **not ported** |

## Devices

`src/spicelib/devices/` totals **464,108 lines**. The port must not attempt to
translate all of it; the roadmap targets a small, useful subset first.

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/spicelib/devices/ckt*.c` (device framework) | 419 | `devices` | trait, scalar factories, `Circuit` incidence topology, node-before-branch binding and immutable linear equation assembly, atomic AST instance insertion and top-level typed model resolution and bounded passive factories; nonlinear factories pending |
| `res/`, `cap/`, `ind/` | 5,326 | `devices::{rlc,passive}` | scalar equations plus bounded model values, R sheet/C area-perimeter geometry, contextual TC1/TC2, scale/multiplicity; trap/Gear orders 1–2 C/L companion stamps from accepted charge/flux state (`capload.c`/`indload.c`); K mutual inductance (`mut*.c`, coupled flux in `indload.c`) ported with `devices::mutual` and `parser/mutual.rs` ([MUTUAL_INDUCTANCE.md](MUTUAL_INDUCTANCE.md)); coil geometry, advanced setters and companion `ic=`/`uic` pending |
| `vsrc/`, `isrc/` | — | `devices::sources` | DC/AC V/I stamps; RFSPICE port sources (`portnum`/`z0`, `#res` node, series `1/z0` in every analysis, `vsrctemp.c` numbering; `pwr`/`freq` PORT accumulation on both transient backends, see [SPARAM.md](SPARAM.md)); Constant/Step/Pwl/Pulse forcing with left/right limits and lazy breakpoints; PULSE (with count)/PWL (with `td=`/`r=`)/SIN/EXP/SFFM/AM deck setters elaborate (`functions.rs`); no TRNOISE/TRRANDOM/EXTERNAL |
| `vcvs/`, `vccs/`, `cccs/`, `ccvs/`; `parser/inp2{e,f,g,h}.c`; `analysis/cktfbran.c` | — | `devices::controlled`, `netlist` `parser/controlled.rs` | linear E/F/G/H gain stamps for OP/DC/AC/transient; F/H controlling V/E/H branches resolved by `Circuit` (hierarchical in subcircuits); POLY/VALUE/TABLE lowered onto behavioural sources (#79), LAPLACE pending ([CONTROLLED_SOURCES.md](CONTROLLED_SOURCES.md)) |
| `asrc/`; `parser/inp2b.c`, `inpptree.c`, `ptfuncs.c`, `ifeval.c`; `frontend/inpcom.c` (`inp_compat`, `inp_meas_current`, `inp_bsource_compat`); `xspice/enh/enhtrans.c` | — | `devices::behavioural`, `netlist::{bexpr, behavioural}`, `parser/{bexpression,behavioural}.rs` | B sources with C's function set and derivative rules; E/G/F/H VALUE/TABLE/POLY lowering; `ddt`/`gauss`/LAPLACE pending ([BEHAVIOURAL_SOURCES.md](BEHAVIOURAL_SOURCES.md)) |
| `sw/`, `csw/`; `parser/inp2{s,w}.c` | — | `devices::switch`, `netlist` `parser/switch.rs` | S/W conductance switches with C's hysteresis/flag rules, Newton `MODEINITF` phases, accepted switch state and `swtrunc.c` step control for OP/DC/AC/companion transient; W controls resolved like F/H; noise ported (#100); pole-zero/`@` queries pending; AC uses the operating-point state ([SWITCHES.md](SWITCHES.md)) |
| `tra/`; `parser/inp2t.c` | — | `devices::{tline,delay}`, `netlist` `parser/tline.rs`, `analysis::companion` | lossless line: DC wire, exact AC `exp(-j omega TD)`, companion transient with accepted-point delay history (`traload.c`/`traacct.c`), device breakpoints and `tratrunc.c` step bound; `uic`, pole-zero, noise, distortion, sensitivity and the diffsol backend refused ([TRANSMISSION_LINES.md](TRANSMISSION_LINES.md)) |
| `urc/`; `parser/inp2u.c` | — | `devices::urc`, `netlist` `parser/urc.rs` | U lines expanded like `urcsetup.c` into generated R/C (or R/diode) sections with C's `#hi`/`#lo`/`#rlo`… names, plus the load-free instance for `@u[l]`/`@u[n]`; `.pz` refused (C aborts), `.sens` pending ([URC.md](URC.md)) |
| `dio/` | 5,598 | `devices::diode` (planned) | **not ported** |
| `bjt/` | 9,482 | `devices::bjt` (planned) | **not ported** |
| `mos1/`…`mos9/`, `bsim*`, `hisim*`, `hfet*`, `vbic`, `soi*` | 218,897 | `devices::mos` shell, `devices::mos1`, `devices::mos3` | **partial**: MOS1 and MOS3 (`mos3set.c`, `mos3temp.c`, `mos3load.c`, `mos3acld.c`, `mos3pzld.c`, `mos3trun.c`, `mos3noi.c`; not `mos3dset.c`/`mos3dist.c` or the sensitivity routines); other levels `NotYetPorted` naming their directory |
| `jfet/` (`jfetset.c`, `jfettemp.c`, `jfetload.c`, `jfetacld.c`, `jfetpzld.c`, `jfettrun.c`, `jfetask.c`, `jfetic.c`); `parser/inp2j.c` | — | `devices::jfet`, `netlist::parser::jfet` | level 1 DC/AC/pole-zero/transient, temperature, observations; `jfetnoi.c`/`jfetdist.c` refused ([JFET.md](JFET.md)) |
| `jfet2/` (`jfet2parm.h`, `jfet2set.c`, `jfet2temp.c`, `psmodel.c`, `jfet2load.c`, `jfet2acld.c`, `jfet2trun.c`, `jfet2ask.c`, `jfet2ic.c`) | — | `devices::jfet2` | level 2 (Parker-Skellern) DC/AC/transient, temperature, observations; no C pole-zero load (refused), `jfet2noi.c` refused ([JFET.md](JFET.md#level-2-parker-skellern)) |
| `src/xspice/` (event-driven code models) | 28,373 | `devices::xspice` (planned) | **not ported** |
| `src/osdi/` (Verilog-A / OSDI) | 3,372 | — | **not ported**; needs a Verilog-A compiler path, probably out of scope |
| `src/ciderlib/` (numerical device simulator) | 28,342 | — | **not ported**; probably out of scope |

## Analyses and results

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/spicelib/analysis/` (whole directory) | 21,993 | `analysis::driver` | trait/runner, linear `.op`, single-independent-source `.dc`, complex `.ac`, the adaptive trap/Gear-2 companion `.tran` driver (`companion.rs`: `dctran.c`, `ckttrunc.c`, `cktterr.c` policy, linear circuits) and explicitly selected restricted diffsol BDF; nonlinear/other analyses pending |
| ↳ `pzan.c`, `cktpzset.c`, `cktpzld.c`, `cktpzstr.c`; `maths/ni/nipzmeth.c`; device `*pzld.c` | 2,118 (without the device loads) | `analysis::pz`, `maths::pencil`, `Device::assemble_pole_zero` | `.pz` with C's card, `PZinit` checks, drive/column modification and plot layout; roots by orthogonal staircase deflation plus QZ instead of the Muller search ([POLE_ZERO_ADR.md](POLE_ZERO_ADR.md)) |
| ↳ `cktdojob.c`, `dctran.c`, `dcop.c`, `acan.c`, `cktload.c` | 2,060 | `analysis::driver` | bounded linear assembly/factor/solve/plot orchestration; no nonlinear Newton/stepping or SPICE trap/Gear driver |
| ↳ `noisean.c`, `cktnoise.c`, `nevalsrc.c`, `ninteg.c`; `maths/ni/niniter.c` | — | `analysis::noise`, `devices::noise`, `maths::complex` (`solve_transposed`) | `.noise` adjoint spectra, per-generator `Nintegrate` totals and C's two plots (#100, [NOISE.md](NOISE.md)) |
| `res/resnoise.c`, `dio/dionoise.c`, `bjt/bjtnoise.c`, `mos1/mos1noi.c`, `mos3/mos3noi.c`, `sw/swnoise.c`, `csw/cswnoise.c` | — | `Device::noise` in `devices::{rlc,passive,nonlinear,bjt,mos,mos1,mos3,switch}` | thermal/shot/flicker generators at the bias point; noiseless devices explicit, unported noise refused ([NOISE.md](NOISE.md)) |
| ↳ `distoan.c`, `cktdisto.c`, `dkerproc.c`, `dloadfns.c`, `dsetparm.c`; `parser/inp2dot.c` `dot_disto`; `maths/deriv/*.c` | — | `analysis::disto`, `devices::distortion` (`Series3` for `Dderivs`) | `.disto` harmonics and IM products on C's sweep with C's plot names and batch order (#104, [DISTORTION.md](DISTORTION.md)) |
| `dio/diodset.c` + `diodisto.c`, `bjt/bjtdset.c` + `bjtdisto.c`, `mos1/mos1dset.c` + `mos1dist.c`; `vsrc`/`isrc` `distof1`/`distof2` setters | — | `Device::distortion` in `devices::{nonlinear,bjt/disto,mos1/disto,sources}` | Taylor terms of C's simplified distortion models at the bias point, four C defects reproduced; linear devices explicit, B sources/code models refused ([DISTORTION.md](DISTORTION.md)) |
| ↳ `cktsens.c`, `cktsgen.c`, `senssetp.c`; `parser/inp2dot.c` `dot_sens` | — | `analysis::sens`, `devices::sensitivity`, `Device::sensitivity` | `.sens` finite-difference DC/AC sensitivities with C's parameter lists, names, filters, stepping and in-place perturbation side effects; Q/MOS1/MOS3/code models and AC nonlinear devices refused (#102, [SENSITIVITY.md](SENSITIVITY.md)) |
| `res/{res,resparam,resmpar,ressetup,restemp,resload}.c`, `cap/`, `ind/`, `vsrc/`, `isrc/`, `vcvs/`, `vccs/`, `cccs/`, `ccvs/`, `asrc/`, `sw/`, `csw/`, `dio/{dio,diompar,dioparam,diomask,dioask,diosetup,diotemp}.c` | — | `devices::sensitivity::{res,reactive,source,controlled,misc}`, `devices::nonlinear::sens` | C's records and setter/setup/temperature routines replayed for `.sens` ([SENSITIVITY.md](SENSITIVITY.md)) |
| ↳ `tfanal.c`, `tfsetp.c`; `parser/inp2dot.c` `dot_tf` | — | `analysis::tf` | `.tf` gain and input/output resistance from one reload of C's operating-point matrix (`TrialState::with_c_jacobian`), C's vector names and `1e20` open rule ([TRANSFER_FUNCTION.md](TRANSFER_FUNCTION.md)) |
| `span.c`, `spsetp.c`, `cktspdum.c`; `devices/vsrc/vsrcacld.c` (`VSRCspinit`/`VSRCspupdate`); `maths/dense/dense.c` (`CMat`) | — | `analysis::sparam`, `maths::dense_complex` | `.sp` S/Y/Z over RF ports with C's vector names and batch order; `donoise` covariances and two-port noise parameters (`noisesp.c`, `CKTspnoise`); see [SPARAM.md](SPARAM.md) |
| `src/frontend/rawfile.c` | 863 | `analysis::rawfile` | ASCII read **and** write ported, plus binary real/complex read **and** write with explicit byte order and validated payload lengths (#45); see [RAWFILES.md](RAWFILES.md) |
| `src/frontend/plotting/` | 9,380 | `analysis::results` | production result tables; interactive plotting not ported |

The #29 experimental `maths::diffsol::higher_index` module is an independent
numeric constrained-RLC formulation, with `CAPload`/`INDload`/`DCtran` as read-only
physical/event references, not a port of the C transient scheduler. Default
index-one guards remain intact; prototype evidence and runtime enabling gates
#69–#72 are recorded in [HIGHER_INDEX_DAE_ADR.md](HIGHER_INDEX_DAE_ADR.md).

## Licensing notes

`COPYING` states ngspice is Modified BSD **except** for `src/maths/KLU`
(LGPLv2), `src/tclspice.c` (LGPLv2), `src/maths/sparse` (MIT) and `m4`
(DFSG-compatible). A from-scratch Rust port distributed under Modified BSD must
not absorb LGPL code:

- **KLU** is a behavioral reference only: no LGPL algorithms are copied and
  neither KLU nor SPARSE 1.3 is translated into the Rust solver. Production
  real/complex LU uses **faer 0.24.4 (MIT)**; bounded adaptive BDF uses
  **diffsol 0.17.1 (MIT)**. SuiteSparse/SUNDIALS features remain disabled; no
  external native solver or FFI is required. This backend choice resolves the
  historical pre-M2 licensing gate without changing the port's BSD-3-Clause
  license. The locked diffsol-la/nalgebra graph requires Rust **1.89**.
- **SPARSE 1.3** is MIT licensed, so translation is permitted with attribution,
  but it should still be reviewed before code is copied rather than reimplemented.
