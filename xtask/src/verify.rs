//! Rust-engine verification against committed C data. Never executes C or
//! writes a deck/rawfile. Extend the registry only with demonstrated coverage.

use std::{fs, path::Path};

use ngspice_rs::analysis::{AnalysisRequest, RawFile, RunConfig, runner};
use ngspice_rs::netlist::Parser;
use ngspice_rs::primitives::{AnalysisKind, parse_spice_number};

use crate::{compare, golden, tran, workspace_root};

/// How a fixture's Rust result is compared with its C golden.
enum Gate {
    /// Point-wise by variable name (DC/AC), optionally along a sweep axis.
    Points {
        axis: Option<&'static str>,
        tolerance: compare::Tolerance,
    },
    /// Event-aware comparison on a shared physical time grid (`tran.rs`).
    Transient(compare::TranTolerance),
}

/// An additional Rust-only run of the same deck against the same C golden.
///
/// `extra` tokens are appended to the request after the deck's own settings,
/// so a variant can select a backend that C itself cannot parse (for example
/// the explicit diffsol BDF `backend=diffsol method=bdf`, which ngspice
/// rejects). The deck text, the golden and the tolerance are unchanged, so the
/// variant must reproduce the very same C waveform; it never loosens a bound.
struct Variant {
    label: &'static str,
    extra: &'static [&'static str],
    /// Overrides the fixture's transient tolerance for this run.
    tolerance: compare::TranTolerance,
}

/// The explicit adaptive BDF backend (not ngspice trapezoidal/Gear-2) under the
/// default C-parity bound: for decks whose C reference has no restart artefact
/// exceeding it.
const DIFFSOL_BDF: Variant = Variant {
    label: "diffsol-bdf",
    extra: &["backend=diffsol", "method=bdf"],
    tolerance: compare::TRAN,
};

/// The same backend under `compare::TRAN_RESTART` (peak-scaled), for decks with
/// source corners where C's backward-Euler restart error exceeds `TRAN` at small
/// values; see that constant for the justification.
const DIFFSOL_BDF_RESTART: Variant = Variant {
    label: "diffsol-bdf, peak-scaled",
    extra: &["backend=diffsol", "method=bdf"],
    tolerance: compare::TRAN_RESTART,
};

struct Supported {
    name: &'static str,
    kind: AnalysisKind,
    gate: Gate,
    /// Extra runs against the same golden, in addition to the deck's own run.
    variants: &'static [Variant],
}

/// Transient registry entry: `compare::TRAN`, no variants unless listed.
const fn tran(name: &'static str, variants: &'static [Variant]) -> Supported {
    Supported {
        name,
        kind: AnalysisKind::Transient,
        gate: Gate::Transient(compare::TRAN),
        variants,
    }
}

