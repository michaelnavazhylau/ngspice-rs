//! HFET1 (`Z` level 5, `hfet1/`): the `inpdomod.c` level map and the
//! `nhfet`/`phfet` polarity, the conditional internal nodes, the `hfeta`
//! channel with both gate junctions, `gatemod`, the `a1`/`a2` corrections, the
//! `cds` charge and the explicit boundaries.
//!
//! The reference values below were printed by `ngspice -b` from
//! `ngspice_test/build/src/ngspice` on the same decks; `cargo xtask golden
//! verify` checks five of them (`m10_hfet1_{dc,op,ac,tran,pz}`).

use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, NodeKind, SpiceError, SpiceResult};
use std::path::Path;

fn deck(body: &str) -> Result<ngspice_rs::netlist::ast::Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("hfet1.cir"),
        &format!("HFET1\n{body}\n.end\n"),
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

/// The upstream deck `tests/hfet/id_vgs.cir`, less its `.print`.
fn upstream(body: &str) -> String {
    format!(
        "z1 1 2 0 hfet l=1u w=10u\n\
         vgs 2 0 dc 0.3\nvds 1 0 dc 1.0\n\
         .model hfet {body}"
    )
}

/// `inpdomod.c` accepts `nhfet`/`phfet` as the `nmf`/`pmf` spellings and picks
/// the device from `level`: 5 is HFET1 whatever the base is. The base also
/// sets the polarity, and an explicit `nhfet` flag later in the card wins -
/// the reference binary answers -2.10801e-04 A for `nhfet level=5` and
/// -1.14799e-05 A for `phfet level=5`, and -2.10801e-04 A again for
/// `phfet level=5 nhfet`.
#[test]
fn the_level_map_and_the_polarity_flags() {
    let probe = |base: &str| {
        let mut c = circuit(&format!(
            "z1 1 2 0 hfet l=1u w=10u\nvgs 2 0 dc 0.3\nvds 1 0 dc 1.0\n\
             .model hfet {base} rdi=0 rsi=0 m=2.57 lambda=0.17 vs=1.5e5 mu=0.385 \
             vt0=0.13 eta=1.32 sigma0=0.04 vsigma=0.1 vsigmat=0.3 nmax=6e15"
        ))
        .unwrap();
        run(&mut c, AnalysisKind::OperatingPoint, &[])
    };
    close(
        value(&probe("nhfet level=5"), "i(vds)", 0),
        -2.108_007_560_825_823e-4,
        1e-9,
        1e-15,
    );
    // The p-type base: the reference binary's own answer for this card.
    close(
        value(&probe("phfet level=5"), "i(vds)", 0),
        -1.147_99e-5,
        2e-4,
        1e-15,
    );
    // A later flag overrides the base, as C applies the setters in card order.
    close(
        value(&probe("phfet level=5 nhfet"), "i(vds)", 0),
        -2.108_007_560_825_823e-4,
        1e-9,
        1e-15,
    );
    // `nmf` is the same device at level 5: C's `inpdomod.c` only reads `level`.
    close(
        value(&probe("nmf level=5"), "i(vds)", 0),
        -2.108_007_560_825_823e-4,
        1e-9,
        1e-15,
    );
}

