//! JFET level 1 (#82, M10 slice 1): front end, model selection, regions of
//! operation against independent restatements of `jfetload.c`, finite-
//! difference Jacobians, gate charge, observations and explicit gaps.
//! C-golden comparisons live in `xtask golden verify` (`m10_jfet_*`).
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::{AnalysisMode, Circuit, LoadRequest, ModelContext};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::ast::ParameterKind;
use ngspice_rs::netlist::{Parser, source::parse_deck_text, write_netlist};
use ngspice_rs::primitives::{AnalysisKind, NodeKind, SpiceError};
use std::path::Path;

const GMIN: f64 = 1e-12;

fn deck(body: &str) -> Result<ngspice_rs::netlist::ast::Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("jfet.cir"),
        &format!("JFET\n{body}\n.end\n"),
    ))
}
fn circuit(body: &str) -> Result<Circuit, SpiceError> {
    Circuit::from_netlist(&deck(body)?)
}
fn run(c: &mut Circuit, kind: AnalysisKind, arguments: &[&str]) -> Plot {
    runner(kind)
        .unwrap()
        .run(
            c,
            &AnalysisRequest::with_arguments(kind, arguments.iter().copied()),
            &AnalysisContext::default(),
        )
        .unwrap()
}
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

/// `jfetload.c` normal-mode drain current for `B = 1` (Shichman-Hodges).
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

#[test]
fn the_instance_grammar_follows_inp2j() {
    let netlist = deck(
        "j1 d g s jm 2.5 off ic=1,-0.5 m=3 temp=40 ic-vgs=-0.25\n\
         J2 D G S JM AREA=2 DTEMP=5 IC=0.7\n\
         .model jm njf",
    )
    .unwrap();
    let j1 = &netlist.devices[0];
    assert_eq!(j1.designator, 'j');
    assert_eq!(j1.nodes, ["d", "g", "s"]);
    assert_eq!(j1.model.as_deref(), Some("jm"));
    let names: Vec<&str> = j1.parameters.iter().map(|p| p.name.as_str()).collect();
    // The leading area is applied after the named setters, as INP2J does.
    assert_eq!(names, ["off", "ic", "m", "temp", "ic-vgs", "area"]);
    assert_eq!(j1.parameters[0].kind, ParameterKind::Flag);
    let ParameterKind::InitialConditions(components) = &j1.parameters[1].kind else {
        panic!("ic vector");
    };
    let components: Vec<(&str, &str)> = components
        .iter()
        .map(|c| (c.name.as_str(), c.value.text.as_str()))
        .collect();
    assert_eq!(components, [("ic-vds", "1"), ("ic-vgs", "-0.5")]);
    assert_eq!(netlist.devices[1].name, "j2");
    for body in [
        // Too many IC fields (jfetpar.c accepts one or two).
        "j1 d g s jm ic=1,2,3",
        // No model name.
        "j1 d g s",
        // An unknown instance setter.
        "j1 d g s jm w=1u",
        // A fourth terminal.
        "j1 d g s b jm",
    ] {
        let error = circuit(&format!("{body}\n.model jm njf\nv1 d 0 1"))
            .err()
            .unwrap_or_else(|| panic!("{body}: accepted"));
        assert!(
            matches!(
                error,
                SpiceError::Parse { .. }
                    | SpiceError::NotYetPorted { .. }
                    | SpiceError::Unsupported { .. }
            ),
            "{body}: {error}"
        );
    }
}

