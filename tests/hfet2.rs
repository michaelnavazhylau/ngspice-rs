//! HFET2 (`Z` level 6, `hfet2/`): the `inpdomod.c` level map and the
//! `nhfet`/`phfet` polarity, the two conditional internal nodes, `hfeta2`'s
//! channel with the `eta2`/`d2` sub-threshold branch, the `pnjlim`/`fetlim`
//! limiting and the explicit boundaries.
//!
//! The reference values below were printed by `ngspice -b` from
//! `ngspice_test/build/src/ngspice` on the same decks; `cargo xtask golden
//! verify` checks six of them (`m10_hfet2_{op,dc,ac,tran,pz}` and the `.sens`
//! fixture `sens_hfet2`).

use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::{AnalysisKind, NodeKind, SpiceError, SpiceResult};
use std::path::Path;

fn deck(body: &str) -> Result<ngspice_rs::netlist::ast::Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("hfet2.cir"),
        &format!("HFET2\n{body}\n.end\n"),
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
fn close(a: f64, b: f64, relative: f64, absolute: f64) {
    assert!(
        (a - b).abs() <= relative * b.abs() + absolute,
        "{a:e} != {b:e}"
    );
}

/// `inpdomod.c` reaches HFET2 through `level=6` whatever the card base is
/// (`nhfet`, `phfet` or `nmf`), the base sets the polarity, and a later
/// `nhfet`/`phfet` flag overrides it. The reference binary answers
/// -7.342447395850566e-4 A for both N-channel spellings and
/// 9.024984093780782e-5 A for a p-channel card.
#[test]
fn the_level_map_and_the_polarity_flags() {
    let n = |base: &str| {
        let mut c = circuit(&format!(
            "z1 1 2 0 hfet2 l=1u w=10u\nvgs 2 0 dc 0.6\nvds 1 0 dc 1.5\n\
             .model hfet2 {base} rd=60 rs=60 rdi=50 rsi=50 js=1e-6"
        ))
        .unwrap();
        run(&mut c, AnalysisKind::OperatingPoint, &[])
    };
    close(
        value(&n("nhfet level=6"), "i(vds)", 0),
        -7.342_447_395_850_566e-4,
        1e-9,
        1e-15,
    );
    close(
        value(&n("nmf level=6"), "i(vds)", 0),
        -7.342_447_395_850_566e-4,
        1e-9,
        1e-15,
    );
    let p = |base: &str| {
        let mut c = circuit(&format!(
            "z1 1 2 0 hfet2 l=1u w=10u\nvgs 2 0 dc -0.6\nvds 1 0 dc -1.5\n\
             .model hfet2 {base} rd=60 rs=60 rdi=50 rsi=50 js=1e-6"
        ))
        .unwrap();
        run(&mut c, AnalysisKind::OperatingPoint, &[])
    };
    close(
        value(&p("phfet level=6"), "i(vds)", 0),
        9.024_984_093_780_782e-5,
        1e-9,
        1e-15,
    );
    // The flag wins over the base for HFET2 as it does for HFET1.
    close(
        value(&p("nhfet level=6 phfet"), "i(vds)", 0),
        9.024_984_093_780_782e-5,
        1e-9,
        1e-15,
    );
}

