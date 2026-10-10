//! JFET level 2, Parker-Skellern (#82 part 2, M10 slice 6): model selection
//! and validation, regions against an independent square-law reduction of
//! `psmodel.c`, thermal reduction, finite-difference Jacobians, the filtered
//! (gate-lag / self-heating) state in transient, observations and explicit
//! gaps. C-golden comparisons live in `xtask golden verify` (`m10_jfet2_*`)
//! and `tests/c_jfet2_reference.rs`.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::{AnalysisMode, Circuit, LoadRequest, ModelContext};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::{Parser, source::parse_deck_text, write_netlist};
use ngspice_rs::primitives::{AnalysisKind, SpiceError};
use std::path::Path;

const GMIN: f64 = 1e-12;

fn deck(body: &str) -> Result<ngspice_rs::netlist::ast::Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("jfet2.cir"),
        &format!("JFET2\n{body}\n.end\n"),
    ))
}
fn circuit(body: &str) -> Result<Circuit, SpiceError> {
    Circuit::from_netlist(&deck(body)?)
}
fn run(c: &mut Circuit, kind: AnalysisKind, arguments: &[&str]) -> SpiceResult<Plot> {
    runner(kind).unwrap().run(
        c,
        &AnalysisRequest::with_arguments(kind, arguments.iter().copied()),
        &AnalysisContext::default(),
    )
}
type SpiceResult<T> = Result<T, SpiceError>;
fn value(plot: &Plot, name: &str, point: usize) -> f64 {
    let index = plot
        .variable_index(name)
        .unwrap_or_else(|| panic!("missing {name}: {:?}", plot.variables));
    plot.points[point][index].re
}
fn close(a: f64, b: f64, relative: f64, absolute: f64) {
    assert!(
        (a - b).abs() <= relative * b.abs() + absolute,
        "{a:e} != {b:e}"
    );
}

/// `P = Q = 2`, `VST = 0`, `Z = 0` and a huge `XI` reduce `PSids` to the
/// Shichman-Hodges square law `beta (1 + lambda vds) vds (2 vgst - vds)`
/// (linear) / `beta (1 + lambda vds) vgst^2` (saturated): with `Z = 0` the
/// early-saturation knee is `vdt = min(vds, vsat)` and `vsat -> vgst`.
fn square_law(beta: f64, lambda: f64, vto: f64, vgs: f64, vds: f64) -> f64 {
    let vgst = vgs - vto;
    if vgst <= 0. {
        0.
    } else if vgst <= vds {
        beta * (1. + lambda * vds) * vgst * vgst
    } else {
        beta * (1. + lambda * vds) * vds * (2. * vgst - vds)
    }
}
const SQUARE: &str = "level=2 p=2 q=2 vst=0 z=0 xi=1e9";

#[test]
fn level_selection_and_model_validation_are_explicit() {
    let base = "vd d 0 1\nvg g 0 0\nj1 d g 0 jm\n";
    for model in [
        "njf level=2",
        "pjf(level=2 vbi=0.8 pb=0.7 vt0=-1 vto=-1.5 hfgam=0.1 lfgam=0.2 ver=3)",
        "njf(level=2 taud=1u taug=2u delta=0.1 cds=1p ibd=1n vbd=3 af=1 kf=1e-15)",
    ] {
        circuit(&format!("{base}.model jm {model}")).unwrap_or_else(|e| panic!("{model}: {e}"));
    }
    let level3 = circuit(&format!("{base}.model jm njf level=3")).unwrap_err();
    assert!(level3.is_not_yet_ported(), "{level3}");
    for (model, what) in [
        ("njf(level=2 b=0.7)", "level-1 setter"),
        ("njf(level=2 tcv=1m)", "level-1 temperature setter"),
        ("njf(level=2 foo=1)", "unknown setter"),
        ("njf(level=2 fc=0.96)", "FC beyond jfet2temp.c's clamp"),
        ("njf(level=2 vbd=0)", "VBD division"),
        ("njf(level=2 xi=0)", "XI division"),
        ("njf(level=2 q=0)", "Q power law"),
        ("njf(level=2 taug=-1n)", "negative relaxation time"),
        ("njf(level=2 is=0)", "nonpositive IS"),
        ("njf(level=2 pb=1 vto=1 p=3)", "VBI = VTO with P != Q (D3)"),
    ] {
        assert!(
            circuit(&format!("{base}.model jm {model}")).is_err(),
            "{what}: {model}"
        );
    }
}