#[test]
fn level_selection_and_model_validation_are_explicit() {
    let base = "vd d 0 1\nvg g 0 0\nj1 d g 0 jm\n";
    assert!(circuit(&format!("{base}.model jm njf level=1")).is_ok());
    // inpdomod.c: level 0 is JFET level 1 too.
    assert!(circuit(&format!("{base}.model jm njf level=0")).is_ok());
    // Level 2 is the Parker-Skellern JFET2 (tests/jfet2.rs).
    assert!(circuit(&format!("{base}.model jm njf level=2")).is_ok());
    let level3 = circuit(&format!("{base}.model jm pjf level=3")).unwrap_err();
    assert!(level3.is_not_yet_ported(), "{level3}");
    for (model, what) in [
        ("njf(foo=1)", "unknown setter"),
        ("njf(fc=0.96)", "FC beyond jfettemp.c's clamp"),
        ("njf(is=0)", "nonpositive IS"),
        ("njf(pb=-2 vto=-2)", "nonpositive PB"),
        ("njf(pb=1 vto=1 b=0.5)", "PB equal to VTO (bFac)"),
    ] {
        assert!(
            circuit(&format!("{base}.model jm {model}")).is_err(),
            "{what}: {model}"
        );
    }
    // A wrong family for the J designator.
    assert!(circuit(&format!("{base}.model jm nmos")).is_err());
}

#[test]
fn regions_of_both_polarities_follow_the_square_law() {
    // VTO is negative for both polarities (it applies in the normalized
    // `type * v` frame of jfetload.c). B = 1 reduces the Sydney formulation to Shichman-Hodges; the gate
    // diodes are reverse biased, so the drain current is the channel current
    // minus the gate-drain diode current (-IS + gmin vgd).
    let (beta, lambda, vto) = (1.2e-3, 0.02, -2.);
    for (vgs, vds) in [(-3., 2.), (-1., 0.5), (-1., 3.), (0., 1.), (0., 4.)] {
        for (family, pol) in [("njf", 1.), ("pjf", -1.)] {
            let body = format!(
                "vd d 0 {}\nvg g 0 {}\nj1 d g 0 jm\n\
                 .model jm {family}(vto={vto} beta={beta} lambda={lambda})",
                pol * vds,
                pol * vgs,
            );
            let mut c = circuit(&body).unwrap();
            let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
            let leakage = -1e-14 + GMIN * (vgs - vds);
            let expected = square_law(beta, lambda, vto, vgs, vds) - leakage;
            // i(vd) is the current into the source's + terminal: minus the
            // drain current.
            close(-pol * value(&plot, "i(vd)", 0), expected, 1e-9, 1e-15);
        }
    }
}

#[test]
fn inverse_mode_mirrors_the_normal_mode_current() {
    // Exchanging drain and source reverses the channel current exactly
    // (`jfetload.c`'s inverse branch, including the B tail and LAMBDA).
    let model = ".model jm njf(vto=-2 beta=1m lambda=0.05 b=0.7)";
    let forward = format!("vd d 0 1.5\nvg g 0 -0.5\nj1 d g 0 jm\n{model}");
    // The same bias with the drain and source terminals exchanged: the
    // device sees vds = -1.5, vgd = -0.5 and conducts from source to drain.
    let reverse = format!("vd d 0 1.5\nvg g 0 -0.5\nj1 0 g d jm\n{model}");
    let mut c = circuit(&forward).unwrap();
    let normal = -value(&run(&mut c, AnalysisKind::OperatingPoint, &[]), "i(vd)", 0);
    let mut c = circuit(&reverse).unwrap();
    let inverse = -value(&run(&mut c, AnalysisKind::OperatingPoint, &[]), "i(vd)", 0);
    close(inverse, normal, 1e-12, 1e-18);
    // Sydney B tail: (1-B)/(PB-VTO) = 0.1, saturation cpart vgst^2 (B + bFac vgst).
    let vgst: f64 = 1.5;
    let expected = 1e-3 * (1. + 0.05 * 1.5) * vgst * vgst * (0.7 + 0.1 * vgst);
    close(normal, expected + 1e-14 + GMIN * 2., 1e-9, 1e-15);
}

