/-
# Closed forms and maximal order

`Corrector.lean` proves *sufficient* conditions: any `ag` meeting the moment
conditions differentiates polynomials of degree `< n` exactly. That still lets
`ag` be "exact for more than the stated order", which would make the scheme's
accuracy claim false in the other direction, and it says nothing about the
order-1 and order-2 coefficients that `.tran` actually uses (the port's
documentation notes that `dctran.c` only ever selects orders 1 and 2, whatever
`maxord` says).

This file closes both gaps for those two orders:

* `order_one_closed_form` / `order_two_closed_form` pin the coefficients to the
  unique values `nicomcof.c`'s LU computes, matching the shape of the closed
  form in `gear_corrector`'s doc comment. Uniqueness is what licenses reading
  the Rust code's *output* as "the" corrector.
* `order_one_not_exact` / `order_two_not_exact` show the next degree up is
  handled *wrongly* — and by a specific nonzero amount — so the order is
  exactly 1 and exactly 2, not accidentally higher.

The general-`n` versions of both statements are recorded as the proof sketch at
the bottom of this file; they are not proved here.
-/

module
public import NgspiceRs.Integrator.Corrector
public import Mathlib.Tactic

namespace NgspiceRs

open Finset

/-! ## Order 1 (backward Euler)

`s 0 = 0` is the trial point itself, so the history is the single elapsed time
`s 1 = h`. The moment conditions are `ag 0 + ag 1 = 0` and `ag 1 * h = -1`,
giving `ag 0 = 1/h`, `ag 1 = -1/h` — exactly the `Trapezoidal, 1` arm of
`StepHistory::trial`, and Gear order 1 through the general path. -/

/-- Closed form of the order-1 corrector. -/
public theorem order_one_closed_form {ag s : Fin 2 → ℝ} (h : IsCorrector ag s 2)
    (hs0 : s 0 = 0) (hs1 : s 1 ≠ 0) :
    ag 0 = 1 / s 1 ∧ ag 1 = -1 / s 1 := by
  have m0 : ag 0 + ag 1 = 0 := by
    have := h 0
    simpa [Fin.sum_univ_two] using this
  have m1 : ag 0 * s 0 + ag 1 * s 1 = -1 := by
    have := h 1
    simpa [Fin.sum_univ_two] using this
  have hmul : ag 1 * s 1 = -1 := by
    simpa [hs0] using m1
  have hag1 : ag 1 = -1 / s 1 := by
    field_simp
    linarith
  refine ⟨?_, hag1⟩
  rw [hag1] at m0
  field_simp at m0 ⊢
  linarith

/-- **Order 1 is exactly order 1.** Order 1 integrates affine functions
exactly (`corrector_exact`); it does *not* integrate the quadratic `s ↦ s²`,
and the error is exactly `-h` when the elapsed history is `s = (0, h)`. -/
public theorem order_one_not_exact {ag s : Fin 2 → ℝ} (h : IsCorrector ag s 2)
    (hs0 : s 0 = 0) (hs1 : s 1 ≠ 0) :
    ∑ i : Fin 2, ag i * s i ^ 2 = -s 1 := by
  obtain ⟨ha0, ha1⟩ := order_one_closed_form h hs0 hs1
  rw [Fin.sum_univ_two, hs0, ha0, ha1]
  field_simp
  ring

/-- The same statement in the "not exact" form the Rust test asserts
(`assert!((derivative - exact).abs() > 1e-6)`): order 1 cannot reproduce the
quadratic, because its error on `s ↦ s²` is nonzero. -/
public theorem order_one_error_ne_zero {ag s : Fin 2 → ℝ} (h : IsCorrector ag s 2)
    (hs0 : s 0 = 0) (hs1 : s 1 ≠ 0) :
    ∑ i : Fin 2, ag i * s i ^ 2 ≠ 0 := by
  rw [order_one_not_exact h hs0 hs1]
  exact neg_ne_zero.mpr hs1

/-! ## Order 2 (the trapezoidal/BDF-2 pair)

With `s = (0, a, b)` the moment conditions are

    ag 0 + ag 1 + ag 2 = 0
    ag 1 * a + ag 2 * b = -1
    ag 1 * a² + ag 2 * b² = 0

Eliminating `ag 1` between the last two gives `ag 2 * b * (a - b) = -a`, and
then `ag 0 = -(a + b) / (a * b)`. The Rust `vandermonde_ag` helper solves
precisely this system numerically. -/