/// `hfet2setup.c` creates only `#source` for `rs` and `#drain` for `rd`.
/// There is no `#gate` node (the model has no `rg`) and no prime-prime pair,
/// which is what separates this level from MESA's and HFET1's node sets.
#[test]
fn only_the_source_and_drain_primes_exist() {
    let c = circuit(
        "vd d 0 3\nvg g 0 0\n\
         z1 d g 0 za\nz2 d g 0 zb\n\
         .model za nhfet level=6\n\
         .model zb nhfet level=6 rd=1 rs=1 ri=1 rf=1",
    )
    .unwrap_err();
    // `ri`/`rf` are not HFET2 setters at all: C's table has `rdi`/`rsi` for the
    // channel's series pair, and `ri`/`rf` are refused rather than ignored.
    assert!(
        matches!(
            c,
            SpiceError::Parse { .. } | SpiceError::Circuit { .. } | SpiceError::Unsupported { .. }
        ),
        "{c}"
    );
    let c = circuit(
        "vd d 0 3\nvg g 0 0\n\
         z1 d g 0 za\nz2 d g 0 zb\n\
         .model za nhfet level=6\n\
         .model zb nhfet level=6 rd=1 rs=1 rdi=1 rsi=1",
    )
    .unwrap();
    let internal = |name: &str| {
        c.nodes()
            .get(name)
            .map(|id| c.nodes().node(id).unwrap().kind)
    };
    assert_eq!(internal("z1#source"), None);
    assert_eq!(internal("z1#drain"), None);
    assert_eq!(internal("z2#source"), Some(NodeKind::Internal));
    assert_eq!(internal("z2#drain"), Some(NodeKind::Internal));
    for absent in ["z2#gate", "z2#gs", "z2#gd"] {
        assert_eq!(internal(absent), None, "{absent}");
    }
}

