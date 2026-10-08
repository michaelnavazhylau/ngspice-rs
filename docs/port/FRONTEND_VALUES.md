# Bounded waveform, flag and IC syntax (#8 / #10)

The waveform and flag/IC forms are **syntax** contracts. PULSE/PWL V/I
setters are elaborated into time forcing by #9 (see
[DIFFSOL_FAER_IMPLEMENTATION.md](DIFFSOL_FAER_IMPLEMENTATION.md)); D/Q/M
factories remain unavailable. A successful parse does not
implement initialization, `.ic`/`uic`, nonlinear physics or trap/Gear integration.
Constant/Step/Pwl device-API forcing is unchanged.

## C-backed flag inventory

Inventory: `src/spicelib/devices/{dio/dio.c,bjt/bjt.c,mos1/mos1.c}` parameter
tables; setters in `dioparam.c`, `bjtparam.c`, `mos1par.c`, `diompar.c`,
`bjtmpar.c`, `mos1mpar.c`; `parser/inpgval.c::INPgetValue(IF_FLAG)` supplies 1
without consuming a numeric value. Only the following bare forms are accepted:

| Site | Accepted IF_FLAG setters | Deliberately rejected flags |
| --- | --- | --- |
| D instance | `off` | `thermal`, `sens_area` |
| Q instance | `off` | `sens_area`, extended-device flags |
| M instance | `off` | `sens_l`, `sens_w`, extended-device flags |
| D model | `d` | `off`, `thermal`, unrelated family flags |
| NPN/PNP model | `npn`, `pnp` | `off`, unrelated family flags |
| NMOS/PMOS model | `nmos`, `pmos` | `nchan`, `pchan`, `off`, unrelated family flags |
| R/C/L model | none | all flag forms |

The `.model` **base type token is not a setter** and does not synthesize a flag
assignment. A repeated family flag in the parameter tail is a real ordered
setter; syntax does not rewrite the base or apply polarity. Family-compatible
opposite-polarity flags are retained, not interpreted by the current resolver.
Factories still reject these unimplemented semantics.

Flags have `ParameterKind::Flag` and empty textual value, not a fabricated
scalar `1`. Duplicates remain visible. `off=0`, `off=1`, `off 1` and similar
model flag/value forms are malformed, never silently consumed as a bare flag.
Sensitivity and self-heating are outside the initial-engine syntax subset.

## Initial conditions

D `ic` remains scalar. Q `ic` accepts **1–2** values in `icvbe`, `icvce` order;
M `ic` accepts **1–3** values in `icvds`, `icvgs`, `icvbs` order. These partial
arities follow BJTparam/MOS1param's fallthrough switches. Missing components
stay omitted. Parentheses are optional, `=` optional, and fields may be
whitespace- or comma-separated: `ic=.6,2`, `ic=(.6 2)`, `ic .6 2`.

`ParameterKind::InitialConditions` retains canonical component names and each
value's original text/location. The assignment also retains original vector
argument text. Vectors and named scalar IC setters remain interleaved in source
application order; leading D/Q area still applies last. IC syntax is not a
promise of initialized states or transient support.

## Source waveforms

References: `parser/inp2v.c`, `inp2i.c`, `inpgval.c`, and
`devices/vsrc/vsrcpar.c::VSRCparam`, `isrc/isrcpar.c::ISRCparam`.

- PULSE accepts **2–8** finite numeric fields:
  `PULSE(V1 V2 [TD [TR [TF [PW [PER [NP]]]]]])`. Timing omissions remain `None`;
  analysis-dependent defaults must be supplied later. More fields are
  rejected (C would silently ignore them). No timing/slope/period validation is inferred from a parse.
- PWL accepts **1–2048 pairs** of finite numeric time/level fields. Supplied
  order, duplicate/decreasing/negative times and raw numeric spelling survive;
  future runtime elaboration must validate the demonstrated waveform subset.
  Syntax never sorts, repairs or drops a knot.
- Parentheses and `=` are optional; comma/whitespace separators are accepted.
  Every comma requires a following numeric field. A bare vector ends at the
  next keyword. Recognized prefixes commit missing/overflow/malformed errors.
- `SIN`/`SINE` (2–6 fields), `EXP` (2–6), `SFFM` (2–8) and `AM` (2–8) parse to
  `SourceWaveform::Function` with positioned fields in C coefficient order
  (`SourceFunction::fields`) (#94). PWL `td=`/`r=` (with or without `=`) are
  ordered scalar setters, as C's `VSRC_TD`/`VSRC_R` (#95); elaboration checks
  that they apply to a PWL. Runtime semantics: [TRANSIENT.md](TRANSIENT.md#source-functions-94-95).
- Expressions in waveform fields, file-backed PWL and TRNOISE/TRRANDOM/EXTERNAL
  sources remain explicit gaps.

`ParameterKind::Waveform` lives in the **same ordered assignment vector** as
DC/AC setters. Duplicate/mixed waveform setters remain visible. Leading source
DC is still applied last, and bare AC still produces magnitude 1, phase 0.
Each waveform field has a `PositionedValue`; the assignment keeps its original
argument text and keyword location. No evaluation occurs during tokenization.

## Provenance and validation boundaries

Positions follow the existing tokenizer: paths and physical start lines are
preserved; columns are **byte offsets in the joined logical card**, including
continuations. This work does not change physical-line mapping or tokenization.
Balanced outer parentheses and strict commas are intentional bounded stricter
behavior than C's permissive delimiter gobbling. Scalar AST spellings/order are
unchanged; `ParameterAssignment` now has an explicit `kind` field. Scalar
schema/factory consumers must reject non-scalar kinds, including manually
constructed ASTs, rather than treating a flag or vector as a finite scalar.

Ordinary parser/CLI/factory regressions need no C toolchain. Live parser probes
are opt-in via `NGSPICE_BIN`; they check C setup/setter behavior, not Rust
simulation. The IC C probe uses unparenthesized comma vectors: ngspice-47+'s
numparam preprocessing rejects some parenthesized whitespace IC forms before
INPgetValue runs. The Rust grammar intentionally retains balanced parenthesized
vectors without that preprocessing rewrite. Also, C's diode terminal-count/model
pruning heuristic can discard a model when its only references have positional
area before OFF/assignments; the probe includes a named-only D reference to keep
that declaration. Rust uses deck-local indexing and does not prune models.
Neither frontend quirk is claimed as whole-deck C parity. No rawfile goldens are
regenerated by this work.
