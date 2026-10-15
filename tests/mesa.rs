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

/// `MESAsetup` creates `#source` for `rd`, `#drain` for `rs`, `#gate` for
/// `rg`, `#gs` for `ri` and `#gd` for `rf`, each only when its resistance is
/// non-zero, and leaves each prime-prime node on its prime node otherwise.
/// `rdi`/`rsi` are the channel's series resistances (`mesa1`'s `RS`/`RD`) and
/// create no node at all.
#[test]
fn the_resistance_paths_create_their_nodes() {
    let c = circuit(
        "vd d 0 3\nvg g 0 0\n\
         z1 d g 0 za\nz2 d g 0 zb\nz3 d g 0 zc\n\
         .model za nmf(level=2 rd=1 rs=1)\n\
         .model zb nmf(level=2 ri=1 rf=1)\n\
         .model zc nmf(level=2 rd=1 rs=1 rg=1 ri=1 rf=1 rdi=1 rsi=1)",
    )
    .unwrap();
    let internal = |name: &str| {
        c.nodes()
            .get(name)
            .map(|id| c.nodes().node(id).unwrap().kind)
    };
    assert_eq!(internal("z1#source"), Some(NodeKind::Internal));
    assert_eq!(internal("z1#drain"), Some(NodeKind::Internal));
    assert_eq!(internal("z1#gate"), None);
    assert_eq!(internal("z1#gs"), None);
    assert_eq!(internal("z2#gs"), Some(NodeKind::Internal));
    assert_eq!(internal("z2#gd"), Some(NodeKind::Internal));
    assert_eq!(internal("z2#source"), None);
    assert_eq!(internal("z2#drain"), None);
    for suffix in ["source", "drain", "gate", "gs", "gd"] {
        assert_eq!(
            internal(&format!("z3#{suffix}")),
            Some(NodeKind::Internal),
            "{suffix}"
        );
    }
    // `rdi`/`rsi` create nothing: `mesatemp.c` folds them into `mesa1`'s
    // `RS`/`RD` terms.
    assert_eq!(internal("z3#rdi"), None);
    assert_eq!(internal("z3#rsi"), None);
}

/// `mesatemp.c` scales `rd`, `rdi` and `rf` at `td` and `rs`, `rsi`, `rg` and
/// `ri` at `ts`, each by `1 + tc1 (T - tnom) + tc2 (T - tnom)^2` with
/// `tc1`/`tc2` spelled `rtc1`/`rtc2` in the model card (`mesa.c`'s
/// `MESAmPTable`). The pair below is the discriminator: at `dtemp = 100` with
/// the coefficients left at zero the reference binary prints
/// 1.2572424291901912e-4 A, and the same deck with `rtc1 = 2e-3`,
/// `rtc2 = 1e-6` prints 1.2493367719759038e-4 A.
#[test]
fn the_resistances_use_the_rtc1_and_rtc2_temperature_scaling() {
    let at = |body: &str, dtemp: &str| {
        let mut c = circuit(&format!(
            "vds 1 0 dc 0.5\nvids 1 2 dc 0\nvgs 3 0 dc -1.0\n\
             z1 2 3 0 zm l=1u w=20u{dtemp}\n.model zm {body}"
        ))
        .unwrap();
        run(&mut c, AnalysisKind::OperatingPoint, &[])
    };
    let plain = at("nmf(level=2 rd=31 rs=31)", " dtemp=100");
    close(
        value(&plain, "i(vids)", 0),
        1.257_242_429_190_191_2e-4,
        1e-9,
        1e-15,
    );
    let scaled = at("nmf(level=2 rd=31 rs=31 rtc1=2e-3 rtc2=1e-6)", " dtemp=100");
    close(
        value(&scaled, "i(vids)", 0),
        1.249_336_771_975_903_8e-4,
        1e-9,
        1e-15,
    );
    // `rdi`/`rsi` scale at `td` too and sit inside the channel.
    let intrinsic = at(
        "nmf(level=2 rd=31 rs=31 rdi=300 rsi=200 rtc1=2e-3 rtc2=1e-6)",
        " dtemp=100",
    );
    close(
        value(&intrinsic, "i(vids)", 0),
        1.055_285_920_583_728_5e-4,
        1e-9,
        1e-15,
    );
}