const SUPPORTED: &[Supported] = &[
    Supported {
        name: "bjt_ce",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "mos_inverter",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "diode_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::LEGACY_DIODE_DC,
        },
        variants: &[],
    },
    Supported {
        name: "m4_diode_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m4_bjt_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m4_mos1_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    tran("m4_diode_tran", &[]),
    tran("m4_bjt_tran", &[]),
    tran("m4_mos1_tran", &[]),
    // MOS1 completion (#88): Meyer gate charge (TOX), series resistance,
    // junction geometry, process extraction and temperature. The transient
    // decks bound the maximum step so that both simulators' discretization
    // error sits well inside `compare::TRAN`; the DC deck tightens RELTOL so
    // that C's own Newton stopping error does not exceed `NONLINEAR` (see
    // docs/port/VERIFICATION.md).
    tran("m7_mos1_inverter_tran", &[]),
    tran("m7_mos1_ring_tran", &[]),
    Supported {
        name: "m7_mos1_meyer_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_mos1_process_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "rc_divider",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    Supported {
        name: "rc_lowpass_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::AC,
        },
        variants: &[],
    },
    tran("rc_transient", &[]),
    Supported {
        name: "rlc_series",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    // Subcircuit elaboration (#18): one `X` instance is flattened through the
    // production `.op` path. The deck is a purely resistive divider, so it keeps
    // the same 1e-12 relative bound as the other linear operating points.
    Supported {
        name: "subckt_divider",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    // `.func` definitions and single-quoted values (#107): top-level and
    // body-local functions, quoted device/instance values, flattened through
    // the production `.op` path. Purely resistive, so the linear DC bound.
    Supported {
        name: "func_quotes",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    // M3 exit-gate fixtures (#48). All use `compare::TRAN`; the physical bound is
    // the simulator's own default accuracy (see `compare.rs`), no fixture-specific
    // tolerance exists. The companion driver (trap, or Gear-2 via
    // `.options method=gear`) runs each deck as written. `DIFFSOL_BDF` variants
    // add the Rust-only BDF tokens: used only where every source corner lies on
    // the `.tran` output grid (the BDF backend emits the requested grid, and the
    // comparator demands a sample at each breakpoint) and only for decks with no
    // `.options method=` (the deck's trap/Gear choice cannot be combined with
    // diffsol and is rejected explicitly by `RunConfig::request`).
    tran("rl_pulse_tran", &[DIFFSOL_BDF_RESTART]),
    tran("rc_gear_tran", &[]),
    tran("rc_pwl_tran", &[DIFFSOL_BDF_RESTART]),
    // The RLC pulse corners restart C's trapezoidal rule with a backward-Euler
    // step, whose error (about 3e-6 V / 2e-6 A near the ringing zero crossings,
    // measured against an independent RK4 solution) exceeds `compare::TRAN`'s
    // 1e-6 V / 1e-12 A near-zero floor; BDF is the more accurate side, so it
    // runs under `compare::TRAN_RESTART`.
    tran("rlc_series_tran", &[DIFFSOL_BDF_RESTART]),
    tran("rlc_series_gear_tran", &[]),
    tran("floating_cap_tran", &[DIFFSOL_BDF]),
    tran("coupled_cap_tran", &[DIFFSOL_BDF_RESTART]),
    // Initialized-state fixtures (#27, #48): `uic` / instance `ic=` / `.ic`. No
    // BDF variants: the diffsol backend deliberately rejects `.ic`, `uic` and
    // instance `ic=` (asserted by a test below). With `uic` C writes no `t = 0`
    // row and adds a breakpoint at the `.tran` step; the Rust driver reproduces
    // both and the comparator (which only demands samples at source breakpoints)
    // needs no special case.
    tran("rc_ic_uic_tran", &[]),
    tran("rlc_ic_uic_tran", &[]),
    tran("rc_ic_node_tran", &[]),
    tran("floating_cap_ic_tran", &[]),
    // M6 source waveforms (#94, #95), companion trap driver under `compare::TRAN`.
    // C sets no breakpoints for SIN/EXP/SFFM/AM, so those decks are compared on
    // the grid only (`tran::breakpoints`); PULSE count and repeated PWL corners
    // are C breakpoints.
    tran("rc_sin_tran", &[]),
    tran("rc_exp_tran", &[]),
    tran("rc_sffm_am_tran", &[]),
    tran("rc_pwl_repeat_tran", &[]),
    tran("rc_pulse_count_tran", &[]),
    Supported {
        name: "rlc_series_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::AC,
        },
        variants: &[],
    }, // `.option` coverage (#110/#107): `gmin={gj}` (evaluated against
    // `.param`) is the junction gmin of the reverse-biased diode/BJT junctions,
    // with itl1/itl2 and documented no-ops; the reverse region is nearly linear,
    // so the 1 ppm NONLINEAR bound applies.
    Supported {
        name: "options_gmin_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    // Trapezoidal `xmu=0.2` and `itl4` on the companion driver. No BDF variant:
    // backend=diffsol rejects both options explicitly.
    tran("options_xmu_tran", &[]),
    // Linear controlled sources (#78): E/F/G/H in an operating point (with an
    // F/H controlled by an E branch and by a voltage source inside a
    // subcircuit), an AC sweep and a PULSE transient. All four devices are
    // linear, so the decks keep the linear DC/AC bounds and `compare::TRAN`.
    Supported {
        name: "controlled_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    Supported {
        name: "controlled_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::AC,
        },
        variants: &[],
    },
    // Its PULSE corners lie on the output grid like `rl_pulse_tran`, and like
    // that deck C's backward-Euler restart after each corner exceeds the plain
    // `compare::TRAN` floor at small values (about 1e-8 A on the sub-uA
    // currents right after the 20 us edge), so the BDF run is peak-scaled.
    tran("controlled_tran", &[DIFFSOL_BDF_RESTART]),
    // K mutual inductance (#80): a 1:2 transformer, a three-winding K inside a
    // subcircuit and a negative coupling in AC; a PULSE transformer transient
    // (trapezoidal companions on the coupled flux); and coupled `ic=` free
    // decay under `uic` with Gear-2. All linear: the linear AC bound and
    // `compare::TRAN`. No BDF variant for `transformer_tran`: the k = 0.99
    // leakage time constant (about 0.6 us) is shorter than C's backward-Euler
    // restart step after each pulse corner, and C's resulting error (1 % of
    // i(l1) at t = 2 us, measured against a reltol = 1e-7 companion run that
    // the BDF result matches to 1e-6) exceeds even `compare::TRAN_RESTART`.
    // The BDF backend's coupled mass matrix is instead checked against that
    // tight reference in `tests/mutual_inductance.rs`.
    Supported {
        name: "transformer_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::AC,
        },
        variants: &[],
    },
    tran("transformer_tran", &[]),
    tran("transformer_ic_uic_tran", &[]),
    tran("transformer_model_uic_tran", &[]),
    // S/W switches (#81): hysteresis bands, ON/OFF flags, gmin off conductance
    // and a W latch at an operating point, and a downward `.dc` sweep whose
    // points continue the previous point's accepted switch state. Switches
    // are nonlinear devices, so the nonlinear 1 ppm bound applies.
    Supported {
        name: "switch_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "switch_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    // PULSE/SIN-controlled S, a self-controlled relaxation oscillator and W
    // switches sensing SIN/PULSE currents. Every plotted node is a source or
    // capacitor node, so no plotted value jumps between samples where a switch
    // flips. No BDF variant: the diffsol backend rejects switches (no
    // immutable linear assembly).
    tran("switch_tran", &[]),
    tran("switch_w_tran", &[]),
    // A `.dc` with a decimal step: C's accumulated sweep values decide
    // switches at their thresholds (`dctrcurv.c` `value += step`).
    Supported {
        name: "switch_dc_decimal",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    // AC uses C's MODEINITSMSIG switch state (the zero CKTstate1: open), not
    // the operating point's.
    Supported {
        name: "switch_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::AC,
        },
        variants: &[],
    },
    // Behavioural sources (#79): B sources over node voltages, branch
    // currents, time and temper in OP, a DC sweep, the AC linearisation at the
    // bias point and a time-dependent transient; E/G VALUE= lowered onto B
    // sources (also inside a subcircuit), E/G TABLE onto the XSPICE pwl
    // transfer and E/G/F/H POLY onto spice2poly (captured with those code
    // models, `* xtask-codemodels:`). Newton-solved, so the nonlinear 1 ppm
    // bound; the transient keeps `compare::TRAN` (the diffsol BDF backend
    // rejects nonlinear devices, so there is no variant).
    Supported {
        name: "bsource_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "bsource_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "bsource_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    tran("bsource_tran", &[]),
    Supported {
        name: "evalue_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "gtable_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "epoly_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    // B sources whose sqrt/log/reciprocal/division slopes are ~1e32 (or whose
    // log() is -1e99) at the 0 V Newton start, in OP, a DC sweep and a
    // transient's initial point (#79); the Newton solve's balanced fallback.
    Supported {
        name: "bsource_zero_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "bsource_zero_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    tran("bsource_zero_tran", &[]),
    // Diode physics (#86): a Zener regulator swept through forward conduction
    // and reverse breakdown, recombination/tunnelling/knee/sidewall currents,
    // a `.dc temp` sweep of the EG/XTI/TLEV/TCV/DTEMP temperature laws, the
    // depletion-charge temperature laws and recombination small signal at
    // `.options temp=100`, and a SIN-driven Zener clipper with junction,
    // sidewall and diffusion charge. Newton-solved: the nonlinear 1 ppm bound;
    // the transient keeps `compare::TRAN`.
    Supported {
        name: "m7_zener_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_diode_physics_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_diode_temp_dc",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    // #97: `.dc @instance[parameter]` (C `param-sweep`) and the `res-sweep`
    // scale of a resistor target.
    Supported {
        name: "m8_dc_param_diode",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m8_dc_param_mos1",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m8_dc_param_gain",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    Supported {
        name: "m8_dc_res_temp",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::DC,
        },
        variants: &[],
    },
    Supported {
        name: "m7_diode_temp_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    tran("m7_zener_tran", &[]),
    // Gummel-Poon BJT (#87): a Gummel plot (VBC = 0 sweep of VBE), nested
    // output characteristics (VCE inner, IB outer), a temperature sweep of
    // NPN/PNP/TLEV=3 devices with series resistances and a substrate junction,
    // and a CE amplifier's AC response and transient with every charge. The
    // DC/AC decks set `.option reltol=1e-8` so that C's convergence test (and
    // bypass) leave a result within the nonlinear 1 ppm bound of the shared
    // root; the transient keeps `compare::TRAN`. No BDF variant: the diffsol
    // backend rejects nonlinear devices.
    Supported {
        name: "m7_bjt_gummel",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_bjt_output",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_bjt_temp",
        kind: AnalysisKind::DcSweep,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_bjt_amp_ac",
        kind: AnalysisKind::Ac,
        gate: Gate::Points {
            axis: Some("frequency"),
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    tran("m7_bjt_amp_tran", &[]),
    // Convergence parity (#106): a BJT latch whose operating point depends on
    // the Newton path (MODEINITJCT start, `DEVpnjlim`, `dynamic_gmin`), the
    // same latch under `.options noopiter` with each of ngspice's other
    // continuation strategies (`gillespie_src`, `spice3_gmin`, `spice3_src`;
    // `spice3_src` and the default land in different states), and a set/reset
    // transient through the regenerative switching. `.option reltol` is tight
    // enough that C's own stopping error stays inside the nonlinear 1 ppm
    // bound; the transient bounds the maximum step and keeps `compare::TRAN`.
    Supported {
        name: "m7_conv_latch_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_conv_latch_gillespie_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_conv_latch_spice3_gmin_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_conv_latch_spice3_src_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    tran("m7_conv_latch_tran", &[]),
    // Nonlinear initial conditions (#99): a diode/RC `uic` start from the
    // `.ic` node vector (the diode's own `ic=` has no effect in C), a BJT
    // flip-flop whose `.ic` selects the state through the forced transient
    // operating point, a BJT flip-flop with an OFF transistor (held through
    // MODEINITJCT/MODEINITFIX until C's device convergence test hands over to
    // dynamic gmin), a MOS1 inverter pair started under `uic` from its IC
    // vectors, and a symmetric CMOS latch whose state a `.nodeset` or MOS1
    // `ic=` start voltages select. The transients keep `compare::TRAN`, the
    // operating points the nonlinear 1 ppm bound; decks tighten `reltol` (and
    // the MOS1 deck uses Gear-2) where C's own step error would be close to
    // the bound.
    tran("m7_ic_diode_uic_tran", &[]),
    tran("m7_ic_bjt_flipflop_tran", &[]),
    tran("m7_ic_bjt_off_tran", &[]),
    tran("m7_ic_mos1_uic_tran", &[]),
    Supported {
        name: "m7_ic_latch_nodeset_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
    Supported {
        name: "m7_ic_latch_mos1_ic_op",
        kind: AnalysisKind::OperatingPoint,
        gate: Gate::Points {
            axis: None,
            tolerance: compare::NONLINEAR,
        },
        variants: &[],
    },
];
/// One plot of a multi-analysis fixture: the analysis type expected at this
/// position of the batch schedule and the gate its plot is compared under.
struct Stage {
    kind: AnalysisKind,
    gate: Gate,
}

/// A multi-analysis fixture (#96): every analysis card of the deck runs in
/// ngspice batch order (`ngspice_rs::analysis::batch::schedule`) and plot `i` is
/// compared with plot `i` of the multi-plot C golden under `stages[i]`. The
/// plot count, the order and every plot name must match exactly; each stage
/// keeps the very tolerance a single-analysis fixture of that type uses.
struct Batch {
    name: &'static str,
    stages: &'static [Stage],
}

const BATCH: &[Batch] = &[
    Batch {
        name: "multi_analysis_rc",
        stages: &[
            Stage {
                kind: AnalysisKind::Ac,
                gate: Gate::Points {
                    axis: Some("frequency"),
                    tolerance: compare::AC,
                },
            },
            Stage {
                kind: AnalysisKind::DcSweep,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::DC,
                },
            },
            Stage {
                kind: AnalysisKind::OperatingPoint,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::DC,
                },
            },
            Stage {
                kind: AnalysisKind::Transient,
                gate: Gate::Transient(compare::TRAN),
            },
        ],
    },
    // The M6 exit gate: a SIN-driven K transformer into a G/E op-amp subcircuit
    // with `.param`/`.func`/`{expr}` values, a B limiter, an H current sense and
    // `.option reltol`, running every analysis card. Nonlinear (the B source), so
    // the DC-type plots and AC keep the nonlinear 1 ppm bound and the transient
    // `compare::TRAN`.
    Batch {
        name: "m6_gate",
        stages: &[
            Stage {
                kind: AnalysisKind::Ac,
                gate: Gate::Points {
                    axis: Some("frequency"),
                    tolerance: compare::NONLINEAR,
                },
            },
            Stage {
                kind: AnalysisKind::DcSweep,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::NONLINEAR,
                },
            },
            Stage {
                kind: AnalysisKind::OperatingPoint,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::NONLINEAR,
                },
            },
            Stage {
                kind: AnalysisKind::Transient,
                gate: Gate::Transient(compare::TRAN),
            },
        ],
    },
    // Convergence parity (#106): Schmitt triggers swept up and down through
    // their hysteresis (each `.dc` point warm-starts from the previous one,
    // as `dctrcurv.c` does, so the two sweeps follow different branches) and
    // an operating point inside the band, which C's (and the port's) CKTop
    // continuation settles on the middle branch for the BJT deck. ngspice
    // runs the sweeps before the operating point and the later `.dc` card
    // first. Nonlinear 1 ppm bound with tightened RELTOL/VNTOL.
    Batch {
        name: "m7_conv_bjt_schmitt",
        stages: &[
            Stage {
                kind: AnalysisKind::DcSweep,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::NONLINEAR,
                },
            },
            Stage {
                kind: AnalysisKind::DcSweep,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::NONLINEAR,
                },
            },
            Stage {
                kind: AnalysisKind::OperatingPoint,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::NONLINEAR,
                },
            },
        ],
    },
    Batch {
        name: "m7_conv_cmos_schmitt",
        stages: &[
            Stage {
                kind: AnalysisKind::DcSweep,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::NONLINEAR,
                },
            },
            Stage {
                kind: AnalysisKind::DcSweep,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::NONLINEAR,
                },
            },
            Stage {
                kind: AnalysisKind::OperatingPoint,
                gate: Gate::Points {
                    axis: None,
                    tolerance: compare::NONLINEAR,
                },
            },
        ],
    },
];

/// Fixtures whose deck the Rust engine deliberately does not run yet. Empty:
/// every committed deck, including `subckt_divider`, is verified through its
/// own production path. A requested excluded fixture still fails the run, so a
/// future entry cannot be reported as a success by accident.
const EXCLUDED: &[(&str, &str)] = &[];

pub(crate) fn main(arguments: &[String]) -> Result<(), String> {
    let only = match arguments {
        [] => None,
        [flag, name] if flag == "--netlist" && !name.starts_with('-') => Some(name.as_str()),
        _ => return Err("usage: cargo xtask golden verify [--netlist <NAME>]".into()),
    };
    run(&workspace_root(), only)
}

fn run(root: &Path, only: Option<&str>) -> Result<(), String> {
    run_with_registries(root, only, SUPPORTED, BATCH, EXCLUDED)
}

/// [`run_with_registries`] without multi-analysis fixtures.
#[cfg(test)]
fn run_with_registry(
    root: &Path,
    only: Option<&str>,
    supported: &[Supported],
    excluded: &[(&str, &str)],
) -> Result<(), String> {
    run_with_registries(root, only, supported, &[], excluded)
}

/// The verification loop, parameterized by the fixture registry.
///
/// `EXCLUDED` is empty while every committed deck verifies, so the
/// `requested unsupported fixture` path has no committed fixture to exercise it;
/// this signature lets a unit test drive that branch with a synthetic entry
/// instead of leaving it untested until an excluded fixture reappears.
fn run_with_registries(
    root: &Path,
    only: Option<&str>,
    supported: &[Supported],
    batch: &[Batch],
    excluded: &[(&str, &str)],
) -> Result<(), String> {
    let paths = golden::netlist_paths_at(root, only)?;
    let mut verified = 0;
    let mut unsupported = 0;
    let mut failures = Vec::new();
    if only.is_none() {
        for name in supported
            .iter()
            .map(|fixture| fixture.name)
            .chain(batch.iter().map(|fixture| fixture.name))
        {
            if !paths
                .iter()
                .any(|path| path.file_stem().is_some_and(|stem| stem == name))
            {
                failures.push(format!("missing supported fixture '{name}'"));
            }
        }
    }
    for path in paths {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("invalid fixture name")?;
        if let Some(fixture) = supported.iter().find(|fixture| fixture.name == name) {
            match fixture_result(root, &path, fixture) {
                Ok(details) => {
                    verified += 1;
                    println!("  verified   {name}");
                    for detail in details {
                        println!("             {detail}");
                    }
                }
                Err(error) => {
                    println!("  FAIL       {name}: {error}");
                    failures.push(format!("{name}: {error}"));
                }
            }
        } else if let Some(fixture) = batch.iter().find(|fixture| fixture.name == name) {
            match batch_result(root, &path, fixture) {
                Ok(details) => {
                    verified += 1;
                    println!("  verified   {name}");
                    for detail in details {
                        println!("             {detail}");
                    }
                }
                Err(error) => {
                    println!("  FAIL       {name}: {error}");
                    failures.push(format!("{name}: {error}"));
                }
            }
        } else if let Some((_, reason)) = excluded.iter().find(|(fixture, _)| *fixture == name) {
            unsupported += 1;
            println!("  unsupported {name}: {reason}");
            if only.is_some() {
                failures.push(format!("requested unsupported fixture '{name}': {reason}"));
            }
        } else {
            failures.push(format!(
                "fixture '{name}' has no verification registry entry"
            ));
        }
    }
    println!(
        "\n{verified} verified fixture(s), {unsupported} unsupported fixture(s), {} failure(s); bounded coverage, not full corpus parity",
        failures.len()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("verification failed: {}", failures.join("\n")))
    }
}

