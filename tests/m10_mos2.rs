//! MOS level 2 on the shared MOS shell (#89): closed forms restated from
//! `mos2temp.c`/`mos2load.c`, whole-circuit Jacobians against finite
//! differences, Meyer capacitances over `L - 2 LD` and the drawn width, C's
//! internal node names and the explicit refusals. C-golden comparisons
//! (`m10_mos2_*`) live in `xtask golden verify`;
//! `tests/c_noise_reference.rs` and `tests/c_pole_zero.rs` compare MOS2
//! `.noise` and `.pz` with live C.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
use ngspice_rs::devices::{AnalysisMode, Circuit, LoadRequest, ModelContext};
use ngspice_rs::maths::{SparseMatrix, Vector};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, NodeKind, SpiceError, SpiceResult};
use std::path::Path;

const BOLTZMANN: f64 = 1.38064852e-23;
const CHARGE: f64 = 1.6021766208e-19;
const K_OVER_Q: f64 = BOLTZMANN / CHARGE;
const REFTEMP: f64 = 300.15;
const EPS0: f64 = 8.854214871e-12;
const GMIN: f64 = 1e-12;

fn deck(body: &str) -> ngspice_rs::netlist::ast::Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("m10.cir"),
            &format!("M10\n{body}\n.end\n"),
        ))
        .unwrap()
}
fn circuit(body: &str) -> Circuit {
    Circuit::from_netlist(&deck(body)).unwrap()
}
fn op(c: &mut Circuit) -> Plot {
    runner(AnalysisKind::OperatingPoint)
        .unwrap()
        .run(
            c,
            &AnalysisRequest::with_arguments(AnalysisKind::OperatingPoint, [""; 0]),
            &AnalysisContext::default(),
        )
        .unwrap()
}
fn close(a: f64, b: f64, relative: f64, absolute: f64) {
    assert!(
        (a - b).abs() <= relative * b.abs() + absolute,
        "{a:e} != {b:e}"
    );
}
fn bias(
    c: &Circuit,
) -> ngspice_rs::analysis::newton::NewtonSolution<ngspice_rs::devices::TrialState> {
    ngspice_rs::analysis::bias::solve_dc(
        c,
        &ModelContext::default(),
        &Default::default(),
        &[],
        None,
        None,
    )
    .unwrap()
}
fn row(c: &Circuit, node: &str) -> usize {
    c.unknowns().node_row(c.nodes().get(node).unwrap()).unwrap()
}
fn entry(matrix: &SparseMatrix, r: usize, col: usize) -> f64 {
    matrix
        .triplets()
        .iter()
        .filter(|t| t.row == r && t.col == col)
        .map(|t| t.value)
        .sum()
}
fn residual(a: &SparseMatrix, b: &Vector, x: &Vector) -> Vec<f64> {
    let mut r: Vec<f64> = b.as_slice().iter().map(|v| -v).collect();
    for t in a.triplets() {
        r[t.row] += t.value * x.as_slice()[t.col];
    }
    r
}
fn band_gap(t: f64) -> f64 {
    1.16 - 7.02e-4 * t * t / (t + 1108.)
}
/// Every analysis of `body`, through `RunConfig` as the CLI runs it.
fn run(body: &str) -> SpiceResult<Vec<Plot>> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("m10.cir"),
        &format!("M10\n{body}\n.end\n"),
    ))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run_plots(&mut circuit, &request, &config.context())
}

/// The Grove-Frohman saturation current of `mos2load.c` at `vbs = 0`
/// without DELTA, XJ, VMAX or mobility degradation: `vdsat` from the body
/// term and `beta ((vgs - vbin - vdsat/2) vdsat - 2/3 gamma ((vdsat +
/// phi)^1.5 - phi^1.5)) / clfact`.
fn grove_frohman(beta: f64, vgs: f64, vbin: f64, gamma: f64, phi: f64, clfact: f64) -> f64 {
    let vdsat = if gamma > 0. {
        let g2 = gamma * gamma;
        vgs - vbin + g2 * (1. - (1. + 4. * (vgs - vbin + phi) / g2).sqrt()) / 2.
    } else {
        vgs - vbin
    };
    beta * ((vgs - vbin - vdsat / 2.) * vdsat
        - gamma * ((vdsat + phi).powf(1.5) - phi.powf(1.5)) / 1.5)
        / clfact
}