/// The reference binary's answers for the series paths, each a separate
/// mechanism: `rd`/`rs` between the external terminals and the prime nodes,
/// `ri`/`rf` between the prime and prime-prime nodes (`#gs`/`#gd`), and
/// `rdi`/`rsi` inside `mesa1`'s `RS`/`RD`. Measured with `ngspice -b` on the
/// probe deck at `vgs = -1.0`, `vds = 0.5`.
#[test]
fn the_resistance_paths_match_the_reference_binary() {
    let at = |body: &str| {
        let mut c = circuit(&format!(
            "vds 1 0 dc 0.5\nvids 1 2 dc 0\nvgs 3 0 dc -1.0\n\
             z1 2 3 0 zm l=1u w=20u\n.model zm {body}"
        ))
        .unwrap();
        run(&mut c, AnalysisKind::OperatingPoint, &[])
    };
    // `rd=rs=1e6` leaves the channel starved: 0.5 V across 2 MOhm.
    let plot = at("nmf(level=2 rd=1e6 rs=1e6)");
    close(
        value(&plot, "i(vids)", 0),
        2.561_533_403_153_888_7e-7,
        1e-9,
        1e-15,
    );
    close(value(&plot, "v(2)", 0), 0.5, 1e-12, 1e-12);
    // `ri`/`rf` put the intrinsic gate behind 1k/2k: `#gs` stays at the source
    // and `#gd` at the drain, and `#gate` does not exist without `rg`.
    let plot = at("nmf(level=2 ri=1e3 rf=2e3)");
    close(
        value(&plot, "i(vids)", 0),
        1.294_798_134_879_495_8e-4,
        1e-9,
        1e-15,
    );
    close(value(&plot, "v(z1#gs)", 0), 0., 1e-9, 1e-12);
    close(value(&plot, "v(z1#gd)", 0), 0.5, 1e-9, 1e-12);
    // `rdi`/`rsi` only move the channel's series resistance.
    let plot = at("nmf(level=2 rdi=300 rsi=200)");
    close(
        value(&plot, "i(vids)", 0),
        1.109_834_591_729_768_7e-4,
        1e-9,
        1e-15,
    );
    let plot = at("nmf(level=2 rd=31 rs=31 rdi=300 rsi=200)");
    close(
        value(&plot, "i(vids)", 0),
        1.083_309_886_025_265_7e-4,
        1e-9,
        1e-15,
    );
    // Every path at once, with `rg` too.
    let plot = at("nmf(level=2 rd=31 rs=31 rg=10 ri=1e3 rf=2e3 rdi=300 rsi=200)");
    close(
        value(&plot, "i(vids)", 0),
        1.083_310_920_599_555_7e-4,
        1e-9,
        1e-15,
    );
    close(
        value(&plot, "v(z1#gs)", 0),
        3.357_761_717_752_397_4e-3,
        1e-9,
        1e-12,
    );
    close(
        value(&plot, "v(z1#gd)", 0),
        0.496_641_736_146_141_4,
        1e-9,
        1e-12,
    );
    close(
        value(&plot, "v(z1#gate)", 0),
        -0.999_999_838_020_610_9,
        1e-9,
        1e-12,
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

/// The boundaries left after the resistance paths landed: the model domains
/// `mesa1` divides by, the charge storage of a transient load (#156) and the
/// pole-zero load (#157).
#[test]
fn the_remaining_boundaries_are_explicit() {
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
    // `mesainit.c` leaves `DEVnoise`/`DEVdisto`/`DEVsen*` NULL, so C's MESA
    // contributes nothing to those analyses and does not fail them. The port
    // is stricter: it refuses rather than silently omitting a device, which
    // #158 tracks. This is the remaining boundary the slice does not close.
    let mut c = circuit(
        "vds 1 0 dc 0.5\nvids 1 2 dc 0\nvgs 3 0 dc 0 ac 1\n\
         z1 2 3 0 zm l=1u w=20u\n.model zm nmf(level=2)",
    )
    .unwrap();
    let error = attempt(
        &mut c,
        AnalysisKind::Noise,
        &["v(2)", "vgs", "dec", "1", "1k", "1meg"],
    )
    .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("noise"), "{error}");
}

/// `mesatrunc.c`'s charge pairs and `mesaload.c`'s transient load, against the
/// reference binary on a gate pulse. The port lands on C's accepted timepoints
/// for this deck (100 points, identical times), so the values are pinned
/// directly: at 2.375 ns the drive has just returned to -1.5 V and the
/// reference binary prints `vids#branch` = 2.250146424221e-3 A.
#[test]
fn the_transient_load_matches_the_reference_binary() {
    let mut c = circuit(
        "vdd 1 0 dc 3\nvids 1 2 dc 0\n\
         vg 3 0 dc 0 pulse(-1.5 0 1n 0.2n 0.2n 2n 5n)\n\
         z1 2 3 0 zm l=1u w=20u\n.model zm nmf(level=2 rd=31 rs=31 rg=10)\n\
         .tran 0.05n 4n",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::Transient, &["0.05n", "4n"]);
    assert_eq!(plot.point_count(), 100);
    close(value(&plot, "time", 0), 0., 1e-12, 1e-15);
    close(value(&plot, "i(vids)", 0), 7.420938031561e-6, 1e-5, 1e-12);
    close(value(&plot, "v(z1#gate)", 0), -1.499961991175, 1e-9, 1e-12);
    close(value(&plot, "time", 99), 4e-9, 1e-9, 1e-15);
    close(value(&plot, "i(vids)", 99), 7.487777405496e-6, 1e-4, 1e-12);
    close(value(&plot, "v(z1#gate)", 99), -1.499959662869, 1e-7, 1e-9);
}

/// The charge storage is what makes the drain current move when the gate
/// drive does: the same deck held at a fixed gate has no capacitances in the
/// load at all, so the difference is the integration.
#[test]
fn the_transient_load_integrates_the_gate_charges() {
    let pulse = |body: &str| {
        circuit(&format!(
            "vdd 1 0 dc 3\nvids 1 2 dc 0\n\
             vg 3 0 dc 0 pulse(-1.5 0 1n 0.2n 0.2n 2n 5n)\n\
             z1 2 3 0 zm l=1u w=20u\n.model zm {body}\n\
             .tran 0.05n 4n"
        ))
        .unwrap()
    };
    let mut c = pulse("nmf(level=2 rd=31 rs=31 rg=10)");
    let plot = run(&mut c, AnalysisKind::Transient, &["0.05n", "4n"]);
    // Half way up the rise the gate charge is still being pushed through
    // `rg`, so the drain current leads the DC curve of the same deck.
    let moving = value(&plot, "i(vids)", 30);
    close(moving, 1.005292358136e-4, 1e-6, 1e-12);
    let mut flat = circuit(
        "vdd 1 0 dc 3\nvids 1 2 dc 0\nvg 3 0 dc -1.2375\n\
         z1 2 3 0 zm l=1u w=20u\n.model zm nmf(level=2 rd=31 rs=31 rg=10)\n\
         .op",
    )
    .unwrap();
    let settled = run(&mut flat, AnalysisKind::OperatingPoint, &[]);
    assert!(
        (value(&settled, "i(vids)", 0) - moving).abs() > 1e-7,
        "the transient load must carry the gate charge, not just the DC channel"
    );
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