/// Detail lines for a verified fixture: one per run (the deck's own, then each
/// variant).
fn fixture_result(root: &Path, path: &Path, fixture: &Supported) -> Result<Vec<String>, String> {
    let mut details = vec![run_variant(root, path, fixture, None)?];
    for variant in fixture.variants {
        let detail = run_variant(root, path, fixture, Some(variant))
            .map_err(|error| format!("variant {}: {error}", variant.label))?;
        details.push(format!("[{}] {detail}", variant.label));
    }
    Ok(details)
}

/// One Rust run of the deck (optionally with a [`Variant`]'s extra request
/// tokens) compared with the committed C golden. Deck `.option` cards are
/// applied through `RunConfig`, exactly as for an ordinary run.
fn run_variant(
    root: &Path,
    path: &Path,
    fixture: &Supported,
    variant: Option<&Variant>,
) -> Result<String, String> {
    let netlist = Parser::new().parse_file(path).map_err(|e| e.to_string())?;
    if netlist.analyses.len() != 1 {
        return Err(format!(
            "expected exactly one analysis, found {}",
            netlist.analyses.len()
        ));
    }
    let config = RunConfig::from_netlist(&netlist).map_err(|e| e.to_string())?;
    let extra = variant.map_or(&[][..], |variant| variant.extra);
    let (request, got) = run_card(&netlist, &config, &netlist.analyses[0], extra)?;
    if request.kind != fixture.kind {
        return Err(format!(
            "registry expects {:?}, deck requests {:?}",
            fixture.kind, request.kind
        ));
    }
    let want = load_golden(root, fixture.name)?;
    if want.plots.len() != 1 {
        return Err(format!("expected one C plot, found {}", want.plots.len()));
    }
    let gate = match (&fixture.gate, variant) {
        (Gate::Transient(_), Some(variant)) => Gate::Transient(variant.tolerance),
        (Gate::Transient(tolerance), None) => Gate::Transient(*tolerance),
        (Gate::Points { axis, tolerance }, _) => Gate::Points {
            axis: *axis,
            tolerance: *tolerance,
        },
    };
    compare_plot(&gate, &netlist, &request, &got, &want.plots[0].plot)
}