/-- Closed form of the order-2 corrector on elapsed times `(0, a, b)`. -/
public theorem order_two_closed_form {ag s : Fin 3 → ℝ} (h : IsCorrector ag s 3)
    (hs0 : s 0 = 0) (hs1 : s 1 ≠ 0) (hs2 : s 2 ≠ 0) (hne : s 1 ≠ s 2) :
    ag 2 = -s 1 / (s 2 * (s 1 - s 2)) ∧
    ag 1 = s 2 / (s 1 * (s 1 - s 2)) ∧
    ag 0 = (s 1 + s 2) / (s 1 * s 2) := by
  have m0 : ag 0 + ag 1 + ag 2 = 0 := by
    have := h 0
    simpa [Fin.sum_univ_three] using this
  have m1 : ag 0 * s 0 + ag 1 * s 1 + ag 2 * s 2 = -1 := by
    have := h 1
    simpa [Fin.sum_univ_three] using this
  have m2 : ag 0 * s 0 ^ 2 + ag 1 * s 1 ^ 2 + ag 2 * s 2 ^ 2 = 0 := by
    have := h 2
    simpa [Fin.sum_univ_three] using this
  have e1 : ag 1 * s 1 + ag 2 * s 2 = -1 := by simpa [hs0] using m1
  have e2 : ag 1 * s 1 ^ 2 + ag 2 * s 2 ^ 2 = 0 := by simpa [hs0] using m2
  have hd0 : s 1 - s 2 ≠ 0 := sub_ne_zero.mpr hne
  -- `-e2 + s 1 * e1` isolates `ag 2`; `e2 - s 2 * e1` isolates `ag 1`.
  have hag2 : ag 2 = -s 1 / (s 2 * (s 1 - s 2)) := by
    field_simp
    linear_combination -e2 + s 1 * e1
  have hag1 : ag 1 = s 2 / (s 1 * (s 1 - s 2)) := by
    field_simp
    linear_combination e2 - s 2 * e1
  have hag0 : ag 0 = (s 1 + s 2) / (s 1 * s 2) := by
    have key : ag 0 = -ag 1 - ag 2 := by linarith [m0]
    rw [key, hag1, hag2]
    field_simp
    ring
  exact ⟨hag2, hag1, hag0⟩

/-- **Order 2 is exactly order 2.** The trapezoidal/BDF-2 corrector reproduces
every quadratic (`corrector_exact`); on the cubic `s ↦ s³` its error is exactly
`s 1 * s 2`, which is nonzero for any admissible history. So `maxord` above 2
buys accuracy here only by changing the rule, not by this one being secretly
higher order. -/
public theorem order_two_not_exact {ag s : Fin 3 → ℝ} (h : IsCorrector ag s 3)
    (hs0 : s 0 = 0) (hs1 : s 1 ≠ 0) (hs2 : s 2 ≠ 0) (hne : s 1 ≠ s 2) :
    ∑ i : Fin 3, ag i * s i ^ 3 = s 1 * s 2 := by
  obtain ⟨ha2, ha1, ha0⟩ := order_two_closed_form h hs0 hs1 hs2 hne
  rw [Fin.sum_univ_three, hs0, ha2, ha1, ha0]
  field_simp
  ring_nf

/-- Order 2 cannot reproduce the cubic, so its effective order is exactly 2. -/
public theorem order_two_error_ne_zero {ag s : Fin 3 → ℝ} (h : IsCorrector ag s 3)
    (hs0 : s 0 = 0) (hs1 : s 1 ≠ 0) (hs2 : s 2 ≠ 0) (hne : s 1 ≠ s 2) :
    ∑ i : Fin 3, ag i * s i ^ 3 ≠ 0 := by
  rw [order_two_not_exact h hs0 hs1 hs2 hne]
  exact mul_ne_zero hs1 hs2

/-
## Maximal order for general `n` (not proved here)

The same argument generalises, and the general statement is worth recording
because it is the honest form of the "the rule is order `n - 1`" claim.

Let `s : Fin n → ℝ` be injective with `s 0 = 0`, and let `ag` satisfy the
moments for `j < n`. Write `P(X) = ∏_i (X - s i)`, and let `L(p) = Σ_i ag i * p (s i)`.
`IsCorrector ag s n` says `L(p) = -p'(0)` on all `p` of degree `< n`.

1. The interpolant of `X^n` at the nodes `s i` is `X^n - P`, so
   `Σ_i (s i)^n * ℓ_i = X^n - P` where `ℓ_i` is the Lagrange basis.
2. `ag i = -ℓ_i'(0)`, because `L` agrees with `-d/dX|_0` on the Lagrange basis.
3. Hence `Σ_i ag i * (s i)^n = -(X^n - P)'(0) = P'(0) - n * 0^(n-1)`, and for
   `n ≥ 2` the second term vanishes while `P'(0) = (-1)^(n-1) * ∏_{i ≠ 0} s i`
   is nonzero.

So the value is `(-1)^(n-1) * ∏_{i ≠ 0} s i`: for `n = 2` that is `-s 1` and
for `n = 3` it is `s 1 * s 2`, matching the two theorems above. Mechanising the
general case needs the Lagrange-basis identities and `coeff 1` of a product of
linear factors; the two concrete cases are what the port can select.
-/

end NgspiceRs