/// `hfetsetup.c` creates `#source` for `rs`, `#drain` for `rd`, `#gate` for
/// `rg`, `#gs` for `ri` and `#gd` for `rf`, each only when its resistance is
/// non-zero, and leaves the prime-prime pair on the prime nodes otherwise.
#[test]
fn the_resistance_paths_create_their_nodes() {
    let c = circuit(
        "vd d 0 3\nvg g 0 0\n\
         z1 d g 0 za\nz2 d g 0 zb\n\
         .model za nhfet level=5 rd=1 rs=1\n\
         .model zb nhfet level=5 ri=1 rf=1 rg=1 rd=1 rs=1",
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
    for suffix in ["source", "drain", "gate", "gs", "gd"] {
        assert_eq!(
            internal(&format!("z2#{suffix}")),
            Some(NodeKind::Internal),
            "{suffix}"
        );
    }
}

/// `hfetload.c`'s channel with every series path live (`rd`/`rs`/`rg`/`ri`/
/// `rf`/`rdi`/`rsi`): the reference binary prints `i(vds)` =
/// -1.6929623840912424e-3 A and `i(vgs)` = 1.8525600842256296e-9 A for
/// `l=2u w=20u m=2`, `vgs = 0.6`, `vds = 1.2`.
#[test]
fn every_series_path_matches_the_reference_binary() {
    let mut c = circuit(
        "z1 1 2 0 hfet l=2u w=20u m=2\nvgs 2 0 dc 0.6\nvds 1 0 dc 1.2\n\
         .model hfet nhfet level=5 rd=31 rs=31 rg=10 ri=1e3 rf=2e3 rdi=50 rsi=50 \
         js1s=1e-10 js1d=1e-10",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(
        value(&plot, "i(vds)", 0),
        -1.692_962_384_091_242_4e-3,
        1e-9,
        1e-15,
    );
    close(
        value(&plot, "i(vgs)", 0),
        1.852_560_084_225_629_6e-9,
        1e-6,
        1e-15,
    );
}

/// The p-type device: `phfet` with negative supplies, the reference binary's
/// `i(vds)` = 2.328043559982988e-6 A and `i(vgs)` = -3.761164856372234e-10 A.
#[test]
fn the_p_channel_card_matches_the_reference_binary() {
    let mut c = circuit(
        "z1 1 2 0 hfet l=1u w=10u\nvgs 2 0 dc -0.3\nvds 1 0 dc -1.0\n\
         .model hfet phfet level=5 rd=60 rs=60 vt0=-0.3 js1s=1e-12 js1d=1e-12",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    close(
        value(&plot, "i(vds)", 0),
        2.328_043_559_982_988e-6,
        1e-9,
        1e-15,
    );
    close(
        value(&plot, "i(vgs)", 0),
        -3.761_164_856_372_234e-10,
        1e-6,
        1e-15,
    );
}

/// The `id_vgs` drain sweep: 21 points, the last at 1 V with `i(vds)` =
/// -2.108007503825823e-4 A.
#[test]
fn the_upstream_drain_sweep_matches_the_reference_binary() {
    let mut c = circuit(&format!(
        "{}\n.dc vds 0 1 0.05",
        upstream(
            "nhfet level=5 rdi=0 rsi=0 m=2.57 lambda=0.17 vs=1.5e5 mu=0.385 \
             vt0=0.13 eta=1.32 sigma0=0.04 vsigma=0.1 vsigmat=0.3 js1s=0 js1d=0 nmax=6e15"
        )
    ))
    .unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["vds", "0", "1", "0.05"]);
    assert_eq!(plot.point_count(), 21);
    close(value(&plot, "v(v-sweep)", 20), 1., 1e-12, 1e-15);
    close(
        value(&plot, "i(vds)", 20),
        -2.108_007_503_825_823e-4,
        1e-9,
        1e-15,
    );
}

/// C gates the `a1`/`a2` corrections on `gatemod != 0` as well as on the
/// coefficients: the reference binary answers 5.707950675392426e-4 A for this
/// card without `gatemod` and 5.707819384864532e-4 A with it, so a port that
/// applied the correction unconditionally would be wrong in the first case
/// (that was this port's own bug).
#[test]
fn the_a1_a2_corrections_need_gatemod() {
    let probe = |gatemod: &str| {
        let mut c = circuit(&format!(
            "z1 2 3 0 hfem l=1u w=10u\nvds 1 0 dc 2.0\nvids 1 2 dc 0\nvgs 3 0 dc 0.6\n\
             .model hfem nhfet level=5 {gatemod} a1=0.02 a2=0.5 mv1=3 cm3=0.17 \
             kappa=0.1 fgds=1e6 delf=1e5 rd=60 rs=60 vt0=0.3 js1s=1e-12 js1d=1e-12"
        ))
        .unwrap();
        run(&mut c, AnalysisKind::OperatingPoint, &[])
    };
    close(
        value(&probe(""), "i(vids)", 0),
        5.707_950_675_392_426e-4,
        1e-9,
        1e-15,
    );
    close(
        value(&probe("gatemod=1"), "i(vids)", 0),
        5.707_819_384_864_532e-4,
        1e-9,
        1e-15,
    );
}

/// `hfetask.c`'s bias asks at the same `gatemod=1` bias, `m`-scaled: the
/// reference binary's `show z1` prints `gm` = 0.00217641 and `gds` =
/// 0.000165535 for the AC fixture's `hfem` card.
#[test]
fn the_device_asks_match_the_reference_binary() {
    let mut c = circuit(
        "z1 2 3 0 hfem l=1u w=10u\nvds 1 0 dc 2.0\nvids 1 2 dc 0\nvgs 3 0 dc 0.6\n\
         .model hfem nhfet level=5 gatemod=1 a1=0.02 a2=0.5 mv1=3 cm3=0.17 \
         kappa=0.1 fgds=1e6 delf=1e5 rd=60 rs=60 vt0=0.3 js1s=1e-12 js1d=1e-12",
    )
    .unwrap();
    c.set_observations(&[
        "@z1[gm]".into(),
        "@z1[gds]".into(),
        "@z1[ggs]".into(),
        "@z1[cg]".into(),
        "@z1[cd]".into(),
    ])
    .unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    // The printed precision of C's `show` is six digits.
    close(value(&plot, "@z1[gm]", 0), 2.17641e-3, 1e-5, 1e-12);
    close(value(&plot, "@z1[gds]", 0), 1.65535e-4, 1e-5, 1e-12);
    close(value(&plot, "@z1[ggs]", 0), 9.67145e-9, 1e-5, 1e-15);
    close(value(&plot, "@z1[cg]", 0), 1.2604e-8, 1e-5, 1e-15);
    close(value(&plot, "@z1[cd]", 0), 5.70782e-4, 1e-5, 1e-12);
}

/// `hfet1init.c` leaves `DEVnoise`/`DEVdisto` NULL, so both analyses run with
/// the card contributing nothing, and `.sens` is refused (#164).
#[test]
fn a_hfet1_card_is_noiseless_and_linear_in_distortion() {
    // The deck whose C numbers are pinned in the MESA test as well: a 1 kOhm
    // load with the device off, so the spectrum is the resistor's.
    let mut c = circuit(
        "vdd 1 0 dc 3 ac 1\nr1 1 2 1k\nz1 2 3 0 zm l=1u w=20u\nvg 3 0 dc -2.5\n\
         .model zm nhfet level=5 rg=10",
    )
    .unwrap();
    let plot = run(
        &mut c,
        AnalysisKind::Noise,
        &["v(2)", "vdd", "dec", "2", "1k", "10k"],
    );
    close(
        value(&plot, "inoise_spectrum", 0),
        4.071_371_529_487e-9,
        1e-9,
        1e-18,
    );
    close(
        value(&plot, "onoise_spectrum", 0),
        4.131_546_162_878e-9,
        1e-9,
        1e-18,
    );
    let mut c = circuit(
        "vds 1 0 dc 0.5 ac 1\nvids 1 2 dc 0\nvg 3 0 dc -1 ac 1\nr1 2 0 1k\n\
         z1 2 3 0 zm l=1u w=20u\n.model zm nhfet level=5 rg=10",
    )
    .unwrap();
    let plot = run(&mut c, AnalysisKind::Distortion, &["dec", "2", "1k", "10k"]);
    for name in ["v(2)", "i(vids)"] {
        for point in 0..plot.point_count() {
            close(value(&plot, name, point), 0., 0., 1e-18);
        }
    }
    // `.sens` is not a no-contribution case: C's modern `.sens` enumerates the
    // card's parameter tables through `DEVparam`, so it *does* report `z1:*`.
    let netlist = deck(
        "vdd 1 0 dc 3 ac 1\nr1 1 drain 1k\nz1 drain gate 0 zm l=1u w=20u\n\
         vg gate 0 dc -2.5\n.model zm nhfet level=5 rg=10\n.sens v(drain)",
    )
    .unwrap();
    let config = ngspice_rs::analysis::RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut c = config.circuit(&netlist).unwrap();
    let error = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &config.context())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("sensitivity"), "{error}");
}
