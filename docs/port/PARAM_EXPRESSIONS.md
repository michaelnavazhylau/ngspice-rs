# Bounded `.param` and parameter-expression syntax (#14)

**Syntax (#14) plus top-level evaluation (#15).** `.param` cards and `{...}`
expressions are parsed into an AST with original text and byte spans; the
evaluator in `netlist::eval` and the literalizer in
`netlist::elaborate` resolve **top-level** values (see "Evaluation"
below). Subcircuit `.param` cards and formals are evaluated per instance when
`X` instantiates a definition; `Circuit` elaboration now expands subcircuits
(#18, [SUBCIRCUITS.md](SUBCIRCUITS.md)) instead of rejecting them.

C references (read-only): `src/frontend/numparam/xpressn.c` (`formula()`,
`fetchoperator()`, `fetchnumber()`, `fetchid()`, `fmathS`, `operate()`,
`nupa_assignment()`), `spicenum.c` (`transform()`), and `inpcom.c`
(`inp_split_multi_param_lines()`).

## Grammar

```text
sum     := [sign] term { ('+' | '-') term }
term    := factor { ('*' | '/') factor }          left associative
factor  := atom { ('^' | '**') atom }             left associative
atom    := number | '-' number | name | call | '(' sum ')'
call    := function '(' sum { ',' sum } ')'
         | name '(' [ sum { ',' sum } ] ')'      user .func call (#107)
```

Implemented in `src/netlist/parser/expression.rs` with winnow
`expression()` for the `^`/`*` levels and a fold for the additive level.

| Behaviour (pinned against the C binary) | Example |
| --- | --- |
| `^` and `**` are one operator, tighter than `*` `/`, **left** associative | `2^3^2 = (2^3)^2 = 64` |
| `*` `/` tighter than `+` `-`, all left associative | `1-2-3 = (1-2)-3` |
| A sign at the start of an expression, `(` group or argument has additive weight | `-2^2 = -(2^2) = -4`, `-a*b+1 = (-(a*b))+1` |
| After a binary operator only `-` directly before a numeric literal is accepted, and binds tightest | `2*-3^2 = 2*((-3)^2)`, `2^-1`, `2--3`, `1+-2`, `--3` |
| Rejected because C errors on them | `2*+3`, `2*-a`, `2*-(3)`, `a b`, `3 k` |
| Whitespace between tokens is allowed inside `{...}`, groups and arguments | `{ a + b * 2 }`, `max(1, 2)` |

`tests/c_param_reference.rs` (opt-in, `NGSPICE_BIN`) folds the parsed trees with
a test-local evaluator and compares 28 expressions with the C binary's
`.param`/`{...}` evaluation; it also checks C rejects the sign forms above.
C's default `^` evaluates `pow(fabs(x), y)`; the evaluator implements exactly that.

### Literals and names

Numbers start with a digit or `.`: SPICE scale factors (`t g meg k m mil u n p
f a`; `m` is milli) and trailing unit letters (`5V`, `1kohm`) are accepted and
the original spelling is kept. A literal that overflows to a non-finite value
(`1e999`) is a committed error at its first byte. RKM spellings (`4k7`) and
hexadecimal/`inf`/`nan` forms are not numbers here.

Names are `[A-Za-z_][A-Za-z0-9_.]*`, lowercased in the AST (case-insensitive,
like numparam). `[`/`]` in names, `&` forms and vector access are not accepted.

### Function allowlist

`sqr sqrt sin cos exp ln arctan abs pow pwr max min int log log10 sinh cosh
tanh sgn ceil floor asin acos atan asinh acosh atanh tan nint` (`Function::ALL`).
`pow`, `pwr`, `max`, `min` take exactly two arguments; every other function one;
wrong counts are errors. Function names cannot be used as parameter names. The
call parentheses may follow optional whitespace.

A call to any other name (`f(1, 2)`, `f()`, and the numparam built-ins outside
the allowlist such as `agauss`/`limit`) parses as `ExprKind::UserCall` and is
resolved during evaluation against the `.func` definitions in scope (see
"`.func` and quoted values" below). Without a definition it is an evaluation
error (`undefined function`), or `NotYetPorted` for an excluded numparam
built-in and for the behavioural probe functions `v(...)`/`i(...)` (C accepts
them in a device value such as `r1 1 0 {1/i(v1)}` by rewriting the device into
a behavioural one, `inpcom.c` `b_transformation_wanted()`; that rewrite is not
ported).

## `.param` cards

`.param name=expr [name=expr ...]` (`.params` classifies the same). Assignments
are separated by whitespace and/or commas, kept **in order including
duplicates** (`ParamCard::assignments`), and spelled with original expression
text. Values are `{braced}`, `'quoted'` (identical to braces, #107) or
unbraced.

The extent of an **unbraced** value follows C's multi-assignment splitter
(`inp_split_multi_param_lines()`): it ends at whitespace or a comma outside
parentheses; spaces inside `(...)` and `{...}` are kept. The port applies this
rule to every card. C is more permissive on a card with a single assignment
(`.param a = 1 + 2` works there; with several assignments `b = a * 3` silently
means `b = a`), so here such input is an explicit error that tells you to use
`{...}`. Continuation lines are joined first; reported columns are byte columns
in the joined card, like all token locations.

`.param` cards in a `.subckt` body are stored in `Subcircuit::params` and
referenced as `ScopedCardKind::Param(index)` in that scope; the root uses
`Netlist::params`. Cards after `.end` are ignored as for every card.

## Value sites

A braced expression is parsed (not evaluated) and stored as
`ParameterKind::Expression(Box<ParameterExpression>)`, with `value` holding the
original `{...}` spelling, at: R/C/L leading, after-model and named scalars
(`tc1`, `w`, `ic`, ...); V/I leading value, `dc`, `ac` magnitude/phase; D and Q
leading `area` and named scalars; M named scalars; scalar `.model` parameters;
and `X`/`.subckt` assignments (also a bare non-function name, e.g. `k=base`).
Braced arguments of analysis cards are additionally parsed into
`AnalysisCard::expressions` (arguments stay opaque text).

Bare names are **not** references in device cards (C substitutes only braces
and quotes there), so `R1 a 0 rval` stays an explicit unported gap, which keeps
terminals and model names unambiguous. Numeric terminals stay terminals; a
declared model name stays a model.

## Errors

Recognised syntax commits: missing operands, unmatched `(`/`)`/`{`/`}`, empty
braces, trailing tokens, wrong arities, bad signs, overflowing literals and
nesting beyond 64 levels are `SpiceError::Parse` with the byte column of the
offending text. Valid numparam outside the subset is `SpiceError::NotYetPorted`
(reference `xpressn.c`), never dropped.

## Explicit exclusions

Not accepted: comparison/logical/ternary operators (`< > <= >= == != <> && || ! ? :`), `%`
and `\`; `ternary_fcn`, randomised `agauss gauss unif aunif limit`, string
`vec`/`var` (unless a `.func` defines the name); double-quoted string
parameters and quotes *inside* an expression; `.param` with `&`, `.if` blocks; expressions inside waveform
(`PULSE`/`PWL`), `ic=` vector, flag and `level=` model-selector sites (explicit
gaps); nested braces; behavioural/time-dependent device equations
(`B`, `E`/`G` expression sources); and full numparam compatibility. Subcircuit
parameter passing semantics and flattening are implemented separately (#18,
[SUBCIRCUITS.md](SUBCIRCUITS.md)).

## Evaluation (#15)

### C-backed rules (probed against the C binary; `c_param_eval.rs`)

C: `inpcom.c` `inp_reorder_params()`/`inp_sort_params()` run before `xpressn.c`
`nupa_assignment()`/`formula()`.

| Case | C result | Port |
| --- | --- | --- |
| `.param` cards anywhere | hoisted before all devices: a device card may use a parameter defined later | same: all top-level `.param` cards form one scope |
| `.param a=1` then `.param a=2` | the **last** definition wins; earlier ones are dropped, never evaluated | same; kept in `ParamScope::entries()` as `ParamState::Superseded` |
| `.param a=1`, `.param b={a*2}`, `.param a=10` | `b` is 20 (sees the surviving `a`) | same |
| `.param b={a}` before `.param a=2` | 2 (sorted by dependency level, then deck order) | same |
| `.param a=1` then `.param a={a+1}` / `.param a={a+1}` | `Undefined parameter [a]` (self references do not see a dropped definition) | undefined-name error; a self reference resolves only through a parent scope |
| `.param a={b} .param b={a}` | fatal abort ("level depth greater 1000") | error printing the cycle `'a' (loc) -> 'b' (loc) -> 'a' (loc)` |
| undefined name, even in an unused definition | `Undefined parameter` error | error with the identifier location and a "required by parameter ..." chain |
| names | case-insensitive | same |
| `1/0`, `sqrt(-1)`, `ln(0)`, overflow | silently `inf`/`nan` (an unused `1/0` parameter is harmless) | **deliberate divergence**: explicit error with source location; every definition is evaluated eagerly even if unused |

Function semantics follow `mathfunction()`/`operate()`: `^`/`**` and `pwr` are
`pow(fabs(x), y)`; `pow` is plain `pow`; `int` truncates; `nint` rounds half to
even; `sgn` is -1/0/1; `log` and `ln` are natural logarithms; `max`/`min` use
the C `MAX`/`MIN` macros. There is no implicit `temp`/`time`; unknown names are
undefined.

### API

- `eval::ParamScope::{root, resolve}` resolves ordered definitions into an
  immutable scope (`entries()`, `get()`, `evaluate()`), with an optional
  `Arc` parent and `ParamBinding`s (subcircuit formal defaults and instance
  overrides). `resolve_instance` is the subcircuit rule (#18): caller overrides
  win, a body `.param` redefining a bound name is kept as `ParamState::Superseded`
  and never evaluated, and precedence is instance override > body `.param` >
  formal default. Dependencies use a petgraph `DiGraph` (SCC for cycles,
  toposort levels for order). See [SUBCIRCUITS.md](SUBCIRCUITS.md).
- `eval::{EvalLimits, EvalBudget}` bound definitions (100 000), evaluated
  nodes (4 000 000) and tree depth (1 024); exceeding them is an error.
- `elaborate::literalize(&Netlist) -> ElaboratedNetlist { netlist, scope,
  sites }` returns a copy where each top-level device/model `Expression`
  parameter and braced analysis argument is replaced by its finite value
  (`ParameterKind::Scalar`; integral values as plain integers, otherwise
  `{:e}`), keeping `.param` cards and locations; `sites` records each site's
  original text, location and value. Subcircuit bodies are not touched here —
  the #18 expansion resolves body values separately; bare
  names, terminals and model names are never substituted.
- Consumers: `Circuit::from_netlist[_with_context]` literalizes first;
  `RunConfig::from_netlist` resolves and keeps the scope, and
  `RunConfig::request_for` evaluates `AnalysisCard::expressions`
  (`AnalysisRequest::from(&card)` alone does not, and the drivers reject raw
  braced text). `spice-rs parse` evaluates all sites and prints a
  `parameters:` line.
- Diagnostics are `SpiceError::Parse` with the failing sub-expression's
  location and text, plus the enclosing parameter/site.

`.option` values may be `{expr}` or single-quoted `'expr'` (#107 option part):
`RunConfig::from_netlist` evaluates them against the top-level scope (see
[FRONTEND_STRUCTURE.md](FRONTEND_STRUCTURE.md#expression-option-values-107-option-part)).

Remaining limits: no comparison/ternary operators or random functions.

## Public API for an evaluator

`netlist::expr`: `ParameterExpression { text, braced, span, root }`
(`references()` lists names in source order), `Expr { kind, span }`,
`ExprKind::{Number, Identifier, Unary, Binary, Call, Group}`, `UnaryOp`,
`BinaryOp`, `Function`, `SourceSpan`, `MAX_NESTING`. Cards:
`Netlist::params`/`Subcircuit::params: Vec<ParamCard>`, each with
`assignments: Vec<ParamAssignment { name, name_span, expression }>`.
`Parser::parse_expression(text, &SourceLoc)` parses a standalone expression.

## `.func` and quoted values (#107)

C references (read-only): `src/frontend/inpcom.c` `inp_change_quotes()`,
`inp_get_func_from_line()`, `inp_grab_func()`, `find_function()`,
`inp_expand_macros_in_func()`, `inp_expand_macro_in_str()`,
`inp_do_macro_param_replace()` and `inp_expand_macros_in_deck()`. C works on
the deck text before numparam runs; the port parses the same constructs into
the AST and evaluates them in `netlist::eval`, which is equivalent for
everything below (pinned by `tests/c_func_eval.rs`, opt-in
`NGSPICE_BIN`, and the `func_quotes` golden).

### Single-quoted expressions

C rewrites every single-quote pair outside `.control` to a brace pair, so
`'expr'` means exactly `{expr}`. The port treats a single-quoted token as a
delimited expression at every site that accepts braces (device, model, `X`,
`.subckt` formal, `.ic`/`.nodeset`, analysis arguments and `.param` values,
whose quoted value may contain spaces like a braced one). The AST keeps the
spelling: `ParameterExpression::quoted` (with `braced` also true),
`ParameterExpression::spelling()`, and `quoted=true` in AST dumps; the writer
writes the quotes back. Sites that reject braces (waveform fields, `ic=`
vectors, `level=`, `.four`, `.measure` values, `.option` values) reject quotes
with the same `NotYetPorted` gap. Double-quoted strings are not expressions;
`.include`/`.lib` paths keep their quoting. Quotes inside a `{...}` expression
and braces inside a quoted one are rejected rather than reinterpreted.

### `.func` cards

`.func name(p1, p2, ...) body` (the undocumented `=` before the body is
accepted, as in C). The body is a `{...}`, `'...'` or bare expression running
to the end of the card. Cards are `ast::FuncCard` in `Netlist::functions` /
`Subcircuit::functions`, ordered by `ScopedCardKind::Func`, written back by
the writer and compared by `semantic_eq`.

`.param name(p1, ...) = body` is the same definition: C rewrites a `.param`
card whose first token contains `(` into `.func` unconditionally
(`inp_fix_macro_param_func_paren_io()`). The port parses it as a `FuncCard`
with `FuncSpelling::Param` (the writer keeps the `.param` spelling;
`semantic_eq` treats both spellings alike). Here the `=` is required: C does
not keep `.param f(x) {x}` as a definition and the deck fails, and the port
reports a positioned parse error. Fixture: `conformance/cases/param_func.cir`.

| Case (probed against the C binary) | C | Port |
| --- | --- | --- |
| visibility | all definitions of a scope are visible throughout it (hoisted), also to `.param` cards and nested definitions, never outside the defining `.subckt` | same: one `FunctionScope` per lexical scope; expansion uses the definition's scope, not the caller's |
| shadowing | a body-local definition hides an outer one; within one scope the **last** definition wins | same |
| built-ins | a `.func` replaces a built-in of the same name (`max`, or excluded names such as `limit`) | same; a different-arity redefinition of an allowlisted built-in is `NotYetPorted` |
| arguments | bound by value; a formal shadows a `.param` of the same name inside the body | same |
| free names | resolved where the call is written (e.g. a body `.param k` inside a subcircuit); an enclosing `.func` call's formals capture them first (textual expansion) | same, through the chain of active calls |
| unused body naming an undefined parameter | accepted | accepted; a used one is an `undefined parameter` error with the call chain |
| undefined function | `Undefined parameter` error | `undefined function` error at the call |
| `v(...)`/`i(...)` in a device value with no `.func` of that name | device rewritten to a behavioural one | `NotYetPorted` |
| wrong argument count (at a site, or inside an unused top-level body) | fatal `parameter mismatch` | error at the call, naming the enclosing definition |
| direct or mutual recursion, even unused at top level | crash (unbounded expansion) | error printing the cycle (petgraph SCC) |
| recursion or wrong arity inside a `.subckt` body | fatal (mismatch or crash) only when the subcircuit is instantiated; an uninstantiated body is never checked | same: a body's `FunctionScope` is built and checked when an instance is expanded |
| `.func f(x,x)` | silently binds the first `x` | `NotYetPorted` (duplicate formal) |
| `.func f(max) {max*2}` (formal named like a built-in) | a bare use binds to the formal; calling that built-in in the body (`{max(1,5)}`) fails | same: bare uses bind to the formal, a call is a positioned parse error |
| text after a delimited body, `.func f(x) {x}+{1}` | glued after stripping braces and whitespace (`x+1`) | `NotYetPorted` |
| a definition inside a multi-assignment card, `.param a=1 f(x)={x}` or `.param f(x)={x} a=2` | split into separate cards (`inp_split_multi_param_lines()`), then rewritten to `.func` | `NotYetPorted`; write the definition on its own card |

Dependency ordering of `.param` cards includes the free names of the
functions they call, so `.param p={f(2)}` before `.func f(x) {x*q}` and
`.param q=3` resolves. Mixed body spellings that C glues together after
stripping braces and whitespace (`{a}+{b}`) are `NotYetPorted`; a bare body
runs to the end of the card and is parsed as one expression, so a bare body
whose whitespace C would delete to join tokens (`2 3` becomes `23` in C) is
still a plain parse error (known gap).

The dependency pre-pass is bounded like evaluation: the free names of each
definition are computed once per scope (memoized), every `.func` body node it
visits is charged to the `EvalBudget`, and it stops at the depth limit. For
the depth limit (1 024) each nested `.func` call counts as 4 levels, which
keeps the deepest accepted input within the stack of a plain 1 024-level
expression (a 2 MiB thread stack, unoptimized); deeper chains are a
positioned error. Expansion that is exponential in the deck size (each
definition calling the previous one twice) runs into the node budget, where C
expands it textually.

API: `eval::FunctionScope::{new, for_netlist, get, definitions, parent}`,
`eval::FunctionDef`; `ParamScope::for_netlist(&Netlist)` (top-level params and
functions; the reusable entry point for evaluating any expression against the
deck's top-level scope), `ParamScope::{resolve_scoped,
resolve_instance_scoped, functions}`. `ParamScope::{root, resolve,
resolve_instance}` keep their signatures and inherit the parent's functions
(none at the root). `elaborate::literalize`, `RunConfig::from_netlist` and
subcircuit expansion use the function-aware scopes.

Known pre-existing divergence (not changed here, #18): C evaluates an `X`
instance's parameter expression inside the callee's numparam scope, so
`x1 n s w={k*5}` sees a body `.param k`; the port evaluates it in the caller's
scope.
