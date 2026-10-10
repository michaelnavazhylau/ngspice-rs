//! #22: the explicit M1 front-end gate. Everything here goes through public
//! production interfaces (`Parser::parse_file`, `write_netlist`, `semantic_*`,
//! `snapshot::generate`). It never needs a C toolchain.
//!
//! A successful D/Q/M parse is syntax only: it is not nonlinear simulation
//! parity, and subcircuit flattening (#18, M5) is not exercised here.
use std::fs;
use std::path::{Path, PathBuf};

use ngspice_rs::netlist::ast::{Netlist, ScopedCardKind};
use ngspice_rs::netlist::snapshot::{self, SNAPSHOT_DIR};
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::netlist::{Parser, semantic_diff, semantic_eq, write_netlist};
use ngspice_rs::primitives::{AnalysisKind, SpiceError};

fn conformance() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

/// (instance, nodes, model) for each top-level device in deck order.
type Dev = (&'static str, &'static [&'static str], Option<&'static str>);

struct Expect {
    deck: &'static str,
    devices: &'static [Dev],
    /// (name, base) for each top-level model.
    models: &'static [(&'static str, &'static str)],
    subcircuits: &'static [&'static str],
    analysis: (AnalysisKind, &'static [&'static str]),
}

const DECKS: &[Expect] = &[
    Expect {
        deck: "rc_divider",
        devices: &[
            ("v1", &["in", "0"], None),
            ("r1", &["in", "out"], None),
            ("r2", &["out", "0"], None),
        ],
        models: &[],
        subcircuits: &[],
        analysis: (AnalysisKind::OperatingPoint, &[]),
    },
    Expect {
        deck: "rc_lowpass_ac",
        devices: &[
            ("v1", &["in", "0"], None),
            ("r1", &["in", "out"], None),
            ("c1", &["out", "0"], None),
        ],
        models: &[],
        subcircuits: &[],
        analysis: (AnalysisKind::Ac, &["lin", "3", "100", "1k"]),
    },
    Expect {
        deck: "rc_transient",
        devices: &[
            ("v1", &["in", "0"], None),
            ("r1", &["in", "out"], None),
            ("c1", &["out", "0"], None),
        ],
        models: &[],
        subcircuits: &[],
        analysis: (AnalysisKind::Transient, &["0.5u", "5u"]),
    },
    Expect {
        deck: "rlc_series",
        devices: &[
            ("v1", &["in", "0"], None),
            ("r1", &["in", "mid"], None),
            ("l1", &["mid", "out"], None),
            ("c1", &["out", "0"], None),
        ],
        models: &[],
        subcircuits: &[],
        analysis: (AnalysisKind::OperatingPoint, &[]),
    },
    Expect {
        deck: "diode_dc",
        devices: &[
            ("v1", &["in", "0"], None),
            ("r1", &["in", "out"], None),
            ("d1", &["out", "0"], Some("dmod")),
        ],
        models: &[("dmod", "d")],
        subcircuits: &[],
        analysis: (AnalysisKind::DcSweep, &["v1", "0", "1", "0.25"]),
    },
    Expect {
        deck: "bjt_ce",
        devices: &[
            ("vcc", &["vcc", "0"], None),
            ("vin", &["base", "0"], None),
            ("rc", &["vcc", "coll"], None),
            ("q1", &["coll", "base", "0"], Some("qmod")),
        ],
        models: &[("qmod", "npn")],
        subcircuits: &[],
        analysis: (AnalysisKind::OperatingPoint, &[]),
    },
    Expect {
        deck: "mos_inverter",
        devices: &[
            ("vdd", &["vdd", "0"], None),
            ("vin", &["gate", "0"], None),
            ("rd", &["vdd", "drain"], None),
            ("m1", &["drain", "gate", "0", "0"], Some("nmos")),
        ],
        models: &[("nmos", "nmos")],
        subcircuits: &[],
        analysis: (AnalysisKind::OperatingPoint, &[]),
    },
    Expect {
        deck: "subckt_divider",
        devices: &[
            ("v1", &["in", "0"], None),
            ("x1", &["in", "out"], Some("div")),
            ("r2", &["out", "0"], None),
        ],
        models: &[],
        subcircuits: &["div"],
        analysis: (AnalysisKind::OperatingPoint, &[]),
    },
];

