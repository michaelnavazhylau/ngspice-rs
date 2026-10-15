/-
# Verified specification of the companion-model corrector

This file formalises what an ngspice "corrector" coefficient vector `CKTag`
(Lean: `ag`) *means*, independently of how the Rust port computes it.

## Where this comes from in the port

`src/maths/integrator.rs` derives `ag[0..=order]` twice, in two different ways:

* `gear_corrector` transcribes `nicomcof.c` verbatim: a `(order+1) × (order+1)`
  upper-Hessenberg matrix is built from ratios `arg / delta` and LU-factored
  without pivoting, then solved by forward/back substitution.
* The doc comment on `gear_corrector` states a closed form
  (`ag_0 = (1/h) Σ_j 1/σ_j`, `ag_i = (1/h) Π_{j≠0,i} σ_j / (-σ_i Π_{j≠0,i}(σ_j-σ_i))`),
  which the code does not use.
* `tests` then check the two agree *numerically*, at a handful of sample steps,
  and check "exact for polynomials of degree `order`" by sampling one
  polynomial per order.

The tests are therefore point checks of a law. The law itself is a finite
algebraic statement, and it is the whole justification for the scheme, so it is
worth stating once and proving.

## The mathematical content

Index the `order + 1 = n` time points backwards from the trial point as
`s i = t_trial - t_i`, so `s 0 = 0` and the `s i` are the *elapsed* times. The
Rust code builds exactly this: `vandermonde_ag` sets `s 0 = 0`,
`s i = Σ_{m<i} deltas m`, and solves

    Σ_i ag i * (s i / h)^j = -(j == 1)      for j = 0 .. order.

Clearing `h` gives the moment conditions in `IsCorrector` below. The sign of
the right-hand side is `-1` at `j = 1` because `s` runs *backwards* in time:
`d/ds = -d/dt`.

`corrector_exact` is then the law: the functional `Σ_i ag i · (-)` annihilates
every polynomial of degree `< n` except that it returns `-c₁` on the degree-1
coefficient. That is exactly "the corrector differentiates polynomials of
degree `≤ order` exactly", stated for arbitrary coefficients rather than for
one sampled polynomial.

The statement is deliberately made over `ℝ`, not over `Float`. See
`formal/README.md` for the trust boundary that this creates.
-/

module
public import Mathlib.Algebra.BigOperators.Field
public import Mathlib.Tactic

namespace NgspiceRs

open Finset

variable {n m : ℕ}

/-- The defining moment conditions of a companion-model corrector.

`ag` are the corrector coefficients (`CKTag`), `s i = t_trial - t_i` the
elapsed times of the charge history, most recent first, so `s 0 = 0`.

For `j = 0` the condition is `Σ_i ag i = 0`; for `j = 1` it is
`Σ_i ag i * s i = -1`; higher moments vanish. Indexing `j` by `Fin m` rather
than by `Fin n` lets callers ask for *more* moments than there are unknowns,
which is how `Corrector.not_exact_at_order` is proved. -/
@[expose] public def IsCorrector {n : ℕ} (ag : Fin n → ℝ) (s : Fin n → ℝ) (m : ℕ) : Prop :=
  ∀ j : Fin m, ∑ i, ag i * s i ^ (j : ℕ) = if (j : ℕ) = 1 then -1 else 0

/-- **The corrector law.**

If `ag` satisfies the moment conditions for degrees `0 .. m-1` then it
differentiates every polynomial of degree `< m` exactly, up to the sign change
that the elapsed-time indexing introduces:

    Σ_i ag i * P(s i) = -c₁,   where P(X) = Σ_j c j * X^j.

Trading `j : Fin n` for a general `m` costs nothing and makes the statement
usable when there are more moments than unknowns. -/
public theorem corrector_exact {ag s : Fin n → ℝ} {m : ℕ} (hm : 1 < m)
    (h : IsCorrector ag s m) (c : Fin m → ℝ) :
    ∑ i, ag i * (∑ j, c j * s i ^ (j : ℕ)) = -c ⟨1, hm⟩ := by
  calc ∑ i, ag i * (∑ j, c j * s i ^ (j : ℕ))
      = ∑ i, ∑ j, ag i * (c j * s i ^ (j : ℕ)) := by
        refine Finset.sum_congr rfl fun i _ => ?_
        rw [Finset.mul_sum]
    _ = ∑ j, ∑ i, ag i * (c j * s i ^ (j : ℕ)) := Finset.sum_comm
    _ = ∑ j, c j * (∑ i, ag i * s i ^ (j : ℕ)) := by
        refine Finset.sum_congr rfl fun j _ => ?_
        rw [Finset.mul_sum]
        refine Finset.sum_congr rfl fun i _ => ?_
        ring
    _ = ∑ j, c j * (if (j : ℕ) = 1 then -1 else 0) := by
        refine Finset.sum_congr rfl fun j _ => ?_
        rw [h j]
    _ = -c ⟨1, hm⟩ := by
        rw [Finset.sum_eq_single ⟨1, hm⟩]
        · simp
        · intro b _ hb
          have : (b : ℕ) ≠ 1 := fun hcon => hb (Fin.ext hcon)
          simp [this]
        · intro hcon
          exact absurd (Finset.mem_univ _) hcon

/-- The corrector law with the moment set and the coefficient set tied to the
same `n`: `ag` has `n` entries, there are `n` moments, and every polynomial of
degree `< n` is differentiated exactly. -/
public theorem corrector_exact_self {ag s : Fin n → ℝ} (hn : 1 < n)
    (h : IsCorrector ag s n) (c : Fin n → ℝ) :
    ∑ i, ag i * (∑ j, c j * s i ^ (j : ℕ)) = -c ⟨1, hn⟩ :=
  corrector_exact hn h c

/-- Restatement in the *time* coordinate, which is how `Coefficients::integrate`
is actually read.

A charge history sampled from a polynomial written in powers of `t - t_trial`
has values `q_i = Σ_j b j * (-1)^j * (s i)^j`, because `s i = t_trial - t_i`.
For such a history the corrector returns `b₁`, which is the Jacobian
`q'(t_trial)`. So this says: **a degree-`< n` charge history whose values are
exact gives the exact derivative at the trial point**, with no sign left over. -/
public theorem corrector_exact_time {ag s : Fin n → ℝ} {m : ℕ} (hm : 1 < m)
    (h : IsCorrector ag s m) (b : Fin m → ℝ) :
    ∑ i, ag i * (∑ j, b j * (-1) ^ (j : ℕ) * s i ^ (j : ℕ)) = b ⟨1, hm⟩ := by
  -- Fold the sign into the coefficient vector and apply the corrector law.
  have key := corrector_exact hm h (fun j => b j * (-1) ^ (j : ℕ))
  have expand : ∀ i : Fin n,
      (∑ j, b j * (-1) ^ (j : ℕ) * s i ^ (j : ℕ))
        = (∑ j, (b j * (-1) ^ (j : ℕ)) * s i ^ (j : ℕ)) := by
    intro i
    exact Finset.sum_congr rfl fun j _ => by ring
  calc ∑ i, ag i * (∑ j, b j * (-1) ^ (j : ℕ) * s i ^ (j : ℕ))
      = ∑ i, ag i * (∑ j, (b j * (-1) ^ (j : ℕ)) * s i ^ (j : ℕ)) := by
        exact Finset.sum_congr rfl fun i _ => by rw [expand i]
    _ = -(b ⟨1, hm⟩ * (-1) ^ ((⟨1, hm⟩ : Fin m) : ℕ)) := key
    _ = b ⟨1, hm⟩ := by norm_num

end NgspiceRs