#[test]
fn regions_of_both_polarities_reduce_to_the_square_law() {
    let (beta, lambda, vto) = (1.2e-3, 0.02, -2.);
    for (vgs, vds) in [(-3., 2.), (-1., 0.5), (-1., 3.), (0., 1.), (0., 4.)] {
        for (family, pol) in [("njf", 1.), ("pjf", -1.)] {
            let body = format!(
                "vd d 0 {}\nvg g 0 {}\nj1 d g 0 jm\n\
                 .model jm {family}({SQUARE} vto={vto} beta={beta} lambda={lambda})",
                pol * vds,
                pol * vgs,
            );
            let mut c = circuit(&body).unwrap();
            let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
            // The reverse-biased gate-drain diode leaks -IS + gmin vgd.
            let leakage = -1e-14 + GMIN * (vgs - vds);
            let expected = square_law(beta, lambda, vto, vgs, vds) - leakage;
            close(-pol * value(&plot, "i(vd)", 0), expected, 1e-7, 1e-15);
        }
    }
}

#[test]
fn inverse_mode_mirrors_the_normal_mode_current() {
    let model = ".model jm njf(level=2 vto=-2 beta=1m lambda=0.05 vst=0.1 mvst=0.2 xi=5 \
                 z=0.4 p=2.3 q=2.1 lfgam=0.02 delta=0.3)";
    let forward = format!("vd d 0 1.5\nvg g 0 -0.5\nj1 d g 0 jm\n{model}");
    let reverse = format!("vd d 0 1.5\nvg g 0 -0.5\nj1 0 g d jm\n{model}");
    let current = |body: &str| {
        let mut c = circuit(body).unwrap();
        -value(
            &run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap(),
            "i(vd)",
            0,
        )
    };
    close(current(&reverse), current(&forward), 1e-12, 1e-18);
}

#[test]
fn thermal_reduction_divides_by_one_plus_delta_power() {
    // psmodel.c: ids = I0 / (1 + DELTA/area * vds * I0) with I0 the
    // channel current at DELTA = 0 (area 2 doubles I0 and halves DELTA).
    let (vgs, vds, delta) = (-0.5, 3., 0.4);
    let body = |delta: f64| {
        format!(
            "vd d 0 {vds}\nvg g 0 {vgs}\nj1 d g 0 jm 2\n\
             .model jm njf({SQUARE} vto=-2 beta=1m delta={delta})"
        )
    };
    let channel = |delta: f64| {
        let mut c = circuit(&body(delta)).unwrap();
        let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
        // Remove the reverse gate-drain leakage -2 IS + gmin vgd.
        -value(&plot, "i(vd)", 0) + (-2e-14 + GMIN * (vgs - vds))
    };
    let cold = channel(0.);
    close(cold, 2e-3 * 1.5 * 1.5, 1e-7, 0.);
    close(
        channel(delta),
        cold / (1. + delta / 2. * vds * cold),
        1e-9,
        1e-15,
    );
}

/// `r(x) = A(x) x - b(x)` of one load, for finite differences.
fn residual(a: &SparseMatrix, b: &Vector, x: &Vector) -> Vec<f64> {
    let mut r: Vec<f64> = b.as_slice().iter().map(|v| -v).collect();
    for t in a.triplets() {
        r[t.row] += t.value * x.as_slice()[t.col];
    }
    r
}
fn entry(matrix: &SparseMatrix, r: usize, col: usize) -> f64 {
    matrix
        .triplets()
        .iter()
        .filter(|t| t.row == r && t.col == col)
        .map(|t| t.value)
        .sum()
}