#[test]
fn series_resistance_creates_source_then_drain_internal_nodes() {
    let c = circuit(
        "vd d 0 3\nvg g 0 0\nj1 d g 0 jm 2\n.model jm njf(rd=40 rs=30)\nj2 d g 0 jn\n.model jn njf",
    )
    .unwrap();
    let source = c.nodes().get("j1#source").expect("source prime");
    let drain = c.nodes().get("j1#drain").expect("drain prime");
    assert!(source < drain, "jfetset.c creates the source node first");
    for node in [source, drain] {
        assert_eq!(c.nodes().node(node).unwrap().kind, NodeKind::Internal);
    }
    assert!(c.nodes().get("j2#source").is_none() && c.nodes().get("j2#drain").is_none());
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

#[test]
fn the_dc_jacobian_matches_finite_differences_over_a_bias_grid() {
    let model = "jm njf(vto=-2 beta=1.2m lambda=0.03 rd=20 rs=15 is=1e-14 n=1.1 b=0.7)";
    let pmodel = "jm pjf(vto=-1.5 beta=0.8m lambda=0.02 b=1.3 rs=10 tcv=1m)";
    let mut bodies = Vec::new();
    for (vd, vg) in [
        (3., -1.),   // saturation
        (0.4, -0.5), // linear
        (-0.6, -1.), // inverse linear
        (-3., -2.5), // inverse saturation (gate relative to drain)
        (1., -2.6),  // cutoff
        (0.2, 0.55), // forward-biased gate
    ] {
        bodies.push(format!(
            "vd d 0 {vd}\nvg g 0 {vg}\nj1 d g 0 jm 1.5 m=2 temp=70\n.model {model}"
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
                close(numerical, entry(&system.a, r, col), 1e-6, 1e-10);
                close(numerical, entry(&newton, r, col), 1e-6, 1e-10);
            }
        }
    }
}

#[test]
fn ac_capacitance_is_the_gate_depletion_charge_derivative() {
    // jfetload.c: q = 2 PB C0 (1 - sqrt(1 - v/PB)) below FC*PB, C = C0/sqrt(...).
    let body = "vd d 0 5\nvg g 0 -1\nj1 d g 0 jm m=2\n\
                .model jm njf(vto=-3 beta=1m cgs=2p cgd=1p pb=0.8)";
    let c = circuit(body).unwrap();
    let context = ModelContext::default();
    let solved =
        ngspice_rs::analysis::bias::solve_dc(&c, &context, &Default::default(), &[], None, None)
            .unwrap();
    let system = c.small_signal_system(&context, &solved.values).unwrap();
    let g = c.unknowns().node_row(c.nodes().get("g").unwrap()).unwrap();
    let d = c.unknowns().node_row(c.nodes().get("d").unwrap()).unwrap();
    let cgs = 2e-12 / (1f64 + 1. / 0.8).sqrt();
    let cgd = 1e-12 / (1f64 + 6. / 0.8).sqrt();
    close(entry(&system.e, g, g), 2. * (cgs + cgd), 1e-12, 0.);
    close(entry(&system.e, g, d), -2. * cgd, 1e-12, 0.);
}

#[test]
fn bias_observations_follow_jfetask() {
    // Saturation with B = 1: gm = 2 beta (1 + lambda vds) vgst, gds = lambda
    // beta vgst^2, both times m; PJF currents are reported normalized.
    for (family, pol) in [("njf", 1.), ("pjf", -1.)] {
        let body = format!(
            "vd d 0 {}\nvg g 0 {}\nj1 d g 0 jm 3 m=2\n\
             .model jm {family}(vto=-2 beta=1m lambda=0.02)\n\
             .dc vd {} {} {}",
            pol * 4.,
            pol * -0.5,
            pol * 4.,
            pol * 5.,
            pol * 1.,
        );
        let netlist = deck(&body).unwrap();
        let mut c = Circuit::from_netlist(&netlist).unwrap();
        c.set_observations(&[
            "@j1[gm]".into(),
            "@j1[gds]".into(),
            "@j1[id]".into(),
            "@j1[ig]".into(),
            "@j1[is]".into(),
            "@j1[vgs]".into(),
            "@j1[vgd]".into(),
            "@j1[area]".into(),
        ])
        .unwrap();
        let plot = run(
            &mut c,
            AnalysisKind::DcSweep,
            &[
                "vd",
                &format!("{}", pol * 4.),
                &format!("{}", pol * 5.),
                &format!("{}", pol * 1.),
            ],
        );
        let (beta, lambda, vgst, m, area) = (1e-3, 0.02, 1.5, 2., 3.);
        for (point, vds) in [(0, 4.), (1, 5.)] {
            close(
                value(&plot, "@j1[gm]", point),
                m * 2. * area * beta * (1. + lambda * vds) * vgst,
                1e-9,
                0.,
            );
            close(
                value(&plot, "@j1[gds]", point),
                m * lambda * area * beta * vgst * vgst,
                1e-9,
                1e-15,
            );
            let id = value(&plot, "@j1[id]", point);
            close(id, -pol * value(&plot, "i(vd)", point), 1e-9, 1e-18);
            assert!(id > 0., "{family}: normalized drain current {id}");
            close(
                value(&plot, "@j1[is]", point),
                -(id + value(&plot, "@j1[ig]", point)),
                1e-9,
                1e-18,
            );
            close(value(&plot, "@j1[vgs]", point), -0.5, 1e-12, 0.);
            close(value(&plot, "@j1[vgd]", point), -0.5 - vds, 1e-12, 0.);
            close(value(&plot, "@j1[area]", point), m * area, 1e-12, 0.);
        }
    }
}

#[test]
fn noise_distortion_and_sensitivity_are_refused_explicitly() {
    let body = "vd d 0 5\nvg g 0 dc -1 ac 1\nrl d o 1k\nvo o 0 0\nj1 d g 0 jm\n.model jm njf";
    let mut c = circuit(body).unwrap();
    let noise = runner(AnalysisKind::Noise).unwrap().run(
        &mut c,
        &AnalysisRequest::with_arguments(
            AnalysisKind::Noise,
            ["v(d)", "vg", "dec", "2", "1k", "10k"],
        ),
        &AnalysisContext::default(),
    );
    let error = noise.unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("jfetnoi.c"), "{error}");
    let mut c = circuit(&body.replace("ac 1", "distof1 1")).unwrap();
    let disto = runner(AnalysisKind::Distortion).unwrap().run(
        &mut c,
        &AnalysisRequest::with_arguments(AnalysisKind::Distortion, ["dec", "2", "1k", "10k"]),
        &AnalysisContext::default(),
    );
    assert!(disto.unwrap_err().is_not_yet_ported());
    // `.sens` through the production front end (C: `cktsens.c`).
    let netlist = deck(&format!("{body}\n.sens v(d)")).unwrap();
    let config = ngspice_rs::analysis::RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut c = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
}

