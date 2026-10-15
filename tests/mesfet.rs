//! MESFET level 1 (#83, M10 wave 3): front end, model selection, regions of
//! operation against an independent restatement of `mesload.c`, finite-
//! difference Jacobians, Statz gate charge, observations, noise and explicit
//! gaps. C-golden comparisons live in `xtask golden verify` (`m10_mesfet_*`).
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
        Path::new("mesfet.cir"),
        &format!("MESFET\n{body}\n.end\n"),
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

/// `mesload.c` normal-mode drain current, restated: the Statz
/// `beta (1 + lambda vds) vgst^2 / (1 + b vgst)` times
/// `1 - (1 - alpha vds / 3)^3` below `vds = 3 / alpha`.
fn statz(beta: f64, lambda: f64, b: f64, alpha: f64, vto: f64, vgs: f64, vds: f64) -> f64 {
    let vgst = vgs - vto;
    if vgst <= 0. {
        return 0.;
    }
    let saturation = if vds >= 3. / alpha {
        1.
    } else {
        1. - (1. - alpha * vds / 3.).powi(3)
    };
    beta * (1. + lambda * vds) * vgst * vgst / (1. + b * vgst) * saturation
}

/// The C reverse-bias gate diode below `-3 Vt` (`mesload.c`), with `gmin`.
fn reverse_leakage(is: f64, v: f64) -> f64 {
    let vt = 1.38064852e-23 * 300.15 / 1.6021766208e-19;
    let arg = (3. * vt / (v * std::f64::consts::E)).powi(3);
    -is * (1. + arg) + GMIN * v
}

#[test]
fn the_instance_grammar_follows_inp2z() {
    let netlist = deck(
        "z1 d g s zm off ic=1,-0.5 m=3 icvgs=-0.25 area=2\n\
         Z2 D G S ZM AREA=2 IC=0.7\n\
         .model zm nmf",
    )
    .unwrap();
    let z1 = &netlist.devices[0];
    assert_eq!(z1.designator, 'z');
    assert_eq!(z1.nodes, ["d", "g", "s"]);
    assert_eq!(z1.model.as_deref(), Some("zm"));
    let names: Vec<&str> = z1.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["off", "ic", "m", "icvgs", "area"]);
    assert_eq!(z1.parameters[0].kind, ParameterKind::Flag);
    let ParameterKind::InitialConditions(components) = &z1.parameters[1].kind else {
        panic!("ic vector");
    };
    let components: Vec<(&str, &str)> = components
        .iter()
        .map(|c| (c.name.as_str(), c.value.text.as_str()))
        .collect();
    assert_eq!(components, [("icvds", "1"), ("icvgs", "-0.5")]);
    assert_eq!(netlist.devices[1].name, "z2");
    for body in [
        // INP2Z discards an unlabeled leading value for MES models.
        "z1 d g s zm 2",
        // Too many IC fields (mesparam.c accepts one or two).
        "z1 d g s zm ic=1,2,3",
        // No model name.
        "z1 d g s",
        // An unknown instance setter (MES has no temperature setters).
        "z1 d g s zm temp=50",
        // A fourth terminal.
        "z1 d g s b zm",
    ] {
        let error = circuit(&format!("{body}\n.model zm nmf\nv1 d 0 1"))
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
    let leading = circuit("z1 d g s zm 2\n.model zm nmf").unwrap_err();
    assert!(leading.is_not_yet_ported(), "{leading}");
    assert!(leading.to_string().contains("area="), "{leading}");
}

/// The `z` grammar recognises the union of the four families' setters
/// (`MESApTable`, `HFETApTable`, `HFET2pTable`), because the family is only
/// known once the model is resolved. Which of them a level accepts is decided
/// there.
#[test]
fn the_instance_grammar_accepts_the_mesa_and_hfet_setters() {
    let netlist = deck(
        "z1 d g s zm l=2u W=20u m=3 td=50 ts=40 dtemp=2\n\
         z2 d g s zm2 l=1u temp=75\n\
         .model zm nmf level=2\n\
         .model zm2 pmf level=2",
    )
    .unwrap();
    let names: Vec<&str> = netlist.devices[0]
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["l", "w", "m", "td", "ts", "dtemp"]);
    let names: Vec<&str> = netlist.devices[1]
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["l", "temp"]);
}