/// Decks added for the M3 transient/AC and initialized-state conformance gates
/// (GitHub #48, #27).
const M3_GATE_DECKS: [&str; 12] = [
    "coupled_cap_tran",
    "floating_cap_ic_tran",
    "floating_cap_tran",
    "rc_gear_tran",
    "rc_ic_node_tran",
    "rc_ic_uic_tran",
    "rc_pwl_tran",
    "rl_pulse_tran",
    "rlc_ic_uic_tran",
    "rlc_series_ac",
    "rlc_series_gear_tran",
    "rlc_series_tran",
];

fn parse_fixture(rel: &str) -> Netlist {
    let path = conformance().join(rel);
    Parser::new()
        .parse_file(&path)
        .unwrap_or_else(|e| panic!("{rel}: {e}"))
}

struct TempDir(PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("spice-gate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn the_gate_covers_exactly_the_eight_corpus_decks() {
    let mut on_disk: Vec<String> = fs::read_dir(conformance().join("netlists"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cir"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    // The M3 exit-gate decks (#48) are separate fixtures with their own gate
    // (`xtask golden verify`, `tests/m3_gate.rs`); the M1 front-end
    // gate stays pinned to the original eight and must list any other deck here.
    // M4 nonlinear charge decks have a production/parser gate of their own.
    let m4 = [
        "m4_bjt_ac",
        "m4_bjt_tran",
        "m4_diode_ac",
        "m4_diode_tran",
        "m4_mos1_ac",
        "m4_mos1_tran",
    ];
    // MOS1 completion decks (#88), gated by `xtask golden verify` and
    // `tests/m7_mos1.rs`.
    let m7 = [
        "m7_mos1_inverter_tran",
        "m7_mos1_meyer_ac",
        "m7_mos1_process_dc",
        "m7_mos1_ring_tran",
    ];
    // M6 common-deck fixtures (#94, #95, #96, #107, #110) and the M6 exit gate
    // are gated by `xtask golden verify` (multi-analysis decks plot by plot)
    // and their own feature tests.
    let m6 = [
        "func_quotes",
        "m6_gate",
        "multi_analysis_rc",
        "options_gmin_dc",
        "options_xmu_tran",
        "rc_exp_tran",
        "rc_pulse_count_tran",
        "rc_pwl_repeat_tran",
        "rc_sffm_am_tran",
        "rc_sin_tran",
    ];
    // Gear maxord deck (#98), gated by `xtask golden verify` and
    // `tests/golden_rawfiles.rs`.
    let gear_maxord = ["rlc_series_gear_maxord6_tran"];
    on_disk.retain(|name| {
        !gear_maxord.contains(&name.as_str())
            && !M3_GATE_DECKS.contains(&name.as_str())
            && !m4.contains(&name.as_str())
            && !m7.contains(&name.as_str())
            && !m6.contains(&name.as_str())
    });
    // M7 diode-physics decks (#86), convergence decks (`m7_conv_*`, #106) and
    // nonlinear initial-condition decks (`m7_ic_*`, #99) are gated by `xtask
    // golden verify` and their unit/production tests.
    on_disk.retain(|name| !name.starts_with("m7_"));
    // M6 controlled-source decks (#78) are gated by `xtask golden verify` and
    // `tests/controlled_sources.rs`.
    on_disk.retain(|name| !name.starts_with("controlled_"));
    // M6 mutual-inductance decks (#80) are gated by `xtask golden verify` and
    // `tests/mutual_inductance.rs`.
    on_disk.retain(|name| !name.starts_with("transformer_"));
    // M6 switch decks (#81) are gated by `xtask golden verify` and
    // `tests/switches.rs`.
    on_disk.retain(|name| !name.starts_with("switch_"));
    // M8 DC parameter-sweep decks (#97) are gated by `xtask golden verify`
    // and `tests/dc_parameter_sweeps.rs`.
    on_disk.retain(|name| !name.starts_with("m8_dc_"));
    // M8 S-parameter decks (#105) are gated by `xtask golden verify`,
    // `tests/sparam.rs` and `tests/deck_writer.rs`.
    on_disk.retain(|name| !name.starts_with("sp_"));
    // M8 `.noise` decks (#100) are gated by `xtask golden verify` and
    // `tests/noise_analysis.rs`.
    on_disk.retain(|name| !name.starts_with("noise_"));
    // M8 `.disto` decks (#104) are gated by `xtask golden verify`,
    // `tests/distortion_analysis.rs` and the opt-in `tests/c_disto_reference.rs`.
    on_disk.retain(|name| !name.starts_with("disto_"));
    // M7 Gummel-Poon BJT decks (#87) are gated by `xtask golden verify`,
    // `tests/bjt_gummel_poon.rs` and the parser round trip there.
    on_disk.retain(|name| !name.starts_with("m7_bjt_"));
    // M8 `.tf` decks (#101) are gated by `xtask golden verify`,
    // `tests/analysis_tf.rs` and the opt-in `tests/c_tf_reference.rs`.
    on_disk.retain(|name| !name.starts_with("m8_tf_"));
    // M9 scoped front-end behavior has its own C-backed integration gate.
    on_disk.retain(|name| !name.starts_with("m9_"));
    // M10 URC decks (#85) are gated by `xtask golden verify`,
    // `tests/golden_rawfiles.rs` and `tests/urc_lines.rs`.
    on_disk.retain(|name| !name.starts_with("m10_urc_"));
    // M6 behavioural-source decks (#79) are gated by `xtask golden verify` and
    // `tests/behavioural_sources.rs`.
    on_disk.retain(|name| {
        !matches!(
            name.as_str(),
            "bsource_op"
                | "bsource_dc"
                | "bsource_ac"
                | "bsource_tran"
                | "evalue_op"
                | "gtable_dc"
                | "epoly_dc"
                | "bsource_zero_op"
                | "bsource_zero_dc"
                | "bsource_zero_tran"
        )
    });
    // M8 pole-zero decks (#103) are gated by `xtask golden verify` and
    // `tests/pole_zero.rs`.
    on_disk.retain(|name| !name.starts_with("pz_") && name != "multi_analysis_pz");
    // M8 sensitivity decks (#102) are gated by `xtask golden verify`,
    // `tests/sensitivity_analysis.rs` and the opt-in `tests/c_sens_reference.rs`.
    on_disk.retain(|name| !name.starts_with("sens_"));
    let mut expected: Vec<&str> = DECKS.iter().map(|d| d.deck).collect();
    expected.sort_unstable();
    assert_eq!(on_disk, expected, "a corpus deck was added or removed");
    assert_eq!(DECKS.len(), 8);
}

#[test]
fn all_eight_decks_have_the_expected_ast() {
    for expect in DECKS {
        let n = parse_fixture(&format!("netlists/{}.cir", expect.deck));
        let name = expect.deck;
        let devices: Vec<(&str, Vec<&str>, Option<&str>)> = n
            .devices
            .iter()
            .map(|d| {
                (
                    d.name.as_str(),
                    d.nodes.iter().map(String::as_str).collect(),
                    d.model.as_deref(),
                )
            })
            .collect();
        let want: Vec<(&str, Vec<&str>, Option<&str>)> = expect
            .devices
            .iter()
            .map(|(n, nodes, m)| (*n, nodes.to_vec(), *m))
            .collect();
        assert_eq!(devices, want, "{name}: devices (terminal/model roles)");
        assert_eq!(n.top_level_device_count(), expect.devices.len(), "{name}");
        let models: Vec<(&str, &str)> = n
            .models
            .iter()
            .map(|m| (m.name.as_str(), m.base.as_str()))
            .collect();
        assert_eq!(models, expect.models, "{name}: models");
        let subs: Vec<&str> = n.subcircuits.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(subs, expect.subcircuits, "{name}: subcircuits");
        assert_eq!(n.analyses.len(), 1, "{name}: analysis count");
        let analysis = &n.analyses[0];
        assert_eq!(analysis.kind, expect.analysis.0, "{name}: analysis kind");
        let args: Vec<&str> = analysis.arguments.iter().map(String::as_str).collect();
        assert_eq!(args, expect.analysis.1, "{name}: analysis arguments");
        assert_eq!(
            n.analysis_kinds().collect::<Vec<_>>(),
            [expect.analysis.0],
            "{name}"
        );
        // Every ordered card is accounted for exactly once and the deck is closed.
        let last = n.cards.last().unwrap();
        assert_eq!(last.kind, ScopedCardKind::End, "{name}: ends with .end");
        let indexed = n.devices.len() + n.models.len() + n.subcircuits.len() + n.analyses.len() + 1; // .end
        assert_eq!(n.cards.len(), indexed, "{name}: ordered cards");
        assert!(
            n.params.is_empty() && n.options.is_empty() && n.globals.is_empty(),
            "{name}"
        );
    }
}

#[test]
fn source_and_model_value_details_are_pinned() {
    let n = parse_fixture("netlists/rc_transient.cir");
    let pulse = &n.devices[0].parameters[0];
    assert_eq!(pulse.name, "pulse");
    assert!(matches!(
        pulse.kind,
        ngspice_rs::netlist::ast::ParameterKind::Waveform(_)
    ));
    let n = parse_fixture("netlists/rc_lowpass_ac.cir");
    let v: Vec<_> = n.devices[0]
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), p.value.as_str()))
        .collect();
    assert_eq!(v, [("dc", "0"), ("acmag", "1"), ("acphase", "0")]);
    let n = parse_fixture("netlists/mos_inverter.cir");
    let m1: Vec<_> = n.devices[3]
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), p.value.as_str()))
        .collect();
    assert_eq!(m1, [("w", "10u"), ("l", "1u")]);
    assert_eq!(n.models[0].level, Some(1.0));
    let n = parse_fixture("netlists/subckt_divider.cir");
    let div = &n.subcircuits[0];
    assert_eq!(div.terminals, ["a", "b"]);
    assert_eq!(div.devices.len(), 1);
    assert_eq!(div.devices[0].nodes, ["a", "b"]);
}