#[test]
fn transient_gate_charge_drives_the_source_follower() {
    // A step on the gate of a follower charges CGS through RG: the gate
    // settles to the source level with a finite delay, and the charge state
    // is consumed by the trapezoidal integrator without error.
    {
        let body = "vdd vdd 0 10\nvin in 0 pulse(-1 1 10n 1n 1n 2u 4u)\nrg in g 20k\n\
             j1 vdd g s jm\nrs s 0 2k\n.model jm njf(vto=-2 beta=1m cgs=5p cgd=2p)";
        let mut c = circuit(body).unwrap();
        let plot = run(&mut c, AnalysisKind::Transient, &["5n", "1.5u"]);
        let last = plot.points.len() - 1;
        // Settled: the source follows the gate within a volt (vgs > vto).
        let (g, s) = (value(&plot, "v(g)", last), value(&plot, "v(s)", last));
        close(g, 1., 1e-3, 1e-3);
        assert!(s > 1. && s - g < 2., "{g} {s}");
    }
}

#[test]
fn decks_round_trip_through_the_writer() {
    let netlist = deck(
        "j1 d g s jm 2 off ic=1,-0.5 m=3 temp=40\nj2 d g s jp ic-vds=0.2 dtemp=3\n\
         .model jm njf(vto=-2 beta=1m pjf njf)\n.model jp pjf(level=1 rs=10)",
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