/// `mes/` has no geometry, drain/source temperature or `temp` setter, so a MES
/// model rejects them. C reports "no such parameter on this device"; the port
/// names the setter it could not apply.
#[test]
fn mesa_and_hfet_setters_are_rejected_for_a_mes_model() {
    for body in [
        "z1 d g s zm l=2u",
        "z1 d g s zm w=20u",
        "z1 d g s zm td=50",
        "z1 d g s zm ts=40",
        "z1 d g s zm dtemp=2",
        "z1 d g s zm temp=50",
    ] {
        let error = circuit(&format!("{body}\n.model zm nmf\nv1 d 0 1"))
            .err()
            .unwrap_or_else(|| panic!("{body}: accepted"));
        assert!(
            error.to_string().contains("unsupported setter"),
            "{body}: {error}"
        );
    }
}

#[test]
fn level_selection_and_model_validation_are_explicit() {
    let base = "vd d 0 1\nvg g 0 0\nz1 d g 0 zm\n";
    assert!(circuit(&format!("{base}.model zm nmf level=1")).is_ok());
    // inpdomod.c: level 0 is MES too.
    assert!(circuit(&format!("{base}.model zm pmf level=0")).is_ok());
    // inpdomod.c: levels 3-6 are the other macspice families. Level 2 is
    // MESA, which M10 H1a builds as a `z` device.
    assert!(circuit(&format!("{base}.model zm nmf level=2")).is_ok());
    for (level, reference) in [
        (3, "mesa/"),
        (3, "mesa/"),
        (4, "mesa/"),
        (5, "hfet1/"),
        (6, "hfet2/"),
        (7, "inpdomod.c"),
    ] {
        let error = circuit(&format!("{base}.model zm nmf level={level}")).unwrap_err();
        assert!(error.is_not_yet_ported(), "{level}: {error}");
        assert!(error.to_string().contains(reference), "{level}: {error}");
    }
    for (model, what) in [
        ("nmf(foo=1)", "unknown setter"),
        ("nmf(alpha=0)", "ALPHA must be positive"),
        ("nmf(b=-0.1)", "negative B"),
        ("nmf(is=0)", "nonpositive IS"),
        ("nmf(pb=0)", "nonpositive PB"),
        ("nmf(rd=-1)", "negative RD"),
    ] {
        assert!(
            circuit(&format!("{base}.model zm {model}")).is_err(),
            "{what}: {model}"
        );
    }
    // A wrong family for the Z designator, and a MES model on a J card.
    assert!(circuit(&format!("{base}.model zm njf")).is_err());
    assert!(circuit("vd d 0 1\nj1 d g 0 zm\n.model zm nmf").is_err());
}

#[test]
fn regions_of_both_polarities_follow_the_statz_law() {
    // VTO applies in the normalized `type * v` frame for both polarities.
    // Both gate diodes are reverse biased, so the drain current is the
    // channel current minus the gate-drain diode current.
    let (beta, lambda, b, alpha, vto, is) = (2e-3, 0.04, 0.5, 2.5, -1.5, 1e-14);
    for (vgs, vds) in [
        (-2., 2.),   // cut off
        (-1., 0.3),  // linear
        (-0.5, 0.8), // linear, near the 3 / alpha boundary
        (-0.5, 1.2), // saturation
        (-0.2, 4.),  // saturation
    ] {
        for (family, pol) in [("nmf", 1.), ("pmf", -1.)] {
            let body = format!(
                "vd d 0 {}\nvg g 0 {}\nz1 d g 0 zm\n\
                 .model zm {family}(vto={vto} beta={beta} lambda={lambda} b={b} alpha={alpha})",
                pol * vds,
                pol * vgs,
            );
            let mut c = circuit(&body).unwrap();
            let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
            let expected =
                statz(beta, lambda, b, alpha, vto, vgs, vds) - reverse_leakage(is, vgs - vds);
            // i(vd) is the current into the source's + terminal: minus the
            // drain current.
            close(-pol * value(&plot, "i(vd)", 0), expected, 1e-9, 1e-17);
        }
    }
}

