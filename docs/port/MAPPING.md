# C source tree → Rust crate mapping

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

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/spicelib/parser/inpeval.c` | 1,139 | `spice-core::value` | numeric literals and scale factors ported |
| `src/frontend/inpcom.c` | 10,237 | `spice-core::node` (`inp_fix_gnd_name`), `spice-netlist::source` (`inp_stripcomments_line`), `spice-netlist::card`, parser terminal canonicalization | ground aliasing, comment stripping, card classification ported; include/lib/numparam preprocessing still missing |
| `src/frontend/inp.c` | 2,967 | `spice-netlist::source` | title line, continuation folding ported |
| `src/frontend/parse-bison.y` | 180 | future front-end expression parser | **not ported**; this is an expression grammar, not the netlist deck grammar |
| `src/spicelib/parser/inp2*.c` (device and dot-card grammars) | 3,828 | `spice-netlist::parser` | scalar/declared-model R/C/L, DC/AC V/I and bounded D/Q/M syntax; analysis arguments retained without validation; remaining grammars unported |
| `src/spicelib/parser/{inpmkmod,inpdomod,inpgmod,inpfindl,inpgval}.c` | — | `spice-netlist::parser::model`, `spice-devices::{models,schema}` | raw scalar cards retained; top-level first-wins resolution, family/level checks and bounded diode input schemas; advanced backends/scopes pending |
| `src/spicelib/parser/inppas*.c` (input passes: models, devices, IC/nodeset, shunts) | 667 | `spice-netlist::parser`, later circuit elaboration | card dispatch and model-name indexing partially ported; model elaboration/IC/shunt passes unported; these are **not** `.param` evaluators |
| `src/frontend/numparam/{spicenum,xpressn}.c`, preprocessing in `inpcom.c` | — | `spice-netlist::expr` (planned) | `.param` expression/scoping behaviour; **not ported** |
| `src/spicelib/parser/ifeval.c` | 190 | future behavioural-device evaluator | **not ported**; evaluates IF parse trees, not numparam `.param` expressions |
| `src/spicelib/parser/inpsymt.c` | 305 | `spice-netlist::symbols` (planned) | **not ported** |
| `src/frontend/circuits.c`, `define.c` | 483 | `spice-devices::registry`, `spice-core::node` | registry with working scalar R/C/L/V/I factories; other designators explicitly unavailable |
| `src/frontend/` (whole directory) | 88,452 | — | includes the command interpreter, plots and measurement; mostly deferred |

## Maths

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/maths/dense/` | 1,742 | `spice-maths::dense` | row-major storage and owned faer pivoted LU with checked solves |
| `src/maths/sparse/` (SPARSE 1.3, MIT) | 10,465 | `spice-maths::sparse` | triplet storage, petgraph row-coupling projection and owned faer sparse LU; finite/rank/residual checks and exact-pattern symbolic reuse |
| `src/maths/KLU/` (LGPLv2) | 18,353 | behavioral reference only | **not translated or linked**; faer supplies real/complex LU, see licensing below |
| `src/maths/ni/` | 1,961 | `spice-maths::integrator`, separate `spice-maths::diffsol` | trap and Gear orders 1–2 coefficients, `NIintegrate`/`NIpred`/`CKTterr` operations and accepted step history; orders 3–6 rejected; explicit adaptive BDF supports index-one DAEs including floating/coupled capacitor mass blocks (higher-index rejected), not ngspice trap/Gear parity |
| `src/maths/cmaths/` | 4,054 | `spice-core::value::Complex` | arithmetic/magnitude/phase/conjugation ported; not the full C transcendental library |
| `src/maths/poly/`, `deriv/`, `fft/`, `misc/` | 6,358 | `spice-maths` (planned modules) | **not ported** |

## Devices

`src/spicelib/devices/` totals **464,108 lines**. The port must not attempt to
translate all of it; the roadmap targets a small, useful subset first.

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/spicelib/devices/ckt*.c` (device framework) | 419 | `spice-devices` | trait, scalar factories, `Circuit` incidence topology, node-before-branch binding and immutable linear equation assembly, atomic AST instance insertion and top-level typed model resolution and bounded passive factories; nonlinear factories pending |
| `res/`, `cap/`, `ind/` | 5,326 | `spice-devices::{rlc,passive}` | scalar equations plus bounded model values, R sheet/C area-perimeter geometry, contextual TC1/TC2, scale/multiplicity; trap/Gear orders 1–2 C/L companion stamps from accepted charge/flux state (`capload.c`/`indload.c`); coil geometry, advanced setters, mutual inductance and companion `ic=`/`uic` pending |
| `vsrc/`, `isrc/` | — | `spice-devices::sources` | DC/AC V/I stamps, device-API Constant/Step/Pwl forcing; waveform deck syntax pending |
| `dio/` | 5,598 | `spice-devices::diode` (planned) | **not ported** |
| `bjt/` | 9,482 | `spice-devices::bjt` (planned) | **not ported** |
| `mos1/`…`mos9/`, `bsim*`, `hisim*`, `hfet*`, `vbic`, `soi*` | 218,897 | `spice-devices::mos` (planned) | **not ported** |
| `src/xspice/` (event-driven code models) | 28,373 | `spice-devices::xspice` (planned) | **not ported** |
| `src/osdi/` (Verilog-A / OSDI) | 3,372 | — | **not ported**; needs a Verilog-A compiler path, probably out of scope |
| `src/ciderlib/` (numerical device simulator) | 28,342 | — | **not ported**; probably out of scope |

## Analyses and results

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/spicelib/analysis/` (whole directory) | 21,993 | `spice-analysis::analysis` | trait/runner, linear `.op`, single-independent-source `.dc`, complex `.ac` and explicitly selected restricted diffsol BDF; nonlinear/other analyses pending |
| ↳ `cktdojob.c`, `dctran.c`, `dcop.c`, `acan.c`, `cktload.c` | 2,060 | `spice-analysis::analysis` | bounded linear assembly/factor/solve/plot orchestration; no nonlinear Newton/stepping or SPICE trap/Gear driver |
| `src/frontend/rawfile.c` | 863 | `spice-analysis::rawfile` | ASCII read **and** write ported; binary rawfiles not ported |
| `src/frontend/plotting/` | 9,380 | `spice-analysis::results` | production result tables; interactive plotting not ported |

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