/// `hfeta2`'s channel with every series path live: the reference binary prints
/// `i(vds)` = -7.342447395850566e-4 A for `vgs = 0.6`, `vds = 1.5`, `js = 1e-6`
/// and `rd`/`rs`/`rdi`/`rsi` set.
#[test]
fn the_channel_matches_the_reference_binary() {
    let mut c = circuit(
        "z1 1 2 0 hfet2 l=1u w=10u\nvgs 2 0 dc 0.6\nvds 1 0 dc 1.5\n\
         .model hfet2 nhfet level=6 rd=60 rs=60 rdi=50 rsi=50 js=1e-6",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(
        value(&plot, "i(vds)", 0),
        -7.342_447_395_850_566e-4,
        1e-9,
        1e-15,
    );
}

/// HFET2 limits with `DEVpnjlim` *and* `DEVfetlim` (HFET1 has only the
/// latter), so a strongly forward-biased gate exercises a code path the other
/// levels do not have. The reference binary answers
/// -2.5518420603303153e-3 A at `vgs = 1.2` and -4.184056448965607e-3 A at
/// `vgs = 2.0` for the same card.
#[test]
fn the_forward_biased_gate_is_limited_like_c() {
    let probe = |vgs: f64| {
        let mut c = circuit(&format!(
            "z1 1 2 0 hfet2 l=1u w=10u\nvgs 2 0 dc {vgs}\nvds 1 0 dc 1.0\n\
             .model hfet2 nhfet level=6 rd=31 rs=31 js=1e-6 ggr=1e-3"
        ))
        .unwrap();
        run(&mut c, AnalysisKind::OperatingPoint, &[])
    };
    close(
        value(&probe(1.2), "i(vds)", 0),
        -2.551_842_060_330_315_3e-3,
        1e-9,
        1e-15,
    );
    close(
        value(&probe(2.0), "i(vds)", 0),
        -4.184_056_448_965_607e-3,
        1e-9,
        1e-15,
    );
}

/// The `eta2`/`d2` sub-threshold branch (`nsc`), which `hfeta2` only takes
/// when both setters are given: the reference binary's drain sweep reaches
/// -1.619316578789979e-3 A at 2 V for the `m10_hfet2_dc` card.
#[test]
fn the_two_exponential_branch_matches_the_reference_binary() {
    let mut c = circuit(
        "z1 1 2 0 h2 l=2u w=20u m=2\nvgs 2 0 dc 0.5\nvds 1 0 dc 1.0\n\
         .model h2 nhfet level=6 rd=31 rs=31 eta2=1.5 d2=0.1u",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["vds", "0", "2", "0.1"]);
    assert_eq!(plot.point_count(), 21);
    close(
        value(&plot, "i(vds)", 20),
        -1.619_316_578_789_979e-3,
        1e-9,
        1e-15,
    );
}

/// `hfet2init.c` leaves `DEVnoise`/`DEVdisto` NULL, so both analyses run with
/// the card contributing nothing.
#[test]
fn a_hfet2_card_is_noiseless_and_linear_in_distortion() {
    let mut c = circuit(
        "vdd 1 0 dc 3 ac 1\nr1 1 2 1k\nz1 2 3 0 zm l=1u w=20u\nvg 3 0 dc -2.5\n\
         .model zm nhfet level=6",
    )
    .unwrap();
    let plot = run(
        &mut c,
        AnalysisKind::Noise,
        &["v(2)", "vdd", "dec", "2", "1k", "10k"],
    );
    // The 1 kOhm load's own noise: the card adds none of it (the reference
    // binary prints 4.071371529487e-9 V/sqrt(Hz) for both spectra here).
    for point in 0..plot.point_count() {
        close(
            value(&plot, "onoise_spectrum", point),
            4.071_371_529_487e-9,
            1e-9,
            1e-18,
        );
        close(
            value(&plot, "inoise_spectrum", point),
            4.071_371_529_487e-9,
            1e-9,
            1e-18,
        );
    }
    let mut c = circuit(
        "vds 1 0 dc 0.5 ac 1\nvids 1 2 dc 0\nvg 3 0 dc -1 ac 1\nr1 2 0 1k\n\
         z1 2 3 0 zm l=1u w=20u\n.model zm nhfet level=6",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::Distortion, &["dec", "2", "1k", "10k"]);
    for name in ["v(2)", "i(vids)"] {
        for point in 0..plot.point_count() {
            close(value(&plot, name, point), 0., 0., 1e-18);
        }
    }
}

/// `.sens` over `hfet2.c`'s tables (#164): 36 `IF_REAL` model setters (minus
/// the `IF_STRING` `type` and the `IF_FLAG` `nhfet`/`phfet`) and six instance
/// ones. `m` passes the table filter but not `hfet2ask.c`'s switch - there is
/// no `HFET2_M` case, unlike `hfetask.c`'s `HFETA_M` - so C prints an error
/// and drops the column; `vto` is the `IOPR` alias of `vt0`.
#[test]
fn sensitivity_covers_hfet2s_tables() {
    let netlist = deck(
        "vds 1 0 dc 2.0\nvids 1 2 dc 0\nvgs 4 0 dc 0.2\nrg 4 3 1k\n\
         z1 2 3 0 n6 l=1u w=20u\n.model n6 nhfet level=6 eta2=2 d2=0.3e-6\n\
         .sens i(vids)",
    )
    .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let names: Vec<&str> = plot
        .variables
        .iter()
        .map(|variable| variable.name.as_str())
        .filter(|name| name.starts_with("v(z1"))
        .collect();
    assert_eq!(names.len(), 42, "{names:?}");
    for absent in ["v(z1_m)", "v(z1:vto)", "v(z1:nhfet)", "v(z1:phfet)"] {
        assert!(!names.contains(&absent), "{absent} in {names:?}");
    }
    for present in ["v(z1:cf)", "v(z1:d2)", "v(z1:vt2)", "v(z1_temp)"] {
        assert!(names.contains(&present), "{present} missing from {names:?}");
    }
}

/// `d2` and `eta2` are ordinary values, but their *given* flags select
/// `hfeta2`'s `nsc` branch: `hfet2temp.c` fills `N02` under `eta2Given` alone,
/// `hfet2load.c` reads `nsc` under `eta2Given && d2Given`, and the walk never
/// clears a flag once a setter has set it. The four cards therefore report two
/// derivatives and two topology jumps, and the reference binary answers
/// exactly these columns (#164).
#[test]
fn sensitivity_reports_cs_given_flag_topology_switches() {
    let columns = |card: &str| {
        let netlist = deck(&format!(
            "vds 1 0 dc 2.0\nvids 1 2 dc 0\nvgs 4 0 dc 0.2\nrg 4 3 1k\n\
             z1 2 3 0 n6 l=1u w=20u\n.model n6 nhfet level=6 {card}\n.sens i(vids)"
        ))
        .unwrap();
        let config = RunConfig::from_netlist(&netlist).unwrap();
        let request = config.request_for(&netlist.analyses[0]).unwrap();
        let mut circuit = config.circuit(&netlist).unwrap();
        let plot = runner(request.kind)
            .unwrap()
            .run(&mut circuit, &request, &config.context())
            .unwrap();
        (value(&plot, "v(z1:d2)", 0), value(&plot, "v(z1:eta2)", 0))
    };
    // Both given: the branch is on for both loads, so both rows are the
    // channel's own derivative of `nsc` (`N02` scales as 1/d2).
    let (d2, eta2) = columns("eta2=2 d2=0.3e-6");
    close(d2, -2.413_767_579_2e2, 1e-6, 1e-9);
    close(eta2, -7.252_681_536e-5, 1e-6, 1e-15);
    // `eta2` only: the `d2` setter turns the branch on in the perturbed load
    // alone, which is a difference of two model topologies rather than a
    // derivative.
    let (d2, eta2) = columns("eta2=2");
    close(d2, -8.320_879_307e8, 1e-6, 1e-9);
    close(eta2, -7.763_247_316e-5, 1e-6, 1e-15);
    // `d2` only: `N02` is zero (`eta2Given` is false) so the perturbed load's
    // channel dies at `hfeta2`'s cutoff and the row is the residual; the later
    // `eta2` setter then enables the branch with `N02` nonzero.
    let (d2, eta2) = columns("d2=0.3e-6");
    assert_eq!(d2, 0., "{d2}");
    close(eta2, -9.850_151_174e1, 1e-6, 1e-9);
    // Neither: the `d2` row dies the same way and the `eta2` row is the jump.
    let (d2, eta2) = columns("");
    assert_eq!(d2, 0., "{d2}");
    close(eta2, -8.320_883_195e1, 1e-6, 1e-9);
}

/// `hfet2ask.c` answers the instance temperature in Celsius while
/// `hfet2param.c` stores it in kelvin, so the walk steps 50 C by a relative
/// 1e-6 and stores 323.15 K - not a kelvin step across 323 K. `HFET2_VT1` is
/// computed by `hfet2temp.c` and read nowhere, so its column is exactly zero.
/// The reference binary prints these columns for the `sens_hfet2` card.
#[test]
fn sensitivity_perturbs_the_temperature_in_celsius() {
    let netlist = deck(
        "vds 1 0 dc 2.0\nvids 1 2 dc 0\nvgs 4 0 dc 0.5\nrg 4 3 1k\n\
         vgs2 8 0 dc 1.8\nrg2 8 7 1k\n\
         z1 2 3 0 nmos l=1u w=20u\nz2 2 3 0 nmos l=2u w=40u dtemp=10\n\
         z3 0 7 2 pmos l=1u w=20u temp=50\n\
         .model nmos nhfet level=6 eta2=2 d2=0.3e-6 rd=20 rs=30 rdi=5 rsi=7\n\
         .model pmos phfet level=6 eta2=2 d2=0.3e-6 rd=20 rs=30\n.sens i(vids)",
    )
    .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let plot = runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    close(
        value(&plot, "v(z1_temp)", 0),
        -1.431_432_772_145_406e-6,
        1e-6,
        1e-14,
    );
    close(
        value(&plot, "v(z2_temp)", 0),
        -1.659_321_851_234_211e-6,
        1e-6,
        1e-14,
    );
    close(
        value(&plot, "v(z3_temp)", 0),
        1.546_802_393_424_271e-8,
        1e-6,
        1e-15,
    );
    assert_eq!(value(&plot, "v(z1:vt1)", 0), 0., "HFET2_VT1 is unread");
    // The p-channel's raw `VTO` stays negative while its `TVTO` is scaled by
    // the type; both `vt0` and `vt2` rows are live.
    close(
        value(&plot, "v(z3:vt0)", 0),
        2.449_294_340_084_447e-4,
        1e-6,
        1e-12,
    );
    close(
        value(&plot, "v(z3:vt2)", 0),
        -3.856_160_039_760_514e-6,
        1e-6,
        1e-15,
    );
}
