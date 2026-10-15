//! MESA level 2 (#83, M10 H1a): the front end, the `inpdomod.c` level map,
//! C's `pmf` -> NMF coercion, the operating point and its asks against the
//! reference binary, the internal-node order and the explicit boundaries of
//! this slice.
//!
//! The reference values below were printed by
//! `ngspice -b` from `ngspice_test/build/src/ngspice` on the same decks. They
//! agree to their printed precision, which `cargo xtask golden verify` now
//! checks for `m10_mesa_op`, `m10_mesa_dc` and `m10_mesa_ac`; the tolerances
//! here state that agreement. `.op`, `.dc` and the small-signal assembly
//! (`mesaacl.c`, which `.ac`, `.sp` and `.tf` all use) are ported; transient
//! charge storage, `.pz` and the resistance paths are not.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, NodeKind, SpiceError, SpiceResult};
use std::path::Path;

fn deck(body: &str) -> Result<ngspice_rs::netlist::ast::Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("mesa.cir"),
        &format!("MESA\n{body}\n.end\n"),
    ))
}
fn circuit(body: &str) -> Result<Circuit, SpiceError> {
    Circuit::from_netlist(&deck(body)?)
}
fn attempt(c: &mut Circuit, kind: AnalysisKind, arguments: &[&str]) -> SpiceResult<Plot> {
    runner(kind).unwrap().run(
        c,
        &AnalysisRequest::with_arguments(kind, arguments.iter().copied()),
        &AnalysisContext::default(),
    )
}
fn run(c: &mut Circuit, kind: AnalysisKind, arguments: &[&str]) -> Plot {
    attempt(c, kind, arguments).unwrap()
}
fn value(plot: &Plot, name: &str, point: usize) -> f64 {
    let index = plot
        .variable_index(name)
        .unwrap_or_else(|| panic!("missing {name}: {:?}", plot.variables));
    plot.points[point][index].re
}
fn phase(plot: &Plot, name: &str, point: usize) -> (f64, f64) {
    let index = plot
        .variable_index(name)
        .unwrap_or_else(|| panic!("missing {name}: {:?}", plot.variables));
    let value = plot.points[point][index];
    (value.re, value.im)
}
fn close(a: f64, b: f64, relative: f64, absolute: f64) {
    assert!(
        (a - b).abs() <= relative * b.abs() + absolute,
        "{a:e} != {b:e}"
    );
}

/// The deck the reference values below were measured on: a 0 V source as an
/// ammeter in the drain, `l = 1u`, `w = 20u` and `MESAsetup`'s defaults.
fn probe(body: &str) -> String {
    format!(
        "vds 1 0 0.5\nvids 1 2 0\nvgs 3 0 0\nz1 2 3 0 zm l=1u w=20u\n\
         .model zm {body}"
    )
}

/// `MESApTable`'s setters and the four families' shared grammar.
#[test]
fn the_level_two_card_and_the_instance_setters_are_accepted() {
    let netlist = deck(
        "z1 d g s zm l=2u W=20u m=3 td=50 ts=40 dtemp=2\n\
         z2 d g s zm2\n\
         .model zm nmf(level=2 rd=31 rs=31)\n\
         .model zm2 pmf(level=2)",
    )
    .unwrap();
    let names: Vec<&str> = netlist.devices[0]
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["l", "w", "m", "td", "ts", "dtemp"]);
    assert!(circuit("vd d 0 1\nz1 d g s zm\n.model zm nmf(level=2)").is_ok());
    // MESA has no `area`, so the unlabeled leading value stays a parse gap.
    let error = circuit("vd d 0 1\nz1 d g s zm 2\n.model zm nmf(level=2)").unwrap_err();
    assert!(error.to_string().contains("area="), "{error}");
}

/// `mesasetup.c` reports "Only nmf model type supported, set to nmf" and
/// forces the type, so a `pmf` MESA model simulates as an N-channel device.
/// The port reproduces the model rather than the message.
#[test]
fn a_pmf_mesa_model_simulates_as_nmf() {
    let mut n = circuit(&probe("nmf(level=2)")).unwrap();
    let mut p = circuit(&probe("pmf(level=2)")).unwrap();
    let a = value(
        &run(&mut n, AnalysisKind::OperatingPoint, &[]),
        "i(vids)",
        0,
    );
    let b = value(
        &run(&mut p, AnalysisKind::OperatingPoint, &[]),
        "i(vids)",
        0,
    );
    close(b, a, 0., 0.);
    assert!(a > 0., "normalized drain current {a}");
}