#[test]
fn every_gate_deck_round_trips_semantically_with_a_writer_fixed_point() {
    let ids: Vec<&str> = DECKS.iter().map(|d| d.deck).collect();
    let mut fixtures: Vec<String> = ids.iter().map(|d| format!("netlists/{d}.cir")).collect();
    // Combined seam decks (includes + subcircuits + params + options).
    fixtures.push("parser/combined_includes.cir".into());
    fixtures.push("parser/combined_params_options.cir".into());
    let temp = TempDir::new("roundtrip");
    copy_dir(&conformance(), &temp.0);
    for rel in fixtures {
        let path = temp.0.join(&rel);
        let parser = Parser::new();
        let first = parser
            .parse_file(&path)
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
        let written = write_netlist(&first).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let out = path.with_file_name("zz written.cir");
        fs::write(&out, &written).unwrap();
        let second = parser
            .parse_file(&out)
            .unwrap_or_else(|e| panic!("{rel}: {e}\n{written}"));
        if let Some(diff) = semantic_diff(&first, &second) {
            panic!("{rel}: {diff}\n{written}");
        }
        assert!(semantic_eq(&first, &second), "{rel}");
        assert_eq!(
            write_netlist(&second).unwrap(),
            written,
            "{rel}: fixed point"
        );
        assert_eq!(
            write_netlist(&first).unwrap(),
            written,
            "{rel}: deterministic"
        );
        assert_eq!(first.devices.len(), second.devices.len(), "{rel}");
        assert_eq!(first.models.len(), second.models.len(), "{rel}");
        assert_eq!(
            first.analysis_kinds().collect::<Vec<_>>(),
            second.analysis_kinds().collect::<Vec<_>>(),
            "{rel}"
        );
        let _ = fs::remove_file(out);
    }
}

