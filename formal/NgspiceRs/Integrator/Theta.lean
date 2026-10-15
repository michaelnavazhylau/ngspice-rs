/-
# The trapezoidal/`xmu` rule, transcribed from C and proved

This file is the smallest complete instance of the "use the C oracle as a
specification" pattern described in
[`docs/port/FORMAL_VERIFICATION.md`](../../../docs/port/FORMAL_VERIFICATION.md):

1. the definitions below **mirror the C routine**, keeping its algebra, so the
   Lean term and the C term can be compared numerically as well as
   mathematically;
2. the theorems state the law the C routine exists to satisfy, over `ℝ`;
3. the proofs are about the *rule*, so they transfer to any implementation that
   is faithful to C — including the Rust port's
   `StepHistory::trial` / `Coefficients::integrate`.

## The C source

`src/maths/ni/nicomcof.c`, `case TRAPEZOIDAL`:

```c
if (order == 1) { CKTag[0] = 1/CKTdelta; CKTag[1] = -1/CKTdelta; }
else            { CKTag[0] = 1/(CKTdelta*(1-xmu)); CKTag[1] = xmu/(1-xmu); }
```

`src/maths/ni/niinteg.c`, `NIintegrate`, order 2:

```c
CKTstate0[ccap] = -CKTag[1]*CKTstate1[ccap] + CKTag[0]*(CKTstate0[ccap] - CKTstate1[ccap]);
```

The Rust port reproduces both literally
(`src/maths/integrator.rs`, the `Trapezoidal, 1` and `Trapezoidal, _` arms and
the `needs_previous_derivative` branch of `integrate`).

`xmu` is C's `CKTxmu`, set from `.option xmu`. `src/spicelib/analysis/cktntask.c`
documents the intended values and defaults to `0.5`:

```c
/*   xmu=0:    Backward Euler
 *   xmu=0.5:  trapezoidal (standard)
 *   xmu=0.49: good damping of current ringing, e.g. in R.O.s. */
tsk->TSKxmu = 0.5;
```

`cktntask.c`/`cktsopt.c` accept the whole `[0, 0.5]` range, and the port's
`companion.rs` does the same. That range is what makes the theorems below
worth stating: **only `xmu = 1/2` is second-order**, so the damping value the C
comment recommends is a first-order rule, by construction and by design rather
than by accident.

## Spec

With `h` the step, `q0`/`q1` the present/previous charge, `d1` the previously
accepted derivative, and `xmu ≠ 1`:

    d_n = -(xmu/(1-xmu)) d_{n-1} + (1/(h(1-xmu))) (q_n - q_{n-1})

`trapStep_exact_affine` shows this returns the exact derivative of any affine
charge, for every admissible `xmu`. `trapStep_quadratic` gives the exact error
on `t ↦ t²` as `h(2 xmu - 1)/(1 - xmu)`, which vanishes exactly at
`xmu = 1/2`.
-/

module
public import Mathlib.Tactic

namespace NgspiceRs

noncomputable section

/-- `nicomcof.c` `case TRAPEZOIDAL`, `order > 1`: `CKTag[0]`.

This is also the corrector coefficient for order 1 after `xmu = 0`, which is
why `ckttask.c` can call `xmu = 0` "backward Euler" and share one formula. -/
@[expose] public def trapAg0 (h xmu : ℝ) : ℝ := 1 / (h * (1 - xmu))

/-- `nicomcof.c` `case TRAPEZOIDAL`, `order > 1`: `CKTag[1]`. -/
@[expose] public def trapAg1 (xmu : ℝ) : ℝ := xmu / (1 - xmu)

/-- `NIintegrate`'s trapezoidal/`xmu` combination, in the operation order the
C and Rust code use: `-ag1 * d1 + ag0 * (q0 - q1)`. -/
@[expose] public def trapStep (h xmu q0 q1 d1 : ℝ) : ℝ :=
  -trapAg1 xmu * d1 + trapAg0 h xmu * (q0 - q1)

/-- `xmu = 0` is backward Euler: the coefficients collapse to `1/h` and `0`,
leaving `d_n = (q_n - q_{n-1})/h`. This is exactly what `cktntask.c` claims. -/
public theorem trap_xmu_zero_is_backward_euler (h : ℝ) :
    trapAg0 h 0 = 1 / h ∧ trapAg1 0 = 0 := by
  refine ⟨?_, ?_⟩ <;> simp [trapAg0, trapAg1]