#[test]
fn the_simplest_level_two_card_is_the_square_law_with_c_defaults() {
    // GAMMA, DELTA, XJ, VMAX, NFS, UEXP all zero and no NSUB: mos2load.c
    // reduces to 0.5 beta vgst^2 in saturation. TOX defaults to 1e-7 m, so
    // KP = U0 Cox 1e-4 = 600 Cox 1e-4. The drain-bulk junction at -4 V
    // carries mos2load.c's constant reverse current IS plus gmin.
    for (card, kp) in [
        ("vto=0.8 kp=50u", 50e-6),
        ("vto=0.8", 600. * 3.9 * EPS0 / 1e-7 * 1e-4),
        ("vto=0.8 uo=400 tox=40n", 400. * 3.9 * EPS0 / 40e-9 * 1e-4),
    ] {
        let mut c = circuit(&format!(
            "vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u\n.model mm nmos(level=2 {card})"
        ));
        let p = op(&mut c);
        close(
            -p.value("i(vd)", 0).unwrap().re,
            0.5 * kp * 5. * (3f64 - 0.8).powi(2) + 1e-14 + 4. * GMIN,
            1e-9,
            1e-18,
        );
    }
}

#[test]
fn process_extraction_uses_the_fixed_intrinsic_density_of_mos2temp() {
    // NSUB with VTO/GAMMA/PHI derived (mos2temp.c uses 1.45e16 m^-3 at
    // every TNOM, unlike MOS3) and a LAMBDA that replaces the XD-derived
    // channel-length modulation: the Grove-Frohman current over 1 - LAMBDA
    // vds.
    for tnom in [27., 50.] {
        let mut c = circuit(&format!(
            "vd d 0 4\nvg g 0 3\nm1 d g 0 0 mm w=10u l=2u\n\
             .model mm nmos(level=2 tox=25n nsub=1e16 lambda=0.01 tnom={tnom})"
        ));
        let tn = tnom + 273.15;
        let vtnom = K_OVER_Q * tn;
        let eg = band_gap(tn);
        let cox = 3.9 * EPS0 / 25e-9;
        let density: f64 = 1e16 * 1e6;
        let phi = (2. * vtnom * (density / 1.45e16).ln()).max(0.1);
        let gamma = (2. * 11.7 * EPS0 * CHARGE * density).sqrt() / cox;
        let vfb = (3.25 + 0.5 * eg - 0.5 * eg) - (3.25 + 0.5 * eg + 0.5 * phi);
        let vto = vfb + gamma * phi.sqrt() + phi;
        // The device runs at 27 C: KP and PHI scale from TNOM.
        let t = REFTEMP;
        let ratio = t / tn;
        let kp = 600. * cox * 1e-4 / (ratio * ratio.sqrt());
        let pbfact = |t: f64| {
            let arg = -band_gap(t) / (2. * BOLTZMANN * t) + 1.1150877 / (BOLTZMANN * 2. * REFTEMP);
            -2. * K_OVER_Q * t * (1.5 * (t / REFTEMP).ln() + CHARGE * arg)
        };
        let tphi = t / REFTEMP * (phi - pbfact(tn)) / (tn / REFTEMP) + pbfact(t);
        let vbi = vto - gamma * phi.sqrt() + 0.5 * (eg - band_gap(t)) + 0.5 * (tphi - phi);
        let is = 1e-14 * (-band_gap(t) / (K_OVER_Q * t) + eg / vtnom).exp();
        let p = op(&mut c);
        close(
            -p.value("i(vd)", 0).unwrap().re,
            grove_frohman(kp * 5., 3., vbi, gamma, tphi, 1. - 0.01 * 4.) + is + 4. * GMIN,
            1e-9,
            1e-18,
        );
    }
}