#[test]
fn token_and_ast_snapshots_exist_and_match_for_every_gate_deck() {
    let root = conformance();
    let generated = snapshot::generate(&root).unwrap();
    let mut decks: Vec<String> = DECKS
        .iter()
        .map(|d| format!("netlists/{}", d.deck))
        .collect();
    decks.push("parser/combined_includes".into());
    decks.push("parser/combined_params_options".into());
    for deck in decks {
        for (dir, ext) in [("tokens", "tokens"), ("ast", "ast")] {
            let rel = format!("{dir}/{deck}.{ext}");
            let file = generated
                .iter()
                .find(|g| g.path == rel)
                .unwrap_or_else(|| panic!("{rel} is not generated"));
            let committed = fs::read_to_string(root.join(SNAPSHOT_DIR).join(&rel))
                .unwrap_or_else(|_| panic!("{rel} is not committed"));
            assert_eq!(
                committed, file.contents,
                "{rel} drifted; see snapshots README"
            );
            assert!(
                !committed.contains("\nerror"),
                "{rel} snapshots an error, not an AST"
            );
        }
    }
    assert!(snapshot::orphans(&root, &generated).unwrap().is_empty());
}

#[test]
fn combined_include_deck_preserves_scope_provenance_and_setter_order() {
    let n = parse_fixture("parser/combined_includes.cir");
    // Root cards in order, with include provenance on spliced cards only.
    let rows: Vec<(String, usize)> = n
        .cards
        .iter()
        .map(|c| (c.source.raw.clone(), c.include_chain.len()))
        .collect();
    let spliced: Vec<&str> = rows
        .iter()
        .filter(|(_, depth)| *depth > 0)
        .map(|(raw, _)| raw.as_str())
        .collect();
    assert_eq!(
        spliced,
        [
            ".param fromfile=2",
            ".model dm d(is=1e-14)",
            ".param speed=1",
            ".model rlib r rsh=50"
        ]
    );
    // The unselected .lib section (fast) is not spliced.
    assert!(!rows.iter().any(|(raw, _)| raw.contains("speed=2")));
    // Option cards keep source order and duplicates.
    let settings: Vec<(&str, Option<&str>)> = n
        .options
        .iter()
        .flat_map(|c| &c.settings)
        .map(|s| (s.name.as_str(), s.value.as_ref().map(|v| v.text.as_str())))
        .collect();
    assert_eq!(
        settings,
        [
            ("reltol", Some("1e-4")),
            ("temp", Some("30")),
            ("reltol", Some("1e-5")),
            ("noopiter", None),
            ("temp", Some("40")),
        ]
    );
    // The subcircuit body keeps its own scope and the included body cards.
    let stage = &n.subcircuits[0];
    assert_eq!(stage.devices.len(), 2);
    assert_eq!(stage.models[0].name, "local");
    assert_eq!(stage.params.len(), 2);
    assert!(stage.cards.iter().any(|c| c.include_chain.len() == 1));
    let body_device = &stage.devices[0];
    assert!(
        body_device
            .location
            .file
            .to_string_lossy()
            .ends_with("stage_body.inc"),
        "{:?}",
        body_device.location
    );
    // The root does not see the body's model, devices or params.
    assert!(n.models.iter().all(|m| m.name != "local"));
    assert_eq!(n.params.len(), 4);
    // Root .param cards, in order, across the include boundary.
    let params: Vec<&str> = n
        .params
        .iter()
        .flat_map(|c| &c.assignments)
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(params, ["vin", "rtop", "fromfile", "speed", "late"]);
    assert_eq!(n.globals[0].nodes[0].name, "vdd");
}

