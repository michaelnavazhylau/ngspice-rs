# Bounded `.param` and parameter-expression syntax (#14)

**Syntax only.** `.param` cards and `{...}` expressions are parsed into an
unevaluated AST with original text and byte spans. Nothing is evaluated,
ordered by dependency, scoped or checked for undefined/cyclic references; that
is GitHub #15. Parsing a value does not make a device usable: elaboration still
rejects any `ParameterKind::Expression` or `.param` card, and the CLI `parse`
exit status 0 does not mean parameters were resolved.

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
```

Implemented in `crates/spice-netlist/src/parser/expression.rs` with winnow
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
C's default `^` evaluates `pow(fabs(x), y)`; #15 must not use plain `powf`.

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

## `.param` cards

`.param name=expr [name=expr ...]` (`.params` classifies the same). Assignments
are separated by whitespace and/or commas, kept **in order including
duplicates** (`ParamCard::assignments`), and spelled with original expression
text. Values are `{braced}` or unbraced.

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

Not accepted: evaluation, ordering, scopes, cycles and undefined names (#15);
comparison/logical/ternary operators (`< > <= >= == != <> && || ! ? :`), `%`
and `\`; `ternary_fcn`, randomised `agauss gauss unif aunif limit`, string
`vec`/`var`, user `.func` functions; quoted `'...'` expressions and string
parameters; `.param` with `&`, `.if` blocks; expressions inside waveform
(`PULSE`/`PWL`), `ic=` vector, flag and `level=` model-selector sites (explicit
gaps); nested braces; behavioural/time-dependent device equations
(`B`, `E`/`G` expression sources); and full numparam compatibility. Subcircuit
parameter passing semantics and flattening are not implemented.

## Public API for an evaluator (#15)

`spice_netlist::expr`: `ParameterExpression { text, braced, span, root }`
(`references()` lists names in source order), `Expr { kind, span }`,
`ExprKind::{Number, Identifier, Unary, Binary, Call, Group}`, `UnaryOp`,
`BinaryOp`, `Function`, `SourceSpan`, `MAX_NESTING`. Cards:
`Netlist::params`/`Subcircuit::params: Vec<ParamCard>`, each with
`assignments: Vec<ParamAssignment { name, name_span, expression }>`.
`Parser::parse_expression(text, &SourceLoc)` parses a standalone expression.
