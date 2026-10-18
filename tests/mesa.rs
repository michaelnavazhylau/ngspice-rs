//! MESA level 2 (#83, M10 H1a/H1b): the front end, the `inpdomod.c` level map,
//! C's `pmf` -> NMF coercion, the operating point and its asks against the
//! reference binary, the internal-node order, the resistance paths, the charge
//! storage and the explicit boundaries of the slice.
//!
//! The reference values below were printed by
//! `ngspice -b` from `ngspice_test/build/src/ngspice` on the same decks. They
//! agree to their printed precision, which `cargo xtask golden verify`
//! checks for `m10_mesa_{op,dc,ac,res_op,res_dc,res_ac,tran,pz}`; the
//! tolerances here state that agreement. Every C load for level 2 is ported
//! (`mesaload.c` with its charge storage, `mesaacl.c`, `mesapzl.c`);
//! `.noise`/`.disto` behave as C's NULL routines do, and `.sens` is refused.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
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
    // `mesainit.c` leaves `DEVnoise`/`DEVdisto`/`DEVsen*` NULL. `.noise` and
    // `.disto` therefore run with the card contributing nothing, which is
    // what the reference binary does; `.sens` is different - C's modern
    // `.sens` enumerates every device's parameter tables through
    // `DEVparam`/`DEVload`, so it *does* report the `z1:*` parameters - and
    // still refuses here (#164).
    let mut c = circuit(
        "vdd 1 0 dc 3 ac 1\nr1 1 2 1k\nz1 2 3 0 zm l=1u w=20u\nvg 3 0 dc -2.5\n\
         .model zm nmf(level=2 rg=10)",
    )
    .unwrap();
    let plot = run(
        &mut c,
        AnalysisKind::Noise,
        &["v(2)", "vdd", "dec", "2", "1k", "10k"],
    );
    // The 1 kOhm load's own noise, as the reference binary prints it: the
    // card adds none of its own.
    close(
        value(&plot, "inoise_spectrum", 0),
        4.071371529487329e-9,
        1e-9,
        1e-18,
    );
    close(
        value(&plot, "onoise_spectrum", 0),
        4.191272227871375e-9,
        1e-9,
        1e-18,
    );
    // `.sens` itself now runs for a level-2 card (#164); the *AC* sensitivity of
    // a nonlinear device stays refused.
    let netlist = deck(
        "vdd 1 0 dc 3 ac 1\nr1 1 drain 1k\nz1 drain gate 0 zm l=1u w=20u\n\
         vg gate 0 dc -2.5\n.model zm nmf(level=2 rg=10)\n\
         .sens v(drain) ac dec 2 1k 10k",
    )
    .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut c = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap_err();
    assert!(error.to_string().contains(".sens ac"), "{error}");
    let mut c = circuit(
        "vds 1 0 dc 0.5 ac 1\nvids 1 2 dc 0\nvg 3 0 dc -1 ac 1\nr1 2 0 1k\n\
         z1 2 3 0 zm l=1u w=20u\n.model zm nmf(level=2 rg=10 ri=1e3)\n\
         .disto dec 2 1k 10k",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::Distortion, &["dec", "2", "1k", "10k"]);
    for name in ["v(2)", "i(vids)", "v(z1#gs)"] {
        for point in 0..plot.point_count() {
            close(value(&plot, name, point), 0., 0., 1e-18);
        }
    }
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