fn parse_text(text: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(Path::new("neg.cir"), text))
}

/// Negative cases never yield a partial AST: each is an explicit error whose
/// message names the problem.
#[test]
fn malformed_and_unsupported_cards_are_explicit_errors() {
    let parse_errors: &[(&str, &str)] = &[
        ("r1 a\n", "negative terminal"),
        ("r1 a 0 1k extra=\n", ""),
        ("v1 a 0 pulse(0 1\n", ""),
        (".model\n", ""),
        (".subckt a\n.ends b\n", "mismatched"),
        (".subckt a\n", ".end before .ends"),
        (".ends\n", "unmatched"),
        (".param x=\n", ""),
        (".param x={1+}\n", ""),
        (".option reltol=\n", ""),
        (".global\n", ""),
        ("r1 a 0 {\n", ""),
    ];
    for (body, needle) in parse_errors {
        let result = parse_text(&format!("t\n{body}.end\n"));
        let error = result.expect_err(body);
        assert!(
            matches!(
                error,
                SpiceError::Parse { .. } | SpiceError::NotYetPorted { .. }
            ),
            "{body}: {error:?}"
        );
        assert!(error.to_string().contains(needle), "{body}: {error}");
    }
    // Valid syntax outside the supported subset is NotYetPorted, never ignored.
    for body in ["q1 c b e s1 s2 s3 qm\n", "d1 a b dm thermal\n"] {
        let result = parse_text(&format!("t\n{body}.end\n"));
        assert!(
            matches!(
                result,
                Err(SpiceError::NotYetPorted { .. } | SpiceError::Parse { .. })
            ),
            "{body}: {result:?}"
        );
    }
    // A call to an unknown function parses as a user `.func` call; without a
    // definition it is an explicit evaluation error, never a dropped value.
    let unknown = parse_text("t\nr1 a 0 {ternary_s(1,2,3)}\n.end\n").unwrap();
    let error = ngspice_rs::netlist::elaborate::literalize(&unknown).unwrap_err();
    assert!(
        error.to_string().contains("undefined function 'ternary_s'"),
        "{error}"
    );
}