#[test]
fn series_resistance_creates_c_named_internal_nodes() {
    let c = circuit(
        "vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm w=10u l=2u nrs=3\n\
         .model mm nmos(level=2 vto=0.8 rd=50 rsh=20)",
    );
    // mos2set.c: CKTmkVolt(..., "internal#drain"/"internal#source").
    for name in ["m1#internal#drain", "m1#internal#source"] {
        let node = c.nodes().get(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(c.nodes().node(node).unwrap().kind, NodeKind::Internal);
    }
    let solved = bias(&c);
    let drain = solved.values.as_slice()[row(&c, "m1#internal#drain")];
    let source = solved.values.as_slice()[row(&c, "m1#internal#source")];
    // RD = 50 ohm carries the drain-terminal current; RSH * NRS = 60 ohm the
    // source current.
    assert!(drain < 3. && source > 0.);
    close((3. - drain) / 50., source / 60., 1e-6, 1e-12);
    let plain = circuit("vd d 0 3\nvg g 0 2\nm1 d g 0 0 mm\n.model mm nmos(level=2)");
    assert!(plain.nodes().get("m1#internal#drain").is_none());
}

#[test]
fn meyer_and_overlap_capacitances_use_the_effective_length_and_drawn_width() {
    // Saturation: the gate-source half is Cox/3 with Cox over (L - 2 LD) W M
    // (mos2load.c `OxideCap`); CGSO/CGDO scale with W, CGBO with L - 2 LD.
    let c = circuit(
        "vd d 0 4\nvg g 0 2\nvb b 0 -0.5\nm1 d g 0 b mm w=10u l=2u m=2\n\
         .model mm nmos(level=2 vto=0.7 kp=50u tox=20n ld=0.1u\n\
         + cgso=0.4n cgdo=0.3n cgbo=0.2n)",
    );
    let solved = bias(&c);
    let (leff, w) = (2e-6 - 0.2e-6, 10e-6);
    let cox = 3.9 * EPS0 / 20e-9 * leff * w * 2.;
    let (ovs, ovd, ovb) = (0.4e-9 * 2. * w, 0.3e-9 * 2. * w, 0.2e-9 * 2. * leff);
    let system = c
        .small_signal_system(&ModelContext::default(), &solved.values)
        .unwrap();
    let (g, d, b) = (row(&c, "g"), row(&c, "d"), row(&c, "b"));
    close(
        entry(&system.e, g, g),
        2. * cox / 3. + ovs + ovd + ovb,
        1e-12,
        0.,
    );
    close(entry(&system.e, g, d), -ovd, 1e-12, 0.);
    close(entry(&system.e, g, b), -ovb, 1e-12, 0.);
    // mos2trun.c: only the gate charges control the timestep.
    let mos = c.devices().iter().find(|d| d.designator() == 'm').unwrap();
    assert_eq!(mos.truncation_slots(), vec![4, 6, 8]);
}

#[test]
fn dc_jacobian_matches_finite_differences_where_c_is_exact() {
    // Bias points where mos2load.c's derivatives are exact (see the
    // `devices::mos2` unit tests for where C's are approximate, among them
    // weak inversion): strong inversion of an NFS device in the linear
    // region, the XD-derived channel-length modulation
    // with DELTA and UCRIT/UEXP mobility, Baum's velocity-saturated vdsat,
    // and inverse-mode PMOS with series resistance, temperature and
    // junction geometry.
    for body in [
        "vd c 0 0.1\nvg g 0 1.5\nvs e 0 0\nvb bulk 0 -0.5\n\
         m1 c g e bulk mm w=10u l=2u ad=20p as=20p pd=24u ps=24u temp=80\n\
         .model mm nmos(level=2 vto=0.7 tox=25n nsub=1e16 nfs=2e11 delta=0.5 \
         lambda=0.02 rsh=20 js=1e-4 cj=0.5m)",
        "vd c 0 3\nvg g 0 2\nvs e 0 0\nvb bulk 0 -1\nm1 c g e bulk mm w=10u l=2u\n\
         .model mm nmos(level=2 vto=0.7 tox=25n nsub=2e16 delta=0.6 ucrit=2e4 uexp=0.15 \
         rd=50 rs=40)",
        "vd c 0 3\nvg g 0 2\nvs e 0 0\nvb bulk 0 -1\nm1 c g e bulk mm w=10u l=2u\n\
         .model mm nmos(level=2 vto=0.7 kp=60u gamma=0.5 phi=0.7 vmax=5e4 neff=2 xj=0.3u)",
        "vd c 0 0.4\nvg g 0 -2.5\nvs e 0 0\nvb bulk 0 0.6\nm1 c g e bulk mm w=8u l=2u dtemp=20\n\
         .model mm pmos(level=2 tox=25n vto=-0.8 u0=250 gamma=0.4 lambda=0.03 delta=0.3 \
         ucrit=3e4 uexp=0.2 rs=60 rd=30)",
    ] {
        let c = circuit(body);
        let context = ModelContext::default();
        let history = c.state_history();
        let solved = bias(&c);
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
            residual(&a, &b, x)
        };
        for node in c.nodes().nodes() {
            let Some(col) = c.unknowns().node_row(node.id) else {
                continue;
            };
            let h = 1e-6;
            let mut low = solved.values.clone();
            let mut high = solved.values.clone();
            low.as_mut_slice()[col] -= h;
            high.as_mut_slice()[col] += h;
            let (rl, rh) = (load(&low), load(&high));
            for r in 0..n {
                let numerical = (rh[r] - rl[r]) / (2. * h);
                close(numerical, entry(&system.a, r, col), 1e-6, 1e-12);
            }
        }
    }
}