/// Every plot of a multi-analysis fixture, in batch order, against the
/// multi-plot C golden. Each analysis runs on a freshly elaborated circuit, as
/// `spice-rs simulate` does.
fn batch_result(root: &Path, path: &Path, fixture: &Batch) -> Result<Vec<String>, String> {
    let netlist = Parser::new().parse_file(path).map_err(|e| e.to_string())?;
    let config = RunConfig::from_netlist(&netlist).map_err(|e| e.to_string())?;
    let schedule = ngspice_rs::analysis::batch::schedule(&netlist.analyses);
    let want = load_golden(root, fixture.name)?;
    if schedule.len() != fixture.stages.len() || want.plots.len() != fixture.stages.len() {
        return Err(format!(
            "expected {} plots: the deck schedules {}, the C golden has {}",
            fixture.stages.len(),
            schedule.len(),
            want.plots.len()
        ));
    }
    let mut details = Vec::with_capacity(schedule.len());
    for ((entry, stage), want) in schedule.iter().zip(fixture.stages).zip(&want.plots) {
        let card = &netlist.analyses[entry.card_index];
        let (request, got) = run_card(&netlist, &config, card, &[])
            .map_err(|error| format!("{}: {error}", entry.plot_name))?;
        if request.kind != stage.kind {
            return Err(format!(
                "{}: registry expects {:?}, the batch schedule runs {:?}",
                entry.plot_name, stage.kind, request.kind
            ));
        }
        if got.plotname != want.plot.plotname {
            return Err(format!(
                "{}: plot name '{}' where C wrote '{}' at this position",
                entry.plot_name, got.plotname, want.plot.plotname
            ));
        }
        let detail = compare_plot(&stage.gate, &netlist, &request, &got, &want.plot)
            .map_err(|error| format!("{}: {error}", entry.plot_name))?;
        details.push(format!("{} ({}): {detail}", entry.plot_name, got.plotname));
    }
    Ok(details)
}