/// What the parser enforces about scoped names: structural scoping only.
///
/// * Definitions are stored in the scope that declares them, so a model,
///   device, `.param` or nested subcircuit declared inside a `.subckt` body is
///   never present in the root vectors (no AST leakage).
/// * Where the grammar needs a model's family to disambiguate a card (Q vs. a
///   4-terminal form, R/C/L model-vs-value), the lookup uses only the local
///   scope and its ancestors; a name declared in a sibling or child scope is
///   *not* visible, so the card errors instead of silently resolving.
/// * Resolving an `X` target, an unknown model on D/M, or a root reference to a
///   body-local name is *not* a parser job: `X` targets and D/M model names are
///   kept as unresolved text (elaboration, #17/#18, owns that), so those
///   references parse. This test pins that boundary so it is not mistaken for
///   enforced resolution.
#[test]
fn scoped_name_leakage_is_contained_in_the_ast_and_in_family_lookup() {
    let n = parse_text(
        "t\n.subckt s a b\n.model inner d\n.param p=1\nr1 a b 1k\n.subckt nested x y\n.ends\n.ends\n.end\n",
    )
    .unwrap();
    assert!(n.models.is_empty() && n.devices.is_empty() && n.params.is_empty());
    assert_eq!(n.subcircuits.len(), 1);
    assert_eq!(n.subcircuits[0].models[0].name, "inner");
    assert_eq!(n.subcircuits[0].subcircuits[0].name, "nested");
    // Family lookup (4-pin Q needs the model's family) cannot see sibling or
    // child scopes, nor a root reference to a body-local model.
    for body in [
        ".subckt one a b\n.model local npn\n.ends\n.subckt two a b\nq1 c b e local\n.ends\n",
        ".subckt outer a b\nq1 c b e local\n.subckt inner a b\n.model local npn\n.ends\n.ends\n",
        ".subckt one a b\n.model local npn\n.ends\nq1 c b e local\n",
    ] {
        let result = parse_text(&format!("t\n{body}.end\n"));
        assert!(
            matches!(result, Err(SpiceError::Parse { .. })),
            "{body}: {result:?}"
        );
    }
    // Not enforced by the parser: unresolved model/subcircuit targets.
    let n = parse_text(
        "t\n.subckt one a b\n.model local d\n.ends\nd1 a b local\nx1 a b one_missing\n.end\n",
    )
    .unwrap();
    assert_eq!(n.devices[0].model.as_deref(), Some("local"));
    assert_eq!(n.devices[1].model.as_deref(), Some("one_missing"));
    // Nested definitions are local: the same name may be reused at another level.
    parse_text("t\n.subckt s\n.subckt t\n.ends\n.ends\n.subckt t\n.ends\n.end\n").unwrap();
}

#[test]
fn include_cycles_and_missing_sources_are_errors_not_partial_asts() {
    let temp = TempDir::new("cycles");
    let write = |name: &str, text: &str| {
        let p = temp.0.join(name);
        fs::write(&p, text).unwrap();
        p
    };
    let main = write("main.cir", "t\nr1 a 0 1k\n.include a.inc\n.end\n");
    write("a.inc", ".include b.inc\n");
    write("b.inc", ".include a.inc\n");
    let error = Parser::new().parse_file(&main).unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");
    write("a.inc", ".include missing.inc\n");
    let error = Parser::new().parse_file(&main).unwrap_err();
    assert!(
        matches!(error, SpiceError::Parse { .. } | SpiceError::Io { .. }),
        "{error}"
    );
}

#[test]
fn error_fixtures_are_snapshotted_as_errors_not_partial_asts() {
    for name in [
        "error_bad_expression",
        "error_malformed_resistor",
        "error_missing_include",
        "error_unclosed_subckt",
        "error_unmatched_close_brace",
        "error_unterminated_brace",
        "error_unterminated_quote",
    ] {
        let path = conformance().join(format!("cases/{name}.cir"));
        assert!(Parser::new().parse_file(&path).is_err(), "{name}");
        let ast =
            fs::read_to_string(conformance().join(format!("{SNAPSHOT_DIR}/ast/cases/{name}.ast")))
                .unwrap();
        assert!(ast.contains("error"), "{name}");
        assert!(!ast.contains("\nscope root"), "{name}: partial AST");
    }
}

#[test]
fn parameter_cycles_and_undefined_names_are_errors_with_context() {
    for body in [
        ".param a={b} b={a}\nr1 x 0 {a}\n", // two-node cycle
        ".param a={a+1}\nr1 x 0 {a}\n",     // self reference
        ".param a=1\nr1 x 0 {missing}\n",   // undefined
    ] {
        let n = parse_text(&format!("t\n{body}.op\n.end\n")).unwrap();
        let error = ngspice_rs::netlist::elaborate::literalize(&n).expect_err(body);
        let text = error.to_string();
        assert!(text.contains("neg.cir:"), "{body}: {text}");
        assert!(
            text.contains("circular") || text.contains("undefined parameter"),
            "{body}: {text}"
        );
    }
}