const FULL: &str = "vto=-2 beta=1.2m lambda=0.03 rd=20 rs=15 is=1e-14 n=1.1 vst=0.08 \
                    mvst=0.1 xi=12 mxi=0.05 z=0.6 p=2.2 q=2.05 delta=0.2 lfgam=0.04 \
                    lfg1=0.01 lfg2=0.006 hfgam=0.02 hfg1=0.01 hfg2=0.003 hfeta=0.02 \
                    hfe1=0.01 hfe2=0.004 ibd=1e-9 vbd=1.5 cgs=2p cgd=1p cds=0.3p";

#[test]
fn the_dc_jacobian_matches_finite_differences_over_a_bias_grid() {
    let pmodel = "jm pjf(level=2 vto=-1.5 beta=0.8m lambda=0.02 rs=10 vst=0.05 delta=0.1 \
                  lfgam=0.03 xi=50)";
    let mut bodies = Vec::new();
    for (vd, vg) in [
        (3., -1.),   // saturation
        (0.4, -0.5), // linear
        (-0.6, -1.), // inverse linear
        (-3., -2.5), // inverse saturation
        (1., -2.6),  // subthreshold
        (0.2, 0.55), // forward-biased gate
        (6., -4.),   // reverse gate breakdown conduction
    ] {
        bodies.push(format!(
            "vd d 0 {vd}\nvg g 0 {vg}\nj1 d g 0 jm 1.5 m=2 temp=70\n\
             .model jm njf(level=2 {FULL})"
        ));
        bodies.push(format!(
            "vd d 0 {}\nvg g 0 {}\nj1 d g 0 jm dtemp=-15\n.model {pmodel}",
            -vd, -vg
        ));
    }
    for body in bodies {
        let c = circuit(&body).unwrap();
        let context = ModelContext::default();
        let history = c.state_history();
        let solved = ngspice_rs::analysis::bias::solve_dc(
            &c,
            &context,
            &Default::default(),
            &[],
            None,
            None,
        )
        .unwrap();
        let system = c.small_signal_system(&context, &solved.values).unwrap();
        let n = c.unknown_count();
        let load = |x: &Vector| {
            let mut a = SparseMatrix::new(n, n);
            let mut b = Vector::zeros(n);
            let mut trial = history.trial();
            c.load(
                &LoadRequest {
                    mode: AnalysisMode::OperatingPoint,
                    solution: x,
                    model_context: &context,
                    integration: None,
                    history: &history,
                    forcing: None,
                },
                &mut a,
                &mut b,
                &mut trial,
            )
            .unwrap();
            (a, b)
        };
        let (newton, _) = load(&solved.values);
        for node in c.nodes().nodes() {
            let Some(col) = c.unknowns().node_row(node.id) else {
                continue;
            };
            let h = 1e-6;
            let mut low = solved.values.clone();
            let mut high = solved.values.clone();
            low.as_mut_slice()[col] -= h;
            high.as_mut_slice()[col] += h;
            let ((al, bl), (ah, bh)) = (load(&low), load(&high));
            let (rl, rh) = (residual(&al, &bl, &low), residual(&ah, &bh, &high));
            for r in 0..n {
                let numerical = (rh[r] - rl[r]) / (2. * h);
                close(numerical, entry(&system.a, r, col), 2e-6, 1e-10);
                close(numerical, entry(&newton, r, col), 2e-6, 1e-10);
            }
        }
    }
}