/// `ngspice -b` on the probe deck prints `vids#branch` = 1.659047e-05 at
/// `vgs = -1.2` (deep subthreshold, where the `b/n0` term dominates `ns`) and
/// 1.926577e-03 at `vgs = 0` (strong inversion, where the
/// `1/(ND D (1 - sqrt1))` term does). The two together pin the channel, its
/// slope and `vsate`.
#[test]
fn the_operating_point_matches_the_reference_binary() {
    let mut c = circuit(&probe("nmf(level=2)")).unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["vgs", "-1.2", "0", "1.2"]);
    close(value(&plot, "i(vids)", 0), 1.659047e-05, 1e-5, 1e-15);
    close(value(&plot, "i(vids)", 1), 1.926577e-03, 1e-5, 0.);
    close(value(&plot, "v(2)", 1), 0.5, 0., 1e-12);

    let mut c = circuit(
        "vds 1 0 0.05\nvids 1 2 0\nvgs 3 0 0\nz1 2 3 0 zm l=1u w=20u\n\
         .model zm nmf(level=2)",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(value(&plot, "i(vids)", 0), 3.203192e-04, 1e-5, 0.);
}

/// `mesaask.c` at `vg = 0`, `vd = 0.5`, from the same binary. The junctions
/// and the channel's derivatives are separate checks: `gds` carried the
/// `ns` error most visibly, `gm` next.
#[test]
fn the_device_asks_match_the_reference_binary() {
    let mut c = circuit(&probe("nmf(level=2)")).unwrap();
    c.set_observations(&[
        "@z1[gm]".into(),
        "@z1[gds]".into(),
        "@z1[cd]".into(),
        "@z1[cg]".into(),
        "@z1[cgd]".into(),
        "@z1[ggs]".into(),
        "@z1[ggd]".into(),
        "@z1[vgs]".into(),
        "@z1[vgd]".into(),
        "@z1[l]".into(),
        "@z1[w]".into(),
        "@z1[m]".into(),
    ])
    .unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["vgs", "0", "0.4", "0.4"]);
    close(value(&plot, "@z1[gm]", 0), 2.671573e-03, 1e-5, 0.);
    close(value(&plot, "@z1[gds]", 0), 1.113757e-03, 1e-5, 0.);
    close(value(&plot, "@z1[cd]", 0), 1.926577e-03, 1e-5, 0.);
    close(value(&plot, "@z1[cg]", 0), -1.01219e-09, 1e-5, 1e-15);
    close(value(&plot, "@z1[cgd]", 0), -1.01219e-09, 1e-5, 1e-15);
    close(value(&plot, "@z1[ggs]", 0), 6.406188e-09, 1e-5, 1e-15);
    close(value(&plot, "@z1[ggd]", 0), 3.940594e-10, 1e-5, 1e-15);
    close(value(&plot, "@z1[vgs]", 0), 0., 0., 1e-15);
    close(value(&plot, "@z1[vgd]", 0), -0.5, 0., 1e-12);
    // `MESAask` leaves LENGTH alone and scales WIDTH by M.
    close(value(&plot, "@z1[l]", 0), 1e-6, 0., 1e-18);
    close(value(&plot, "@z1[w]", 0), 20e-6, 0., 1e-18);
    close(value(&plot, "@z1[m]", 0), 1., 0., 1e-15);
}

/// `.model m nmf(d=0.24u)` is `mesa1`'s device depth, not the `d` model-type
/// flag: `mesaload.c`'s `vpo` scales with `d^2`, and doubling the depth takes
/// the reference binary's `vids#branch` from 1.926577e-03 A to 4.162441e-04 A.
/// A bare `d` stays a flag (`tests/flags_ic_parser.rs`).
#[test]
fn the_device_depth_setter_is_an_assignment_not_a_flag() {
    let mut c = circuit(&probe("nmf(level=2 d=0.24u)")).unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(value(&plot, "i(vids)", 0), 4.162441e-04, 1e-5, 0.);
    close(value(&plot, "v(2)", 0), 0.5, 0., 1e-12);
}

