//! MOS model binning through the production parser, subcircuit expansion and
//! model resolver (#109). C bins only BSIM3/BSIM4/HiSIM2/HiSIM-HV models, none
//! of which this port simulates, so a selected bin is observed through the
//! explicit `NotYetPorted` error that names it; every C failure case is a
//! parse error. `tests/c_binning_reference.rs` checks the same selections
//! against the C binary.

use std::path::Path;

use ngspice_rs::analysis::RunConfig;
use ngspice_rs::devices::binning::{BinOptions, model_name_match};
use ngspice_rs::devices::{Circuit, ModelResolver};
use ngspice_rs::netlist::ast::Netlist;
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::SpiceError;

const BINS: &str = "\
.model nch.1 nmos level=8 lmin=0.5u lmax=1u wmin=0.5u wmax=5u vth0=0.3
.model nch.2 nmos level=8 lmin=1u lmax=5u wmin=0.5u wmax=5u vth0=0.5
.model nch.3 nmos level=8 lmin=0.1u lmax=1u wmin=5u wmax=20u vth0=0.7
";

fn parse(body: &str) -> Netlist {
    let text = format!("binning\nvd d 0 1\nvg g 0 1\n{body}.op\n.end\n");
    Parser::new()
        .parse_deck(&parse_deck_text(Path::new("bins.cir"), &text))
        .unwrap()
}

fn elaborate(body: &str) -> SpiceError {
    let netlist = parse(body);
    let config = RunConfig::from_netlist(&netlist).unwrap();
    config
        .circuit(&netlist)
        .expect_err("binned BSIM decks are not simulated")
}

/// The bin a deck selects, read from the `NotYetPorted` diagnostic.
fn selected(body: &str) -> String {
    let error = elaborate(body);
    assert!(error.is_not_yet_ported(), "{error}");
    let text = error.to_string();
    let start = text
        .find("binned to model '")
        .unwrap_or_else(|| panic!("{text}"))
        + "binned to model '".len();
    text[start..start + text[start..].find('\'').unwrap()].to_owned()
}

fn rejected(body: &str) -> String {
    let error = elaborate(body);
    assert!(matches!(error, SpiceError::Parse { .. }), "{error}");
    error.to_string()
}

#[test]
fn selects_by_length_and_width_with_inclusive_edges() {
    assert_eq!(
        selected(&format!("m1 d g 0 0 nch w=1u l=0.7u\n{BINS}")),
        "nch.1"
    );
    assert_eq!(
        selected(&format!("m1 d g 0 0 nch w=1u l=2u\n{BINS}")),
        "nch.2"
    );
    assert_eq!(
        selected(&format!("m1 d g 0 0 nch w=10u l=0.5u\n{BINS}")),
        "nch.3"
    );
    // L=1u is inside nch.1 (lmax) and nch.2 (lmin): the last declared wins.
    assert_eq!(
        selected(&format!("m1 d g 0 0 nch w=1u l=1u\n{BINS}")),
        "nch.2"
    );
    // W=5u, L=1u is inside all three; nch.3 is declared last.
    assert_eq!(
        selected(&format!("m1 d g 0 0 nch w=5u l=1u\n{BINS}")),
        "nch.3"
    );
    // A 0.9 nm excess is still on the edge, 1.1 nm is not.
    assert_eq!(
        selected(&format!("m1 d g 0 0 nch w=1u l=5.0009u\n{BINS}")),
        "nch.2"
    );
    assert!(
        rejected(&format!("m1 d g 0 0 nch w=1u l=5.0011u\n{BINS}"))
            .contains("no bin with all of lmin/lmax/wmin/wmax")
    );
}

#[test]
fn declaration_order_breaks_overlap_and_multiplier_is_ignored() {
    let reversed = "\
.model nch.2 nmos level=8 lmin=1u lmax=5u wmin=0.5u wmax=5u
.model nch.1 nmos level=8 lmin=0.5u lmax=1u wmin=0.5u wmax=5u
";
    assert_eq!(
        selected(&format!("m1 d g 0 0 nch w=1u l=1u\n{reversed}")),
        "nch.1"
    );
    assert_eq!(
        selected(&format!("m1 d g 0 0 nch w=1u l=0.7u m=8\n{BINS}")),
        "nch.1"
    );
    // Forward references bin like any other declaration.
    assert_eq!(
        selected(&format!("{BINS}m1 d g 0 0 NCH W=1U L=0.7U\n")),
        "nch.1"
    );
}

#[test]
fn exact_names_win_and_suffixes_must_be_digits() {
    let exact = "\
.model nch.1 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=5u
.model nch nmos level=8
";
    let error = elaborate(&format!("m1 d g 0 0 nch w=1u l=1u\n{exact}"));
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(!error.to_string().contains("binned"), "{error}");
    assert!(error.to_string().contains("model 'nch'"), "{error}");
    assert_eq!(
        selected(
            "m1 d g 0 0 nch w=1u l=1u\n.model nch.01 nmos level=49 lmin=0.5u lmax=2u wmin=0.5u wmax=5u\n"
        ),
        "nch.01"
    );
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("bins.cir"),
        "t\nm1 d g 0 0 nch w=1u l=1u\n.model nch.a nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=5u\n.end\n",
    ));
    assert!(netlist.is_err());
    assert!(model_name_match("nch", "nch.12").is_some());
}

