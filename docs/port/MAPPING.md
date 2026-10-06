# C source tree → Rust crate mapping

Paths in the tables below are paths **in the upstream ngspice tree**, relative to
its source root. The Rust port does not vendor the C sources; see `NOTICE`.

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
| `src/spicelib/parser/inp2*.c` (device and dot-card grammars) | 3,828 | `spice-netlist::parser` | scalar R/C/L, DC/AC V/I and bounded D/Q/M syntax; analysis arguments retained without validation; remaining grammars unported |
| `src/spicelib/parser/{inpdomod,inpgmod,inpfindl}.c` | — | `spice-netlist::parser::model` | scalar D/BJT/MOS/R/C/L cards and first raw level retained; schema, model defaults and backend selection deferred |
| `src/spicelib/parser/inppas*.c` (input passes: models, devices, IC/nodeset, shunts) | 667 | `spice-netlist::parser`, later circuit elaboration | card dispatch and model-name indexing partially ported; model elaboration/IC/shunt passes unported; these are **not** `.param` evaluators |
| `src/frontend/numparam/{spicenum,xpressn}.c`, preprocessing in `inpcom.c` | — | `spice-netlist::expr` (planned) | `.param` expression/scoping behaviour; **not ported** |
| `src/spicelib/parser/ifeval.c` | 190 | future behavioural-device evaluator | **not ported**; evaluates IF parse trees, not numparam `.param` expressions |
| `src/spicelib/parser/inpsymt.c` | 305 | `spice-netlist::symbols` (planned) | **not ported** |
| `src/frontend/circuits.c`, `define.c` | 483 | `spice-devices::registry`, `spice-core::node` | registry skeleton only |
| `src/frontend/` (whole directory) | 88,452 | — | includes the command interpreter, plots and measurement; mostly deferred |

## Maths

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/maths/dense/` | 1,742 | `spice-maths::dense` | storage ported, solver stubbed |
| `src/maths/sparse/` (SPARSE 1.3, MIT) | 10,465 | `spice-maths::sparse` | triplet storage ported, factor/solve stubbed |
| `src/maths/KLU/` (LGPLv2) | 18,353 | `spice-maths::sparse` | **not ported**; see licensing below |
| `src/maths/ni/` | 1,961 | `spice-maths::integrator` | types ported, stepping stubbed |
| `src/maths/cmaths/` | 4,054 | `spice-core::value::Complex` | arithmetic ported, transcendental helpers stubbed |
| `src/maths/poly/`, `deriv/`, `fft/`, `misc/` | 6,358 | `spice-maths` (planned modules) | **not ported** |

## Devices

`src/spicelib/devices/` totals **464,108 lines**. The port must not attempt to
translate all of it; the roadmap targets a small, useful subset first.

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/spicelib/devices/ckt*.c` (device framework) | 419 | `spice-devices` | trait, registry and `Circuit` skeleton only |
| `res/`, `cap/`, `ind/` | 5,326 | `spice-devices::rlc` | types only; first porting targets |
| `dio/` | 5,598 | `spice-devices::diode` (planned) | **not ported** |
| `bjt/` | 9,482 | `spice-devices::bjt` (planned) | **not ported** |
| `mos1/`…`mos9/`, `bsim*`, `hisim*`, `hfet*`, `vbic`, `soi*` | 218,897 | `spice-devices::mos` (planned) | **not ported** |
| `src/xspice/` (event-driven code models) | 28,373 | `spice-devices::xspice` (planned) | **not ported** |
| `src/osdi/` (Verilog-A / OSDI) | 3,372 | — | **not ported**; needs a Verilog-A compiler path, probably out of scope |
| `src/ciderlib/` (numerical device simulator) | 28,342 | — | **not ported**; probably out of scope |

## Analyses and results

| C | Lines | Rust crate | Status |
| --- | --- | --- | --- |
| `src/spicelib/analysis/` (whole directory) | 21,993 | `spice-analysis::analysis` | `Analysis` trait and runner dispatch ported, drivers stubbed |
| ↳ `cktdojob.c`, `dctran.c`, `dcop.c`, `acan.c`, `cktload.c` | 2,060 | `spice-analysis::analysis` | stubbed; the first analysis work will land here |
| `src/frontend/rawfile.c` | 863 | `spice-analysis::rawfile` | ASCII read **and** write ported; binary rawfiles not ported |
| `src/frontend/plotting/` | 9,380 | `spice-analysis::results` | result types only |

## Licensing notes

`COPYING` states ngspice is Modified BSD **except** for `src/maths/KLU`
(LGPLv2), `src/tclspice.c` (LGPLv2), `src/maths/sparse` (MIT) and `m4`
(DFSG-compatible). A from-scratch Rust port distributed under Modified BSD must
not absorb LGPL code:

- **KLU** is listed as a licensing question to resolve *before solver work starts*.
  Netlist parsing does not depend on it; the gate belongs before M2, not M1.
  The scaffold's `spice-maths::sparse` module is written from the description of
  sparse LU, not translated from KLU or SPARSE 1.3, and cites them only as the
  behaviour to match via golden data.
- **SPARSE 1.3** is MIT licensed, so translation is permitted with attribution,
  but it should still be reviewed before code is copied rather than reimplemented.