/// `MESAsetup` creates `#gate` for `RG` (and nothing for `rd`/`rs`, which
/// `mesaload.c` applies without nodes) and uses the external gate when it is
/// zero.
#[test]
fn gate_resistance_creates_the_gate_node() {
    let c = circuit(
        "vd d 0 3\nvg g 0 0\nz1 d g 0 zm\nz2 d g 0 zn\n\
         .model zm nmf(level=2 rg=10)\n\
         .model zn nmf(level=2)",
    )
    .unwrap();
    let gate = c.nodes().get("z1#gate").expect("gate prime");
    assert_eq!(c.nodes().node(gate).unwrap().kind, NodeKind::Internal);
    assert!(c.nodes().get("z2#gate").is_none());
    assert!(
        c.nodes().get("z1#source").is_none() && c.nodes().get("z1#drain").is_none(),
        "the reference binary creates no node for rd/rs"
    );
}

/// `mesaparam.c` stores `td`/`ts` as `value + CONSTCtoK` and `mesaask.c`
/// reports them back in Celsius, so a deck round-trips.
#[test]
fn the_drain_and_source_temperatures_round_trip_in_celsius() {
    let mut c = circuit(
        "vd d 0 1\nz1 d g s zm td=26.85 ts=50\n\
         .model zm nmf(level=2)",
    )
    .unwrap();
    c.set_observations(&["@z1[td]".into(), "@z1[ts]".into()])
        .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(value(&plot, "@z1[td]", 0), 26.85, 1e-12, 1e-12);
    close(value(&plot, "@z1[ts]", 0), 50., 1e-12, 1e-12);
}

/// The boundaries of this slice, each naming the C reference it is missing
/// from: every resistance path other than `rg` (`rd`/`rs` fold into
/// `mesaload.c`'s `MESAtRd`/`MESAtRs` with no node, `ri`/`rf`/`rdi`/`rsi` add
/// the `#gs`/`#gd` and prime-prime nodes), the charge storage of a transient
/// load (H1b) and the model domains `mesa1` divides by.
#[test]
fn the_h1a_boundaries_are_explicit() {
    for setter in ["rd=1", "rs=1", "ri=1", "rf=1", "rdi=1", "rsi=1"] {
        let error = circuit(&format!(
            "vd d 0 1\nz1 d g s zm\n.model zm nmf(level=2 {setter})"
        ))
        .unwrap_err();
        assert!(error.is_not_yet_ported(), "{setter}: {error}");
        assert!(error.to_string().contains("mesa"), "{setter}: {error}");
    }
    for setter in ["vsigma=0", "nd=0", "mu=-1"] {
        let error = circuit(&format!(
            "vd d 0 1\nz1 d g s zm\n.model zm nmf(level=2 {setter})"
        ))
        .unwrap_err();
        assert!(
            matches!(
                error,
                SpiceError::Parse { .. }
                    | SpiceError::Circuit { .. }
                    | SpiceError::Unsupported { .. }
            ),
            "{setter}: {error}"
        );
    }
    let mut c = circuit(&probe("nmf(level=2)")).unwrap();
    let error = attempt(&mut c, AnalysisKind::Transient, &["1n", "10n"]).unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("mesa"), "{error}");
}

/// `mesaacl.c` is not pending work: `.ac` already matches the reference
/// binary, which the `m10_mesa_ac` fixture now verifies. The values are that
/// golden's first and last points (1 MHz and 1 GHz) for the same pair as
/// `m10_mesa_op`, including the `rg = 10` device's `#gate` node.
#[test]
fn the_small_signal_sweep_matches_the_reference_binary() {
    let mut c = circuit(
        "vds 1 0 dc 0.5 ac 1\nvids 1 2 dc 0\nvgs 3 0 dc -1.2\n\
         z1 2 3 0 m1 l=1u w=20u\nz2 2 3 0 m2 l=2u w=40u\n\
         .model m1 nmf(level=2)\n.model m2 nmf(level=2 rg=10)",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::Ac, &["dec", "2", "1meg", "1g"]);
    let (re, im) = phase(&plot, "i(vds)", 0);
    close(re, -4.718125954256094e-05, 1e-12, 1e-18);
    close(im, -2.062884307211449e-08, 1e-12, 1e-18);
    let (re, im) = phase(&plot, "i(vds)", 6);
    close(re, -4.718328428487106e-05, 1e-12, 1e-18);
    close(im, -2.062883685225082e-05, 1e-12, 1e-18);
    // The internal node `rg` creates is part of the small-signal plot.
    let (re, _) = phase(&plot, "v(z2#gate)", 0);
    close(re, -7.225285158125405e-07, 1e-9, 1e-15);
}