#[test]
fn noise_runs_through_the_shared_mos_generators() {
    let plots = run(
        "vdd vdd 0 5\nv1 in 0 dc 1.5 ac 1\nm1 out in 0 0 mm w=10u l=2u\nr1 vdd out 10k\n\
         .model mm nmos(level=2 vto=0.8 kp=1e-4 kf=1e-24 af=1.2 rd=20 rs=10)\n\
         .noise v(out) v1 dec 2 1k 100k 1",
    )
    .unwrap();
    let spectrum = &plots[0];
    for name in [
        "onoise_m1_rd",
        "onoise_m1_rs",
        "onoise_m1_id",
        "onoise_m1_1overf",
        "onoise_m1",
    ] {
        let value = spectrum.value(name, 0).unwrap_or_else(|| panic!("{name}"));
        assert!(value.re > 0., "{name}");
    }
}

#[test]
fn unported_and_invalid_mos2_inputs_fail_explicitly() {
    for body in [
        // Level-3 setters that MOS2mParam does not have (E_BADPARM).
        "m1 d g 0 0 mm\n.model mm nmos(level=2 theta=0.1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 kappa=0.2)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 eta=0.1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 xl=0.1u)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 wd=0.1u)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 delvto=0.1)",
        // TOX = 0 is an infinite oxide capacitance in C.
        "m1 d g 0 0 mm\n.model mm nmos(level=2 tox=0)",
        // mos2temp.c only warns for L - 2 LD <= 0; the port refuses it.
        "m1 d g 0 0 mm l=1u\n.model mm nmos(level=2 ld=0.5u)",
        // NSUB below the intrinsic density (mos2temp.c "Nsub < Ni").
        "m1 d g 0 0 mm\n.model mm nmos(level=2 nsub=1e9)",
        // mos2temp.c "Phi is not positive".
        "m1 d g 0 0 mm\n.model mm nmos(level=2 phi=0)",
        // C takes the logarithm of UCRIT and the square root of NEFF.
        "m1 d g 0 0 mm\n.model mm nmos(level=2 ucrit=0)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 neff=0)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 mjsw=1)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 nlev=4)",
        "m1 d g 0 0 mm\n.model mm nmos(level=2 tpg=0.5)",
    ] {
        assert!(Circuit::from_netlist(&deck(body)).is_err(), "{body}");
    }
    // A swept length that C would only warn about is refused too.
    let c = circuit("vd d 0 1\nm1 d d 0 0 mm l=2u\n.model mm nmos(level=2 ld=0.5u)");
    let mos = c.devices().iter().find(|d| d.designator() == 'm').unwrap();
    assert!(
        mos.with_instance_parameter("l", 1e-6, &ModelContext::default())
            .is_err()
    );
    assert!(
        mos.with_instance_parameter("l", 3e-6, &ModelContext::default())
            .is_ok()
    );
    // .disto (mos2dset.c) and .sens are not ported: explicit errors, never
    // a silently linear or missing device.
    let stage = "vdd vdd 0 5\nv1 in 0 dc 1.5 ac 1 distof1 0.1\nm1 out in 0 0 mm w=10u l=2u\n\
                 r1 vdd out 10k\n.model mm nmos(level=2 vto=0.8 kp=1e-4)";
    let error = run(&format!("{stage}\n.disto dec 2 1k 10k")).unwrap_err();
    assert!(
        matches!(&error, SpiceError::NotYetPorted { c_reference, .. } if c_reference.contains("mos2dset.c")),
        "{error}"
    );
    assert!(run(&format!("{stage}\n.sens v(out)")).is_err());
}

