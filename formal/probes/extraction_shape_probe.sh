#!/usr/bin/env bash
#
# Which Rust *shapes* can Charon/Aeneas translate?
#
# This answers the question "can c2rust output be verified?" by testing the two
# things c2rust changes — the memory model (raw pointers, `unsafe`) and the
# control flow (C-shaped loops) — separately from the thing that actually
# blocks the pipeline.
#
# It builds a scratch crate outside the repository, runs `cargo hax into lean`
# on each function in isolation, and reports whether a Lean body was produced.
# Nothing is written inside the checkout, and no edit to ngspice-rs is needed.
#
# Requires `cargo-hax` on PATH and a Lean toolchain for the generated package.
# The first run downloads Charon/Aeneas and compiles the crate (~4 min); each
# subsequent function is ~30 s.
#
# Usage:  bash formal/probes/extraction_shape_probe.sh
#
set -uo pipefail

WORKDIR=$(mktemp -d "${TMPDIR:-/tmp}/shape-probe-XXXXXX")
trap 'rm -rf "$WORKDIR"' EXIT

echo "scratch crate: $WORKDIR"
mkdir -p "$WORKDIR/src"

cat > "$WORKDIR/Cargo.toml" <<'TOML'
[package]
name = "shapeprobe"
version = "0.1.0"
edition = "2021"
TOML

# Each probe isolates one property. The comments say what the probe is for;
# the "control" entries exist to separate one cause from another.
cat > "$WORKDIR/src/lib.rs" <<'RUST'
//! Shapes probed by formal/probes/extraction_shape_probe.sh.

// ---- float arithmetic: the whole content of a SPICE kernel ----------------

/// A bare float literal.
pub fn f64_literal() -> f64 {
    1.5
}

/// Float arithmetic on parameters, no literal, no control flow.
pub fn f64_binop(a: f64, b: f64) -> f64 {
    a * b
}

/// Float accumulation in a plain while loop.
pub fn f64_loop(a: &[f64]) -> f64 {
    let mut s = 0.0f64;
    let mut i = 0usize;
    while i < a.len() {
        s += a[i];
        i += 1;
    }
    s
}

// ---- controls: the same shapes in integer arithmetic ----------------------

/// Control for `f64_binop`: identical shape, integer type.
pub fn int_binop(a: usize, b: usize) -> usize {
    a * b
}

/// Control for the `evaluate` shape: iterator + closure + fold, integers only.
pub fn int_closure_fold(a: &[usize]) -> usize {
    a.iter().rev().fold(0usize, |s, c| s + c)
}

/// Nested loops with a return in the inner loop (`gear_corrector`'s failure).
pub fn int_nested_early_return(m: &[usize], n: usize) -> usize {
    let mut acc = 0usize;
    for i in 0..n {
        for j in i..n {
            if m[j] == 0 {
                return acc;
            }
            acc += m[j];
        }
    }
    acc
}

// ---- the c2rust memory model: raw pointers and `unsafe` -------------------

/// Control for `raw_ptr_f64_loop`: raw pointer, integer element type, so the
/// pointer question is not confounded with the float question.
///
/// # Safety
/// `a` must be valid for `n` reads.
pub unsafe fn raw_ptr_i32_loop(a: *const i32, n: usize) -> i32 {
    let mut s = 0i32;
    let mut i = 0usize;
    while i < n {
        s += *a.add(i);
        i += 1;
    }
    s
}

/// A c2rust-style `*p.offset(i)` loop over `f64`.
///
/// # Safety
/// `a` must be valid for `n` reads.
pub unsafe fn raw_ptr_f64_loop(a: *const f64, n: usize) -> f64 {
    let mut s = 0.0f64;
    let mut i = 0usize;
    while i < n {
        s += *a.add(i);
        i += 1;
    }
    s
}
RUST

PROBES=(
  f64_literal
  f64_binop
  f64_loop
  int_binop
  int_closure_fold
  int_nested_early_return
  raw_ptr_i32_loop
  raw_ptr_f64_loop
)

printf '%-24s %-10s %s\n' "probe" "extracted" "first diagnostic"
printf '%-24s %-10s %s\n' "------------------------" "---------" "----------------"

for fn in "${PROBES[@]}"; do
  out="$WORKDIR/out-$fn"
  rm -rf "$out"
  # `cargo hax` must run inside the crate it is translating.
  log=$(cd "$WORKDIR" && cargo hax into --output-dir "$out" lean \
        --charon-args="--start-from 'shapeprobe::$fn' --include 'shapeprobe::$fn'" 2>&1)

  # Aeneas emits a `def` with body `sorry` for functions it failed to
  # translate, so the name alone proves nothing: the body must be real.
  block=$(awk -v fn="$fn" '
      index($0, "def " fn " ") == 1 || index($0, "def " fn ":") == 1 { inside = 1 }
      inside && index($0, "def ") == 1 && index($0, "def " fn) != 1 { inside = 0 }
      inside { print }
    ' "$out/Shapeprobe/Extraction/Funs.lean" 2>/dev/null)
  if [ -n "$block" ] && ! printf '%s' "$block" | grep -q "sorry"; then
    extracted="yes"
  else
    extracted="NO"
  fi

  diag=$(printf '%s\n' "$log" \
    | sed 's/\x1b\[[0-9;]*m//g' \
    | grep -E '\[Error\]|not supported|Improperly|Invalid inputs' \
    | head -1 \
    | sed 's/^ *aeneas > //' \
    | sed 's/\[Error\] //')
  printf '%-24s %-10s %s\n' "$fn" "$extracted" "${diag:--}"
done

cat <<'NOTE'

Reading the table (measured with cargo-hax 0.4.2, Charon nightly-2026.09.02,
Aeneas build-183e4f0):

  * int_binop and int_closure_fold extract, so neither integer arithmetic nor
    iterator/closure shapes are the problem. The `evaluate` failure recorded in
    FORMAL_VERIFICATION.md section 4 is therefore *not* caused by its closure.
  * Every f64 probe fails, on a bare literal ("Improperly typed constant
    value") and on arithmetic ("Invalid inputs for binop"). Aeneas's Lean
    backend has no floating-point model at this build: there is no F32, F64 or
    Float anywhere under its Aeneas/Std.
  * int_nested_early_return fails, so that limitation is real and independent
    of floats.
  * Compare raw_ptr_i32_loop with raw_ptr_f64_loop to see whether the raw
    pointer model or the float model is responsible.

See docs/port/FORMAL_VERIFICATION.md sections 4 and 11.
NOTE
