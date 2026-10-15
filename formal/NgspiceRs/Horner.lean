/-
# Horner evaluation

`src/maths/polynomial.rs::evaluate` evaluates a coefficient slice from the
highest power down:

```rust
coefficients.iter().rev().fold(0., |sum, c| sum * x + c)
```

Its doc comment says "using Horner's rule", which is the claim that this fold
computes `Σ_k c_k x^k`. The Rust unit test only checks a single 3rd-degree
example; the identity is what the front-end resampling code actually relies on,
so it is proved here for every list and every point.
-/

module
public import Mathlib.Algebra.BigOperators.Ring.Finset
public import Mathlib.Data.List.GetD
public import Mathlib.Tactic

namespace NgspiceRs

open Finset

/-- The Rust `evaluate`: fold the coefficients from the highest power down,
running `acc ↦ acc * x + c`.

`List.foldr` against the reversed list is the same loop as the Rust
`iter().rev().fold(..)`; written this way `List.foldr_cons` applies directly,
which keeps the proofs short. -/
@[expose] public def horner (c : List ℝ) (x : ℝ) : ℝ :=
  c.foldr (fun coeff acc => acc * x + coeff) 0

/-- **Horner's rule.** For every coefficient list `c` and every `x`, the
descending fold equals the plain power sum `Σ_k c_k x^k`.

A missing coefficient beyond the end of the list reads as `0`, matching the
Rust `fold`'s accumulator. -/
public theorem horner_eq_sum (c : List ℝ) (x : ℝ) :
    horner c x = ∑ k ∈ Finset.range c.length, c.getD k 0 * x ^ k := by
  induction c with
  | nil => simp [horner]
  | cons a t ih =>
      have step : horner (a :: t) x = horner t x * x + a := by simp [horner]
      rw [step, ih, List.length_cons, Finset.sum_range_succ']
      simp only [List.getD_cons_zero, List.getD_cons_succ, pow_zero, mul_one]
      rw [Finset.sum_mul]
      congr 1
      refine Finset.sum_congr rfl fun k _ => ?_
      rw [pow_succ]
      ring
end NgspiceRs