/-- `xmu = 1/2` is the classical trapezoidal rule, `d_n = 2(q_n - q_{n-1})/h -
d_{n-1}`. This is the other half of `cktntask.c`'s table. -/
public theorem trap_xmu_half_is_trapezoidal (h q0 q1 d1 : ℝ) (hh : h ≠ 0) :
    trapStep h (1 / 2) q0 q1 d1 = 2 * (q0 - q1) / h - d1 := by
  simp only [trapStep, trapAg0, trapAg1]
  field_simp
  ring

/-- **Affine charges are differentiated exactly, for every admissible `xmu`.**
A charge `t ↦ c*t + b` has derivative `c` everywhere, so the rule must return
`c` whatever `xmu` is. -/
public theorem trapStep_exact_affine (h xmu t c b : ℝ) (hh : h ≠ 0)
    (hx : xmu ≠ 1) :
    trapStep h xmu (c * t + b) (c * (t - h) + b) c = c := by
  simp only [trapStep, trapAg0, trapAg1]
  field_simp
  ring

/-- **The exact error on `t ↦ t²`.** With `d1 = 2(t - h)` the true previous
derivative, the rule returns `2t + h(2 xmu - 1)/(1 - xmu)`. The correction
term is the whole order-of-accuracy story: it depends on `h`, so the rule is
first order whenever it is nonzero. -/
public theorem trapStep_quadratic (h xmu t : ℝ) (hh : h ≠ 0) (hx : xmu ≠ 1) :
    trapStep h xmu (t ^ 2) ((t - h) ^ 2) (2 * (t - h))
      = 2 * t + h * (2 * xmu - 1) / (1 - xmu) := by
  simp only [trapStep, trapAg0, trapAg1]
  field_simp
  ring

/-- **Second order holds exactly at the default `xmu = 1/2`.** For any nonzero
step, the trapezoidal/`xmu` rule reproduces the derivative of the quadratic
`t ↦ t²` if and only if `xmu = 1/2`.

Consequence for the port and for C: `.option xmu` inside `[0, 0.5]` is
accepted, and every value other than `0.5` silently reduces the "trapezoidal"
method to first order. `cktntask.c`'s recommended damping value `0.49` is
therefore a deliberate, documented trade of accuracy for damping — not an
accident, and provably not second-order. -/
public theorem trapStep_quadratic_exact_iff (h xmu t : ℝ) (hh : h ≠ 0)
    (hx : xmu ≠ 1) :
    trapStep h xmu (t ^ 2) ((t - h) ^ 2) (2 * (t - h)) = 2 * t ↔ xmu = 1 / 2 := by
  have hx' : (1 : ℝ) - xmu ≠ 0 := sub_ne_zero.mpr (Ne.symm hx)
  rw [trapStep_quadratic h xmu t hh hx]
  constructor
  · intro hzero
    have hterm : h * (2 * xmu - 1) / (1 - xmu) = 0 := by linarith
    field_simp [hh, hx'] at hterm
    rw [mul_zero] at hterm
    have hlin : 2 * xmu - 1 = 0 := by
      rcases mul_eq_zero.mp hterm with h1 | h2
      · exact absurd h1 hh
      · exact h2
    linarith
  · intro hx2
    rw [hx2]
    ring_nf

/-- **`xmu = 0.49` is a first-order rule.** The damping value `cktntask.c`
recommends for current ringing costs exactly `2h/51` of accuracy on the
quadratic, so it is not second-order. -/
public theorem trap_xmu_49_is_first_order (h t : ℝ) (hh : h ≠ 0) :
    trapStep h (49 / 100) (t ^ 2) ((t - h) ^ 2) (2 * (t - h))
      = 2 * t - 2 * h / 51 := by
  rw [trapStep_quadratic h (49 / 100) t hh (by norm_num)]
  ring_nf

/-- The same fact stated as the failure the rust test asserts numerically:
the rule does not reproduce the quadratic's derivative. -/
public theorem trap_xmu_49_error_ne_zero (h t : ℝ) (hh : h ≠ 0) :
    trapStep h (49 / 100) (t ^ 2) ((t - h) ^ 2) (2 * (t - h)) ≠ 2 * t := by
  rw [trap_xmu_49_is_first_order h t hh]
  intro hcon
  have : 2 * h / 51 = 0 := by linarith
  exact hh (by linarith)

end

end NgspiceRs