#[test]
fn ac_without_time_constants_is_frequency_independent_conductance() {
    // With TAUG = TAUD = 0 PSacload returns the DC gm/gds, so E holds only
    // the Statz gate capacitances and CDS (g row: m (cgs + cgd)).
    let body = format!("vd d 0 5\nvg g 0 -1\nj1 d g 0 jm m=2\n.model jm njf(level=2 {FULL})");
    let c = circuit(&body).unwrap();
    let solved = ngspice_rs::analysis::bias::solve_dc(
        &c,
        &ModelContext::default(),
        &Default::default(),
        &[],
        None,
        None,
    )
    .unwrap();
    let low = c
        .small_signal_system(&ModelContext::default().with_frequency(1e3), &solved.values)
        .unwrap();
    let high = c
        .small_signal_system(&ModelContext::default().with_frequency(1e9), &solved.values)
        .unwrap();
    let g = c.unknowns().node_row(c.nodes().get("g").unwrap()).unwrap();
    for col in 0..c.unknown_count() {
        for r in 0..c.unknown_count() {
            assert_eq!(entry(&low.a, r, col), entry(&high.a, r, col));
            assert_eq!(entry(&low.e, r, col), entry(&high.e, r, col));
        }
    }
    let gate = entry(&low.e, g, g);
    assert!(gate > 0. && gate < 2. * 3e-12 * 5., "{gate}");
}

#[test]
fn gate_lag_makes_the_ac_transconductance_frequency_dependent() {
    let body = "vd d 0 5\nvg g 0 dc -1 ac 1\nrl d o 1k\nvo o 0 0\nj1 d g 0 jm\n\
                .model jm njf(level=2 vto=-2 beta=1m lfgam=0.1 hfgam=0.02 taug=1u)";
    let mut c = circuit(body).unwrap();
    let plot = run(&mut c, AnalysisKind::Ac, &["dec", "1", "1", "1g"]).unwrap();
    let index = plot.variable_index("i(vd)").unwrap();
    let first = plot.points[0][index];
    let last = plot.points[plot.points.len() - 1][index];
    // psmodel.c: gm(0) = gmo (1 - lfgam) and gm(inf) = gmo (1 - hfgam).
    let ratio =
        (last.re.powi(2) + last.im.powi(2)).sqrt() / (first.re.powi(2) + first.im.powi(2)).sqrt();
    close(ratio, (1. - 0.02) / (1. - 0.1), 1e-4, 0.);
}

#[test]
fn self_heating_relaxes_after_a_step_and_settles_at_the_dc_point() {
    // A drain step with TAUD = 100 ns: the current starts at the cold
    // (pre-step average power) value and relaxes to the hot DC value. The
    // filter state is committed only at accepted points, so the end point
    // matches an independent operating point at the final bias.
    let model = ".model jm njf(level=2 vto=-2 beta=2m delta=2 taud=100n)";
    let mut c = circuit(&format!(
        "vd d 0 pwl(0 1 10n 1 11n 5)\nvg g 0 -0.5\nj1 d g 0 jm\n{model}"
    ))
    .unwrap();
    let plot = run(&mut c, AnalysisKind::Transient, &["5n", "2u"]).unwrap();
    let current = |k: usize| -value(&plot, "i(vd)", k);
    let after = (0..plot.points.len())
        .find(|k| value(&plot, "time", *k) >= 15e-9)
        .unwrap();
    let last = plot.points.len() - 1;
    let mut dc = circuit(&format!("vd d 0 5\nvg g 0 -0.5\nj1 d g 0 jm\n{model}")).unwrap();
    let hot = -value(
        &run(&mut dc, AnalysisKind::OperatingPoint, &[]).unwrap(),
        "i(vd)",
        0,
    );
    assert!(current(after) > 1.02 * hot, "{} {hot}", current(after));
    close(current(last), hot, 1e-6, 1e-12);
}