/// `mesa2` (level 3): the two-population channel with the four regions of
/// `nsa`'s `vpou`/`vpod` selection. The values are the `m10_mesa_l3_*`
/// fixtures' — the upstream `tests/mesa/mesa21.cir` card carries
/// 3.750424463803059e-2 A at `vgs = 0`/`vds = 4`, and the sweep reaches
/// 1.184032126626747e-2 A at 4 V.
#[test]
fn the_level_three_channel_matches_the_reference_binary() {
    let mut c = circuit(
        "z1 3 2 0 mesmod l=1u w=150u\nvgs 2 0 dc 0\nvds 1 0 dc 4\nvids 1 3 dc 0\n\
         .model mesmod nmf level=3 rdi=2.7 rsi=2.7 mu=0.2 m=2.2 vto=-2.04 \
         eta=1.5 lambda=0.04 tc=0.001 sigma0=0.02 vsigma=0.1 vsigmat=1.37 \
         delta=5 beta=0.0085",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(
        value(&plot, "i(vds)", 0),
        -3.750_424_463_803_059e-2,
        1e-9,
        1e-15,
    );
    let mut c = circuit(
        "z1 3 2 0 mesmod l=1u w=150u\nvgs 2 0 dc -1\nvds 1 0 dc 0\nvids 1 3 dc 0\n\
         .model mesmod nmf level=3 rdi=2.7 rsi=2.7 mu=0.2 m=2.2 vto=-2.04 \
         eta=1.5 lambda=0.04 tc=0.001 sigma0=0.02 vsigma=0.1 vsigmat=1.37 \
         delta=5 beta=0.0085 ndu=1e22 ndelta=6e24 du=0.035e-6 th=0.01e-6",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["vds", "0", "4", "0.1"]);
    assert_eq!(plot.point_count(), 41);
    close(
        value(&plot, "i(vds)", 40),
        -1.184_032_126_626_747e-2,
        1e-9,
        1e-15,
    );
}

/// `mesa3` (level 4): the `nmax`/`gamma` knee with the `cas`/`cbs`/`epsi`
/// model, in both polarities and in transient (where those capacitances
/// reach the load). Reference values from the `m10_mesa_l4_*` fixtures.
#[test]
fn the_level_four_channel_matches_the_reference_binary() {
    let mut c = circuit(
        "vdd 1 0 dc 3\nvgg 2 0 dc 0.5\nz1 1 2 0 n4 l=1u w=20u\n\
         .model n4 nmf level=4 rd=31 rs=31 rdi=20 rsi=20 mu=0.3 gamma=2.5 \
         nmax=4e16 cas=0.9 cbs=1.1 epsi=1.142189e-10",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(
        value(&plot, "i(vdd)", 0),
        -3.600_846_442_234_158e-3,
        1e-8,
        1e-15,
    );
    close(
        value(&plot, "i(vgg)", 0),
        -3.256_904_067_548_815e-4,
        1e-6,
        1e-15,
    );
    // The p-type device: `phfet` at negative supplies. The reference binary
    // prints i(vds) = 9.024984093780782e-5 A for the level-6 deck; the level-4
    // card below answers 6.408350022225523e-2 A for its own.
    let mut c = circuit(
        "vdd2 3 0 dc -3\nvgg2 4 0 dc -0.5\nz2 3 4 0 p4 l=1u w=20u\n\
         .model p4 pmf level=4 rd=31 rs=31 mu=0.3 nmax=4e16",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(
        value(&plot, "i(vdd2)", 0),
        6.408_350_022_225_523e-2,
        1e-7,
        1e-15,
    );
    let mut c = circuit(
        "vdd 1 0 dc 3\nvg 3 0 dc 0 pulse(-1.5 0.5 0.5n 0.1n 0.1n 1n 4n)\n\
         rg 3 2 1k\nz1 1 2 0 n4 l=1u w=20u\nr1 1 2 1k\n\
         .model n4 nmf level=4 rd=31 rs=31 mu=0.3 gamma=2.5 nmax=4e16 \
         cas=0.9 cbs=1.1 epsi=1.142189e-10\n.tran 0.05n 3n",
    )
    .unwrap();
    // This harness does not resolve `.option` cards, so the run below uses C's
    // default tolerances and its accepted grid is not the fixture's (113
    // points, checked by `cargo xtask golden verify`). The t = 0 solution is
    // the operating point and independent of the step control.
    let plot = run(&mut c, AnalysisKind::Transient, &["0.05n", "3n"]);
    // With the pulse on the gate (through `rg`) the level-4 gate junction
    // conducts: the node sits 0.23 V below the divider's 0.75 V and the edges
    // push 2.02 mA through `rg`.
    close(
        value(&plot, "v(2)", 0),
        5.199_741_145_085_228e-1,
        1e-6,
        1e-9,
    );
    close(
        value(&plot, "i(vdd)", 0),
        -6.302_723_570_347_829e-3,
        1e-6,
        1e-12,
    );
    close(
        value(&plot, "i(vg)", 0),
        2.019_974_114_508_523e-3,
        1e-6,
        1e-12,
    );
}

/// Level 4's *swept* small-signal analyses are refused, because C's
/// `mesaacl.c`/`mesapzl.c` build their conductances from `gm0`/`gm1`/`gm2`,
/// `gds0` and `delidgch0`/`delidvds0`/`delidvds1`, which `mesa1` and `mesa2`
/// store and `mesa3` never writes. The reference binary therefore answers
/// 1.07e-8 A for a card that carries -1.55e-4 A in DC, at both 1 kHz and
/// 1 MHz. A `.dc` sweep still works (its source probe needs only the correct
/// linearisation, which the port computes), and so does `.tf`, which
/// linearises the DC Jacobian rather than loading `mesaacl.c`.
#[test]
fn the_level_four_small_signal_paths_are_refused() {
    let deck = |analysis: &str| {
        format!(
            "vds 1 0 dc 2.0 ac 1\nvids 1 2 dc 0\nvgs 3 0 dc 0.4\n\
             z1 2 3 0 m4 l=1u w=20u\n\
             .model m4 nmf level=4 rd=31 rs=31 mu=0.3 nmax=4e16\n{analysis}"
        )
    };
    // `.ac` and `.pz` are the swept path; `.sp` reaches the same assembly but
    // needs `portnum` cards first, so those two carry the check.
    for (kind, arguments) in [
        (AnalysisKind::Ac, vec!["dec", "2", "1k", "1meg"]),
        (
            AnalysisKind::PoleZero,
            vec!["3", "0", "2", "0", "vol", "pz"],
        ),
    ] {
        let mut c = circuit(&deck("")).unwrap();
        let error = attempt(&mut c, kind, &arguments).unwrap_err();
        assert!(error.is_not_yet_ported(), "{kind:?}: {error}");
        assert!(error.to_string().contains("level 4"), "{kind:?}: {error}");
    }
    // The DC sweep is unaffected: it is the `m10_mesa_l4_dc` fixture, and the
    // reference binary agrees with it to round-off.
    let mut c = circuit(
        "z1 1 2 0 m4 l=2u w=40u m=2\nvgs 2 0 dc 0.3\nvds 1 0 dc 2.0\n\
         .model m4 nmf level=4 rd=50 rs=50 rdi=30 rsi=30 mu=0.35 nmax=3e16 cas=1 cbs=1",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["vds", "0", "3", "0.1"]);
    close(
        value(&plot, "i(vds)", 30),
        -7.869_868_653_972_65e-3,
        1e-7,
        1e-15,
    );
}

/// Level 4's `.tf` is **not** refused: C's `CKTtf` linearises the converged DC
/// Jacobian instead of loading `mesaacl.c`, so the channel's real `gm` and
/// `gds` are in the system and the answer is a genuine transfer function. The
/// reference binary answers 1.818568640594385e-01 for the gain, 1.222280009536926e+03
/// for the input impedance and 1.818568640594385e+02 for the output impedance.
#[test]
fn the_level_four_transfer_function_matches_the_reference_binary() {
    let mut c = circuit(
        "vds 1 0 dc 2.0\nvgs 3 0 dc 0.4\nz1 2 3 0 m4 l=1u w=20u\nrd 1 2 1k\n\
         .model m4 nmf level=4 rd=31 rs=31 mu=0.3 nmax=4e16\n.tf v(2) vds",
    )
    .unwrap();
    // The card tokenizer splits `v(2)` into `v ( 2 )` before the driver
    // sees it, so the arguments are passed pre-split here.
    let plot = run(
        &mut c,
        AnalysisKind::TransferFunction,
        &["v", "(", "2", ")", "vds"],
    );
    close(
        value(&plot, "v(Transfer_function)", 0),
        1.818_568_640_594_385e-1,
        1e-8,
        1e-15,
    );
    close(
        value(&plot, "v(vds#Input_impedance)", 0),
        1.222_280_009_536_926e3,
        1e-8,
        1e-12,
    );
    close(
        value(&plot, "v(output_impedance_at_V(2))", 0),
        1.818_568_640_594_385e2,
        1e-8,
        1e-12,
    );
    // The swept small-signal path stays refused even with `.tf` as a sibling
    // analysis, so the guard is per-assembly, not per-deck.
    let mut c = circuit(
        "vds 1 0 dc 2.0 ac 1\nvgs 3 0 dc 0.4\nz1 2 3 0 m4 l=1u w=20u\nrd 1 2 1k\n\
         .model m4 nmf level=4 rd=31 rs=31 mu=0.3 nmax=4e16",
    )
    .unwrap();
    let error = attempt(&mut c, AnalysisKind::Ac, &["dec", "2", "1k", "1meg"]).unwrap_err();
    assert!(error.to_string().contains("AC/SP/pole-zero"), "{error}");
}

/// A mixed-polarity level-4 `.op` has **two** roots, and C's continuation
/// picks a different one from the port's. Retained as a reproducer, not as a
/// golden: pinning the `.op` root would pin a continuation accident.
///
/// Measured with the reference binary on this card, the n-channel device
/// settles at `@z1[vgs] = 4.287953e-01`, `@z1[gm] = 2.024911e-14` (channel cut
/// off), `@z1[cd] = 9.567193e-08` and `@z1[cg] = 2.296831e-03` A. That is a
/// genuine fixed point: the gate current feeds the source resistor, so
/// `0.5 - 0.4288 = 0.0712 V` across `RS = 31` carries exactly 2.297 mA. The
/// port instead lands on the *conducting* root.
///
/// The conducting root is not the port's invention: the same n-channel card
/// alone, with `vgg` swept up to the same 0.5 V, is answered by the reference
/// binary as `i(vdd) = -3.785019e-3` and `i(vgg) = -2.768870e-4` A - the
/// values the port reaches in the two-device deck, to the last printed digit.
/// The p-channel device agrees between the two implementations either way
/// (`i(vdd2) = 6.408350e-2` A).
#[test]
fn the_mixed_polarity_level_four_operating_point_is_the_conducting_root() {
    let mut c = circuit(
        "vdd 1 0 dc 3\nvgg 2 0 dc 0.5\nz1 1 2 0 n4 l=1u w=20u\n\
         vdd2 3 0 dc -3\nvgg2 4 0 dc -0.5\nz2 3 4 0 p4 l=1u w=20u\n\
         .model n4 nmf level=4 rd=31 rs=31 mu=0.3 gamma=2.5 nmax=4e16 \
         cas=0.9 cbs=1.1 epsi=1.142189e-10\n\
         .model p4 pmf level=4 rd=31 rs=31 mu=0.3 nmax=4e16",
    )
    .unwrap();
    let op = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(value(&op, "i(vdd)", 0), -3.785_017_694_202e-3, 5e-5, 1e-12);
    close(value(&op, "i(vgg)", 0), -2.768_986_527_183e-4, 5e-5, 1e-12);
    // The p-channel device's root is not in dispute.
    close(value(&op, "i(vdd2)", 0), 6.408_350_300_858e-2, 5e-5, 1e-12);
    close(value(&op, "v(1)", 0), 3., 0., 1e-15);
    close(value(&op, "v(3)", 0), -3., 0., 1e-15);

    // The same n-channel card alone, swept up to the same bias: this run is
    // bit-identical to the reference binary and carries the conducting root.
    let mut c = circuit(
        "vdd 1 0 dc 3\nvgg 2 0 dc 0.5\nz1 1 2 0 n4 l=1u w=20u\n\
         .model n4 nmf level=4 rd=31 rs=31 mu=0.3 gamma=2.5 nmax=4e16 \
         cas=0.9 cbs=1.1 epsi=1.142189e-10",
    )
    .unwrap();
    let sweep = run(
        &mut c,
        AnalysisKind::DcSweep,
        &["vgg", "-0.4", "0.7", "0.1"],
    );
    assert_eq!(sweep.point_count(), 12);
    // The point at vgg = 0.5 V is index 9.
    close(value(&sweep, "i(vdd)", 9), -3.785_019e-3, 1e-6, 1e-12);
    close(value(&sweep, "i(vgg)", 9), -2.768_870e-4, 1e-5, 1e-12);
    close(value(&sweep, "i(vdd)", 11), -3.972_156e-3, 1e-6, 1e-12);
    close(value(&sweep, "i(vgg)", 11), -4.260_834e-3, 1e-6, 1e-12);
}

/// `.sens` (#164) for MESA levels 2 and 4, and the explicit refusal for level 3.
///
/// Level 4 is verified by `sens_mesa4` (a level-4 card whose output is the
/// drain current, not `sens_mesa`'s ~1 uV gate node, where every row is a
/// cancellation-limited difference). Level 3 is **not** verified: its `mesa2`
/// channel leaves `du` 16% and `th` 24% from C on an otherwise agreeing deck
/// (see `docs/port/SENSITIVITY.md`).
#[test]
fn sensitivity_follows_mesa_cs_table_for_levels_two_and_four_and_refuses_three() {
    let cards = |level: u8, analysis: &str| {
        format!(
            "vds 1 0 dc 0.5\nvids 1 2 dc 0\nvgs 4 0 dc 0\nrg 4 3 1k\n\
             z1 2 3 0 m{level} l=1u w=20u\n.model m{level} nmf(level={level})\n{analysis}"
        )
    };
    for level in [2, 3, 4] {
        let text = cards(level, ".sens v(3)");
        let parsed = deck(&text).unwrap();
        let config = ngspice_rs::analysis::RunConfig::from_netlist(&parsed).unwrap();
        let request = config.request_for(&parsed.analyses[0]).unwrap();
        let mut c = config.circuit(&parsed).unwrap();
        let result = ngspice_rs::analysis::runner(request.kind).unwrap().run(
            &mut c,
            &request,
            &config.context(),
        );
        if level != 3 {
            let plot = result.unwrap();
            let names: Vec<&str> = plot
                .variables
                .iter()
                .map(|variable| variable.name.as_str())
                .filter(|name| name.starts_with("v(z1"))
                .collect();
            // `mesa.c`'s 61 model setters plus 8 instance ones.
            assert_eq!(names.len(), 69, "{names:?}");
            for absent in [
                "v(z1:vt0)",
                "v(z1:tphib)",
                "v(z1:tggr)",
                "v(z1:alphat)",
                "v(z1:gd)",
                "v(z1:vcrit)",
            ] {
                assert!(!names.contains(&absent), "{absent} in {names:?}");
            }
        } else {
            let error = result.unwrap_err();
            assert!(error.is_not_yet_ported(), "level {level}: {error}");
            assert!(
                error
                    .to_string()
                    .contains(&format!("MESA level {level} sensitivity")),
                "{error}"
            );
        }
    }
}

/// A level-4 instance moved by `TS`/`DTEMP` still differs from C on the two
/// RTC rows (measured: 0.808x C's `rtc1`/`rtc2` at `ts=40`), so it is refused
/// rather than reported. Level 2 verifies with `dtemp` set (`sens_mesa`).
#[test]
fn sensitivity_refuses_a_level_four_instance_at_a_shifted_temperature() {
    for shift in ["ts=40", "dtemp=10"] {
        let text = format!(
            "vds 1 0 dc 1
vids 1 2 dc 0
vgs 4 0 dc 0.5
rg 4 3 1k
             z1 2 3 0 m4 l=1u w=20u {shift}
.model m4 nmf(level=4)
.sens i(vids)"
        );
        let parsed = deck(&text).unwrap();
        let config = ngspice_rs::analysis::RunConfig::from_netlist(&parsed).unwrap();
        let request = config.request_for(&parsed.analyses[0]).unwrap();
        let mut c = config.circuit(&parsed).unwrap();
        let error = ngspice_rs::analysis::runner(request.kind)
            .unwrap()
            .run(&mut c, &request, &config.context())
            .unwrap_err();
        assert!(error.is_not_yet_ported(), "{shift}: {error}");
        assert!(
            error
                .to_string()
                .contains("MESA level 4 sensitivity at a shifted temperature"),
            "{shift}: {error}"
        );
    }
}