#[test]
fn c_failure_cases_are_explicit_parse_errors() {
    let level1 = ".model nch.1 nmos level=1 lmin=0.5u lmax=2u wmin=0.5u wmax=5u\n";
    let message = rejected(&format!("m1 d g 0 0 nch w=1u l=1u\n{level1}"));
    assert!(message.contains("ngspice bins only BSIM3"), "{message}");
    assert!(message.contains("'nch.1' (bins.cir:5:1)"), "{message}");
    let message = rejected(&format!("m1 d g 0 0 nch l=1u\n{BINS}"));
    assert!(message.contains("needs both l and w"), "{message}");
    let message = rejected(&format!("m1 d g 0 0 nch w=1u l=10u\n{BINS}"));
    assert!(message.contains(" W=1e-6;"), "{message}");
    // A candidate without all four bounds is skipped.
    let partial = ".model nch.1 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u\n";
    assert!(rejected(&format!("m1 d g 0 0 nch w=1u l=1u\n{partial}")).contains("no bin"));
}

#[test]
fn bjt_references_are_never_binned() {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("bins.cir"),
        "t\nq1 c b 0 qn\n.model qn.1 npn\n.end\n",
    ));
    assert!(netlist.is_err());
}

#[test]
fn subcircuit_bins_shadow_outer_models_like_subckt_c() {
    let local = "\
x1 d g inv
.subckt inv a b
m1 a b 0 0 nch w=1u l=1u
.model nch.1 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=20u
.model nch.2 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=20u
.ends
";
    // Local bins are renamed with the instance path; the last declared wins.
    assert_eq!(selected(&format!("{local}{BINS}")), "x1.nch.2");
    // Root bins are used when the body declares none.
    let global = "\
x1 d g inv
.subckt inv a b
m1 a b 0 0 nch w=1u l=0.7u
.ends
";
    assert_eq!(selected(&format!("{global}{BINS}")), "nch.1");
    // A local bin set captures the name even when none of it fits and the
    // root has a fitting bin or an exact model (C: "could not find a valid
    // modelname").
    let narrow = "\
x1 d g inv
.subckt inv a b
m1 a b 0 0 nch w=10u l=1u
.model nch.1 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=2u
.ends
";
    let message = rejected(&format!("{narrow}{BINS}"));
    assert!(message.contains("model 'x1.nch'"), "{message}");
    let message = rejected(&format!("{narrow}.model nch nmos level=1\n"));
    assert!(message.contains("'x1.nch.1'"), "{message}");
    // An exact local model still shadows root bins.
    let exact = "\
x1 d g inv
.subckt inv a b
m1 a b 0 0 nch w=1u l=1u
.model nch nmos level=8
.ends
";
    let error = elaborate(&format!("{exact}{BINS}"));
    assert!(error.to_string().contains("model 'x1.nch'"), "{error}");
}

#[test]
fn root_models_may_not_join_a_local_bin_set() {
    let local = "\
x1 d g inv
.subckt inv a b
m1 a b 0 0 nch w=1u l=1u
.model nch.1 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=20u
.ends
";
    for intruder in [
        ".model x1.nch.7 nmos level=8 lmin=0.5u lmax=2u wmin=0.5u wmax=20u\n",
        ".model x1.nch nmos level=8\n",
    ] {
        let message = rejected(&format!("{local}{intruder}"));
        assert!(
            message.contains("binned subcircuit models 'x1.nch.<n>'"),
            "{message}"
        );
    }
}

#[test]
fn resolver_exposes_candidates_and_selection() {
    let netlist = parse(&format!("m1 d g 0 0 nch w=2u l=2u\n{BINS}"));
    let resolver = ModelResolver::new(&netlist.models).unwrap();
    let names = |cards: Vec<&ngspice_rs::netlist::ast::ModelCard>| {
        cards.iter().map(|c| c.name.clone()).collect::<Vec<_>>()
    };
    assert_eq!(
        names(resolver.bin_candidates("NCH")),
        ["nch.1", "nch.2", "nch.3"]
    );
    assert_eq!(
        names(resolver.declarations_for("nch")),
        ["nch.1", "nch.2", "nch.3"]
    );
    assert_eq!(names(resolver.declarations_for("nch.2")), ["nch.2"]);
    let mos = netlist
        .devices
        .iter()
        .find(|d| d.designator == 'm')
        .unwrap();
    assert_eq!(resolver.select_bin(mos).unwrap().unwrap().name, "nch.2");
    // `.options scale` is not yet a deck setting; the API takes it explicitly.
    let scaled = ModelResolver::new(&netlist.models)
        .unwrap()
        .with_bin_options(BinOptions {
            scale: 0.35,
            wnflag: false,
        });
    assert_eq!(scaled.select_bin(mos).unwrap().unwrap().name, "nch.1");
    // An exact-name or non-MOS reference is not a binning lookup.
    let vd = netlist.devices.iter().find(|d| d.name == "vd").unwrap();
    assert!(resolver.select_bin(vd).unwrap().is_none());
    assert!(Circuit::from_netlist(&netlist).is_err());
}