#[test]
fn bias_observations_follow_jfet2ask() {
    // DC: vtrap is the gate-drain voltage, vpave the drain power vds * I0
    // before thermal division; gm/gds/igd times m; PJF values normalized.
    for (family, pol) in [("njf", 1.), ("pjf", -1.)] {
        let body = format!(
            "vd d 0 {}\nvg g 0 {}\nj1 d g 0 jm 3 m=2\n\
             .model jm {family}({SQUARE} vto=-2 beta=1m lambda=0.02)",
            pol * 4.,
            pol * -0.5,
        );
        let mut c = circuit(&body).unwrap();
        c.set_observations(&[
            "@j1[gm]".into(),
            "@j1[gds]".into(),
            "@j1[id]".into(),
            "@j1[vgs]".into(),
            "@j1[vgd]".into(),
            "@j1[vtrap]".into(),
            "@j1[vpave]".into(),
            "@j1[area]".into(),
        ])
        .unwrap();
        let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]).unwrap();
        let (beta, lambda, vgst, m, area, vds) = (1e-3, 0.02, 1.5, 2., 3., 4.);
        close(
            value(&plot, "@j1[gm]", 0),
            m * 2. * area * beta * (1. + lambda * vds) * vgst,
            1e-7,
            0.,
        );
        close(
            value(&plot, "@j1[gds]", 0),
            m * lambda * area * beta * vgst * vgst,
            1e-6,
            1e-15,
        );
        let id = value(&plot, "@j1[id]", 0);
        assert!(id > 0., "{family}: normalized drain current {id}");
        close(value(&plot, "@j1[vtrap]", 0), -0.5 - vds, 1e-12, 0.);
        let channel = area * beta * (1. + lambda * vds) * vgst * vgst;
        close(value(&plot, "@j1[vpave]", 0), vds * channel, 1e-7, 0.);
        close(value(&plot, "@j1[vgs]", 0), -0.5, 1e-12, 0.);
        close(value(&plot, "@j1[area]", 0), m * area, 1e-12, 0.);
    }
    // In transient gm and the filtered state are refused, not approximated.
    let mut c =
        circuit("vd d 0 4\nvg g 0 -0.5\nj1 d g 0 jm\n.model jm njf(level=2 taug=1u)").unwrap();
    c.set_observations(&["@j1[gm]".into()]).unwrap();
    let error = run(&mut c, AnalysisKind::Transient, &["1n", "5n"]).unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
}

#[test]
fn noise_distortion_sensitivity_and_pole_zero_are_refused_explicitly() {
    let body = "vd d 0 5\nvg g 0 dc -1 ac 1\nrl d o 1k\nvo o 0 0\nj1 d g 0 jm\n\
                .model jm njf(level=2)";
    let mut c = circuit(body).unwrap();
    let error = run(
        &mut c,
        AnalysisKind::Noise,
        &["v(d)", "vg", "dec", "2", "1k", "10k"],
    )
    .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("jfet2noi.c"), "{error}");
    let mut c = circuit(&body.replace("ac 1", "distof1 1")).unwrap();
    let error = run(&mut c, AnalysisKind::Distortion, &["dec", "2", "1k", "10k"]).unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    let netlist = deck(&format!("{body}\n.sens v(d)")).unwrap();
    let config = ngspice_rs::analysis::RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut c = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    // C has no JFET2 pole-zero load: refused, never silently omitted.
    let mut c = circuit(body).unwrap();
    let error = run(
        &mut c,
        AnalysisKind::PoleZero,
        &["g", "0", "d", "0", "vol", "pz"],
    )
    .unwrap_err();
    assert!(error.to_string().contains("pole-zero"), "{error}");
}

#[test]
fn decks_round_trip_through_the_writer() {
    let netlist = deck(
        "j1 d g s jm 2 off ic=1,-0.5 m=3 temp=40\n\
         .model jm njf(level=2 vbi=0.8 vto=-2 taug=1u hfgam=0.1 pjf njf)",
    )
    .unwrap();
    let written = write_netlist(&netlist).unwrap();
    let again = deck(
        written
            .lines()
            .skip(1)
            .filter(|l| *l != ".end")
            .collect::<Vec<_>>()
            .join("\n")
            .as_str(),
    )
    .unwrap();
    assert_eq!(write_netlist(&again).unwrap(), written, "{written}");
}
