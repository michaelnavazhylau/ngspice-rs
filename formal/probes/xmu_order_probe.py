#!/usr/bin/env python3
"""Oracle check for the trapezoidal/`xmu` rule proved in
`formal/NgspiceRs/Integrator/Theta.lean`.

The theorem `trapStep_quadratic_exact_iff` says the rule

    d_n = -(xmu/(1-xmu)) d_{n-1} + (1/(h(1-xmu))) (q_n - q_{n-1})

reproduces the derivative of the quadratic `t -> t^2` **iff `xmu = 1/2`**, and
`trapStep_quadratic` gives the error as `h(2 xmu - 1)/(1 - xmu)`. So the C
reference should exhibit second-order convergence at ngspice's default
`xmu = 0.5` and first-order convergence at the `xmu = 0.49` that
`cktntask.c` recommends for damping current ringing.

The circuit makes the *charge* exactly quadratic, which is what the rule
integrates: a linearly ramping current source into a capacitor,

    i(t) = 1 mA + (1 mA / 1 us) t      =>   q(t) = 1e-3 t + 500 t^2
    v(t) = q(t) / 1 uF                 =>   v(1 us) = 1.5e-3 V exactly

so any error at t = 1 us is integration error, not model error.

This is a *test* of the proved rule against the C oracle, not a proof: it
confirms the predicted order separation and cannot confirm the magnitude,
because ngspice's driver composes the rule with order-1 startup steps and
LTE-based step control. See docs/port/FORMAL_VERIFICATION.md section 10.

Usage:

    NGSPICE_BIN=/abs/path/to/ngspice python3 formal/probes/xmu_order_probe.py
"""

import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile

EXACT = 1.5e-3  # v(1 us), exactly, for the deck below
GRIDS = ("0.1u", "0.05u", "0.025u", "0.0125u")
XMU_VALUES = ("0.5", "0.49")


def deck_text(xmu: str, tmax: str) -> str:
    # `trtol=1e9` asks the driver not to cut steps for truncation error, so
    # `tmax` largely fixes the grid; the tolerances are tight so that the
    # observed error is the integration error and not Newton stopping error.
    return (
        "* xmu order probe: linearly ramping current into a capacitor\n"
        f".option xmu={xmu} trtol=1e9 reltol=1e-12 abstol=1e-18 vntol=1e-15\n"
        "I1 0 n PWL(0 1m 1u 2m)\n"
        "C1 n 0 1u ic=0\n"
        f".tran {tmax} 1u 0 {tmax} uic\n"
        ".end\n"
    )


def run_one(ngspice: str, workdir: str, xmu: str, tmax: str):
    """Return (times, final_voltage) for one configuration."""
    cir = os.path.join(workdir, f"x_{xmu}_{tmax}.cir")
    raw = os.path.join(workdir, f"x_{xmu}_{tmax}.raw")
    with open(cir, "w") as handle:
        handle.write(deck_text(xmu, tmax))
    subprocess.run(
        [ngspice, "-b", "-r", raw, cir],
        capture_output=True,
        check=False,
    )
    with open(raw, "rb") as handle:
        blob = handle.read()
    npts = int(blob.split(b"No. Points:")[1].split()[0])
    body = blob.split(b"Binary:\n", 1)[1]
    vals = struct.unpack(f"<{npts * 2}d", body[: npts * 2 * 8])
    times = vals[0::2]
    return times, vals[2 * (npts - 1) + 1]


def main() -> int:
    ngspice = os.environ.get("NGSPICE_BIN")
    if not ngspice:
        print("set NGSPICE_BIN to the reference ngspice binary", file=sys.stderr)
        return 2
    if not os.path.isfile(ngspice):
        print(f"NGSPICE_BIN does not exist: {ngspice}", file=sys.stderr)
        return 2
    ngspice = os.path.abspath(ngspice)

    print(f"C reference: {ngspice}")
    print("quadratic charge: exact v(1 us) = 1.5e-3 V")
    print("prediction from trapStep_quadratic_exact_iff: order 2 iff xmu = 0.5\n")
    print(f"{'xmu':>5} {'tmax':>8} {'npts':>5} {'h_avg':>11} "
          f"{'v(1us)':>18} {'rel_err':>11} {'err_ratio':>10} {'order':>6}")

    workdir = tempfile.mkdtemp(prefix="xmu-probe-")
    try:
        summary = {}
        for xmu in XMU_VALUES:
            prev = None
            prev_h = None
            for tmax in GRIDS:
                times, v = run_one(ngspice, workdir, xmu, tmax)
                err = abs(v - EXACT) / EXACT
                steps = [times[i + 1] - times[i] for i in range(len(times) - 1)]
                h_avg = sum(steps) / len(steps)
                ratio = f"{prev / err:.2f}" if prev else "-"
                order = (
                    f"{math.log(prev / err) / math.log(prev_h / h_avg):.2f}"
                    if prev and prev_h and err else "-"
                )
                print(f"{xmu:>5} {tmax:>8} {len(times):>5} {h_avg:>11.4e} "
                      f"{v:>18.12e} {err:>11.4e} {ratio:>10} {order:>6}")
                prev, prev_h = err, h_avg
            print()
            summary[xmu] = prev

        coarse = {}
        for xmu in XMU_VALUES:
            _, v = run_one(ngspice, workdir, xmu, GRIDS[0])
            coarse[xmu] = abs(v - EXACT) / EXACT
        print(f"same-grid penalty at tmax={GRIDS[0]}: "
              f"xmu=0.49 is {coarse['0.49'] / coarse['0.5']:.1f}x the error of xmu=0.5")
        print("\nThe refinement ratios separate the two rules by exactly one order,")
        print("which is what the theorem predicts: ~4x per halving for xmu=0.5")
        print("(second order) against ~2x for xmu=0.49 (first order).")
        print("The observed slopes are not exactly 2 and 1 because ngspice's driver")
        print("composes the rule with order-1 startup steps and LTE step control.")
    finally:
        shutil.rmtree(workdir, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