/// Runs one analysis card through the production driver and projects the plot
/// onto C's default save set and naming. `extra` tokens are appended to the
/// request after the deck's own settings.
fn run_card(
    netlist: &ngspice_rs::netlist::ast::Netlist,
    config: &RunConfig,
    card: &ngspice_rs::netlist::ast::AnalysisCard,
    extra: &[&str],
) -> Result<(AnalysisRequest, ngspice_rs::analysis::Plot), String> {
    if !card.expressions.is_empty() {
        return Err("braced analysis arguments are not supported in verification fixtures".into());
    }
    let mut request: AnalysisRequest = AnalysisRequest::from(card);
    request
        .arguments
        .extend(extra.iter().map(|token| (*token).to_owned()));
    let request = config.request(request).map_err(|e| e.to_string())?;
    let mut circuit = config.circuit(netlist).map_err(|e| e.to_string())?;
    let mut got = runner(request.kind)
        .and_then(|driver| driver.run(&mut circuit, &request, &config.context()))
        .map_err(|e| e.to_string())?;
    // C's default save set omits simulator-created internal nodes (e.g. a
    // diode's series-resistance anode). Project only those known internal rows;
    // every externally visible variable still goes through exact set checks.
    let internal: Vec<_> = circuit
        .nodes()
        .nodes()
        .iter()
        .filter(|node| node.kind == ngspice_rs::primitives::NodeKind::Internal)
        .map(|node| format!("v({})", node.name))
        .collect();
    for column in (0..got.variables.len()).rev() {
        if internal.contains(&got.variables[column].name) {
            got.variables.remove(column);
            for row in &mut got.points {
                row.remove(column);
            }
        }
    }
    if request.kind == AnalysisKind::DcSweep
        && got.variables.first().is_some_and(|v| v.name == "sweep")
    {
        // Rust's public DC scale name predates the nonlinear gate; C wraps its
        // independent-source scale in the voltage/current naming convention,
        // and names a temperature scale (and its unit) `temp-sweep`, a
        // resistance scale `res-sweep` and an `@inst[param]` scale
        // `param-sweep`, which its rawfile writes as a voltage
        // (`dctrcurv.c`).
        let (name, unit) = match got.variables[0].unit.as_str() {
            "voltage" => ("v(v-sweep)", None),
            "temperature" => ("temp-sweep", Some("temp-sweep")),
            "resistance" => ("res-sweep", Some("res-sweep")),
            "parameter" => ("v(param-sweep)", Some("voltage")),
            _ => ("i(i-sweep)", None),
        };
        got.variables[0].name = name.into();
        if let Some(unit) = unit {
            got.variables[0].unit = unit.into();
        }
        // A nested sweep's outer value is a Rust-only `sweep(<name>)` column;
        // C writes every point of a nested `.dc` without it (the outer source
        // value is still checked through the node voltages and currents).
        for column in (1..got.variables.len()).rev() {
            if got.variables[column].name.starts_with("sweep(") {
                got.variables.remove(column);
                for row in &mut got.points {
                    row.remove(column);
                }
            }
        }
    }
    Ok((request, got))
}