#[test]
fn inverse_mode_mirrors_the_normal_mode_current() {
    // Exchanging drain and source reverses the channel current exactly
    // (`mesload.c`'s inverse branches, including LAMBDA and B).
    let model = ".model zm nmf(vto=-2 beta=1m lambda=0.05 b=0.7 alpha=3)";
    let forward = format!("vd d 0 0.6\nvg g 0 -0.5\nz1 d g 0 zm\n{model}");
    let reverse = format!("vd d 0 0.6\nvg g 0 -0.5\nz1 0 g d zm\n{model}");
    let mut c = circuit(&forward).unwrap();
    let normal = -value(&run(&mut c, AnalysisKind::OperatingPoint, &[]), "i(vd)", 0);
    let mut c = circuit(&reverse).unwrap();
    let inverse = -value(&run(&mut c, AnalysisKind::OperatingPoint, &[]), "i(vd)", 0);
    close(inverse, normal, 1e-12, 1e-18);
    let expected = statz(1e-3, 0.05, 0.7, 3., -2., -0.5, 0.6) - reverse_leakage(1e-14, -1.1);
    close(normal, expected, 1e-9, 1e-17);
}

#[test]
fn series_resistance_creates_source_then_drain_internal_nodes() {
    let c = circuit(
        "vd d 0 3\nvg g 0 0\nz1 d g 0 zm area=2\n.model zm nmf(rd=40 rs=30)\nz2 d g 0 zn\n\
         .model zn nmf",
    )
    .unwrap();
    let source = c.nodes().get("z1#source").expect("source prime");
    let drain = c.nodes().get("z1#drain").expect("drain prime");
    assert!(source < drain, "messetup.c creates the source node first");
    for node in [source, drain] {
        assert_eq!(c.nodes().node(node).unwrap().kind, NodeKind::Internal);
    }
    assert!(c.nodes().get("z2#source").is_none() && c.nodes().get("z2#drain").is_none());
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
    let model = "zm nmf(vto=-1.8 beta=3m lambda=0.05 alpha=2.5 b=0.4 rd=20 rs=15 is=1e-14)";
    let pmodel = "zm pmf(vto=-1.2 beta=2m lambda=0.02 b=0.1 alpha=3 rs=10)";
    let mut bodies = Vec::new();
    for (vd, vg) in [
        (3., -1.),   // saturation
        (0.4, -0.5), // linear
        (-0.6, -1.), // inverse linear
        (-3., -2.5), // inverse saturation (gate relative to drain)
        (1., -2.6),  // cutoff
        (0.2, 0.55), // forward-biased gate
        (-0.1, 0.6), // forward-biased gate, inverse mode
    ] {
        bodies.push(format!(
            "vd d 0 {vd}\nvg g 0 {vg}\nz1 d g 0 zm area=1.5 m=2\n.model {model}"
        ));
        bodies.push(format!(
            "vd d 0 {}\nvg g 0 {}\nz1 d g 0 zm\n.model {pmodel}",
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

/// `qggnew` of `mesload.c`, restated: the Statz gate charge at (`vgs`,
/// `vgd`).
fn qggnew(vgs: f64, vgd: f64, phib: f64, vcap: f64, vto: f64, cgs: f64, cgd: f64) -> f64 {
    let veroot = ((vgs - vgd).powi(2) + vcap * vcap).sqrt();
    let veff1 = 0.5 * (vgs + vgd + veroot);
    let veff2 = veff1 - veroot;
    let vnew = 0.5 * (veff1 + vto + ((veff1 - vto).powi(2) + 0.04).sqrt());
    let (vnew1, ext) = if vnew < 0.5 {
        (vnew, 0.)
    } else {
        (0.5, (vnew - 0.5) / (1. - 0.5 / phib).sqrt())
    };
    cgs * (2. * phib * (1. - (1. - vnew1 / phib).sqrt()) + ext) + cgd * veff2
}

#[test]
fn ac_capacitances_are_the_statz_charge_partials() {
    // mesacl.c stamps qggnew's capacitances at the bias as two-terminal
    // g-s' and g-d' elements, scaled by area and m.
    let (vgs, vgd) = (-0.4, -3.4);
    let body = format!(
        "vd d 0 3\nvg g 0 {vgs}\nz1 d g 0 zm area=1.5 m=2\n\
         .model zm nmf(vto=-1.8 beta=1m alpha=2.5 cgs=2p cgd=0.5p pb=0.8)"
    );
    let c = circuit(&body).unwrap();
    let context = ModelContext::default();
    let solved =
        ngspice_rs::analysis::bias::solve_dc(&c, &context, &Default::default(), &[], None, None)
            .unwrap();
    let system = c.small_signal_system(&context, &solved.values).unwrap();
    let g = c.unknowns().node_row(c.nodes().get("g").unwrap()).unwrap();
    let d = c.unknowns().node_row(c.nodes().get("d").unwrap()).unwrap();
    let q = |a: f64, b: f64| qggnew(a, b, 0.8, 1. / 2.5, -1.8, 1.5 * 2e-12, 1.5 * 0.5e-12);
    let h = 1e-6;
    let cgs = (q(vgs + h, vgd) - q(vgs - h, vgd)) / (2. * h);
    let cgd = (q(vgs, vgd + h) - q(vgs, vgd - h)) / (2. * h);
    close(entry(&system.e, g, g), 2. * (cgs + cgd), 1e-7, 0.);
    close(entry(&system.e, g, d), -2. * cgd, 1e-7, 0.);
}

#[test]
fn bias_observations_follow_mesask() {
    // Saturation with B = 0 and LAMBDA = 0.02: gm = 2 beta (1 + lambda vds)
    // vgst and gds = lambda beta vgst^2, both times area and m; PMF currents
    // are reported normalized.
    for (family, pol) in [("nmf", 1.), ("pmf", -1.)] {
        let body = format!(
            "vd d 0 {}\nvg g 0 {}\nz1 d g 0 zm area=3 m=2\n\
             .model zm {family}(vto=-2 beta=1m lambda=0.02 b=0)\n\
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
            "@z1[gm]".into(),
            "@z1[gds]".into(),
            "@z1[cd]".into(),
            "@z1[cg]".into(),
            "@z1[is]".into(),
            "@z1[cgd]".into(),
            "@z1[vgs]".into(),
            "@z1[vgd]".into(),
            "@z1[area]".into(),
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
                value(&plot, "@z1[gm]", point),
                m * 2. * area * beta * (1. + lambda * vds) * vgst,
                1e-9,
                0.,
            );
            close(
                value(&plot, "@z1[gds]", point),
                m * lambda * area * beta * vgst * vgst,
                1e-9,
                1e-15,
            );
            let cd = value(&plot, "@z1[cd]", point);
            close(cd, -pol * value(&plot, "i(vd)", point), 1e-9, 1e-18);
            assert!(cd > 0., "{family}: normalized drain current {cd}");
            close(
                value(&plot, "@z1[is]", point),
                -(cd + value(&plot, "@z1[cg]", point)),
                1e-9,
                1e-18,
            );
            close(
                value(&plot, "@z1[cgd]", point),
                m * reverse_leakage(area * 1e-14, -0.5 - vds),
                1e-9,
                1e-20,
            );
            close(value(&plot, "@z1[vgs]", point), -0.5, 1e-12, 0.);
            close(value(&plot, "@z1[vgd]", point), -0.5 - vds, 1e-12, 0.);
            close(value(&plot, "@z1[area]", point), m * area, 1e-12, 0.);
        }
    }
}

#[test]
fn noise_follows_mesnoise_and_distortion_and_sensitivity_are_refused() {
    let body = "vd d 0 5\nvg g 0 dc -1 ac 1\nrl d o 1k\nvo o 0 0\nz1 d g 0 zm m=2\n\
                .model zm nmf(rd=10 rs=5 kf=1e-12 af=1.2)";
    let mut c = circuit(body).unwrap();
    let plot = runner(AnalysisKind::Noise)
        .unwrap()
        .run(
            &mut c,
            &AnalysisRequest::with_arguments(
                AnalysisKind::Noise,
                ["v(o)", "vg", "dec", "2", "1k", "10k", "1"],
            ),
            &AnalysisContext::default(),
        )
        .unwrap();
    // mesnoise.c's generators, then the instance total.
    for name in [
        "onoise_z1_rd",
        "onoise_z1_rs",
        "onoise_z1_id",
        "onoise_z1_1overf",
        "onoise_z1",
    ] {
        assert!(
            plot.variable_index(name).is_some(),
            "{name}: {:?}",
            plot.variables
        );
    }
    let mut c = circuit(&body.replace("ac 1", "distof1 1")).unwrap();
    let disto = runner(AnalysisKind::Distortion).unwrap().run(
        &mut c,
        &AnalysisRequest::with_arguments(AnalysisKind::Distortion, ["dec", "2", "1k", "10k"]),
        &AnalysisContext::default(),
    );
    let error = disto.unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("mesdisto.c"), "{error}");
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
fn transient_gate_charge_delays_the_common_source_stage() {
    // A gate step through RG charges the Statz gate capacitance: the gate
    // reaches the source level with a finite delay, and the charge state is
    // consumed by the trapezoidal integrator without error.
    let body = "vdd vdd 0 6\nvin in 0 pulse(-1.5 0 10n 1n 1n 500n 1u)\nrg in g 2k\n\
                rl vdd d 1k\nz1 d g 0 zm\n\
                .model zm nmf(vto=-1.8 beta=3m cgs=5p cgd=1p pb=0.8)";
    let mut c = circuit(body).unwrap();
    let plot = run(&mut c, AnalysisKind::Transient, &["2n", "300n"]);
    let index = |t: f64| {
        plot.points
            .iter()
            .position(|p| p[0].re >= t)
            .unwrap_or(plot.points.len() - 1)
    };
    // 2 ns after the edge the gate lags the 0 V source level (RG CGS ~ 10 ns).
    let early = value(&plot, "v(g)", index(13e-9));
    assert!(early < -1., "{early}");
    let last = plot.points.len() - 1;
    close(value(&plot, "v(g)", last), 0., 0., 1e-3);
    assert!(value(&plot, "v(d)", last) < value(&plot, "v(d)", 0));
    // With PB below qggnew's 0.5 V vmax a forward-biased gate takes the
    // square root of a negative number (C: NaN): an explicit numerical error.
    let forward = "vdd vdd 0 3\nvg g 0 pulse(0 0.7 1n 1n 1n 10n 20n)\nrl vdd d 1k\nz1 d g 0 zm\n\
                   .model zm nmf(cgs=1p pb=0.4)";
    let mut c = circuit(forward).unwrap();
    let error = runner(AnalysisKind::Transient)
        .unwrap()
        .run(
            &mut c,
            &AnalysisRequest::with_arguments(AnalysisKind::Transient, ["0.1n", "5n"]),
            &AnalysisContext::default(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("qggnew"), "{error}");
    // The same deck with the default PB = 1 V runs.
    let mut c = circuit(&forward.replace("pb=0.4", "")).unwrap();
    run(&mut c, AnalysisKind::Transient, &["0.1n", "5n"]);
}

#[test]
fn instance_parameter_sweeps_rederive_the_device() {
    // `.dc @z1[area]`: the drain current of a saturated device without
    // series resistance scales with the area.
    let body = "vd d 0 3\nvg g 0 -0.5\nz1 d g 0 zm\n.model zm nmf(vto=-2 beta=1m b=0.3)";
    let mut c = circuit(body).unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["@z1[area]", "1", "3", "1"]);
    let one = -value(&plot, "i(vd)", 0);
    for (point, area) in [(1, 2.), (2, 3.)] {
        close(-value(&plot, "i(vd)", point), area * one, 1e-6, 1e-16);
    }
}

#[test]
fn decks_round_trip_through_the_writer() {
    let netlist = deck(
        "z1 d g s zm off ic=1,-0.5 m=3 area=2\nz2 d g s zp icvds=0.2 icvgs=-1\n\
         .model zm nmf(vto=-2 beta=1m pmf nmf)\n.model zp pmf(level=1 rs=10 vt0=-1)",
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

#[test]
fn an_off_instance_starts_held_and_converges() {
    // C's shared held-load rule: an `off` instance starts and stays at
    // vgs = vgd = 0 through MODEINITFIX, so its `.op` is the held-start
    // solution and need not land exactly on the on-instance point (C leaves
    // the held start only in a transient, through its operating-point
    // fallback). Both must still converge to the same conducting region.
    let body = "vdd vdd 0 5\nrl vdd d 1k\nvg g 0 -0.5\nz1 d g 0 zm{off}\n\
                .model zm nmf(vto=-1.8 beta=3m)";
    let mut on = circuit(&body.replace("{off}", "")).unwrap();
    let mut off = circuit(&body.replace("{off}", " off")).unwrap();
    let on = run(&mut on, AnalysisKind::OperatingPoint, &[]);
    let off = run(&mut off, AnalysisKind::OperatingPoint, &[]);
    close(value(&off, "v(d)", 0), value(&on, "v(d)", 0), 1e-2, 1e-9);
    assert!(value(&on, "v(d)", 0) < 2., "conducting");
}

#[test]
fn the_transient_jacobian_follows_mesloads_matrix() {
    use ngspice_rs::devices::{Forcing, Limit, TransientTiming};
    use ngspice_rs::maths::integrator::{DEFAULT_XMU, IntegrationMethod, StepHistory};
    // The incremental Statz charges in both modes and both polarities. The
    // Newton matrix is `mesload.c`'s, which is *not* the exact derivative of
    // the load's residual: C adds one companion conductance per charge to
    // `ggs` and `ggd` (`capgs = cgsna`, `capgd = cgdna`, the values of the
    // first evaluation) and stamps no cross derivatives, so the entries
    // coupling the gate to the internal nodes differ from the
    // finite-difference derivative at the 1e-4 level. The bound below records
    // C's approximation; a structural error in the stamping is far larger.
    for (family, pol, vd, vg) in [
        ("nmf", 1., 3., -1.),
        ("nmf", 1., -0.6, -1.),
        ("nmf", 1., 0.3, 0.4),
        ("pmf", -1., 2., -0.5),
    ] {
        let c = circuit(&format!(
            "vd d 0 {}\nvg g 0 {}\nz1 d g 0 zm area=1.5 m=2\n\
             .model zm {family}(vto=-1.8 beta=3m lambda=0.05 alpha=2.5 b=0.4 rd=10 rs=8 \
             cgs=2p cgd=0.5p pb=0.8)",
            pol * vd,
            pol * vg
        ))
        .unwrap();
        let solved = ngspice_rs::analysis::bias::solve_dc(
            &c,
            &ModelContext::default(),
            &Default::default(),
            &[],
            None,
            None,
        )
        .unwrap();
        let mut history = c.state_history();
        for _ in 0..ngspice_rs::devices::ACCEPTED_DEPTH {
            history.commit(solved.trial.clone()).unwrap();
        }
        let dt = 1e-9;
        let mut steps = StepHistory::new();
        let first = steps
            .trial(IntegrationMethod::Trapezoidal, 1, dt, DEFAULT_XMU)
            .unwrap();
        steps.accept(&first);
        let coefficients = steps
            .trial(IntegrationMethod::Trapezoidal, 2, dt, DEFAULT_XMU)
            .unwrap();
        let n = c.unknown_count();
        let load = |x: &Vector| {
            let mut a = SparseMatrix::new(n, n);
            let mut b = Vector::zeros(n);
            let mut trial = history.trial();
            c.load(
                &LoadRequest {
                    mode: AnalysisMode::Transient { time: dt, dt },
                    solution: x,
                    model_context: &ModelContext::default(),
                    integration: Some(&coefficients),
                    history: &history,
                    forcing: Some(Forcing {
                        limit: Limit::Right,
                        timing: TransientTiming::new(dt, 10. * dt).unwrap(),
                    }),
                },
                &mut a,
                &mut b,
                &mut trial,
            )
            .unwrap();
            (a, b)
        };
        // Move the gate and drain away from the accepted point so the
        // incremental charges and their cross derivatives are exercised.
        let mut x = solved.values.clone();
        let (d, g) = (
            c.unknowns().node_row(c.nodes().get("d").unwrap()).unwrap(),
            c.unknowns().node_row(c.nodes().get("g").unwrap()).unwrap(),
        );
        x.as_mut_slice()[g] += pol * 0.05;
        x.as_mut_slice()[d] -= pol * 0.1;
        let (a, _) = load(&x);
        for col in 0..n {
            let h = 1e-6;
            let mut low = x.clone();
            let mut high = x.clone();
            low.as_mut_slice()[col] -= h;
            high.as_mut_slice()[col] += h;
            let ((al, bl), (ah, bh)) = (load(&low), load(&high));
            let (rl, rh) = (residual(&al, &bl, &low), residual(&ah, &bh, &high));
            for r in 0..n {
                let numerical = (rh[r] - rl[r]) / (2. * h);
                // The entries coupling the gate to the internal d'/s' nodes
                // (and those two to each other) carry C's approximation; every
                // other entry is the exact derivative of the residual, and a
                // structural stamping error is far larger than this bound.
                close(numerical, entry(&a, r, col), 1e-2, 1e-4);
            }
        }
    }
}