#[test]
fn m10_mos2_fixtures_parse_write_parse_with_a_fixed_point() {
    for name in ["m10_mos2_dc", "m10_mos2_ac", "m10_mos2_tran"] {
        let n = Parser::new()
            .parse_file(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("conformance/netlists/{name}.cir")),
            )
            .unwrap();
        let text = ngspice_rs::netlist::write_netlist(&n).unwrap();
        let round = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("round.cir"), &text))
            .unwrap();
        assert!(ngspice_rs::netlist::semantic_eq(&n, &round));
        assert_eq!(text, ngspice_rs::netlist::write_netlist(&round).unwrap());
    }
}

/// `inpgmod.c` discards `m` on a `.model` card ("no instance parameter default
/// for level and multiplier ... since we already have that info from pass1"),
/// while the other instance parameters on that card become model defaults.
/// Measured against ngspice-47+: `m1 d g s 0 mm` with
/// `.model mm nmos level=1 m=3 w=25u l=12u` draws C's `m = 1` current
/// (8.333e-5 A at vgs = 2 V), not three times it.
fn model_card_current(body: &str) -> f64 {
    let mut c = circuit(&format!("vd d 0 3\nvg g 0 2\nvsense s 0 0\n{body}"));
    op(&mut c);
    let plot = op(&mut circuit(&format!(
        "vd d 0 3\nvg g 0 2\nvsense s 0 0\n{body}"
    )));
    plot.points[0][plot.variable_index("i(vsense)").unwrap()].re
}

#[test]
fn a_model_card_multiplier_is_discarded_like_inpgmod() {
    let defaults = model_card_current("m1 d g s 0 mm\n.model mm nmos level=1 m=3 w=25u l=12u");
    let no_multiplier = model_card_current("m1 d g s 0 mm\n.model mm nmos level=1 w=25u l=12u");
    close(defaults, no_multiplier, 1e-12, 1e-18);
    // The 25/12 geometry default is applied, so this is not a "nothing set" pass.
    let geometry = model_card_current("m1 d g s 0 mm\n.model mm nmos level=1 w=25u l=12u");
    let drawn = model_card_current("m1 d g s 0 mm w=25u l=12u\n.model mm nmos level=1");
    close(geometry, drawn, 1e-12, 1e-18);
    // An instance multiplier still scales the current by three.
    let instance = model_card_current("m1 d g s 0 mm m=3\n.model mm nmos level=1 w=25u l=12u");
    close(instance, 3. * no_multiplier, 1e-12, 1e-18);
}