/// The committed C golden of `name`.
fn load_golden(root: &Path, name: &str) -> Result<RawFile, String> {
    let target = root.join(golden::GOLDEN_DIR).join(format!("{name}.raw"));
    let text =
        fs::read_to_string(&target).map_err(|e| format!("reading {}: {e}", target.display()))?;
    RawFile::parse(&text).map_err(|e| format!("{}: {e}", target.display()))
}

/// Compares one Rust plot with one C plot under `gate`.
fn compare_plot(
    gate: &Gate,
    netlist: &ngspice_rs::netlist::ast::Netlist,
    request: &AnalysisRequest,
    got: &ngspice_rs::analysis::Plot,
    want: &ngspice_rs::analysis::Plot,
) -> Result<String, String> {
    match *gate {
        Gate::Points { axis, tolerance } => compare::plots(got, want, tolerance, axis)
            .map(|()| format!("{} point(s)", got.point_count())),
        Gate::Transient(tolerance) => {
            let time = |index: usize, what: &str| {
                request
                    .argument(index)
                    .and_then(parse_spice_number)
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .ok_or_else(|| format!(".tran {what} is not a positive number"))
            };
            let (step, stop) = (time(0, "tstep")?, time(1, "tstop")?);
            let breakpoints = tran::breakpoints(netlist, step, stop)?;
            // With `uic` C writes no t = 0 row: its first row is the first
            // accepted step. The comparison then starts at that time, which the
            // Rust plot must reproduce (`tran::Series::new`); without `uic`
            // both plots start at 0 as always.
            let start = if request.uic {
                want.value("time", 0)
                    .map(|t| t.re)
                    .filter(|t| t.is_finite() && *t > 0.0)
                    .ok_or("uic golden has no positive first time")?
            } else {
                0.0
            };
            tran::transient(
                got,
                want,
                tolerance,
                &tran::Grid { start, stop, step },
                &breakpoints,
            )
            .map(|summary| {
                format!(
                    "{} instants + {} breakpoint limits, worst error {:.3} of bound",
                    summary.instants, summary.limits, summary.worst_ratio
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Temp(std::path::PathBuf);
    impl Temp {
        fn divider() -> Self {
            Self::with(&["rc_divider"])
        }
        fn with(names: &[&str]) -> Self {
            static ID: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "spice-verify-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join(golden::NETLIST_DIR)).unwrap();
            fs::create_dir_all(root.join(golden::GOLDEN_DIR)).unwrap();
            for name in names {
                for path in [
                    format!("conformance/netlists/{name}.cir"),
                    format!("conformance/golden/{name}.raw"),
                ] {
                    fs::copy(workspace_root().join(&path), root.join(&path)).unwrap();
                }
            }
            Self(root)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn production_fixtures_pass_without_modifying_inputs() {
        let paths: Vec<_> = golden::netlist_paths_at(&workspace_root(), None)
            .unwrap()
            .into_iter()
            .flat_map(|p| {
                let raw = workspace_root()
                    .join(golden::GOLDEN_DIR)
                    .join(format!("{}.raw", p.file_stem().unwrap().to_str().unwrap()));
                [p, raw]
            })
            .collect();
        let before: Vec<_> = paths.iter().map(|p| fs::read(p).unwrap()).collect();
        run(&workspace_root(), None).unwrap();
        // The full run already verifies every fixture; a selected run per
        // registry kind covers `--netlist` dispatch without a second pass.
        run(&workspace_root(), Some(SUPPORTED[0].name)).unwrap();
        run(&workspace_root(), Some(BATCH[0].name)).unwrap();
        for (path, bytes) in paths.iter().zip(before) {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }

    #[test]
    fn rust_only_variants_select_the_explicit_diffsol_bdf_backend() {
        let mut with_variants = 0;
        for fixture in SUPPORTED {
            for variant in fixture.variants {
                with_variants += 1;
                assert_eq!(fixture.kind, AnalysisKind::Transient, "{}", fixture.name);
                assert_eq!(variant.extra, ["backend=diffsol", "method=bdf"]);
                // Trap/Gear selections in the deck cannot be combined with BDF
                // (`RunConfig::request` rejects them); the fixture must not set one.
                let deck = fs::read_to_string(
                    workspace_root().join(format!("conformance/netlists/{}.cir", fixture.name)),
                )
                .unwrap();
                assert!(!deck.to_ascii_lowercase().contains("method"), "{deck}");
            }
        }
        assert_eq!(with_variants, 6);
        // The same deck with a deck-level Gear selection and the BDF tokens is an
        // explicit error, never a silent downgrade.
        let temp = Temp::with(&["rc_gear_tran"]);
        let fixture = Supported {
            variants: &[DIFFSOL_BDF],
            ..tran("rc_gear_tran", &[])
        };
        let path = temp.0.join("conformance/netlists/rc_gear_tran.cir");
        let error = fixture_result(&temp.0, &path, &fixture).unwrap_err();
        assert!(error.contains("variant diffsol-bdf"), "{error}");
    }

    #[test]
    fn bdf_variants_of_initialized_state_decks_are_rejected_explicitly() {
        // The diffsol backend deliberately has no `.ic`/`uic`/`ic=` support, so
        // the initialized-state fixtures register no BDF variant, and asking for
        // one is an explicit error rather than a silent downgrade.
        for name in [
            "rc_ic_uic_tran",
            "rlc_ic_uic_tran",
            "rc_ic_node_tran",
            "floating_cap_ic_tran",
        ] {
            let entry = SUPPORTED.iter().find(|f| f.name == name).unwrap();
            assert!(entry.variants.is_empty(), "{name}");
            let path = workspace_root().join(format!("conformance/netlists/{name}.cir"));
            let fixture = Supported {
                variants: &[DIFFSOL_BDF],
                ..tran(name, &[])
            };
            let error = fixture_result(&workspace_root(), &path, &fixture).unwrap_err();
            assert!(error.contains("variant diffsol-bdf"), "{name}: {error}");
            assert!(
                error.contains("unsupported") || error.contains("uic") || error.contains(".ic"),
                "{name}: {error}"
            );
        }
    }

    #[test]
    fn corrupted_transient_goldens_fail_both_the_companion_and_bdf_runs() {
        let temp = Temp::with(&["floating_cap_tran"]);
        let raw = temp.0.join("conformance/golden/floating_cap_tran.raw");
        let original = fs::read_to_string(&raw).unwrap();
        run(&temp.0, Some("floating_cap_tran")).unwrap();
        // Perturb v(b) of point 600 (t ~ 6 ms, a smooth interior sample) by 1 mV:
        // far above the 1e-3 relative + 1 uV bound at v(b) ~ 1e-3 V.
        let mut lines: Vec<String> = original.lines().map(String::from).collect();
        let start = lines
            .iter()
            .position(|line| line.starts_with(" 600\t"))
            .unwrap();
        let name = lines
            .iter()
            .position(|l| l.ends_with("v(b)\tvoltage"))
            .unwrap();
        let column: usize = lines[name]
            .trim_start()
            .split('\t')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let line = &mut lines[start + column];
        let value: f64 = line.trim().parse().unwrap();
        *line = format!("\t{:.15e}", value + 1e-3);
        fs::write(&raw, lines.join("\n") + "\n").unwrap();
        let error = run(&temp.0, Some("floating_cap_tran")).unwrap_err();
        assert!(error.contains("transient mismatch"), "{error}");
        // The BDF variant alone also detects it.
        let path = temp.0.join("conformance/netlists/floating_cap_tran.cir");
        let fixture = tran("floating_cap_tran", &[]);
        assert!(run_variant(&temp.0, &path, &fixture, Some(&DIFFSOL_BDF)).is_err());
    }

    #[test]
    fn requested_unsupported_unknown_and_bad_options_fail() {
        // `EXCLUDED` is empty now that every committed deck is verified. Drive
        // the excluded branch with a synthetic entry so the
        // `requested unsupported fixture` report stays tested, rather than
        // waiting for a future excluded fixture to exercise it.
        let error = run_with_registry(
            &workspace_root(),
            Some("rc_divider"),
            &[],
            &[("rc_divider", "synthetic exclusion")],
        )
        .unwrap_err();
        assert!(
            error.contains("requested unsupported fixture 'rc_divider': synthetic exclusion"),
            "{error}"
        );
        for (name, _) in EXCLUDED {
            assert!(
                run(&workspace_root(), Some(name))
                    .unwrap_err()
                    .contains("requested unsupported")
            );
        }
        assert!(run(&workspace_root(), Some("unknown")).is_err());
        for args in [
            vec!["--netlist"],
            vec!["--ngspice", "anything"],
            vec!["--verbose"],
            vec!["--netlist", "--verbose"],
            vec!["--netlist", "rc_divider", "extra"],
        ] {
            assert!(main(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }

    #[test]
    fn corrupted_missing_and_multi_plot_goldens_fail_through_verification() {
        let temp = Temp::divider();
        let raw = temp.0.join("conformance/golden/rc_divider.raw");
        let original = fs::read_to_string(&raw).unwrap();
        assert!(
            run(&temp.0, None)
                .unwrap_err()
                .contains("missing supported fixture")
        );
        run(&temp.0, Some("rc_divider")).unwrap();
        for text in [
            original.replace("2.500000000000000e+00", "2.600000000000000e+00"),
            original.replace("v(out)", "v(wrong)"),
            original.replace("2.500000000000000e+00", "NaN"),
            "not a rawfile".into(),
            format!("{original}{original}"),
        ] {
            fs::write(&raw, text).unwrap();
            assert!(run(&temp.0, Some("rc_divider")).is_err());
        }
        fs::remove_file(&raw).unwrap();
        assert!(
            run(&temp.0, Some("rc_divider"))
                .unwrap_err()
                .contains("reading")
        );
    }

    #[test]
    fn deck_errors_are_not_silently_skipped() {
        let temp = Temp::divider();
        let deck = temp.0.join("conformance/netlists/rc_divider.cir");
        let original = fs::read_to_string(&deck).unwrap();
        for text in [
            original.replace(".op", ""),
            original.replace(".op", ".op\n.op"),
            original.replace(".op", ".ac lin 3 100 1k"),
            original.replace("r2 out 0 1k", "r2 out nowhere 1k"),
            original.replace("r2 out 0 1k", "r2 out 0 {expr}"),
        ] {
            fs::write(&deck, text).unwrap();
            assert!(run(&temp.0, Some("rc_divider")).is_err());
        }
        fs::write(
            temp.0.join("conformance/netlists/new_fixture.cir"),
            original,
        )
        .unwrap();
        assert!(
            run(&temp.0, Some("new_fixture"))
                .unwrap_err()
                .contains("no verification registry")
        );
    }

    #[test]
    fn multi_plot_goldens_are_verified_plot_by_plot_and_in_order() {
        let temp = Temp::with(&["multi_analysis_rc"]);
        let raw = temp.0.join("conformance/golden/multi_analysis_rc.raw");
        let deck = temp.0.join("conformance/netlists/multi_analysis_rc.cir");
        let original = fs::read_to_string(&raw).unwrap();
        let original_deck = fs::read_to_string(&deck).unwrap();
        run(&temp.0, Some("multi_analysis_rc")).unwrap();
        // The C plots, split at their `Title:` headers.
        let plots: Vec<String> =
            original
                .split_inclusive('\n')
                .fold(Vec::<String>::new(), |mut plots, line| {
                    if line.starts_with("Title:") || plots.is_empty() {
                        plots.push(String::new());
                    }
                    plots.last_mut().unwrap().push_str(line);
                    plots
                });
        assert_eq!(plots.len(), 4);
        let swapped = [&plots[1], &plots[0], &plots[2], &plots[3]]
            .map(String::as_str)
            .concat();
        let dropped = plots[..3].concat();
        let doubled = format!("{original}{}", plots[3]);
        // v(out) of the operating point: 4/3 V -> 1.4 V.
        let op_value = original.replacen("\t1.333333333333333e+00", "\t1.400000000000000e+00", 1);
        assert_ne!(op_value, original);
        for (label, text) in [
            ("swapped", swapped),
            ("dropped", dropped),
            ("doubled", doubled),
            ("op value", op_value),
        ] {
            fs::write(&raw, text).unwrap();
            let error = run(&temp.0, Some("multi_analysis_rc")).unwrap_err();
            assert!(error.contains("multi_analysis_rc"), "{label}: {error}");
        }
        fs::write(&raw, &original).unwrap();
        // A deck whose schedule no longer matches the registered stages fails
        // too, rather than being compared against the wrong plots.
        for text in [
            original_deck.replace(".op\n", ""),
            original_deck.replace(".op\n", ".op\n.op\n"),
        ] {
            fs::write(&deck, text).unwrap();
            assert!(run(&temp.0, Some("multi_analysis_rc")).is_err());
        }
    }
}
