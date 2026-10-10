//! Conformance tests over the golden rawfiles captured from the C binary.
//!
//! These tests read the committed data in `conformance/`, so they need neither
//! the C tree nor a built `ngspice`: re-running the reference implementation to
//! detect drift is `cargo xtask golden check`.
//!
//! The writer is pinned against real output rather than prose: the tests below
//! check it line by line against every golden and require it to be a fixed
//! point. See
//! [`the_writer_agrees_with_ngspice_apart_from_non_round_trippable_decimals`]
//! for the one difference that is inherent to the format.

use std::fs;
use std::path::{Path, PathBuf};

use ngspice_rs::analysis::RawFile;
use ngspice_rs::analysis::results::PlotFlags;
use ngspice_rs::primitives::{Complex, Real, approx_eq, format_spice_number};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".")
        .canonicalize()
        .expect("the workspace root exists")
}

fn fixture_names() -> Vec<String> {
    let directory = workspace().join("conformance/netlists");
    let mut names: Vec<String> = fs::read_dir(&directory)
        .expect("conformance/netlists exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "cir"))
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "there are no fixtures");
    names
}

fn golden_text(name: &str) -> String {
    let path = workspace().join(format!("conformance/golden/{name}.raw"));
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

fn netlist_text(name: &str) -> String {
    let path = workspace().join(format!("conformance/netlists/{name}.cir"));
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

/// Compare actual production solves, not just rawfile round-tripping.
///
/// `subckt_divider` goes through the same `Circuit::from_netlist` entry point as
/// the linear fixtures, so its `X` instance is expanded by production code, not
/// by a test helper.
#[test]
fn production_linear_drivers_match_c_goldens() {
    for name in [
        "rc_divider",
        "rlc_series",
        "rc_lowpass_ac",
        "subckt_divider",
        "func_quotes",
    ] {
        let deck =
            ngspice_rs::netlist::source::parse_deck_text(Path::new(name), &netlist_text(name));
        let netlist = ngspice_rs::netlist::Parser::new()
            .parse_deck(&deck)
            .unwrap();
        let mut circuit = ngspice_rs::devices::Circuit::from_netlist(&netlist).unwrap();
        let request = ngspice_rs::analysis::AnalysisRequest::from(&netlist.analyses[0]);
        let got = ngspice_rs::analysis::runner(request.kind)
            .unwrap()
            .run(
                &mut circuit,
                &request,
                &ngspice_rs::analysis::AnalysisContext::default(),
            )
            .unwrap();
        let golden = RawFile::parse(&golden_text(name)).unwrap();
        let want = &golden.plots[0].plot;
        assert_eq!(got.variable_count(), want.variable_count(), "{name}");
        assert_eq!(got.point_count(), want.point_count());
        for variable in &want.variables {
            let index = got.variable_index(&variable.name).unwrap();
            assert_eq!(&got.variables[index], variable);
            for (a, b) in got
                .column(&variable.name)
                .unwrap()
                .iter()
                .zip(want.column(&variable.name).unwrap())
            {
                let (rtol, atol) = if got.flags.is_complex() {
                    (1e-10, 1e-12)
                } else {
                    (1e-12, 1e-15)
                };
                assert!(
                    (*a - b).magnitude() <= rtol * b.magnitude() + atol,
                    "{name}: {a} != {b}"
                );
            }
        }
    }
}

#[test]
fn every_fixture_has_a_golden_and_no_golden_is_orphaned() {
    let fixtures = fixture_names();
    let golden_directory = workspace().join("conformance/golden");
    let mut goldens: Vec<String> = fs::read_dir(&golden_directory)
        .expect("conformance/golden exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "raw"))
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
        })
        .collect();
    goldens.sort();

    assert_eq!(
        fixtures, goldens,
        "each fixture needs exactly one golden; run `cargo xtask golden capture`"
    );
}

/// The writer agrees with ngspice line for line, apart from values whose 16
/// significant digits do not round-trip.
///
/// A rawfile value carries 16 significant digits, which is not always enough to
/// round-trip a `double`: two adjacent doubles can share one 16-digit spelling,
/// so parsing the text yields the nearest double, whose own spelling may differ
/// in the last digit. That is what happens to one `time` sample in the transient
/// golden. This test permits exactly that difference and nothing else.
///
/// This is a limitation of reading a golden, not of the port's parity goal: when
/// the Rust engine computes the same double ngspice computed, it formats it with
/// the same `%.15e` algorithm and the bytes match.
#[test]
fn the_writer_agrees_with_ngspice_apart_from_non_round_trippable_decimals() {
    for name in fixture_names() {
        let text = golden_text(&name);
        let rawfile = RawFile::parse(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
        let rendered = rawfile.to_ascii();

        assert_eq!(
            rendered.lines().count(),
            text.lines().count(),
            "{name}: the writer changed the number of lines"
        );
        for (index, (rendered_line, golden_line)) in rendered.lines().zip(text.lines()).enumerate()
        {
            if rendered_line == golden_line {
                continue;
            }
            // C logarithmic sweeps carry display-only grid=3 metadata, which
            // the documented rawfile projection does not retain. Numeric values
            // and all other variable metadata remain compared unchanged.
            if golden_line.strip_suffix(" grid=3") == Some(rendered_line)
                && golden_line.contains("\tfrequency\tfrequency")
            {
                continue;
            }
            match (numeric_parts(rendered_line), numeric_parts(golden_line)) {
                (Some(ours), Some(theirs)) if ours == theirs => {}
                _ => panic!(
                    "{name}: line {} differs beyond a last-digit spelling:\n  \
                     ours:   {rendered_line:?}\n  golden: {golden_line:?}",
                    index + 1
                ),
            }
        }
    }
}

/// Pin every known token in the corpus that cannot be re-derived from its own
/// spelling. Pinning it keeps the tolerance above bounded: if a re-capture
/// introduces more of them, this test fails and somebody looks.
#[test]
fn known_golden_tokens_are_not_round_trippable() {
    let mut offenders: Vec<String> = Vec::new();
    for name in fixture_names() {
        for (index, line) in golden_text(&name).lines().enumerate() {
            let token = line.rsplit('\t').next().unwrap_or(line).trim();
            for part in token.split(',').map(str::trim) {
                let Ok(value) = part.parse::<f64>() else {
                    continue;
                };
                let spelling = format_spice_number(value);
                if spelling != part {
                    offenders.push(format!("{name}:{} {part} -> {spelling}", index + 1));
                }
            }
        }
    }
    assert_eq!(
        offenders,
        [
            "m4_bjt_tran:25 1.000000000000000e-11 -> 9.999999999999999e-12",
            "m4_diode_tran:19 1.000000000000000e-11 -> 9.999999999999999e-12",
            "rc_transient:19 1.000000000000000e-11 -> 9.999999999999999e-12",
        ]
    );
}

/// The writer is a fixed point: once a rawfile has been through it, another pass
/// changes nothing. Not every golden satisfies this on the first pass, because of
/// the rounding above, but every canonicalised golden does.
#[test]
fn the_writer_reaches_a_fixed_point() {
    for name in fixture_names() {
        let first = RawFile::parse(&golden_text(&name)).expect("parses");
        let once = first.to_ascii();
        let reparsed = RawFile::parse(&once).expect("reparses");
        assert_eq!(
            once,
            reparsed.to_ascii(),
            "{name}: the second pass changed the bytes"
        );
        assert_eq!(first, reparsed, "{name}: the values changed");
    }
}

/// The numeric parts of the value at the end of a rawfile line.
///
/// Returns `None` for a line that does not end in a number, such as a header or
/// a variable declaration, and for a value with two parts where either fails to
/// parse.
fn numeric_parts(line: &str) -> Option<Vec<f64>> {
    let token = line.rsplit('\t').next()?.trim();
    if token.is_empty() {
        return None;
    }
    token
        .split(',')
        .map(|part| part.trim().parse().ok())
        .collect()
}

#[test]
fn goldens_are_well_formed_and_finite() {
    for name in fixture_names() {
        let rawfile = RawFile::parse(&golden_text(&name)).expect("parses");
        assert!(!rawfile.is_empty(), "{name}: no plots");
        for raw_plot in &rawfile.plots {
            let plot = &raw_plot.plot;
            assert!(
                !plot.plotname.is_empty() && !raw_plot.title.is_empty(),
                "{name}: empty headers"
            );
            assert!(plot.point_count() > 0, "{name}: no points");
            assert!(plot.variable_count() > 0, "{name}: no variables");
            assert!(plot.is_finite(), "{name}: non-finite value");
            assert!(
                plot.variables
                    .iter()
                    .all(|variable| !variable.unit.is_empty()),
                "{name}: a variable has no unit"
            );
        }
    }
}

/// ngspice lowercases the deck's title line before putting it in the rawfile.
///
/// Nothing in the rawfile format requires this; `raw_write()` writes
/// `pl->pl_title`, and the title was lowercased when the deck was read
/// (`inp_readall()` / `INPgetTitle()` in `src/frontend/inpcom.c`). The port has
/// to do the same to produce identical files.
#[test]
fn ngspice_lowercases_the_deck_title() {
    for name in fixture_names() {
        let deck = netlist_text(&name);
        let first_line = deck.lines().next().expect("the deck has a title line");
        let rawfile = RawFile::parse(&golden_text(&name)).expect("parses");
        let title = &rawfile.plots[0].title;
        assert_eq!(*title, first_line.to_lowercase(), "{name}: title mismatch");
    }
}

/// One expected value: variable name, point index, real and imaginary parts.
type ValueCheck = (&'static str, usize, Real, Real);

struct Expectation {
    fixture: &'static str,
    plotname: &'static str,
    flags: PlotFlags,
    points: usize,
    variables: &'static [&'static str],
    values: &'static [ValueCheck],
}

/// The fixtures, with their hand-checked results.
///
/// The numbers come from the C binary; the comments say why each one is the
/// value an independent reader of the deck would predict, so that a golden that
/// changes shape is caught rather than merely re-recorded.
///
/// The literals keep every digit ngspice printed, including trailing zeros, so a
/// change in the last digit fails rather than being lost to precision.
#[allow(clippy::excessive_precision)]
const EXPECTATIONS: &[Expectation] = &[
    // The two scoped .ic values set capacitor voltages to .25 and .5 V.
    // At t=0 the sine source is zero, so its outward current is the sum
    // of the two resistor currents: (.25 + .5)/1k = .75 mA.
    Expectation {
        fixture: "m9_scoped_frontend",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 2008,
        variables: &[
            "time",
            "v(out)",
            "v(rail)",
            "i(v.x1.vlocal)",
            "i(v.x2.vlocal)",
            "i(v1)",
            "i(vrail)",
            "v(x1.local)",
            "v(x1.other)",
            "v(x2.local)",
            "v(x2.other)",
        ],
        values: &[
            ("time", 0, 0., 0.),
            ("v(x1.local)", 0, 0.25, 0.),
            ("v(x2.local)", 0, 0.5, 0.),
            ("i(v1)", 0, 0.00075, 0.),
            ("v(rail)", 0, 0., 0.),
        ],
    },
    Expectation {
        fixture: "bjt_ce",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(vcc)", "v(base)", "v(coll)", "i(vcc)", "i(vin)"],
        // 0.65 V on the base gives about 0.82 mA of collector current, so the
        // 1 k collector resistor drops 0.82 V; the base current is that over
        // beta = 100.
        values: &[
            ("v(vcc)", 0, 5.0, 0.0),
            ("v(base)", 0, 0.65, 0.0),
            ("v(coll)", 0, 4.179523627704651, 0.0),
            ("i(vcc)", 0, -8.204763722953494e-4, 0.0),
            ("i(vin)", 0, -8.204760756139570e-6, 0.0),
        ],
    },
    Expectation {
        fixture: "controlled_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 20,
        variables: &[
            "frequency",
            "i(e1)",
            "i(h1)",
            "v(in)",
            "v(inv)",
            "v(o2)",
            "v(o3)",
            "v(o4)",
            "v(o5)",
            "v(out)",
            "i(vin)",
            "i(vs)",
        ],
        // Inverting integrator with A = 1e4: v(out) = -A x with
        // x = (1/R) / (1/R + (1 + A) jwC), about +j/(wRC) = 1.5914j at 100 Hz.
        // G (1 mS) drives Y = jw 1u + 2 mS; i(vs) = v(o2)/1k, F gives
        // v(o4) = 2 i(vs) 1k and H gives v(o5) = 100 i(vs) (C sign convention).
        values: &[
            ("frequency", 0, 1.000000000000000e+02, 0.0),
            ("v(out)", 0, -2.532522996984260e-04, 1.591390251587439e+00),
            ("v(o2)", 0, 2.274044503765859e-01, 7.242539107240491e-01),
            ("i(vs)", 0, 2.274044503765859e-04, 7.242539107240491e-04),
            ("v(o4)", 0, 4.548089007531717e-01, 1.448507821448098e+00),
            ("v(o5)", 0, 2.274044503765859e-02, 7.242539107240491e-02),
            ("i(h1)", 0, -2.274044503765859e-05, -7.242539107240491e-05),
            ("frequency", 19, 2.000000000000000e+03, 0.0),
            ("v(out)", 19, -6.331307652401620e-07, 7.956951458945044e-02),
        ],
    },
    Expectation {
        fixture: "controlled_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(in)",
            "i(e1)",
            "i(e2)",
            "v(fb)",
            "i(h.x1.h1)",
            "i(h1)",
            "v(o2)",
            "v(o3)",
            "v(o4)",
            "v(o5)",
            "v(o6)",
            "v(o7)",
            "v(out)",
            "i(v.x1.vs)",
            "i(vin)",
            "i(vsense)",
            "v(x1.hout)",
            "v(x1.mid)",
        ],
        // E (A = 1e4) closes a 1 + 3k/1k loop: v(out) = 6/(1 + 4/A). The leading
        // 2m of g1 follows m=2, so 4 mS x 1.5 V flows into 1k || 4k: 4.8 V and
        // 1.2 mA through vsense; F x3 gives 3.6 V, H x500 gives 0.6 V (and
        // i(h1) = -0.3 mA into its 2k load), e2 (vcvs keyword) -2 x 0.6 V.
        // i(e1) = -v(out) (1/4k + 1/2k) feeds f2 x0.5 into 1k; inside x1, h1
        // senses v.x1.vs (out/2k) through a hierarchical controlling name.
        values: &[
            ("v(out)", 0, 5.997600959615738e+00, 0.0),
            ("i(e1)", 0, -4.498200719711768e-03, 0.0),
            ("v(o2)", 0, 4.800000000000001e+00, 0.0),
            ("i(vsense)", 0, 1.200000000000000e-03, 0.0),
            ("v(o4)", 0, 3.600000000000001e+00, 0.0),
            ("v(o5)", 0, 6.000000000000001e-01, 0.0),
            ("i(h1)", 0, -3.000000000000000e-04, 0.0),
            ("v(o6)", 0, 2.249100359855884e+00, 0.0),
            ("v(o7)", 0, -1.200000000000000e+00, 0.0),
            ("i(e2)", 0, 4.000000000000000e-04, 0.0),
            ("i(v.x1.vs)", 0, 2.998800479807869e-03, 0.0),
            ("v(x1.hout)", 0, 2.998800479807869e+00, 0.0),
            ("i(h.x1.h1)", 0, -2.998800479807869e-04, 0.0),
            ("i(vin)", 0, -1.500000000000000e-04, 0.0),
        ],
    },
    Expectation {
        fixture: "controlled_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 632,
        variables: &[
            "time", "v(a)", "v(b)", "v(c)", "v(d)", "i(e1)", "i(h1)", "v(in)", "v(o4)", "v(o5)",
            "i(vin)", "i(vs)",
        ],
        // At 0.6 ms the second pulse is high: v(b) = 2 v(a) (E), i(e1) = -v(b)/1k,
        // and the current the G charges through r2 into vs gives
        // v(o4) = 2 i(vs) 1k (F) and v(o5) = 100 i(vs) (H).
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(b)", 0, 0.0, 0.0),
            ("time", 631, 5.999999999999999e-04, 0.0),
            ("v(a)", 631, 5.681063687010960e-01, 0.0),
            ("v(b)", 631, 1.136212737402192e+00, 0.0),
            ("i(e1)", 631, -1.136212737402192e-03, 0.0),
            ("i(vs)", 631, 5.548169243707399e-04, 0.0),
            ("v(o4)", 631, 1.109633848741480e+00, 0.0),
            ("v(o5)", 631, 5.548169243707399e-02, 0.0),
            ("i(h1)", 631, -5.548169243707399e-05, 0.0),
        ],
    },
    Expectation {
        fixture: "transformer_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 41,
        variables: &[
            "frequency",
            "v(in)",
            "i(l.x1.la)",
            "i(l.x1.lb)",
            "i(l.x1.lc)",
            "i(l1)",
            "i(l2)",
            "i(l3)",
            "v(p)",
            "v(q)",
            "v(s)",
            "v(t1)",
            "v(t2)",
            "v(u)",
            "i(vin)",
        ],
        // K mutual inductance (#80). At 1 kHz an independent complex MNA solve
        // with -j w M between every coupled branch pair (k1 0.98, kn -0.15,
        // and the three pairs of kabc at 0.6 inside x1) reproduces these values
        // to 1e-15; v(s) = -1k i(l2) across the secondary load.
        values: &[
            ("frequency", 20, 1.000000000000001e+03, 0.0),
            ("v(s)", 20, 1.179101980781495e+00, 9.663452185500626e-01),
            ("i(l2)", 20, -1.179101980781495e-03, -9.663452185500627e-04),
            ("v(t2)", 20, 4.024739519207775e-01, 4.515530188829902e-01),
            (
                "i(l.x1.la)",
                20,
                7.062171790396722e-02,
                -4.133835192472151e-02,
            ),
            ("v(u)", 20, 5.352513910176180e-01, -4.921833025874221e-01),
        ],
    },
    Expectation {
        fixture: "transformer_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 437,
        variables: &["time", "v(in)", "i(l1)", "i(l2)", "v(p)", "v(s)", "i(vin)"],
        // A 1:2 transformer (k = 0.99) under a 1 V PULSE: the secondary current
        // opposes the primary's (v(s) = -100 i(l2)); at 0.4 ms the source is low
        // and the magnetizing current decays through both windings.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("i(l1)", 0, 0.0, 0.0),
            ("time", 1, 1.000000000000000e-08, 0.0),
            ("i(l2)", 1, -2.444441426615508e-06, 0.0),
            ("v(s)", 1, 2.444441426615508e-04, 0.0),
            ("time", 436, 4.000000000000000e-04, 0.0),
            ("i(l1)", 436, 2.254497890661927e-02, 0.0),
            ("i(l2)", 436, 4.489536564091304e-03, 0.0),
            ("v(s)", 436, -4.489536564091304e-01, 0.0),
        ],
    },
    Expectation {
        fixture: "transformer_ic_uic_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 511,
        variables: &["time", "v(a)", "v(b)", "i(l1)", "i(l2)"],
        // Coupled RL free decay from l1 ic=10m, l2 ic=-5m (k = 0.7, Gear-2,
        // uic: no t = 0 row). The modes of L^-1 R decay at 5882 /s and
        // 33333 /s; the analytic currents at 0.5 ms are 7.7330e-5 and
        // 5.4680e-5 A, within Gear-2 truncation error of these values.
        values: &[
            ("time", 0, 1.000000000000000e-08, 0.0),
            ("i(l1)", 0, 9.997069630110690e-03, 0.0),
            ("i(l2)", 0, -4.998049735932326e-03, 0.0),
            ("time", 510, 5.000000000000000e-04, 0.0),
            ("v(a)", 510, -7.732693024550181e-04, 0.0),
            ("i(l1)", 510, 7.732693024550182e-05, 0.0),
            ("i(l2)", 510, 5.467770368085151e-05, 0.0),
        ],
    },
    Expectation {
        fixture: "transformer_model_uic_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 311,
        variables: &["time", "v(a)", "v(b)", "i(l1)", "i(l2)"],
        // Model-backed coupled inductors (l1: tc1 and m = 2 at 50 C) decay from
        // their instance ic= under uic and Gear-2: the first point keeps the
        // ic= currents, M uses INDinduct before /m (muttemp.c).
        values: &[
            ("time", 0, 1.000000000000000e-08, 0.0),
            ("i(l1)", 0, 9.997473067814353e-03, 0.0),
            ("i(l2)", 0, -4.998098938423450e-03, 0.0),
            ("time", 310, 3.000000000000000e-04, 0.0),
            ("i(l1)", 310, 4.628569432759175e-04, 0.0),
            ("i(l2)", 310, 2.817878342905811e-04, 0.0),
        ],
    },
    Expectation {
        fixture: "coupled_cap_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 817,
        variables: &["time", "v(a)", "v(b)", "v(in)", "i(v1)"],
        // Ramp 0 -> 1 V over 1.0-1.1 ms into a coupled-capacitor network
        // (modes 1000 /s and 200 /s): at 8 ms only the slow mode remains, so
        // v(a) = 1 - e^-1.38 / 2 = 0.875 and v(b) = e^-1.38 / 2 = 0.124.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(a)", 0, 0.0, 0.0),
            ("v(b)", 0, 0.0, 0.0),
            ("time", 816, 8.000000000000000e-03, 0.0),
            ("v(in)", 816, 1.000000000000000e+00, 0.0),
            ("v(a)", 816, 8.749807989177535e-01, 0.0),
            ("v(b)", 816, 1.240602142961135e-01, 0.0),
        ],
    },
    Expectation {
        fixture: "diode_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 5,
        variables: &["v(v-sweep)", "v(in)", "v(out)", "i(v1)"],
        // The first point is a sweep of zero volts, so everything is zero apart
        // from the diode's junction leakage. The last point is a 1 V sweep
        // through 1 k, so the diode drops 1 V - 0.6297 V at 0.370 mA.
        values: &[
            ("v(v-sweep)", 0, 0.0, 0.0),
            ("v(out)", 4, 6.296706738025182e-1, 0.0),
            ("v(v-sweep)", 4, 1.0, 0.0),
            ("v(in)", 4, 1.0, 0.0),
        ],
    },
    Expectation {
        fixture: "floating_cap_ic_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 817,
        variables: &["time", "v(a)", "v(b)", "v(in)", "i(v1)"],
        // `uic` with c1 ic=2: no t = 0 row, the first row is C's first step (0.1 us,
        // 1 V on each plate against the 0 V source, i = 1 mA). The ramp at 1.0-1.1 ms
        // then drives the loop; at 8 ms v(a) - v(b) = 1.0056 V.
        values: &[
            ("time", 0, 1.000000000000000e-07, 0.0),
            ("v(a)", 0, 9.999500024996800e-01, 0.0),
            ("v(b)", 0, -9.999500025000700e-01, 0.0),
            ("v(in)", 0, 0.000000000000000e+00, 0.0),
            ("i(v1)", 0, 9.999500024996800e-04, 0.0),
            ("time", 816, 8.000000000000000e-03, 0.0),
            ("v(in)", 816, 1.000000000000000e+00, 0.0),
            ("v(a)", 816, 1.002833083651511e+00, 0.0),
            ("v(b)", 816, -2.833083651534665e-03, 0.0),
        ],
    },
    Expectation {
        fixture: "floating_cap_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 817,
        variables: &["time", "v(a)", "v(b)", "v(in)", "i(v1)"],
        // c1 floats between a and b: after the 0 -> 1 V ramp (1.0-1.1 ms) the series
        // R-C-R loop (tau = 2 ms) leaves i = e^-(t - 1.1 ms)/tau / 2 k and v(b) = 1 k i.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(a)", 0, 0.0, 0.0),
            ("v(b)", 0, 0.0, 0.0),
            ("time", 816, 8.000000000000000e-03, 0.0),
            ("v(in)", 816, 1.000000000000000e+00, 0.0),
            ("v(a)", 816, 9.845175521824301e-01, 0.0),
            ("v(b)", 816, 1.548244781757727e-02, 0.0),
        ],
    },
    Expectation {
        fixture: "func_quotes",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(in)", "v(out)", "i(v1)", "v(x1.mid)", "v(x2.mid)"],
        // rtop = pll(3k, 6k) + gain(500) = 2k + 500*k(=2) = 3k and rload =
        // twice(base()) = 2k. Each leg is gain(100) = 100*k(=3)*w plus
        // pll(1k, 1k) + x = 600: 1.8k for x1 (w = twice(2) = 4), 0.9k for x2
        // (w = 1). The load is 2k || 1.8k || 0.9k = 6k/13, so v(out) = 4/3 V,
        // i(v1) = -(26/9) mA and the taps sit at 1/3 and 2/3 of v(out).
        values: &[
            ("v(in)", 0, 1.000000000000000e+01, 0.0),
            ("v(out)", 0, 1.333333333333334e+00, 0.0),
            ("i(v1)", 0, -2.888888888888889e-03, 0.0),
            ("v(x1.mid)", 0, 4.444444444444446e-01, 0.0),
            ("v(x2.mid)", 0, 8.888888888888892e-01, 0.0),
        ],
    },
    Expectation {
        fixture: "m4_bjt_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 31,
        variables: &[
            "frequency",
            "v(base)",
            "v(coll)",
            "v(in)",
            "v(vcc)",
            "i(vcc)",
            "i(vin)",
        ],
        values: &[
            ("frequency", 0, 1e3, 0.),
            ("v(coll)", 0, -4.207884901514965, 1.324201254836868e-3),
        ],
    },
    Expectation {
        fixture: "m4_bjt_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1020,
        variables: &[
            "time", "v(base)", "v(coll)", "v(in)", "v(vcc)", "i(vcc)", "i(vin)",
        ],
        values: &[
            ("time", 0, 0., 0.),
            ("v(base)", 0, 5.498293510058270e-1, 0.),
            ("v(coll)", 0, 4.982934701819048, 0.),
        ],
    },
    // Gummel-Poon BJT (#87). At VBE = 1.1 V the IKF = 20 mA knee and the
    // 21.5 ohm of RC + RE (plus RB) compress the forward gain to about 15
    // (3.0 mA of base current for 46.6 mA of collector current).
    Expectation {
        fixture: "m7_bjt_gummel",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 41,
        variables: &["v(v-sweep)", "v(b)", "v(c)", "i(vbe)", "i(vcb)"],
        values: &[
            ("v(v-sweep)", 40, 1.100000000000001, 0.),
            ("i(vbe)", 40, -4.966010012111799e-2, 0.),
            ("i(vcb)", 40, -4.664713816377888e-2, 0.),
        ],
    },
    // Five 51-point VCE sweeps (IB = 5..25 uA); at 5 V and 25 uA the gain is
    // about 110: BF = 150 lowered by ISE leakage and high injection, raised by
    // the Early effect (VAF = 60 V).
    Expectation {
        fixture: "m7_bjt_output",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 255,
        variables: &["v(v-sweep)", "v(b)", "v(c)", "i(vce)"],
        values: &[
            ("v(v-sweep)", 254, 4.999999999999998, 0.),
            ("v(b)", 254, 8.017159997526093e-1, 0.),
            ("i(vce)", 254, -2.739006031922822e-3, 0.),
        ],
    },
    // -40..125 C: the NPN emitter rises as VBE falls with temperature; the
    // PNP collector and the M=2 TLEV=3 follower follow their biasing.
    Expectation {
        fixture: "m7_bjt_temp",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 12,
        variables: &[
            "temp-sweep",
            "v(fb)",
            "v(fe)",
            "v(nb)",
            "v(nc)",
            "v(ne)",
            "v(pb)",
            "v(pc)",
            "v(pe)",
            "v(sub)",
            "v(vcc)",
            "i(vcc)",
            "i(vsub)",
        ],
        values: &[
            ("temp-sweep", 0, -40., 0.),
            ("v(ne)", 0, 7.446680380960549e-2, 0.),
            ("v(pc)", 11, 1.077693071789567, 0.),
            ("v(fe)", 11, 2.066642618251229, 0.),
        ],
    },
    // A bypassed common-emitter stage: about -97 (gm times 3.3k || 10k) at
    // 1 kHz, rolled off by the coupling capacitors and the junction charges.
    Expectation {
        fixture: "m7_bjt_amp_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 91,
        variables: &[
            "frequency",
            "v(in)",
            "v(nb)",
            "v(nc)",
            "v(ne)",
            "v(ns)",
            "v(out)",
            "v(vcc)",
            "i(vcc)",
            "i(vin)",
        ],
        values: &[
            ("frequency", 0, 10., 0.),
            ("v(out)", 40, -9.708196298518708e1, 7.191525861659186),
        ],
    },
    // The same stage's 10 mV input pulse; the first row is its bias point
    // (1.26 V base from the 82k/15k divider, 3.6 V collector).
    Expectation {
        fixture: "m7_bjt_amp_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 2623,
        variables: &[
            "time", "v(in)", "v(nb)", "v(nc)", "v(ne)", "v(ns)", "v(out)", "v(vcc)", "i(vcc)",
            "i(vin)",
        ],
        values: &[
            ("time", 0, 0., 0.),
            ("v(nc)", 0, 3.615525216417331, 0.),
            ("v(nb)", 0, 1.257160102738815, 0.),
        ],
    },
    // Convergence parity (#106). The cross-coupled BJT pair has two stable
    // states and a metastable one between them; which one a solver returns is
    // decided by its Newton path. ngspice's default start (MODEINITJCT at
    // vcrit for both base-emitter junctions, pnjlim, then dynamic gmin)
    // settles on the nearly balanced point: both collectors near 2.3 V, both
    // bases near 0.68 V.
    Expectation {
        fixture: "m7_conv_latch_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(vcc)", "v(b1)", "v(b2)", "v(c1)", "v(c2)", "i(vcc)"],
        values: &[
            ("v(c1)", 0, 2.307312486503814e+00, 0.),
            ("v(c2)", 0, 2.376980766869119e+00, 0.),
            ("i(vcc)", 0, -4.878536874438588e-03, 0.),
        ],
    },
    // `noopiter` with gillespie_src (srcsteps=1, no gmin stepping): the
    // adaptive source ramp tracks the same balanced point, to C's RELTOL.
    Expectation {
        fixture: "m7_conv_latch_gillespie_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(vcc)", "v(b1)", "v(b2)", "v(c1)", "v(c2)", "i(vcc)"],
        values: &[
            ("v(c1)", 0, 2.307312497700058e+00, 0.),
            ("v(c2)", 0, 2.376980774917631e+00, 0.),
        ],
    },
    // `noopiter` with spice3_gmin (gminsteps=4): gmin 1e-8 .. 1e-12 S at full
    // sources, again the balanced point.
    Expectation {
        fixture: "m7_conv_latch_spice3_gmin_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(vcc)", "v(b1)", "v(b2)", "v(c1)", "v(c2)", "i(vcc)"],
        values: &[
            ("v(c1)", 0, 2.307312486503806e+00, 0.),
            ("v(c2)", 0, 2.376980766869156e+00, 0.),
        ],
    },
    // `noopiter` with spice3_src (srcsteps=4): the quarter-supply steps tip
    // the pair into a stable state, q1 saturated (c1 = 87 mV) and q2 off
    // (c2 = 5 V less the RC2 drop of the current RB2 feeds into q1's base).
    Expectation {
        fixture: "m7_conv_latch_spice3_src_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(vcc)", "v(b1)", "v(b2)", "v(c1)", "v(c2)", "i(vcc)"],
        values: &[
            ("v(c1)", 0, 8.695128948953385e-02, 0.),
            ("v(c2)", 0, 4.539084265598849e+00, 0.),
            ("v(b2)", 0, 2.780076678812424e-02, 0.),
        ],
    },
    // The pair with a 20k base resistor on q1 has a single DC state (q1
    // conducting, q2 off); the 20 us set pulse saturates q1 and the 50 us
    // reset pulse flips the pair, after which it relaxes back to the initial
    // state, which the last sample reproduces.
    Expectation {
        fixture: "m7_conv_latch_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 8841,
        variables: &[
            "time", "v(b1)", "v(b2)", "v(c1)", "v(c2)", "v(r)", "v(s)", "v(vcc)", "i(vcc)",
            "i(vr)", "i(vs)",
        ],
        values: &[
            ("time", 0, 0., 0.),
            ("v(c1)", 0, 2.776201169580282e+00, 0.),
            ("v(c2)", 0, 4.571172043463810e+00, 0.),
            ("time", 8840, 7.999999999999999e-05, 0.),
            ("v(c1)", 8840, 2.776201169580682e+00, 0.),
        ],
    },
    Expectation {
        fixture: "m7_ic_diode_uic_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 2037,
        variables: &["time", "v(a)", "v(b)"],
        values: &[
            // uic: no t = 0 row; the first row is the first accepted step.
            ("time", 0, 1e-9, 0.),
            ("v(a)", 0, 1.993464826232599e+00, 0.),
            ("v(b)", 0, 5.647869507232903e-01, 0.),
            ("time", 2036, 4e-5, 0.),
            ("v(b)", 2036, 5.154745850508444e-02, 0.),
        ],
    },
    Expectation {
        fixture: "m7_ic_bjt_flipflop_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 6028,
        variables: &[
            "time", "v(b1)", "v(b2)", "v(c1)", "v(c2)", "v(r)", "v(vcc)", "i(vcc)", "i(vr)",
        ],
        values: &[
            // The .ic values are exact at t = 0; the reset pulse flips the pair.
            ("v(c1)", 0, 2e-1, 0.),
            ("v(c2)", 0, 4., 0.),
            ("v(b1)", 0, 7.276538644059967e-01, 0.),
            ("v(c1)", 6027, 4.608859901358739e+00, 0.),
            ("v(c2)", 6027, 9.683877378445170e-02, 0.),
        ],
    },
    Expectation {
        fixture: "m7_ic_bjt_off_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 6181,
        variables: &[
            "time", "v(b1)", "v(b2)", "v(c1)", "v(c2)", "v(s)", "v(vcc)", "i(vcc)", "i(vs)",
        ],
        values: &[
            ("v(c1)", 0, 9.683877378445103e-02, 0.),
            ("v(c2)", 0, 4.608859901358740e+00, 0.),
            ("v(c1)", 6180, 4.608926182116719e+00, 0.),
            ("v(c2)", 6180, 8.607175092483089e-02, 0.),
        ],
    },
    Expectation {
        fixture: "m7_ic_mos1_uic_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 20036,
        variables: &[
            "time", "v(in)", "v(mid)", "v(out)", "v(vdd)", "i(vdd)", "i(vin)",
        ],
        values: &[
            ("time", 0, 2e-13, 0.),
            ("v(mid)", 0, 2.146194135632362e+00, 0.),
            ("v(out)", 0, 2.422129664440773e+00, 0.),
            ("time", 20035, 4e-8, 0.),
            ("v(out)", 20035, 2.999999986318057e+00, 0.),
        ],
    },
    Expectation {
        fixture: "m7_ic_latch_nodeset_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(vdd)", "v(q)", "v(qb)", "i(vdd)"],
        values: &[
            ("v(q)", 0, 2.999999986318182e+00, 0.),
            ("v(qb)", 0, 1.090579700714479e-08, 0.),
        ],
    },
    Expectation {
        fixture: "m7_ic_latch_mos1_ic_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(vdd)", "v(q)", "v(qb)", "i(vdd)"],
        values: &[
            ("v(q)", 0, 2.999999986318182e+00, 0.),
            ("v(qb)", 0, 1.090579709549613e-08, 0.),
        ],
    },
    Expectation {
        fixture: "m4_diode_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 31,
        variables: &["frequency", "v(in)", "v(out)", "i(v1)"],
        values: &[
            ("frequency", 0, 1e3, 0.),
            ("v(out)", 0, 2.898859268332467e-1, -2.023191324596308e-5),
        ],
    },
    Expectation {
        fixture: "m4_diode_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1020,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        values: &[("time", 0, 0., 0.), ("v(out)", 0, 4.977256113494994e-1, 0.)],
    },
    Expectation {
        fixture: "m4_mos1_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 31,
        variables: &[
            "frequency",
            "v(drain)",
            "v(gate)",
            "v(vdd)",
            "i(vdd)",
            "i(vin)",
        ],
        values: &[
            ("frequency", 0, 1e3, 0.),
            ("v(drain)", 0, -1.400396755029898, 2.088523065460837e-4),
        ],
    },
    Expectation {
        fixture: "m4_mos1_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1020,
        variables: &["time", "v(drain)", "v(gate)", "v(vdd)", "i(vdd)", "i(vin)"],
        values: &[
            ("time", 0, 0., 0.),
            ("v(drain)", 0, 3.749999962400001, 0.),
            ("v(gate)", 0, 1.5, 0.),
        ],
    },
    // MOS1 completion (#88). At t = 0 the input is low: the NMOS is off, the
    // PMOS pulls the output to VDD, and the supply only feeds the reverse NMOS
    // drain junction (gmin * 3.3 V plus JS * AD).
    Expectation {
        fixture: "m7_mos1_inverter_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 10029,
        variables: &["time", "v(in)", "v(out)", "v(vdd)", "i(vdd)", "i(vin)"],
        values: &[
            ("time", 0, 0., 0.),
            ("v(in)", 0, 0., 0.),
            ("v(out)", 0, 3.299999997161830, 0.),
            ("i(vdd)", 0, -3.300190593273314e-12, 0.),
        ],
    },
    // The operating point of a symmetric three-stage ring is metastable: every
    // node sits at the same inverter switching voltage and each stage draws
    // the same crowbar current until the current kick at 0.2 ns.
    Expectation {
        fixture: "m7_mos1_ring_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 12017,
        variables: &["time", "v(n1)", "v(n2)", "v(n3)", "v(vdd)", "i(vdd)"],
        values: &[
            ("time", 0, 0., 0.),
            ("v(n1)", 0, 1.583269636740751, 0.),
            ("v(n2)", 0, 1.583269636740767, 0.),
            ("v(n3)", 0, 1.583269636740759, 0.),
            ("i(vdd)", 0, -6.843983314869297e-4, 0.),
        ],
    },
    // A common-source stage: the supply current is the load-resistor current,
    // i(vdd) = v(drain) / 10 k (the drain is the only path from VDD).
    Expectation {
        fixture: "m7_mos1_meyer_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 36,
        variables: &[
            "frequency",
            "v(drain)",
            "v(gate)",
            "v(vdd)",
            "i(vdd)",
            "i(vin)",
        ],
        values: &[
            ("frequency", 0, 1e3, 0.),
            ("v(drain)", 0, -2.642323147155134, 4.132728256333773e-6),
            ("i(vdd)", 0, -2.642323147155135e-4, 4.132728256333773e-10),
        ],
    },
    // Gate sweep with fixed body biases: the forward bulk-junction current
    // i(vbs) of m1/m2 does not depend on the gate; at vgs = 0 the PMOS is off
    // and i(vd3) is its reverse drain junction (gmin * 1.7 V plus IS).
    Expectation {
        fixture: "m7_mos1_process_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 31,
        variables: &[
            "v(v-sweep)",
            "v(b)",
            "v(bp)",
            "v(d1)",
            "v(d2)",
            "v(d3)",
            "i(egp)",
            "v(gp)",
            "v(g)",
            "i(vbp)",
            "i(vbs)",
            "i(vd1)",
            "i(vd2)",
            "i(vd3)",
            "i(vgs)",
        ],
        values: &[
            ("v(v-sweep)", 0, 0., 0.),
            ("i(vbs)", 0, -1.024308251724379e-7, 0.),
            ("i(vbs)", 30, -1.024308251724379e-7, 0.),
            ("i(vd3)", 0, 1.710000042799543e-12, 0.),
            ("i(vd1)", 30, -9.838819194962108e-4, 0.),
        ],
    },
    Expectation {
        fixture: "mos_inverter",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(vdd)", "v(drain)", "v(gate)", "i(vdd)", "i(vin)"],
        // Level-1 NMOS with vto = 1 V and vgs = 2 V, so vov = 1 V and the drain
        // current is 0.5 kp W/L vov^2 = 4.36e-4 A, which the 10 k load resistor
        // turns into a 4.36 V drop. The gate draws no current.
        values: &[
            ("v(drain)", 0, 6.417424290817677e-1, 0.0),
            ("v(gate)", 0, 2.0, 0.0),
            ("i(vdd)", 0, -4.358257570918232e-4, 0.0),
            ("i(vin)", 0, 0.0, 0.0),
        ],
    },
    Expectation {
        fixture: "options_gmin_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 11,
        variables: &["v(v-sweep)", "v(in)", "v(out)", "i(v1)"],
        // `.options gmin={gj}` = 1 uS: at v1 = -10 V the reverse diode and
        // q1's base-collector junction each conduct gmin, q2's (m=2, area=3)
        // conducts m * gmin = 2 uS (area does not scale gmin; the lateral PNPs'
        // substrate gmin sits base-to-ground at 0 V), so the 1 k source
        // resistor sees 4 uS: v(out) = -10 / (1 + 1k * 4u).
        values: &[
            ("v(v-sweep)", 0, -10.0, 0.0),
            ("v(out)", 0, -9.960159362538445, 0.0),
            ("i(v1)", 0, 3.984063746155542e-5, 0.0),
            ("v(v-sweep)", 10, 0.0, 0.0),
        ],
    },
    Expectation {
        fixture: "options_xmu_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 629,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        // RC (tau = 100 us) charging again 290 us after the second 10 V pulse
        // edge at 910 us, integrated with trapezoidal xmu = 0.2.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("time", 628, 1.2e-3, 0.0),
            ("v(in)", 628, 10.0, 0.0),
            ("v(out)", 628, 9.473638039474475, 0.0),
            ("i(v1)", 628, -5.263619605255255e-4, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_divider",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(in)", "v(out)", "i(v1)"],
        // 5 V across two equal resistors: half of it at the tap, and 5 V / 2 k
        // flowing out of the source terminal, hence negative.
        values: &[
            ("v(in)", 0, 5.0, 0.0),
            ("v(out)", 0, 2.5, 0.0),
            ("i(v1)", 0, -2.5e-3, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_exp_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 817,
        variables: &["time", "v(in)", "v(mark)", "v(out)", "i(v1)", "i(v2)"],
        // EXP(0 1 0.2m 0.3m 1.5m 0.5m): at 4 ms v(in) = (1 - e^-(3.8/0.3))
        // - (1 - e^-(2.5/0.5)) = e^-5 - e^-12.67 = 6.7348e-3; the marker is 0.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(in)", 0, 0.0, 0.0),
            ("time", 816, 4.000000000000000e-03, 0.0),
            ("v(in)", 816, 6.734792455280303e-03, 0.0),
            ("v(mark)", 816, 0.0, 0.0),
            ("v(out)", 816, 8.417684883876841e-03, 0.0),
            ("i(v1)", 816, 1.682892428596538e-06, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_gear_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 630,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        // Gear-2 RC (tau = 100 us), PULSE 0 -> 1 V: by 1.2 ms the capacitor has
        // charged for 0.29 ms through the second pulse, v(out) = 0.948.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(out)", 0, 0.0, 0.0),
            ("time", 629, 1.200000000000000e-03, 0.0),
            ("v(in)", 629, 1.000000000000000e+00, 0.0),
            ("v(out)", 629, 9.480297958450857e-01, 0.0),
            ("i(v1)", 629, -5.197020415491427e-05, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_ic_node_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 508,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        // `.ic v(out)=0.25` without uic: enforced in the initial bias only (the
        // t = 0 row has v(out) = 0.25 and i(v1) = -(1 - 0.25)/1k), then released:
        // v(out) = 1 - 0.75 e^(-t/1 ms), 0.99495 at 5 ms.
        values: &[
            ("time", 0, 0.000000000000000e+00, 0.0),
            ("v(in)", 0, 1.000000000000000e+00, 0.0),
            ("v(out)", 0, 2.500000000000000e-01, 0.0),
            ("i(v1)", 0, -7.500000000000000e-04, 0.0),
            ("time", 507, 5.000000000000000e-03, 0.0),
            ("v(out)", 507, 9.949467496583029e-01, 0.0),
            ("i(v1)", 507, -5.053250341697124e-06, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_ic_uic_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 511,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        // `uic` with c1 ic=2 and a 0 V source: no t = 0 row; v(out) = 2 e^(-t/1 ms),
        // so 1.9998 V at the first step (0.1 us) and 2 e^-5 = 0.013476 V at 5 ms.
        values: &[
            ("time", 0, 1.000000000000000e-07, 0.0),
            ("v(in)", 0, 0.000000000000000e+00, 0.0),
            ("v(out)", 0, 1.999800019998000e+00, 0.0),
            ("i(v1)", 0, 1.999800019998001e-03, 0.0),
            ("time", 510, 5.000000000000000e-03, 0.0),
            ("v(out)", 510, 1.347533818591319e-02, 0.0),
            ("i(v1)", 510, 1.347533818591319e-05, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_lowpass_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 3,
        variables: &["frequency", "v(in)", "v(out)", "i(v1)"],
        // The source is 1 V AC, so |v(out)| = 1/sqrt(1 + (2 pi f R C)^2), which
        // is 0.8467 at 100 Hz and 0.1570 at 1 kHz.
        values: &[
            ("frequency", 0, 100.0, 0.0),
            ("v(in)", 0, 1.0, 0.0),
            ("v(out)", 0, 7.169568003248978e-1, -4.504772433683887e-1),
            ("v(out)", 2, 2.470452303185765e-2, -1.552230961346477e-1),
            ("frequency", 2, 1000.0, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_pulse_count_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 353,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        // Three pulses (NP = 3) end by 1.6 ms; at 3 ms the source holds V1 = 0
        // and the capacitor has decayed for 14 time constants.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(in)", 0, 0.0, 0.0),
            ("time", 352, 3.000000000000000e-03, 0.0),
            ("v(in)", 352, 0.0, 0.0),
            ("v(out)", 352, 4.945681909430162e-08, 0.0),
            ("i(v1)", 352, 4.945681909430162e-11, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_pwl_repeat_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 435,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        // A 1 ms triangle repeated from 0.2 ms: at 4 ms it is 0.8 ms into a
        // cycle, on the falling edge at 1 - 0.3/0.5 = 0.4 V.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(in)", 0, 0.0, 0.0),
            ("time", 434, 4.000000000000000e-03, 0.0),
            ("v(in)", 434, 4.000000000000001e-01, 0.0),
            ("v(out)", 434, 5.802638284670004e-01, 0.0),
            ("i(v1)", 434, 1.802638284670003e-04, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_pwl_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 823,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        // PWL-driven RC (tau = 1 ms): the source ends at 0.25 V after 3.1 ms and
        // the capacitor is still 4.4 mV above it at 8 ms.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(out)", 0, 0.0, 0.0),
            ("time", 822, 8.000000000000000e-03, 0.0),
            ("v(in)", 822, 2.500000000000000e-01, 0.0),
            ("v(out)", 822, 2.543555791170526e-01, 0.0),
            ("i(v1)", 822, 4.355579117052644e-06, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_sffm_am_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 2012,
        variables: &["time", "v(in)", "v(out)", "i(v1)", "v(x)"],
        // At 2 ms both carriers complete whole cycles: sin(20 pi + sin(2 pi)) is
        // zero up to rounding; the filtered outputs lag behind.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(in)", 0, 0.0, 0.0),
            ("v(x)", 0, 0.0, 0.0),
            ("time", 2011, 2.000000000000000e-03, 0.0),
            ("v(in)", 2011, -2.449293598294707e-15, 0.0),
            ("v(out)", 2011, -3.087246188813056e-01, 0.0),
            ("i(v1)", 2011, -3.087246188813032e-04, 0.0),
            ("v(x)", 2011, -2.777895339201290e-01, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_sin_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1511,
        variables: &["time", "v(in)", "v(mark)", "v(out)", "i(v1)", "i(v2)"],
        // Before TD the source holds 0.5 + sin(30 deg) = 1 V (the bias point);
        // at 3 ms, 0.5 + sin(5.6 pi + pi/6) e^-0.56 = 0.1178 V.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(in)", 0, 1.000000000000000e+00, 0.0),
            ("v(out)", 0, 1.000000000000000e+00, 0.0),
            ("time", 1510, 3.000000000000000e-03, 0.0),
            ("v(in)", 1510, 1.177865327491662e-01, 0.0),
            ("v(out)", 1510, 2.679214278175245e-02, 0.0),
            ("i(v1)", 1510, -9.099438996741378e-05, 0.0),
        ],
    },
    Expectation {
        fixture: "rc_transient",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 70,
        variables: &["time", "v(in)", "v(out)", "i(v1)"],
        // tstop is five time constants, so the capacitor reaches 5(1 - e^-5).
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(out)", 0, 0.0, 0.0),
            ("time", 69, 5e-6, 0.0),
            ("v(out)", 69, 4.966429361579027, 0.0),
        ],
    },
    Expectation {
        fixture: "rl_pulse_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 630,
        variables: &["time", "v(in)", "i(l1)", "v(out)", "i(v1)"],
        // R-L (tau = 100 us) driven by PULSE; the second pulse starts at 0.52 ms,
        // so at 0.6 ms the source is high and i(l1) has risen to about 5.7 mA.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("i(l1)", 0, 0.0, 0.0),
            ("time", 629, 5.999999999999999e-04, 0.0),
            ("v(in)", 629, 1.000000000000000e+00, 0.0),
            ("i(l1)", 629, 5.681063666152677e-03, 0.0),
            ("v(out)", 629, 4.318936333847322e-01, 0.0),
            ("i(v1)", 629, -5.681063666152677e-03, 0.0),
        ],
    },
    Expectation {
        fixture: "rlc_ic_uic_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1011,
        variables: &["time", "v(a)", "v(in)", "i(l1)", "v(out)", "i(v1)"],
        // Free decay of l1 ic=20m and c1 ic=1 (zeta = 0.158, damped period 200 us):
        // no t = 0 row, the first step (10 ns) sits at 20 mA and 1 V; the envelope
        // e^(-5000 t) leaves 5.6 mV at 1 ms.
        values: &[
            ("time", 0, 1.000000000000000e-08, 0.0),
            ("v(a)", 0, -1.998799920127995e-01, 0.0),
            ("v(in)", 0, 0.000000000000000e+00, 0.0),
            ("i(l1)", 0, 1.998799920127995e-02, 0.0),
            ("v(out)", 0, 1.000199879992013e+00, 0.0),
            ("i(v1)", 0, -1.998799920127996e-02, 0.0),
            ("time", 1010, 1.000000000000000e-03, 0.0),
            ("i(l1)", 1010, 1.780552167603876e-04, 0.0),
            ("v(out)", 1010, 5.583038600297606e-03, 0.0),
        ],
    },
    Expectation {
        fixture: "rlc_series",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(in)", "i(l1)", "v(mid)", "v(out)", "i(v1)"],
        // At DC the inductor is a short and the capacitor an open, so no current
        // flows at all: every node sits at the source voltage and both currents
        // are zero. This is the fixture that pins the inductor branch current.
        values: &[
            ("v(in)", 0, 1.0, 0.0),
            ("i(l1)", 0, 0.0, 0.0),
            ("v(mid)", 0, 1.0, 0.0),
            ("v(out)", 0, 1.0, 0.0),
            ("i(v1)", 0, 0.0, 0.0),
        ],
    },
    Expectation {
        fixture: "rlc_series_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 100,
        variables: &["frequency", "v(a)", "v(in)", "i(l1)", "v(out)", "i(v1)"],
        // Series RLC low-pass, v(out) across C: H(jw) = 1/(1 - w^2 LC + jwRC).
        // At 10 kHz w^2 LC = 3.948 so H = 1/(-2.948 + 0.628j) = (-0.3245, -0.0692).
        values: &[
            ("frequency", 0, 1.000000000000000e+02, 0.0),
            ("v(out)", 0, 1.000355416442797e+00, -6.287900818294491e-03),
            ("frequency", 99, 1.000000000000000e+04, 0.0),
            ("v(in)", 99, 1.000000000000000e+00, 0.0),
            ("v(out)", 99, -3.244893876309116e-01, -6.916337844392535e-02),
            ("i(v1)", 99, -4.345663232337732e-03, 2.038826952698245e-02),
        ],
    },
    Expectation {
        fixture: "rlc_series_gear_maxord6_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1030,
        variables: &["time", "v(a)", "v(in)", "i(l1)", "v(out)", "i(v1)"],
        // rlc_series_gear_tran with maxord=6: dctran.c only ever raises the
        // order from 1 to 2, so these are exactly the Gear-2 values below.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(out)", 0, 0.0, 0.0),
            ("time", 1029, 1.000000000000000e-03, 0.0),
            ("v(out)", 1029, -6.380437251083911e-02, 0.0),
            ("i(l1)", 1029, 9.117920253419437e-04, 0.0),
        ],
    },
    Expectation {
        fixture: "rlc_series_gear_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1030,
        variables: &["time", "v(a)", "v(in)", "i(l1)", "v(out)", "i(v1)"],
        // Same circuit as rlc_series_tran with Gear-2; both ring down to about -64 mV
        // at 1 ms (analytic damped response).
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(out)", 0, 0.0, 0.0),
            ("time", 1029, 1.000000000000000e-03, 0.0),
            ("v(out)", 1029, -6.380437251083911e-02, 0.0),
            ("i(l1)", 1029, 9.117920253419437e-04, 0.0),
        ],
    },
    Expectation {
        fixture: "rlc_series_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1026,
        variables: &["time", "v(a)", "v(in)", "i(l1)", "v(out)", "i(v1)"],
        // Underdamped series RLC (zeta = 0.158) after a pulse that ended at 0.49 ms.
        // i(l1) = -i(v1) is the series current.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(out)", 0, 0.0, 0.0),
            ("time", 1025, 1.000000000000000e-03, 0.0),
            ("v(out)", 1025, -6.366414858193775e-02, 0.0),
            ("i(l1)", 1025, 9.174249997766158e-04, 0.0),
            ("i(v1)", 1025, -9.174249997766159e-04, 0.0),
        ],
    },
    Expectation {
        fixture: "subckt_divider",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &["v(in)", "v(out)", "i(v1)"],
        // The subcircuit adds a 1 k resistor in series with the 1 k load, so the
        // 10 V source is halved across the tap.
        values: &[
            ("v(in)", 0, 10.0, 0.0),
            ("v(out)", 0, 5.0, 0.0),
            ("i(v1)", 0, -5.0e-3, 0.0),
        ],
    },
    // Behavioural sources (#79). v(in) = 0.8, v(b) = -0.3: b1 is
    // 2.5 v(in)^2 - sqrt|v(b)| + exp(v(in) - v(b)); its current is sensed by
    // i(b1) through the zero-volt v_b1 that inp_meas_current() inserts at
    // o1 (o1_vmeas_0). b3 = 2 ln(1.8) (m=2), b4 = pwl 2.4 + min -0.3 + max
    // 0.8, b7 = v(o1)/2 x (1 + 1m 23 + 1u 23^2) (temp=50) and b9 =
    // -v(o1) + v(o1)^2/10 + 0.8^v(o3) + 2^-0.3.
    Expectation {
        fixture: "bsource_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(in)",
            "i(b1)",
            "i(b3)",
            "i(b4)",
            "i(b5)",
            "v(b)",
            "i(b6)",
            "i(b8)",
            "i(b9)",
            "v(o1)",
            "v(o1_vmeas_0)",
            "v(o2)",
            "v(o3)",
            "v(o4)",
            "v(o5)",
            "v(o6)",
            "v(o7)",
            "v(o8)",
            "v(o9)",
            "i(v_b1)",
            "i(vb)",
            "i(vin)",
        ],
        values: &[
            ("v(o1)", 0, 4.056443466441268e+00, 0.0),
            ("v(o1_vmeas_0)", 0, 4.056443466441268e+00, 0.0),
            ("i(b1)", 0, -4.056443466441267e-03, 0.0),
            ("i(v_b1)", 0, -4.056443466441267e-03, 0.0),
            ("v(o3)", 0, 1.175573329804238e+00, 0.0),
            ("v(o4)", 0, 2.900000000000000e+00, 0.0),
            ("v(o7)", 0, 2.075943762381582e+00, 0.0),
            ("v(o9)", 0, -8.294541275689211e-01, 0.0),
            ("i(vin)", 0, -8.000000000000000e-04, 0.0),
            ("v(o2)", 0, -1.285207636379154e-01, 0.0),
        ],
    },
    // v(lim) = 2 tanh(1.5 v) + 0.1 v, v(ter) = 2 x 1m (0.34 - v) or 2 x 1m v
    // into 1k, v(a) = 100 x 1p (exp(v/0.05) - 1) (about 1.07 kV at 1.5 V) and
    // v(sq) = sqrt(v^2 + 0.01) at the sweep ends.
    Expectation {
        fixture: "bsource_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 31,
        variables: &[
            "v(v-sweep)",
            "v(a)",
            "i(b2)",
            "i(b3)",
            "i(b4)",
            "i(b6)",
            "v(in)",
            "v(lim)",
            "v(pw)",
            "v(sh)",
            "v(sq)",
            "v(ter)",
            "i(vin)",
        ],
        values: &[
            ("v(v-sweep)", 0, -1.500000000000000e+00, 0.0),
            ("v(lim)", 0, -2.106052229477627e+00, 0.0),
            ("v(ter)", 0, 3.680000000000000e+00, 0.0),
            ("v(v-sweep)", 30, 1.500000000000001e+00, 0.0),
            ("v(lim)", 30, 2.106052229477628e+00, 0.0),
            ("v(ter)", 30, 3.000000000000001e+00, 0.0),
            ("v(a)", 30, 1.068647458152356e+03, 0.0),
            ("v(sq)", 30, 1.503329637837291e+00, 0.0),
        ],
    },
    // The AC drive of vin passes through each B source's derivative at the
    // 0.6 V bias: v(sq) = 6 x 0.6 + 0.5 cos(0.6) at every frequency, and
    // b3 = 1k i(vin) v(in) linearises to 1k (0.6 x -1m + -0.6m x 1) = -1.2.
    // b4 reads hertz (C re-solves the bias per frequency): its derivative is
    // f/1k + 1.2 sqrt(f), 3.8047 at 10 Hz and 2200 at 1 MHz.
    Expectation {
        fixture: "bsource_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 26,
        variables: &[
            "frequency",
            "i(b1)",
            "i(b3)",
            "i(b4)",
            "v(cur)",
            "v(hz)",
            "v(in)",
            "v(out1)",
            "v(out2)",
            "v(sq)",
            "i(vin)",
        ],
        values: &[
            ("frequency", 0, 1.000000000000000e+01, 0.0),
            ("v(sq)", 0, 4.012667807454839e+00, 0.0),
            ("v(cur)", 0, -1.200000000000000e+00, 0.0),
            ("i(vin)", 0, -1.000000000000000e-03, 0.0),
            ("v(hz)", 0, 3.804733192202056e+00, 0.0),
            ("frequency", 25, 1.000000000000002e+06, 0.0),
            ("v(hz)", 25, 2.200000000000002e+03, 0.0),
            ("v(out1)", 25, 1.016418054920059e-05, -6.386342988625776e-03),
            ("v(out2)", 25, 3.008985388246513e-06, -1.366126857795614e-02),
        ],
    },
    // At 1 ms the envelope has settled: v(drive) = 1.5 sin(4 pi) x ... +
    // pwl(1m) = -0.25, and v(sq) = v(out) v(drive) + 2 within C's Newton
    // tolerance.
    Expectation {
        fixture: "bsource_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1008,
        variables: &["time", "i(b1)", "i(b3)", "v(drive)", "v(out)", "v(sq)"],
        values: &[
            ("time", 0, 0.000000000000000e+00, 0.0),
            ("v(drive)", 0, 0.000000000000000e+00, 0.0),
            ("time", 1007, 1.000000000000000e-03, 0.0),
            ("v(drive)", 1007, -2.499999999999999e-01, 0.0),
            ("v(out)", 1007, -3.623175662242532e-02, 0.0),
            ("v(sq)", 1007, 2.009060417479346e+00, 0.0),
            ("i(b3)", 1007, -2.009060417479346e-03, 0.0),
        ],
    },
    // Every B source here starts Newton at 0 V on a ~1e32 slope (or log()'s
    // -1e99). v(in) = 2, v(mid) = 1.5: v(o1) = 1/2, v(o2) = sqrt(1.5),
    // v(o3) = ln 2 + ln 1.5 = ln 3, v(o4) = 2/2 - 1/1.5 and
    // v(o5) = 1k x 1m (log10 1.5 + sqrt(2)/1.5).
    Expectation {
        fixture: "bsource_zero_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(in)", "i(b1)", "i(b2)", "i(b3)", "i(b4)", "v(mid)", "v(o1)", "v(o2)", "v(o3)",
            "v(o4)", "v(o5)", "i(vin)",
        ],
        values: &[
            ("v(mid)", 0, 1.500000000000000e+00, 0.0),
            ("v(o1)", 0, 5.000000000000000e-01, 0.0),
            ("v(o2)", 0, 1.224744871391589e+00, 0.0),
            ("v(o3)", 0, 1.098612288668110e+00, 0.0),
            ("v(o4)", 0, 3.333333333333335e-01, 0.0),
            ("v(o5)", 0, 1.118900300637745e+00, 0.0),
        ],
    },
    // The first point (v(in) = 0.5, v(mid) = 0.375) starts Newton at 0 V:
    // v(o1) = 2, v(o2) = sqrt(0.375) ln 0.5 and v(o3) = 0.5/0.375 +
    // log10 0.375; at 3 V (v(mid) = 2.25) v(o2) = 1.5 ln 3.
    Expectation {
        fixture: "bsource_zero_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 11,
        variables: &[
            "v(v-sweep)",
            "i(b1)",
            "i(b2)",
            "v(in)",
            "v(mid)",
            "v(o1)",
            "v(o2)",
            "v(o3)",
            "i(vin)",
        ],
        values: &[
            ("v(v-sweep)", 0, 5.000000000000000e-01, 0.0),
            ("v(o1)", 0, 2.000000000000000e+00, 0.0),
            ("v(o2)", 0, -4.244642272551664e-01, 0.0),
            ("v(o3)", 0, 9.073646010610521e-01, 0.0),
            ("v(v-sweep)", 10, 3.000000000000000e+00, 0.0),
            ("v(o2)", 10, 1.647918433002165e+00, 0.0),
            ("v(o3)", 10, 1.685515851444696e+00, 0.0),
        ],
    },
    // The initial point (v(in) = 2) starts Newton at 0 V: v(o1) = 1/2 +
    // sqrt 2, v(o2) = 1k x 1m ln 2 / 2. At 1 ms v(in) is 2 again; C's B
    // outputs there carry its converged-Newton linearisation error (reltol).
    Expectation {
        fixture: "bsource_zero_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1006,
        variables: &["time", "i(b1)", "v(in)", "v(o1)", "v(o2)", "i(vin)"],
        values: &[
            ("time", 0, 0.000000000000000e+00, 0.0),
            ("v(o1)", 0, 1.914213562373095e+00, 0.0),
            ("v(o2)", 0, 3.465735902799727e-01, 0.0),
            ("time", 1005, 1.000000000000000e-03, 0.0),
            ("v(in)", 1005, 2.000000000000000e+00, 0.0),
            ("v(o1)", 1005, 1.914207239425050e+00, 0.0),
        ],
    },
    // v(in) = 1.2: e1 drives 3 v - v^2/2 = 2.88 through e1_int1, e2 adds
    // tanh(1.2) on top, g1 sources 2 (m) x 1m x 1.2^3 into o3, g2 sinks
    // 0.5m exp(1.2) from o4, x1's E gives 1.44/3 at o5 (x1.e1_int1), and
    // i(e2) is sensed through the inserted v_e2 (e2 is not a simple E).
    Expectation {
        fixture: "evalue_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(in)",
            "i(b.x1.be1)",
            "i(b1)",
            "i(be1)",
            "i(be2)",
            "i(bg1)",
            "i(bg2)",
            "i(e.x1.e1)",
            "i(e1)",
            "v(e1_int1)",
            "i(e2)",
            "v(e2_int1)",
            "v(g1_int1)",
            "v(g2_int1)",
            "v(o1)",
            "v(o2)",
            "v(o2_vmeas_0)",
            "v(o3)",
            "v(o4)",
            "v(o5)",
            "v(o6)",
            "i(v_e2)",
            "i(vin)",
            "v(x1.e1_int1)",
        ],
        values: &[
            ("v(o1)", 0, 2.880000000000001e+00, 0.0),
            ("v(e1_int1)", 0, 2.880000000000001e+00, 0.0),
            ("v(o2)", 0, 3.713654607012156e+00, 0.0),
            ("v(o3)", 0, 3.456000000000002e+00, 0.0),
            ("v(o4)", 0, -1.660058461368274e+00, 0.0),
            ("v(o5)", 0, 4.800000000000000e-01, 0.0),
            ("v(x1.e1_int1)", 0, 4.800000000000000e-01, 0.0),
            ("i(v_e2)", 0, -1.856827303506078e-03, 0.0),
            ("v(o6)", 0, -1.376827303506078e+00, 0.0),
        ],
    },
    // Below the tables the XSPICE pwl is flat (limit=TRUE): g1 = 2 (m) x -1m
    // into 1k, e2 (LTspice four-node form) = 2; the single pair of g2 is a
    // 3 mA source; on linear segments g1 = 2 x (2m + 0.5m (v - 0.5)), and at
    // the top e1's input is 2 x 2.05 + 0.1 with the output flat at 1.75.
    Expectation {
        fixture: "gtable_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 42,
        variables: &[
            "v(v-sweep)",
            "i(ae1)",
            "i(ae2)",
            "i(ag1)",
            "i(be1)",
            "i(bg1)",
            "i(e1)",
            "v(e1_int1)",
            "v(e1_int2)",
            "i(e2)",
            "v(e2_int1)",
            "v(g1_int1)",
            "v(g1_int2)",
            "v(g2_int1)",
            "v(in)",
            "v(o1)",
            "v(o2)",
            "v(o3)",
            "v(o4)",
            "i(vg2)",
            "i(vin)",
        ],
        values: &[
            ("v(v-sweep)", 0, -2.050000000000000e+00, 0.0),
            ("v(o1)", 0, -2.000000000000000e+00, 0.0),
            ("v(o3)", 0, 2.000000000000000e+00, 0.0),
            ("v(o4)", 0, 3.000000000000000e+00, 0.0),
            ("v(o1)", 41, 5.000000000000000e+00, 0.0),
            ("v(o2)", 41, 1.750000000000000e+00, 0.0),
            ("v(e1_int2)", 41, 4.200000000000003e+00, 0.0),
            ("v(in)", 30, 9.500000000000008e-01, 0.0),
            ("v(o1)", 30, 4.450000000000001e+00, 0.0),
        ],
    },
    // At v(in) = 0.4 (v(b) = 0.5, v(c) = -0.2, i(vin) = -0.4m, i(vb) =
    // -0.25m): e1 = 0.1 + a + 2b + 0.5a^2 - 0.25ab + 0.3b^2, h1 = 0.2 +
    // 100 i + 1e4 i^2, the implicit POLY(1) e2 = 0.5 + d + 0.25 d^2 with
    // d = v(in, b), g1 = 2 (m) x (1m a + 0.5m a^2 + 0.2m a^3) into 1k, f1 =
    // i(vin) - 2 i(vb) + 0.5 i(vin)^2 into 1k and e3 the full 3-D quadratic
    // in SPICE2 term order (a, b, c, a^2, ab, ac, b^2, bc, c^2).
    Expectation {
        fixture: "epoly_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 21,
        variables: &[
            "v(v-sweep)",
            "i(a$poly$e1)",
            "i(a$poly$e2)",
            "i(a$poly$e3)",
            "i(a$poly$h1)",
            "v(b)",
            "v(c)",
            "v(in)",
            "v(o1)",
            "v(o2)",
            "v(o3)",
            "v(o4)",
            "v(o5)",
            "v(o6)",
            "i(vb)",
            "i(vc)",
            "i(vin)",
        ],
        values: &[
            ("v(v-sweep)", 14, 3.999999999999999e-01, 0.0),
            ("v(o1)", 14, 1.605000000000000e+00, 0.0),
            ("v(o4)", 14, 1.616000000000000e-01, 0.0),
            ("v(o5)", 14, 4.024999999999999e-01, 0.0),
            ("v(o2)", 14, 9.855999999999998e-01, 0.0),
            ("v(o6)", 14, -9.400000000000010e-02, 0.0),
            ("v(o3)", 14, 1.000800000000001e-01, 0.0),
            ("i(vb)", 14, -2.500000000000000e-04, 0.0),
        ],
    },
    Expectation {
        fixture: "switch_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 5,
        variables: &[
            "frequency",
            "v(a)",
            "v(b)",
            "v(ctrl)",
            "v(e)",
            "v(f)",
            "i(vc)",
            "v(vdd)",
            "i(vdd)",
            "i(vs)",
            "v(x)",
        ],
        // ACan's MODEINITSMSIG load copies the zero CKTstate1 into CKTstate0,
        // so every switch is open in the AC sweep although s2 (on in its band),
        // s3 and w1 are closed at the operating point: v(b) = 2M/2.001M,
        // v(f) = 1M/1.001M, and v(a)/v(e) are 1k into 1u with the open switch.
        values: &[
            ("frequency", 0, 1.000000000000000e+01, 0.0),
            ("v(a)", 0, 9.950804240186061e-01, -6.246028670984753e-02),
            ("v(b)", 0, 9.995002498750625e-01, 0.0),
            ("v(e)", 0, 9.960676814189385e-01, -6.258477814589433e-02),
            ("v(f)", 0, 9.990009990009990e-01, 0.0),
            ("i(vs)", 0, 1.000000000000000e-03, 0.0),
            ("frequency", 4, 1.000000000000000e+03, 0.0),
            ("v(a)", 4, 2.472800515685005e-02, -1.552154232541272e-01),
            ("i(vdd)", 4, -1.952066222911747e-03, -3.104385193811054e-04),
        ],
    },
    Expectation {
        fixture: "switch_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 49,
        variables: &[
            "v(v-sweep)",
            "v(a)",
            "v(b)",
            "v(c)",
            "v(ctrl)",
            "v(d)",
            "v(e)",
            "v(sx)",
            "i(vc)",
            "v(vdd)",
            "i(vdd)",
            "i(vsense)",
        ],
        // vc falls from 3 V in 0.125 V steps. s1 (1 +- 0.5 V) closes at 3 V
        // (10 ohm: 1/101) and stays closed through the band down to 0.5 V,
        // opening at -0.25 V (1 Mohm: 0.999). w1 (1 +- 0.6 mA) stays closed
        // (50 ohm: 1/21) until the sensed vc/1k drops below 0.4 mA. w2's
        // negative band (-1.4 .. -0.6 mA) opens it at -0.75 mA (3 Mohm). s3
        // (VT = -2, VH = 0) closes exactly at its threshold (vc = 2 V), where
        // C maps the previous really-off state to on.
        values: &[
            ("v(a)", 0, 9.900990099009901e-03, 0.0),
            ("v(c)", 0, 9.999000099990001e-01, 0.0),
            ("v(d)", 0, 4.761904761904762e-02, 0.0),
            ("v(e)", 0, 3.846153846153846e-02, 0.0),
            ("v(c)", 8, 9.990009990009992e-04, 0.0),
            ("v(b)", 12, 9.900990099009901e-01, 0.0),
            ("v(a)", 20, 9.900990099009901e-03, 0.0),
            ("v(d)", 20, 4.761904761904762e-02, 0.0),
            ("v(a)", 26, 9.990009990009990e-01, 0.0),
            ("v(d)", 26, 9.995002498750625e-01, 0.0),
            ("v(e)", 26, 3.846153846153846e-02, 0.0),
            ("v(e)", 30, 9.996667777407531e-01, 0.0),
            ("i(vsense)", 48, -3.000000000000000e-03, 0.0),
        ],
    },
    Expectation {
        fixture: "switch_dc_decimal",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 18,
        variables: &[
            "v(v-sweep)",
            "v(a)",
            "v(b)",
            "v(c)",
            "v(ctrl)",
            "v(d)",
            "v(e)",
            "v(sx)",
            "i(vc)",
            "v(vdd)",
            "i(vdd)",
            "i(vsense)",
        ],
        // C accumulates vc += 0.1 from 0.5 V: the fifth step is
        // 0.9999999999999999, so s1 (VT = 1, no band) is still open (0.999)
        // and closes at 1.1 V (1/101); the tenth is 1.5000000000000002, above
        // s2's band edge, so s2 and w1 (IT = 1.5 mA) close there. s4 (ON) is
        // closed from the first point by its flag (20 ohm: 1/51), s3 (OFF)
        // open until 2.1 V.
        values: &[
            ("v(a)", 5, 9.990009990009990e-01, 0.0),
            ("v(a)", 6, 9.900990099009901e-03, 0.0),
            ("v(b)", 9, 9.990009990009990e-01, 0.0),
            ("v(b)", 10, 9.900990099009901e-03, 0.0),
            ("v(e)", 9, 9.996667777407531e-01, 0.0),
            ("v(e)", 10, 2.912621359223301e-02, 0.0),
            ("v(c)", 15, 9.995002498750625e-01, 0.0),
            ("v(c)", 16, 1.960784313725490e-02, 0.0),
            ("v(d)", 0, 1.960784313725490e-02, 0.0),
        ],
    },
    Expectation {
        fixture: "switch_op",
        plotname: "Operating Point",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(ctrl)",
            "v(a)",
            "v(b)",
            "v(c)",
            "v(d)",
            "v(e)",
            "v(f)",
            "v(g)",
            "v(hi)",
            "v(l1)",
            "v(l2)",
            "v(src)",
            "v(sx)",
            "i(vc)",
            "v(vdd)",
            "i(vdd)",
            "i(vhi)",
            "i(vi)",
            "v(vl)",
            "i(vl)",
            "i(vsense)",
            "i(vsl)",
        ],
        // 1 V through 1k into each switch: open (1 Mohm) gives 0.999, closed
        // (10 ohm) 1/101. s1/s2 sit inside their band at 1.5 V, so the instance
        // flag decides; s3/s4 follow a control outside it whatever the flag;
        // s5 closes with the default 1 S (1/1001). w1 senses 2 mA > 1.5 mA
        // (20 ohm: 1/51). w2 (ON, band 1.4 .. 2.2 mA) opens: in MODEINITFLOAT
        // CSWload keeps CKTstate1, zero ("really off") at an operating point.
        // w3 holds itself closed: 1 V across 1k + 100 ohm is 0.909 mA.
        values: &[
            ("v(a)", 0, 9.990009990009990e-01, 0.0),
            ("v(b)", 0, 9.900990099009901e-03, 0.0),
            ("v(c)", 0, 9.900990099009901e-03, 0.0),
            ("v(d)", 0, 9.990009990009990e-01, 0.0),
            ("v(e)", 0, 9.990009990009992e-04, 0.0),
            ("v(f)", 0, 1.960784313725490e-02, 0.0),
            ("v(g)", 0, 9.995002498750625e-01, 0.0),
            ("v(l1)", 0, 9.090909090909091e-02, 0.0),
            ("i(vsl)", 0, 9.090909090909091e-04, 0.0),
            ("i(vsense)", 0, 2.000000000000000e-03, 0.0),
        ],
    },
    Expectation {
        fixture: "switch_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1046,
        variables: &[
            "time", "v(c1)", "v(c2)", "v(c3)", "v(c4)", "v(p)", "v(s)", "v(vdd)", "i(vdd)",
            "i(vp)", "i(vs)",
        ],
        // At t = 0 every switch is open (1 Gohm, 10 Mohm, 1 Mohm): c1 sits at
        // 2 V less the 2k/(2k + 1G + 20k) divider, c3 at 2 x 10M/(10M + 5k).
        // The controls draw no current.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(c1)", 0, 1.999996000087998e+00, 0.0),
            ("v(c2)", 0, 3.999912001935957e-05, 0.0),
            ("v(c3)", 0, 1.999000499750125e+00, 0.0),
            ("v(c4)", 0, 0.0, 0.0),
            ("time", 1045, 1.000000000000000e-03, 0.0),
            ("v(c1)", 1045, 1.859214521122436e+00, 0.0),
            ("v(c2)", 1045, 1.378613168658710e+00, 0.0),
            ("v(c4)", 1045, 5.418052695450274e-01, 0.0),
            ("i(vp)", 1045, 0.0, 0.0),
        ],
    },
    Expectation {
        fixture: "switch_w_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 1038,
        variables: &[
            "time",
            "v(a)",
            "v(b)",
            "v(c1)",
            "v(c2)",
            "v(c3)",
            "v(p)",
            "v(q)",
            "i(va)",
            "v(vdd)",
            "i(vdd)",
            "i(vp)",
            "i(vsense)",
            "i(vsense2)",
        ],
        // At t = 0 both switches are open: c1 = 2 x 1M/(1M + 4k), and w2 (100
        // Mohm) leaves c3 at 2 x 10k/(3k + 100M + 10k). At 2 ms the PULSE is
        // high and vsense2 carries 2 V / 1k.
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(c1)", 0, 1.992031872509960e+00, 0.0),
            ("v(c3)", 0, 1.999740033795607e-04, 0.0),
            ("time", 1037, 2.000000000000000e-03, 0.0),
            ("v(c1)", 1037, 1.493033752313648e+00, 0.0),
            ("v(c3)", 1037, 1.393870438628249e+00, 0.0),
            ("i(vsense2)", 1037, 2.000000000000000e-03, 0.0),
        ],
    }, // Diode physics (#86). The regulator conducts forward at -2 V and holds
    // its output a little above BV = 5.1 V (series resistance plus the
    // breakdown exponential) at 12 V in.
    Expectation {
        fixture: "m7_zener_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 141,
        variables: &["v(v-sweep)", "v(in)", "v(out)", "i(vin)"],
        values: &[
            ("v(out)", 0, -7.308665019568273e-01, 0.0),
            ("v(out)", 140, 5.282797124103860e+00, 0.0),
            ("i(vin)", 140, -6.717202875896115e-02, 0.0),
        ],
    },
    Expectation {
        fixture: "m7_diode_physics_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 205,
        variables: &["v(v-sweep)", "v(a)", "v(k1)", "v(k2)", "v(k3)", "i(v1)"],
        values: &[
            ("v(k1)", 0, -6.205250986919840e+00, 0.0),
            ("v(k2)", 0, -5.188330173225660e+00, 0.0),
            ("v(k3)", 0, -7.431592663148733e+00, 0.0),
            ("i(v1)", 204, -1.335006444774208e-02, 0.0),
        ],
    },
    // C names the `.dc temp` scale `temp-sweep` (type `temp-sweep`). Forward
    // voltages fall and the TCV-shifted breakdown knee drops when heated.
    Expectation {
        fixture: "m7_diode_temp_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 34,
        variables: &["temp-sweep", "v(a)", "v(b)", "v(c)", "v(k)", "i(v3)"],
        values: &[
            ("temp-sweep", 0, -40.0, 0.0),
            ("v(a)", 0, 8.105491866356285e-01, 0.0),
            ("v(k)", 0, 5.795576367813973e+00, 0.0),
            ("temp-sweep", 33, 125.0, 0.0),
            ("v(a)", 33, 5.432783558641020e-01, 0.0),
            ("v(k)", 33, 5.407655948379028e+00, 0.0),
        ],
    },
    // #97: C names an `@inst[param]` scale `param-sweep` and writes it as a
    // voltage. The diode's AREA (inner, 0.5..4) multiplies IS, so v(a) falls
    // by about N Vt ln(8) = 66 mV across it at -20 C; heating (outer -20, 30,
    // 80 C) lowers it further. The 1 k resistor carries (2 - v(a)) / 1 k.
    Expectation {
        fixture: "m8_dc_param_diode",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 24,
        variables: &["v(param-sweep)", "v(a)", "v(in)", "i(v1)"],
        values: &[
            ("v(param-sweep)", 0, 5.000000000000000e-01, 0.0),
            ("v(a)", 0, 8.362909664457829e-01, 0.0),
            ("i(v1)", 0, -1.163709033554217e-03, 0.0),
            ("v(param-sweep)", 7, 4.000000000000000e+00, 0.0),
            ("v(a)", 7, 7.675757347799221e-01, 0.0),
            ("v(param-sweep)", 16, 5.000000000000000e-01, 0.0),
            ("v(a)", 23, 6.052476121547538e-01, 0.0),
        ],
    },
    // E's gain (outer -2, 0, 2) sets v(eo) = gain * 1 V; G's swept gain is
    // multiplied by the card's m=3 (`VCCSparam`), so v(go) = gain * 3 *
    // v(eo) * 1 k: -6 V at 1 mS and v(eo) = -2 V, zero while v(eo) = 0.
    Expectation {
        fixture: "m8_dc_param_gain",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 15,
        variables: &[
            "v(param-sweep)",
            "i(e1)",
            "v(eo)",
            "v(go)",
            "v(in)",
            "i(v1)",
        ],
        values: &[
            ("v(param-sweep)", 0, 1.000000000000000e-03, 0.0),
            ("v(eo)", 0, -2.000000000000000e+00, 0.0),
            ("v(go)", 0, -6.000000000000000e+00, 0.0),
            ("v(go)", 7, 0.0, 0.0),
            ("v(go)", 14, 1.800000000000000e+01, 0.0),
            ("i(e1)", 14, -2.000000000000000e-03, 0.0),
        ],
    },
    // Saturated NMOS: Id = KP/2 W/(L - 2 LD) (Vgs - VTO)^2 (1 + LAMBDA Vds),
    // about 0.105 mA at W = 5u, L = 1u, roughly proportional to W (inner) and
    // inversely to the effective length (outer 1u, 2u, 3u); RSH*NRS slightly
    // degenerates the source.
    Expectation {
        fixture: "m8_dc_param_mos1",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 15,
        variables: &["v(param-sweep)", "v(d)", "v(dd)", "v(g)", "i(vdd)", "i(vg)"],
        values: &[
            ("v(param-sweep)", 0, 5.000000000000000e-06, 0.0),
            ("i(vdd)", 0, -1.050792717875804e-04, 0.0),
            ("i(vdd)", 4, -5.079976774109523e-04, 0.0),
            ("i(vdd)", 14, -1.495588365521305e-04, 0.0),
            ("v(dd)", 14, 2.850441163447869e+00, 0.0),
            ("i(vg)", 14, 0.0, 0.0),
        ],
    },
    // C names a resistor scale `res-sweep` (type `res-sweep`). The swept
    // supplied value is scaled by scale/m = 1/2 and TC1 = 0.01: 500 ohm at
    // 27 C gives v(out) = 2 * 1k / 1.5k; at 47 C 600 ohm gives 1.25 V.
    Expectation {
        fixture: "m8_dc_res_temp",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 6,
        variables: &["res-sweep", "v(in)", "v(out)", "i(v1)"],
        values: &[
            ("res-sweep", 0, 1.000000000000000e+03, 0.0),
            ("v(out)", 0, 1.333333333333333e+00, 0.0),
            ("v(out)", 3, 1.250000000000000e+00, 0.0),
            ("res-sweep", 5, 2.000000000000000e+03, 0.0),
            ("v(out)", 5, 9.090909090909091e-01, 0.0),
        ],
    },
    Expectation {
        fixture: "m7_diode_temp_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 31,
        variables: &[
            "frequency",
            "v(b1)",
            "v(b2)",
            "v(b3)",
            "v(k1)",
            "v(k2)",
            "v(k3)",
            "i(vb1)",
            "i(vb2)",
            "i(vb3)",
        ],
        values: &[
            ("frequency", 0, 1e3, 0.0),
            ("v(k1)", 0, 9.999771077537682e-01, -4.783484309209417e-03),
            ("v(k3)", 0, 4.676800133344254e-01, -3.705732363331333e-05),
        ],
    },
    Expectation {
        fixture: "m7_zener_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 432,
        variables: &["time", "v(in)", "v(out)", "i(vin)"],
        values: &[
            ("time", 0, 0.0, 0.0),
            ("time", 431, 1.8e-3, 0.0),
            ("v(out)", 431, -8.215683334810343e-01, 0.0),
        ],
    },
    // Pole-zero analysis (#103). C lists poles then zeros, ascending real
    // part, each complex root followed by its conjugate.
    //
    // Current into `in`, read on b (the positive output node is ground, so C
    // swaps the drive): c3 at the input gives -1/(r1 c3) = -1e6; l1 and r2
    // with c1 give the pair -r2/(2 l1) = -2.5e4 +- j sqrt(1/(l1 c1) - 6.25e8).
    Expectation {
        fixture: "pz_ladder_cur",
        plotname: "Pole-Zero Analysis",
        flags: PlotFlags::Complex,
        points: 1,
        variables: &["v(pole(1))", "v(pole(2))", "v(pole(3))"],
        values: &[
            ("v(pole(1))", 0, -1.000998950625846e+06, 0.0),
            (
                "v(pole(2))",
                0,
                -2.500052468707703e+04,
                1.933845422083931e+04,
            ),
            (
                "v(pole(3))",
                0,
                -2.500052468707703e+04,
                -1.933845422083931e+04,
            ),
        ],
    },
    // The bridge's two RC arms balance at DC (both nodes follow v1), so the
    // differential output has a zero at the origin; C's determinant is singular
    // there and the zero is exact.
    Expectation {
        fixture: "pz_bridge_diff",
        plotname: "Pole-Zero Analysis",
        flags: PlotFlags::Complex,
        points: 1,
        variables: &["v(pole(1))", "v(pole(2))", "v(zero(1))"],
        values: &[
            ("v(pole(1))", 0, -1.213601717195853e+03, 0.0),
            ("v(pole(2))", 0, -2.197316161374808e+02, 0.0),
            ("v(zero(1))", 0, 0.0, 0.0),
        ],
    },
    // A transformer passes no DC: zero at the origin. Three energy stores
    // (two coupled windings, the load capacitor) give three poles.
    Expectation {
        fixture: "pz_transformer",
        plotname: "Pole-Zero Analysis",
        flags: PlotFlags::Complex,
        points: 1,
        variables: &["v(pole(1))", "v(pole(2))", "v(pole(3))", "v(zero(1))"],
        values: &[
            (
                "v(pole(1))",
                0,
                -6.587015879976568e+04,
                5.139172478027967e+04,
            ),
            (
                "v(pole(2))",
                0,
                -6.587015879976568e+04,
                -5.139172478027967e+04,
            ),
            ("v(pole(3))", 0, -4.522308663094482e+03, 0.0),
            ("v(zero(1))", 0, 0.0, 0.0),
        ],
    },
    // With vdd an AC ground, the two nodes a, b give
    // 2e-18 s^2 + 3.6e-12 s + 6.5e-7: poles summing to -1.8e6 with product
    // 3.25e11. cdec across vdd adds no pole.
    Expectation {
        fixture: "pz_cv_loop",
        plotname: "Pole-Zero Analysis",
        flags: PlotFlags::Complex,
        points: 1,
        variables: &["v(pole(1))", "v(pole(2))"],
        values: &[
            ("v(pole(1))", 0, -1.596419413859206e+06, 0.0),
            ("v(pole(2))", 0, -2.035805861407940e+05, 0.0),
        ],
    },
    // The junction's diffusion and depletion charge and c1 behind r1 and the
    // diode's rs: two poles, one zero, all real and in the left half-plane.
    Expectation {
        fixture: "pz_diode",
        plotname: "Pole-Zero Analysis",
        flags: PlotFlags::Complex,
        points: 1,
        variables: &["v(pole(1))", "v(pole(2))", "v(zero(1))"],
        values: &[
            ("v(pole(1))", 0, -1.257597815812064e+10, 0.0),
            ("v(pole(2))", 0, -6.983243413825105e+07, 0.0),
            ("v(zero(1))", 0, -2.545810592258894e+09, 0.0),
        ],
    },
    // The gate floats behind rs once the AC source is removed (an exact pole
    // at the origin in C); the gate-drain overlap capacitance gives the
    // right-half-plane zero gm/Cgd.
    Expectation {
        fixture: "pz_mos1",
        plotname: "Pole-Zero Analysis",
        flags: PlotFlags::Complex,
        points: 1,
        variables: &["v(pole(1))", "v(pole(2))", "v(zero(1))"],
        values: &[
            ("v(pole(1))", 0, -6.871243749912874e+07, 0.0),
            ("v(pole(2))", 0, 0.0, 0.0),
            ("v(zero(1))", 0, 3.208712145408839e+08, 0.0),
        ],
    },
    // `.tf` (#101): the inductor shorts b to a and the capacitor is open, so
    // v(b) = 5 V * (3k || 8k) / (1k + 3k || 8k) and v(out) = 3/4 v(b): gain
    // 0.5142857 = 18/35; the source sees 1k + 3k || 8k = 3181.8 ohm and out
    // sees 6k || (2k + 1k || 3k) = 1885.7 ohm.
    Expectation {
        fixture: "m8_tf_divider",
        plotname: "Transfer Function",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(transfer_function)",
            "v(output_impedance_at_v(out))",
            "v(v1#input_impedance)",
        ],
        values: &[
            ("v(transfer_function)", 0, 5.142857142857142e-01, 0.0),
            (
                "v(output_impedance_at_v(out))",
                0,
                1.885714285714286e+03,
                0.0,
            ),
            ("v(v1#input_impedance)", 0, 3.181818181818182e+03, 0.0),
        ],
    },
    // 1 A into `in` sees 5k || (1k + 4k) = 2.5k, so v(a) = 2 kV; e1 drives
    // 40 kV behind 600 ohm and g1 injects 2 A, giving v(out) = 68.67 / (1/600
    // + 1/2000) and i(vs) = v(out) / 2k = 15.846 A/A. A unit voltage in vs
    // sees rl + ro = 2.6k (the sources are controlled by `a`, untouched).
    Expectation {
        fixture: "m8_tf_controlled",
        plotname: "Transfer Function",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(transfer_function)",
            "v(i1#input_impedance)",
            "v(vs#output_impedance)",
        ],
        values: &[
            ("v(transfer_function)", 0, 1.584615384615384e+01, 0.0),
            ("v(i1#input_impedance)", 0, 2.5e+03, 0.0),
            ("v(vs#output_impedance)", 0, 2.6e+03, 0.0),
        ],
    },
    // A CE stage with 330 ohm emitter degeneration: gain about -rc/re = -10
    // reduced by the 600 ohm source and the base network; the output
    // resistance is slightly below rc (the Early effect); the input sees
    // rs + rb1 || rb2 || (beta * re) = about 10 k.
    Expectation {
        fixture: "m8_tf_bjt",
        plotname: "Transfer Function",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(transfer_function)",
            "v(output_impedance_at_v(nc))",
            "v(vin#input_impedance)",
        ],
        values: &[
            ("v(transfer_function)", 0, -8.886149756533730e+00, 0.0),
            (
                "v(output_impedance_at_v(nc))",
                0,
                3.281273729423023e+03,
                0.0,
            ),
            ("v(vin#input_impedance)", 0, 1.042663215494944e+04, 0.0),
        ],
    },
    // `.sp` (#105). A K = 3 matched T pad between two 50 ohm ports (series
    // arms 25, shunt 37.5): S11 = S22 = 0 (C prints 5e-16 rounding), S21 = 1/K;
    // Z11 = 25 + 37.5, Z12 = 37.5, and Y = Z^-1 = [[0.025, -0.015], ...]. The
    // node vectors are port 2's excitation, the last one C solves: 1 V behind
    // 50 ohm into the 50 ohm match gives v(out) = 0.5 and i(v2) = -10 mA.
    // `v(rbase)` is port 1's z0.
    Expectation {
        fixture: "sp_attenuator",
        plotname: "SP Analysis",
        flags: PlotFlags::Complex,
        points: 3,
        variables: &[
            "frequency",
            "v(rbase)",
            "s_1_1",
            "s_1_2",
            "s_2_1",
            "s_2_2",
            "y_1_1",
            "y_1_2",
            "y_2_1",
            "y_2_2",
            "z_1_1",
            "z_1_2",
            "z_2_1",
            "z_2_2",
            "v(in)",
            "v(mid)",
            "v(out)",
            "i(v1)",
            "v(v1#res)",
            "i(v2)",
            "v(v2#res)",
        ],
        values: &[
            ("frequency", 2, 1.0e6, 0.0),
            ("v(rbase)", 0, 50.0, 0.0),
            ("s_1_1", 0, 4.756033529969746e-16, 0.0),
            ("s_2_1", 0, 3.333333333333335e-01, 0.0),
            ("y_1_1", 0, 2.499999999999999e-02, 0.0),
            ("y_1_2", 0, -1.5e-02, 0.0),
            ("z_1_1", 0, 6.250000000000008e+01, 0.0),
            ("z_1_2", 0, 3.750000000000007e+01, 0.0),
            ("v(out)", 0, 5.000000000000002e-01, 0.0),
            ("v(mid)", 0, 2.500000000000002e-01, 0.0),
            ("i(v2)", 0, -9.999999999999995e-03, 0.0),
        ],
    },
    // A lossy RC two-port (100 ohm, 1 nF || 1 k shunt, 20 ohm) between a 50 and
    // a 75 ohm port. Z12 = Z21 = 1 k || 1/(j w 1 nF), 716.96 - j450.48 ohm at
    // 100 kHz; the power-wave S21 = 2 sqrt(50 * 75) Z21 / ((Z11 + 50)(Z22 + 75)
    // - Z12 Z21) equals S12 (reciprocal), 0.4719 - j0.0163 there.
    Expectation {
        fixture: "sp_rc",
        plotname: "SP Analysis",
        flags: PlotFlags::Complex,
        points: 5,
        variables: &[
            "frequency",
            "v(rbase)",
            "s_1_1",
            "s_1_2",
            "s_2_1",
            "s_2_2",
            "y_1_1",
            "y_1_2",
            "y_2_1",
            "y_2_2",
            "z_1_1",
            "z_1_2",
            "z_2_1",
            "z_2_2",
            "v(in)",
            "v(mid)",
            "v(out)",
            "i(v1)",
            "v(v1#res)",
            "i(v2)",
            "v(v2#res)",
        ],
        values: &[
            ("frequency", 4, 1.0e7, 0.0),
            ("s_1_1", 0, 5.773367356802208e-01, -8.42697779885093e-03),
            ("s_2_1", 0, 4.718556562522875e-01, -1.629615448495809e-02),
            ("s_1_2", 0, 4.718556562522876e-01, -1.629615448495809e-02),
            ("z_1_2", 0, 7.16956800324899e+02, -4.504772433683906e+02),
            ("z_1_2", 4, 2.532388129651598e-01, -1.591146388830292e+01),
            ("y_2_2", 4, 3.011433737141879e-02, 2.048283659859536e-02),
            ("v(rbase)", 4, 50.0, 0.0),
        ],
    },
    // `.sens` (#102), DC. C's forward differences of v(c) to every parameter
    // `sgen` perturbs. A resistor's m divides and its scale multiplies the
    // resistance, so `r1_m = -R1 r1` and `r1_scale = R1 r1` (1 kohm); a current
    // source's m multiplies its 1 mA (`i1_m = 1e-3 i1`). `g1_gain` is C's
    // artefact: `m=3` was written after the gain, so the card's coefficient is
    // 1 mS, but the perturbed gain is multiplied by the now-given m (3 mS).
    // Parameters without effect at TNOM (`r1:rsh`, `r1_tc`) read exactly zero.
    Expectation {
        fixture: "sens_divider",
        plotname: "Sensitivity Analysis",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(f1_gain)",
            "v(e1_gain)",
            "v(f1_m)",
            "v(g1_gain)",
            "v(g1_m)",
            "v(h1_gain)",
            "v(i1)",
            "v(i1_m)",
            "v(r1:af)",
            "v(r1:bv_max)",
            "v(r1:ef)",
            "v(r1:kf)",
            "v(r1:lf)",
            "v(r1:narrow)",
            "v(r1:r)",
            "v(r1:rsh)",
            "v(r1:short)",
            "v(r1:tc1)",
            "v(r1:tc2)",
            "v(r1:tce)",
            "v(r1:wf)",
            "v(r1_bv_max)",
            "v(r1_dtemp)",
            "v(r1_l)",
            "v(r1_m)",
            "v(r1_scale)",
            "v(r1)",
            "v(r1_tc)",
            "v(r1_tc2)",
            "v(r1_tce)",
            "v(r1_temp)",
            "v(r1_w)",
            "v(r2:af)",
            "v(r2:bv_max)",
            "v(r2:ef)",
            "v(r2:kf)",
            "v(r2:lf)",
            "v(r2:narrow)",
            "v(r2:r)",
            "v(r2:rsh)",
            "v(r2:short)",
            "v(r2:tc1)",
            "v(r2:tc2)",
            "v(r2:tce)",
            "v(r2:wf)",
            "v(r2_bv_max)",
            "v(r2_dtemp)",
            "v(r2_l)",
            "v(r2)",
            "v(r2_m)",
            "v(r2_scale)",
            "v(r2_tc)",
            "v(r2_tc2)",
            "v(r2_tce)",
            "v(r2_temp)",
            "v(r2_w)",
            "v(r3:af)",
            "v(r3:bv_max)",
            "v(r3:ef)",
            "v(r3:kf)",
            "v(r3:lf)",
            "v(r3:narrow)",
            "v(r3:r)",
            "v(r3:rsh)",
            "v(r3:short)",
            "v(r3:tc1)",
            "v(r3:tc2)",
            "v(r3:tce)",
            "v(r3:wf)",
            "v(r3)",
            "v(r3_bv_max)",
            "v(r3_dtemp)",
            "v(r3_l)",
            "v(r3_m)",
            "v(r3_scale)",
            "v(r3_tc)",
            "v(r3_tc2)",
            "v(r3_tce)",
            "v(r3_temp)",
            "v(r3_w)",
            "v(rb:af)",
            "v(rb:bv_max)",
            "v(rb:ef)",
            "v(rb:kf)",
            "v(rb:lf)",
            "v(rb:narrow)",
            "v(rb:rsh)",
            "v(rb:r)",
            "v(rb:short)",
            "v(rb:tc1)",
            "v(rb:tc2)",
            "v(rb:tce)",
            "v(rb:wf)",
            "v(rb_bv_max)",
            "v(rb_dtemp)",
            "v(rb_l)",
            "v(rb)",
            "v(rb_m)",
            "v(rb_scale)",
            "v(rb_tc)",
            "v(rb_tc2)",
            "v(rb_tce)",
            "v(rb_temp)",
            "v(rb_w)",
            "v(rc:af)",
            "v(rc:bv_max)",
            "v(rc:ef)",
            "v(rc:kf)",
            "v(rc:lf)",
            "v(rc:narrow)",
            "v(rc:r)",
            "v(rc:rsh)",
            "v(rc:short)",
            "v(rc:tc1)",
            "v(rc:tc2)",
            "v(rc:tce)",
            "v(rc:wf)",
            "v(rc_bv_max)",
            "v(rc_dtemp)",
            "v(rc_l)",
            "v(rc_m)",
            "v(rc_scale)",
            "v(rc)",
            "v(rc_tc)",
            "v(rc_tc2)",
            "v(rc_tce)",
            "v(rc_temp)",
            "v(rc_w)",
            "v(re:af)",
            "v(re)",
            "v(re:bv_max)",
            "v(re:ef)",
            "v(re:kf)",
            "v(re:lf)",
            "v(re:narrow)",
            "v(re:rsh)",
            "v(re:r)",
            "v(re:short)",
            "v(re:tc1)",
            "v(re:tc2)",
            "v(re:tce)",
            "v(re:wf)",
            "v(re_bv_max)",
            "v(re_dtemp)",
            "v(re_l)",
            "v(re_m)",
            "v(re_scale)",
            "v(re_tc2)",
            "v(re_tce)",
            "v(re_tc)",
            "v(re_temp)",
            "v(re_w)",
            "v(rh:af)",
            "v(rh:bv_max)",
            "v(rh:ef)",
            "v(rh:kf)",
            "v(rh:lf)",
            "v(rh:narrow)",
            "v(rh:r)",
            "v(rh:rsh)",
            "v(rh:short)",
            "v(rh:tc1)",
            "v(rh:tc2)",
            "v(rh:tce)",
            "v(rh:wf)",
            "v(rh_bv_max)",
            "v(rh_dtemp)",
            "v(rh_l)",
            "v(rh_m)",
            "v(rh_scale)",
            "v(rh_tc)",
            "v(rh)",
            "v(rh_tc2)",
            "v(rh_tce)",
            "v(rh_temp)",
            "v(rh_w)",
            "v(v1)",
            "v(v1_freq)",
            "v(v1_phase)",
            "v(v1_pwr)",
            "v(v1_z0)",
        ],
        values: &[
            ("v(r1)", 0, -1.429279053850123e-03, 0.0),
            ("v(r1_m)", 0, 1.429280482917992e+00, 0.0),
            ("v(r1_scale)", 0, -1.429279053540198e+00, 0.0),
            ("v(i1)", 0, 7.479437108959781e+02, 0.0),
            ("v(i1_m)", 0, 7.479437109037631e-01, 0.0),
            ("v(g1_gain)", 0, 1.081383549497984e+09, 0.0),
            ("v(r1:rsh)", 0, -0.000000000000000e+00, 0.0),
            ("v(r1_tc)", 0, -0.000000000000000e+00, 0.0),
        ],
    },
    // `.sens` (#102) at 60 C through i(vm). r4 takes its resistance from model
    // `r=3k`, so perturbing its sheet resistance from 0 to 1e-6 switches it to
    // the (tiny) geometry resistance: C's huge `r4:rsh`. A model `tce`
    // perturbation switches the temperature factor to 1.01^(tce dT) (`r4:tce`).
    // `v1` (a PULSE without DC value) is perturbed from 0, replacing its
    // time-zero level 2 V by 1e-6 V. m and scale relate as for sens_divider.
    Expectation {
        fixture: "sens_hot",
        plotname: "Sensitivity Analysis",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(b1_temp)",
            "v(b1_dtemp)",
            "v(b1_m)",
            "v(b1_tc1)",
            "v(b1_tc2)",
            "v(e1_gain)",
            "v(r0:af)",
            "v(r0:bv_max)",
            "v(r0:ef)",
            "v(r0:kf)",
            "v(r0)",
            "v(r0:lf)",
            "v(r0:narrow)",
            "v(r0:r)",
            "v(r0:rsh)",
            "v(r0:short)",
            "v(r0:tc1)",
            "v(r0:tc2)",
            "v(r0:tce)",
            "v(r0:wf)",
            "v(r0_bv_max)",
            "v(r0_dtemp)",
            "v(r0_l)",
            "v(r0_m)",
            "v(r0_scale)",
            "v(r0_tc2)",
            "v(r0_tce)",
            "v(r0_tc)",
            "v(r0_temp)",
            "v(r0_w)",
            "v(r1:af)",
            "v(r1:bv_max)",
            "v(r1:ef)",
            "v(r1:kf)",
            "v(r1:lf)",
            "v(r1:narrow)",
            "v(r1:r)",
            "v(r1:rsh)",
            "v(r1:short)",
            "v(r1:tc1)",
            "v(r1:tc2)",
            "v(r1)",
            "v(r1:tce)",
            "v(r1:wf)",
            "v(r1_bv_max)",
            "v(r1_dtemp)",
            "v(r1_l)",
            "v(r1_m)",
            "v(r1_scale)",
            "v(r1_tc2)",
            "v(r1_tce)",
            "v(r1_tc)",
            "v(r1_temp)",
            "v(r1_w)",
            "v(r2:af)",
            "v(r2:bv_max)",
            "v(r2:ef)",
            "v(r2:kf)",
            "v(r2:lf)",
            "v(r2:narrow)",
            "v(r2:r)",
            "v(r2:rsh)",
            "v(r2:short)",
            "v(r2:tc1)",
            "v(r2:tc2)",
            "v(r2:tce)",
            "v(r2:wf)",
            "v(r2_bv_max)",
            "v(r2)",
            "v(r2_dtemp)",
            "v(r2_l)",
            "v(r2_m)",
            "v(r2_scale)",
            "v(r2_tc)",
            "v(r2_tc2)",
            "v(r2_tce)",
            "v(r2_temp)",
            "v(r2_w)",
            "v(r3:af)",
            "v(r3:bv_max)",
            "v(r3:ef)",
            "v(r3:kf)",
            "v(r3:lf)",
            "v(r3:narrow)",
            "v(r3:r)",
            "v(r3:rsh)",
            "v(r3:short)",
            "v(r3:tc1)",
            "v(r3:tc2)",
            "v(r3:tce)",
            "v(r3:wf)",
            "v(r3_bv_max)",
            "v(r3_dtemp)",
            "v(r3_l)",
            "v(r3_m)",
            "v(r3_scale)",
            "v(r3_tc)",
            "v(r3_tc2)",
            "v(r3_tce)",
            "v(r3_temp)",
            "v(r3_w)",
            "v(r3)",
            "v(r4:af)",
            "v(r4:bv_max)",
            "v(r4:ef)",
            "v(r4:kf)",
            "v(r4:lf)",
            "v(r4:narrow)",
            "v(r4:r)",
            "v(r4:rsh)",
            "v(r4:short)",
            "v(r4:tc1)",
            "v(r4:tc2)",
            "v(r4:tce)",
            "v(r4:wf)",
            "v(r4_bv_max)",
            "v(r4_dtemp)",
            "v(r4_l)",
            "v(r4_m)",
            "v(r4_scale)",
            "v(r4_tc)",
            "v(r4_tc2)",
            "v(r4_tce)",
            "v(r4_temp)",
            "v(r4)",
            "v(r4_w)",
            "v(rbb:af)",
            "v(rbb:bv_max)",
            "v(rbb:ef)",
            "v(rbb:kf)",
            "v(rbb:lf)",
            "v(rbb:narrow)",
            "v(rbb:rsh)",
            "v(rbb:r)",
            "v(rbb:short)",
            "v(rbb:tc1)",
            "v(rbb:tc2)",
            "v(rbb:tce)",
            "v(rbb:wf)",
            "v(rbb_bv_max)",
            "v(rbb_dtemp)",
            "v(rbb_l)",
            "v(rbb_m)",
            "v(rbb_scale)",
            "v(rbb_tc2)",
            "v(rbb_tc)",
            "v(rbb_tce)",
            "v(rbb_temp)",
            "v(rbb_w)",
            "v(rbb)",
            "v(rs:af)",
            "v(rs:bv_max)",
            "v(rs:ef)",
            "v(rs:kf)",
            "v(rs:lf)",
            "v(rs:narrow)",
            "v(rs:r)",
            "v(rs:rsh)",
            "v(rs:short)",
            "v(rs:tc1)",
            "v(rs:tc2)",
            "v(rs:tce)",
            "v(rs:wf)",
            "v(rs)",
            "v(rs_bv_max)",
            "v(rs_dtemp)",
            "v(rs_l)",
            "v(rs_m)",
            "v(rs_scale)",
            "v(rs_tc)",
            "v(rs_tc2)",
            "v(rs_tce)",
            "v(rs_temp)",
            "v(rs_w)",
            "v(s1:roff)",
            "v(s1:ron)",
            "v(s1:vh)",
            "v(s1:vt)",
            "v(v1_freq)",
            "v(v1_phase)",
            "v(v1)",
            "v(v1_pwr)",
            "v(v1_z0)",
            "v(vm)",
            "v(vm_freq)",
            "v(vm_phase)",
            "v(vm_pwr)",
            "v(vm_z0)",
        ],
        values: &[
            ("v(r4:rsh)", 0, -3.869214143341930e+11, 0.0),
            ("v(r4:tce)", 0, 6.835676067216831e+00, 0.0),
            ("v(r3)", 0, -1.045889344878947e-07, 0.0),
            ("v(r3_scale)", 0, -1.045889344652156e-04, 0.0),
            ("v(b1_m)", 0, -1.145539530496061e-04, 0.0),
            ("v(v1)", 0, -4.028505416588158e+02, 0.0),
            ("v(vm)", 0, -1.999999999835467e-03, 0.0),
        ],
    },
    // `.sens` (#102) of v(a) over two diode models. `d1:rs` is exactly zero (C's
    // setup alone derives 1/RS), `d1:isr` is huge (ISR reads its 1e-14 default
    // and its perturbation turns the recombination current on), `d1:nr` is
    // nonzero only because that current stays on. `d1_m` is twice `d1_area`
    // (area=2, both scale the junction).
    Expectation {
        fixture: "sens_diode",
        plotname: "Sensitivity Analysis",
        flags: PlotFlags::Real,
        points: 1,
        variables: &[
            "v(d2:is)",
            "v(d1:af)",
            "v(d1:area)",
            "v(d1:bv)",
            "v(d1:cjp)",
            "v(d1:cta)",
            "v(d1:ctp)",
            "v(d1:eg)",
            "v(d1:fc)",
            "v(d1:fcs)",
            "v(d1:gap1)",
            "v(d1:gap2)",
            "v(d1:ibv)",
            "v(d1:ikf)",
            "v(d1:ikp)",
            "v(d1:ikr)",
            "v(d1:is)",
            "v(d1:isr)",
            "v(d1:jsw)",
            "v(d1:jtun)",
            "v(d1:jtunsw)",
            "v(d1:keg)",
            "v(d1:kf)",
            "v(d1:lm)",
            "v(d1:lp)",
            "v(d1:m)",
            "v(d1:mjsw)",
            "v(d1:nbv)",
            "v(d1:n)",
            "v(d1:nr)",
            "v(d1:ns)",
            "v(d1:ntun)",
            "v(d1:php)",
            "v(d1:pj)",
            "v(d1:qpscale)",
            "v(d1:rs)",
            "v(d1:tcv)",
            "v(d1:tm1)",
            "v(d1:tm2)",
            "v(d1:tpb)",
            "v(d1:tphp)",
            "v(d1:trs)",
            "v(d1:trs2)",
            "v(d1:vj)",
            "v(d1:vp)",
            "v(d1:wm)",
            "v(d1:wp)",
            "v(d1:xm)",
            "v(d1:xoi)",
            "v(d1:xom)",
            "v(d1:xp)",
            "v(d1:xti)",
            "v(d1:xtitun)",
            "v(d1:xw)",
            "v(d1_area)",
            "v(d1_dtemp)",
            "v(d1_l)",
            "v(d1_lm)",
            "v(d1_lp)",
            "v(d1_m)",
            "v(d1_pj)",
            "v(d1_temp)",
            "v(d1_w)",
            "v(d1_wm)",
            "v(d1_wp)",
            "v(d2:af)",
            "v(d2:area)",
            "v(d2:bv)",
            "v(d2:cjp)",
            "v(d2:cta)",
            "v(d2:ctp)",
            "v(d2:eg)",
            "v(d2:fc)",
            "v(d2:fcs)",
            "v(d2:gap1)",
            "v(d2:gap2)",
            "v(d2:ibv)",
            "v(d2:ikf)",
            "v(d2:ikp)",
            "v(d2:ikr)",
            "v(d2:isr)",
            "v(d2:jsw)",
            "v(d2:jtun)",
            "v(d2:jtunsw)",
            "v(d2:keg)",
            "v(d2:kf)",
            "v(d2:lm)",
            "v(d2:lp)",
            "v(d2:m)",
            "v(d2:mjsw)",
            "v(d2:nbv)",
            "v(d2:nr)",
            "v(d2:n)",
            "v(d2:ns)",
            "v(d2:ntun)",
            "v(d2:php)",
            "v(d2:pj)",
            "v(d2:qpscale)",
            "v(d2:rs)",
            "v(d2:tcv)",
            "v(d2:tm1)",
            "v(d2:tm2)",
            "v(d2:tpb)",
            "v(d2:tphp)",
            "v(d2:trs2)",
            "v(d2:trs)",
            "v(d2:vj)",
            "v(d2:vp)",
            "v(d2:wm)",
            "v(d2:wp)",
            "v(d2:xm)",
            "v(d2:xoi)",
            "v(d2:xom)",
            "v(d2:xp)",
            "v(d2:xti)",
            "v(d2:xtitun)",
            "v(d2:xw)",
            "v(d2_area)",
            "v(d2_dtemp)",
            "v(d2_l)",
            "v(d2_lm)",
            "v(d2_lp)",
            "v(d2_m)",
            "v(d2_pj)",
            "v(d2_temp)",
            "v(d2_w)",
            "v(d2_wm)",
            "v(d2_wp)",
            "v(r1:af)",
            "v(r1:bv_max)",
            "v(r1:ef)",
            "v(r1:kf)",
            "v(r1:lf)",
            "v(r1:narrow)",
            "v(r1:r)",
            "v(r1:rsh)",
            "v(r1:short)",
            "v(r1:tc1)",
            "v(r1:tc2)",
            "v(r1:tce)",
            "v(r1:wf)",
            "v(r1_bv_max)",
            "v(r1_dtemp)",
            "v(r1_l)",
            "v(r1_m)",
            "v(r1_scale)",
            "v(r1_tc)",
            "v(r1)",
            "v(r1_tc2)",
            "v(r1_tce)",
            "v(r1_temp)",
            "v(r1_w)",
            "v(rb:af)",
            "v(rb:bv_max)",
            "v(rb:ef)",
            "v(rb:kf)",
            "v(rb:lf)",
            "v(rb:narrow)",
            "v(rb:r)",
            "v(rb:rsh)",
            "v(rb:short)",
            "v(rb:tc1)",
            "v(rb:tc2)",
            "v(rb)",
            "v(rb:tce)",
            "v(rb:wf)",
            "v(rb_bv_max)",
            "v(rb_dtemp)",
            "v(rb_l)",
            "v(rb_m)",
            "v(rb_scale)",
            "v(rb_tc)",
            "v(rb_tc2)",
            "v(rb_tce)",
            "v(rb_temp)",
            "v(rb_w)",
            "v(v1_freq)",
            "v(v1_phase)",
            "v(v1)",
            "v(v1_pwr)",
            "v(v1_z0)",
        ],
        values: &[
            ("v(d1:is)", 0, -3.689741239819482e+12, 0.0),
            ("v(d1:rs)", 0, -0.000000000000000e+00, 0.0),
            ("v(d1:isr)", 0, -1.623782799906984e+15, 0.0),
            ("v(d1:nr)", 0, 1.577993019698197e-04, 0.0),
            ("v(d1_area)", 0, -2.589987310923426e-02, 0.0),
            ("v(d1_m)", 0, -5.179974621846852e-02, 0.0),
            ("v(r1)", 0, -6.704189545176466e-05, 0.0),
            ("v(v1)", 0, 1.705634108915664e-02, 0.0),
        ],
    },
    // `.sens` (#102), AC over a decade at 40 C, complex. `r1` equals `r1_ac`
    // at the first frequency and reads zero afterwards: restoring `ac` leaves
    // it given, so RESacload stops using the perturbed resistance. `c1_m` is
    // 1e-9 `c1` (C's capacitance ask is capac*m).
    Expectation {
        fixture: "sens_ac",
        plotname: "Sensitivity Analysis",
        flags: PlotFlags::Complex,
        points: 5,
        variables: &[
            "frequency",
            "v(c1:bv_max)",
            "v(c1)",
            "v(c1:cap)",
            "v(c1:cj)",
            "v(c1:cjsw)",
            "v(c1:del)",
            "v(c1:di)",
            "v(c1:narrow)",
            "v(c1:short)",
            "v(c1:tc1)",
            "v(c1:tc2)",
            "v(c1:thick)",
            "v(c1_bv_max)",
            "v(c1_dtemp)",
            "v(c1_ic)",
            "v(c1_l)",
            "v(c1_m)",
            "v(c1_scale)",
            "v(c1_tc1)",
            "v(c1_tc2)",
            "v(c1_temp)",
            "v(c1_w)",
            "v(c2:bv_max)",
            "v(c2:cap)",
            "v(c2:cj)",
            "v(c2:cjsw)",
            "v(c2:del)",
            "v(c2:di)",
            "v(c2:narrow)",
            "v(c2:short)",
            "v(c2:tc1)",
            "v(c2:tc2)",
            "v(c2:thick)",
            "v(c2_bv_max)",
            "v(c2_dtemp)",
            "v(c2_ic)",
            "v(c2_l)",
            "v(c2_m)",
            "v(c2_scale)",
            "v(c2)",
            "v(c2_tc1)",
            "v(c2_tc2)",
            "v(c2_temp)",
            "v(c2_w)",
            "v(e1_gain)",
            "v(f1_gain)",
            "v(f1_m)",
            "v(g1_gain)",
            "v(g1_m)",
            "v(h1_gain)",
            "v(i1_acmag)",
            "v(i1_acphase)",
            "v(i1)",
            "v(i1_m)",
            "v(l1:csect)",
            "v(l1:dia)",
            "v(l1:ind)",
            "v(l1:length)",
            "v(l1:mu)",
            "v(l1:nt)",
            "v(l1:tc1)",
            "v(l1:tc2)",
            "v(l1_dtemp)",
            "v(l1_ic)",
            "v(l1_m)",
            "v(l1_nt)",
            "v(l1_scale)",
            "v(l1_tc1)",
            "v(l1)",
            "v(l1_tc2)",
            "v(l1_temp)",
            "v(l2:csect)",
            "v(l2:dia)",
            "v(l2:ind)",
            "v(l2:length)",
            "v(l2:mu)",
            "v(l2:nt)",
            "v(l2:tc1)",
            "v(l2:tc2)",
            "v(l2)",
            "v(l2_dtemp)",
            "v(l2_ic)",
            "v(l2_m)",
            "v(l2_nt)",
            "v(l2_scale)",
            "v(l2_tc1)",
            "v(l2_tc2)",
            "v(l2_temp)",
            "v(r1:af)",
            "v(r1:bv_max)",
            "v(r1:ef)",
            "v(r1:kf)",
            "v(r1:lf)",
            "v(r1:narrow)",
            "v(r1:rsh)",
            "v(r1:r)",
            "v(r1:short)",
            "v(r1:tc1)",
            "v(r1:tc2)",
            "v(r1:tce)",
            "v(r1:wf)",
            "v(r1)",
            "v(r1_ac)",
            "v(r1_bv_max)",
            "v(r1_dtemp)",
            "v(r1_l)",
            "v(r1_m)",
            "v(r1_scale)",
            "v(r1_tc)",
            "v(r1_tc2)",
            "v(r1_tce)",
            "v(r1_temp)",
            "v(r1_w)",
            "v(r2:af)",
            "v(r2:bv_max)",
            "v(r2:ef)",
            "v(r2:kf)",
            "v(r2:lf)",
            "v(r2:narrow)",
            "v(r2:r)",
            "v(r2:rsh)",
            "v(r2:short)",
            "v(r2)",
            "v(r2:tc1)",
            "v(r2:tc2)",
            "v(r2:tce)",
            "v(r2:wf)",
            "v(r2_ac)",
            "v(r2_bv_max)",
            "v(r2_dtemp)",
            "v(r2_l)",
            "v(r2_m)",
            "v(r2_scale)",
            "v(r2_tc)",
            "v(r2_tc2)",
            "v(r2_tce)",
            "v(r2_temp)",
            "v(r2_w)",
            "v(re:af)",
            "v(re:bv_max)",
            "v(re:ef)",
            "v(re:kf)",
            "v(re:lf)",
            "v(re:narrow)",
            "v(re:r)",
            "v(re:rsh)",
            "v(re:short)",
            "v(re:tc1)",
            "v(re:tc2)",
            "v(re:tce)",
            "v(re:wf)",
            "v(re_ac)",
            "v(re_bv_max)",
            "v(re_dtemp)",
            "v(re_l)",
            "v(re_m)",
            "v(re_scale)",
            "v(re_tc)",
            "v(re_tc2)",
            "v(re_tce)",
            "v(re_temp)",
            "v(re_w)",
            "v(re)",
            "v(rf:af)",
            "v(rf:bv_max)",
            "v(rf:ef)",
            "v(rf:kf)",
            "v(rf:lf)",
            "v(rf:narrow)",
            "v(rf:r)",
            "v(rf:rsh)",
            "v(rf:short)",
            "v(rf:tc1)",
            "v(rf:tc2)",
            "v(rf:tce)",
            "v(rf:wf)",
            "v(rf_ac)",
            "v(rf_bv_max)",
            "v(rf_dtemp)",
            "v(rf_l)",
            "v(rf_m)",
            "v(rf_scale)",
            "v(rf)",
            "v(rf_tc)",
            "v(rf_tc2)",
            "v(rf_tce)",
            "v(rf_temp)",
            "v(rf_w)",
            "v(rg:af)",
            "v(rg:bv_max)",
            "v(rg:ef)",
            "v(rg:kf)",
            "v(rg:lf)",
            "v(rg:narrow)",
            "v(rg:r)",
            "v(rg:rsh)",
            "v(rg:short)",
            "v(rg:tc1)",
            "v(rg:tc2)",
            "v(rg:tce)",
            "v(rg)",
            "v(rg:wf)",
            "v(rg_ac)",
            "v(rg_bv_max)",
            "v(rg_dtemp)",
            "v(rg_l)",
            "v(rg_m)",
            "v(rg_scale)",
            "v(rg_tc)",
            "v(rg_tc2)",
            "v(rg_tce)",
            "v(rg_temp)",
            "v(rg_w)",
            "v(rh)",
            "v(rh:af)",
            "v(rh:bv_max)",
            "v(rh:ef)",
            "v(rh:kf)",
            "v(rh:lf)",
            "v(rh:narrow)",
            "v(rh:r)",
            "v(rh:rsh)",
            "v(rh:short)",
            "v(rh:tc1)",
            "v(rh:tc2)",
            "v(rh:tce)",
            "v(rh:wf)",
            "v(rh_ac)",
            "v(rh_bv_max)",
            "v(rh_dtemp)",
            "v(rh_l)",
            "v(rh_m)",
            "v(rh_scale)",
            "v(rh_tc2)",
            "v(rh_tc)",
            "v(rh_tce)",
            "v(rh_temp)",
            "v(rh_w)",
            "v(rl2:af)",
            "v(rl2:bv_max)",
            "v(rl2:ef)",
            "v(rl2:kf)",
            "v(rl2:lf)",
            "v(rl2)",
            "v(rl2:narrow)",
            "v(rl2:r)",
            "v(rl2:rsh)",
            "v(rl2:short)",
            "v(rl2:tc1)",
            "v(rl2:tc2)",
            "v(rl2:tce)",
            "v(rl2:wf)",
            "v(rl2_ac)",
            "v(rl2_bv_max)",
            "v(rl2_dtemp)",
            "v(rl2_l)",
            "v(rl2_m)",
            "v(rl2_scale)",
            "v(rl2_tc2)",
            "v(rl2_tc)",
            "v(rl2_tce)",
            "v(rl2_temp)",
            "v(rl2_w)",
            "v(v1_acmag)",
            "v(v1_acphase)",
            "v(v1_freq)",
            "v(v1)",
            "v(v1_phase)",
            "v(v1_pwr)",
            "v(v1_z0)",
        ],
        values: &[
            ("frequency", 4, 1.000000000000000e+05, 0.000000000000000e+00),
            ("v(c1)", 0, 1.387239209134980e+06, -2.451200138753172e+06),
            ("v(c1_m)", 0, 1.387239209034130e-03, -2.451200138574973e-03),
            ("v(r1)", 0, -1.911714369315196e-04, -1.114226977499523e-04),
            ("v(r1)", 1, 0.000000000000000e+00, 0.000000000000000e+00),
            (
                "v(r1_ac)",
                1,
                -1.913228811101668e-04,
                -1.132820216956903e-04,
            ),
            ("v(l1)", 0, 3.415532488528342e+00, -6.019073708933492e+00),
            (
                "v(g1_gain)",
                0,
                2.806548924600987e+04,
                -4.884261394280977e+04,
            ),
        ],
    },
    // JFET level 1 (#82). A reverse-biased PJF gate (v(g2) = 0.5 V, both
    // junctions at -0.5 V normalized, area 2, m = 3) leaks
    // 6 (IS (1 + arg) + gmin 0.5 V) with jfetload.c's cubic continuation
    // arg = (3 Vt / (-0.5 e))^3 = -1.86e-4: 3.11998e-12 A. j1 cut off
    // (vgs = -2.5 < VTO) at vds = 6 V leaks IS + 8.5 V gmin through its drain;
    // j3 (inverse, drain at 0, source at 0.3 V, area 0.5) leaks
    // 0.5 IS + 2.5 V gmin and 0.5 IS + 2.8 V gmin. At vgs = 0, vds = 6 V
    // the Sydney saturation current beta (1 + lambda vds) vgst^2 (B + bFac
    // vgst) with bFac = 0.3 / 3 is 5.1 mA at vgst = 2 V, 4.69 mA after the
    // RS = 15 ohm source drop.
    Expectation {
        fixture: "m10_jfet_dc",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 231,
        variables: &[
            "v(v-sweep)",
            "v(d)",
            "v(d2)",
            "v(d3)",
            "v(g)",
            "v(g2)",
            "v(s3)",
            "i(vds)",
            "i(vds2)",
            "i(vds3)",
            "i(vgs)",
            "i(vgs2)",
            "i(vs3)",
        ],
        values: &[
            ("v(v-sweep)", 0, -2.0, 0.),
            ("v(v-sweep)", 230, 6.0, 0.),
            ("i(vgs2)", 0, -3.119977670193058e-12, 0.),
            ("i(vds)", 32, -8.510020466090395e-12, 0.),
            ("i(vds3)", 0, -2.504999992531683e-12, 0.),
            ("i(vs3)", 0, -2.80499999470203e-12, 0.),
            ("v(g)", 197, 0.0, 0.),
            ("i(vds)", 197, -4.688663451518488e-3, 0.),
        ],
    },
    // A common-source JFET stage: the supply current is the load-resistor
    // current, i(vdd) = v(drain) / 4.7 k (the drain is the only path from
    // VDD), and the input source drives the gate through 50 k.
    Expectation {
        fixture: "m10_jfet_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 81,
        variables: &[
            "frequency",
            "v(d)",
            "v(g)",
            "v(in)",
            "v(s)",
            "v(vdd)",
            "i(vdd)",
            "i(vin)",
        ],
        values: &[
            ("frequency", 0, 10., 0.),
            ("v(d)", 0, -3.894274406962087, -1.758438756966057),
            ("i(vdd)", 0, -8.285690227578909e-4, -3.741359057374590e-4),
            ("v(in)", 0, 1., 0.),
            ("v(d)", 40, -9.081958090353348, 8.307667754627575),
            ("i(vdd)", 40, -1.932331508585819e-3, 1.767588883963314e-3),
        ],
    },
    // At t = 0 the PJF stage (VTO = -1.8 V normalized, gate at 0 V) is in
    // its linear region: beta vds (2 vgst - vds) = 1 m * 0.6 * 3 = 1.8 mA
    // through 3 k puts its drain at -0.6 V. The follower gate sits at the
    // -1 V pulse level plus 10 k times its junction leakage.
    Expectation {
        fixture: "m10_jfet_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 4029,
        variables: &[
            "time", "v(d2)", "v(dd2)", "v(g)", "v(g2)", "v(in)", "v(s)", "v(vdd)", "i(vdd)",
            "i(vdd2)", "i(vin)", "i(vin2)",
        ],
        values: &[
            ("time", 0, 0., 0.),
            ("v(d2)", 0, -5.999999997695838e-1, 0.),
            ("v(g)", 0, -9.999998539847536e-1, 0.),
            ("time", 4028, 1e-6, 0.),
        ],
    },
    // j4's forward-biased gate at 125 C: jfettemp.c's IS(T) =
    // IS exp((T/TNOM - 1) EG / (N Vt)) (T/TNOM)^XTI = 1.9024e-9 A for
    // IS = 1e-13, EG = 1.1, N = 1.2, XTI = 4, so the gate draws
    // IS(T) (exp(0.45 / (N Vt)) - 1) + 0.45 gmin from the gate-source diode
    // and IS(T) + 4.55 gmin back through the reverse gate-drain diode,
    // 1.0617852856717e-4 A in total (restated independently).
    Expectation {
        fixture: "m10_jfet_temp",
        plotname: "DC transfer characteristic",
        flags: PlotFlags::Real,
        points: 12,
        variables: &[
            "temp-sweep",
            "v(d)",
            "v(dp)",
            "v(g)",
            "v(gf)",
            "v(gp)",
            "i(vd)",
            "i(vdp)",
            "i(vg)",
            "i(vgf)",
            "i(vgp)",
        ],
        values: &[
            ("temp-sweep", 0, -40., 0.),
            ("temp-sweep", 11, 125., 0.),
            ("i(vgf)", 11, -1.0617852856717e-4, 0.),
        ],
    },
    // URC lines (#85). C expands each U instance at setup (urcsetup.c) and
    // saves the generated `<name>#hi<i>`/`#lo<i>` nodes (outitf.c only hides
    // device-internal names such as `#internal`).
    //
    // AC: u1 has 16 sections (FMAX rule, wnorm = 6283), u2 6 and u3 one. At
    // 1 kHz the lines are their series resistance: node a sees 50 ohms into
    // 1.01meg || 14k || 15k (0.99309 V); out = a x 1meg/1.01meg, c = a x 10k/15k
    // and u3's single section splits its 5k in half (hi1 = a x 12.5k/15k). At
    // 1 GHz u2, whose reference is node a, follows a (bootstrapped).
    Expectation {
        fixture: "m10_urc_ac",
        plotname: "AC Analysis",
        flags: PlotFlags::Complex,
        points: 61,
        variables: &[
            "frequency",
            "v(a)",
            "v(b)",
            "v(c)",
            "v(in)",
            "v(out)",
            "v(u1#hi1)",
            "v(u1#hi2)",
            "v(u1#hi3)",
            "v(u1#hi4)",
            "v(u1#hi5)",
            "v(u1#hi6)",
            "v(u1#hi7)",
            "v(u1#hi8)",
            "v(u1#hi9)",
            "v(u1#hi10)",
            "v(u1#hi11)",
            "v(u1#hi12)",
            "v(u1#hi13)",
            "v(u1#hi14)",
            "v(u1#hi15)",
            "v(u1#hi16)",
            "v(u1#lo1)",
            "v(u1#lo2)",
            "v(u1#lo3)",
            "v(u1#lo4)",
            "v(u1#lo5)",
            "v(u1#lo6)",
            "v(u1#lo7)",
            "v(u1#lo8)",
            "v(u1#lo9)",
            "v(u1#lo10)",
            "v(u1#lo11)",
            "v(u1#lo12)",
            "v(u1#lo13)",
            "v(u1#lo14)",
            "v(u1#lo15)",
            "v(u2#hi1)",
            "v(u2#hi2)",
            "v(u2#hi3)",
            "v(u2#hi4)",
            "v(u2#hi5)",
            "v(u2#hi6)",
            "v(u2#lo1)",
            "v(u2#lo2)",
            "v(u2#lo3)",
            "v(u2#lo4)",
            "v(u2#lo5)",
            "v(u3#hi1)",
            "i(v1)",
        ],
        values: &[
            ("frequency", 0, 1.000000000000000e+03, 0.000000000000000e+00),
            ("v(a)", 0, 9.930936632071502e-01, -5.382241520880548e-05),
            ("v(out)", 0, 9.832528899959881e-01, -3.121963569868132e-03),
            ("v(c)", 0, 6.620612607450966e-01, -9.025177738822004e-04),
            (
                "v(u3#hi1)",
                0,
                8.275765759313708e-01,
                -1.128147217352751e-03,
            ),
            ("v(b)", 0, 7.093531542884740e-01, 2.895352155594307e-04),
            (
                "frequency",
                60,
                1.000000000000003e+09,
                0.000000000000000e+00,
            ),
            ("v(b)", 60, 7.330566902714178e-01, -1.500625671561303e-01),
            ("v(a)", 60, 7.356733966178166e-01, -1.537677306069264e-01),
        ],
    },
    // Transient: u1 10 sections (K=2), u2 n=4 on a 0.5 V reference, u3 C's
    // minimum of 3. At t = 0 every line is at rest and the reference source
    // carries no current; by the end of the pulse plateau (4.1 us) the lines
    // are resistive: out -> 100k/111k, b -> 20k/(20k + 5) and c -> 50k/51k.
    Expectation {
        fixture: "m10_urc_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 539,
        variables: &[
            "time",
            "v(a)",
            "v(b)",
            "v(c)",
            "v(in)",
            "v(out)",
            "v(ref)",
            "v(u1#hi1)",
            "v(u1#hi2)",
            "v(u1#hi3)",
            "v(u1#hi4)",
            "v(u1#hi5)",
            "v(u1#hi6)",
            "v(u1#hi7)",
            "v(u1#hi8)",
            "v(u1#hi9)",
            "v(u1#hi10)",
            "v(u1#lo1)",
            "v(u1#lo2)",
            "v(u1#lo3)",
            "v(u1#lo4)",
            "v(u1#lo5)",
            "v(u1#lo6)",
            "v(u1#lo7)",
            "v(u1#lo8)",
            "v(u1#lo9)",
            "v(u2#hi1)",
            "v(u2#hi2)",
            "v(u2#hi3)",
            "v(u2#hi4)",
            "v(u2#lo1)",
            "v(u2#lo2)",
            "v(u2#lo3)",
            "v(u3#hi1)",
            "v(u3#hi2)",
            "v(u3#hi3)",
            "v(u3#lo1)",
            "v(u3#lo2)",
            "i(v1)",
            "i(vref)",
        ],
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(out)", 0, 0.0, 0.0),
            ("v(ref)", 0, 5.000000000000000e-01, 0.0),
            ("i(vref)", 0, 0.0, 0.0),
            ("time", 227, 4.106385346547844e-06, 0.0),
            ("v(out)", 227, 9.006617947405140e-01, 0.0),
            ("v(b)", 227, 9.997557712633194e-01, 0.0),
            ("v(c)", 227, 9.803922652419178e-01, 0.0),
            ("time", 538, 9.999999999999999e-06, 0.0),
            ("v(ref)", 538, 5.000000000000000e-01, 0.0),
        ],
    },
    // ISPERL: 9 sections (17 diodes of u1#diodemod, no capacitors); the
    // diodes' `#internal` series-resistance nodes are not saved. At t = 0 the
    // reference reverse-biases every diode by 0.8 V, so vref sinks the total
    // leakage IS + 17 x gmin x 0.8 V = 1e-14 + 1.36e-11 A. Near the first
    // positive peak the line conducts into the reference.
    Expectation {
        fixture: "m10_urc_diode_tran",
        plotname: "Transient Analysis",
        flags: PlotFlags::Real,
        points: 521,
        variables: &[
            "time",
            "v(a)",
            "v(in)",
            "v(out)",
            "v(ref)",
            "v(u1#hi1)",
            "v(u1#hi2)",
            "v(u1#hi3)",
            "v(u1#hi4)",
            "v(u1#hi5)",
            "v(u1#hi6)",
            "v(u1#hi7)",
            "v(u1#hi8)",
            "v(u1#hi9)",
            "v(u1#lo1)",
            "v(u1#lo2)",
            "v(u1#lo3)",
            "v(u1#lo4)",
            "v(u1#lo5)",
            "v(u1#lo6)",
            "v(u1#lo7)",
            "v(u1#lo8)",
            "i(v1)",
            "i(vref)",
        ],
        values: &[
            ("time", 0, 0.0, 0.0),
            ("v(ref)", 0, 8.000000000000000e-01, 0.0),
            ("v(a)", 0, 2.414677183998285e-08, 0.0),
            ("i(v1)", 0, 1.207338591999142e-11, 0.0),
            ("i(vref)", 0, -1.360999864456567e-11, 0.0),
            ("time", 76, 1.241349991185178e-06, 0.0),
            ("v(a)", 76, 1.415726361379865e+00, 0.0),
            ("v(out)", 76, 1.152598588810652e+00, 0.0),
            ("time", 520, 9.999999999999999e-06, 0.0),
        ],
    },
];

/// Multi-analysis fixtures (#96): one [`Expectation`] per plot, in rawfile
/// order, which is ngspice's batch order (`.ac`, `.dc`, `.op`, `.tran`), not
/// the deck order.
#[allow(clippy::excessive_precision)]
const MULTI_EXPECTATIONS: &[&[Expectation]] = &[
    &[
        // #102. `.ac`, `.op` and a filtered `.sens` (r* and v1), run in that order;
        // the divider gives v(out) = (2/3)/(1 + j w 0.667 ms) at 10 Hz.
        Expectation {
            fixture: "sens_multi",
            plotname: "AC Analysis",
            flags: PlotFlags::Complex,
            points: 5,
            variables: &["frequency", "v(in)", "v(out)", "i(v1)"],
            values: &[("v(out)", 0, 6.654989845853894e-01, -2.787635627926568e-02)],
        },
        // 10 V across 1k + 2k.
        Expectation {
            fixture: "sens_multi",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &["v(in)", "v(out)", "i(v1)"],
            values: &[("v(out)", 0, 6.666666666666666e+00, 0.0)],
        },
        // dV/dR2 = 10 R1/(R1+R2)^2 = 1.1111e-3 to first order in the 1e-6 step;
        // dV/dV1 = 2/3.
        Expectation {
            fixture: "sens_multi",
            plotname: "Sensitivity Analysis",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(r2:rsh)",
                "v(r1:af)",
                "v(r1:bv_max)",
                "v(r1:ef)",
                "v(r1:kf)",
                "v(r1:lf)",
                "v(r1:narrow)",
                "v(r1:r)",
                "v(r1:rsh)",
                "v(r1:short)",
                "v(r1:tc1)",
                "v(r1:tc2)",
                "v(r1:tce)",
                "v(r1:wf)",
                "v(r1_bv_max)",
                "v(r1_dtemp)",
                "v(r1_l)",
                "v(r1_m)",
                "v(r1)",
                "v(r1_scale)",
                "v(r1_tc)",
                "v(r1_tc2)",
                "v(r1_tce)",
                "v(r1_temp)",
                "v(r1_w)",
                "v(r2:af)",
                "v(r2:bv_max)",
                "v(r2:ef)",
                "v(r2:kf)",
                "v(r2:lf)",
                "v(r2:narrow)",
                "v(r2)",
                "v(r2:r)",
                "v(r2:short)",
                "v(r2:tc1)",
                "v(r2:tc2)",
                "v(r2:tce)",
                "v(r2:wf)",
                "v(r2_bv_max)",
                "v(r2_dtemp)",
                "v(r2_l)",
                "v(r2_m)",
                "v(r2_scale)",
                "v(r2_tc2)",
                "v(r2_tce)",
                "v(r2_tc)",
                "v(r2_temp)",
                "v(r2_w)",
                "v(v1)",
            ],
            values: &[
                ("v(r2)", 0, 1.111110000062313e-03, 0.0),
                ("v(v1)", 0, 6.666666666414282e-01, 0.0),
                ("v(r2:rsh)", 0, -0.000000000000000e+00, 0.0),
            ],
        },
    ],
    &[
        // Thevenin source of the 1k/2k divider: 2/3 of the drive behind 2/3 k, so
        // the corner is 1/(2 pi 666.7 ohm 100 nF) = 2.39 kHz; at 100 Hz the
        // response is (2/3) / (1 + j 0.041888).
        Expectation {
            fixture: "multi_analysis_rc",
            plotname: "AC Analysis",
            flags: PlotFlags::Complex,
            points: 5,
            variables: &["frequency", "v(in)", "v(out)", "i(v1)"],
            values: &[
                ("frequency", 0, 100.0, 0.0),
                ("v(out)", 0, 6.654989845853894e-01, -2.787635627926568e-02),
                ("i(v1)", 0, -3.345010154146106e-04, -2.787635627926568e-05),
            ],
        },
        // The sweep scales the divider: v(out) = 2/3 v1, i(v1) = -v1 / 3 k.
        Expectation {
            fixture: "multi_analysis_rc",
            plotname: "DC transfer characteristic",
            flags: PlotFlags::Real,
            points: 5,
            variables: &["v(v-sweep)", "v(in)", "v(out)", "i(v1)"],
            values: &[
                ("v(v-sweep)", 3, 1.5, 0.0),
                ("v(out)", 3, 1.0, 0.0),
                ("i(v1)", 3, -5.0e-4, 0.0),
            ],
        },
        // The operating point uses the source's `dc 2`, not the pulse's t = 0 value.
        Expectation {
            fixture: "multi_analysis_rc",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &["v(in)", "v(out)", "i(v1)"],
            values: &[
                ("v(in)", 0, 2.0, 0.0),
                ("v(out)", 0, 1.333333333333333e+00, 0.0),
                ("i(v1)", 0, -6.666666666666668e-04, 0.0),
            ],
        },
        // The transient starts from the pulse's 0 V and, 480 us after the pulse
        // ended, has decayed by about exp(-480/66.7) toward zero.
        Expectation {
            fixture: "multi_analysis_rc",
            plotname: "Transient Analysis",
            flags: PlotFlags::Real,
            points: 124,
            variables: &["time", "v(in)", "v(out)", "i(v1)"],
            values: &[
                ("v(out)", 0, 0.0, 0.0),
                ("time", 123, 1.0e-3, 0.0),
                ("v(out)", 123, 4.549183592268200e-04, 0.0),
                ("i(v1)", 123, 4.549183592268200e-07, 0.0),
            ],
        },
    ],
    &[
        // M6 exit gate. At 10 Hz the closed loop is 10 / (1 + 10/1e5) with
        // the op-amp's 10 Hz pole already turning; the B limiter's slope at
        // the 0 V bias is 1, so v(lim) = v(out), and the H sense is v(lim)/10.
        Expectation {
            fixture: "m6_gate",
            plotname: "AC Analysis",
            flags: PlotFlags::Complex,
            points: 21,
            variables: &[
                "frequency",
                "i(blim)",
                "i(e.xop.e1)",
                "v(fb)",
                "i(hsense)",
                "v(in)",
                "v(isense)",
                "i(l1)",
                "i(l2)",
                "v(lim)",
                "v(out)",
                "v(p1)",
                "v(ref)",
                "v(s1)",
                "v(sense)",
                "i(vin)",
                "i(vref)",
                "i(vsense)",
                "v(xop.x)",
            ],
            values: &[
                ("frequency", 0, 1.000000000000000e+01, 0.0),
                ("v(s1)", 0, 3.106057325456545e-02, 2.423226442281318e-01),
                ("v(out)", 0, 3.108167103009356e-01, 2.422953095559862e+00),
                ("v(lim)", 0, 3.108167103009356e-01, 2.422953095559862e+00),
                ("v(isense)", 0, 3.108167103009356e-02, 2.422953095559862e-01),
            ],
        },
        // The secondary shorts s1 at DC: v(out) = -9 vref / (1 + 10/1e5)
        // (4.49955 V at vref = -0.5), v(lim) = 2 tanh(v(out)/2).
        Expectation {
            fixture: "m6_gate",
            plotname: "DC transfer characteristic",
            flags: PlotFlags::Real,
            points: 11,
            variables: &[
                "v(v-sweep)",
                "i(blim)",
                "i(e.xop.e1)",
                "v(fb)",
                "i(hsense)",
                "v(in)",
                "v(isense)",
                "i(l1)",
                "i(l2)",
                "v(lim)",
                "v(out)",
                "v(p1)",
                "v(ref)",
                "v(s1)",
                "v(sense)",
                "i(vin)",
                "i(vref)",
                "i(vsense)",
                "v(xop.x)",
            ],
            values: &[
                ("v(v-sweep)", 0, -5.000000000000000e-01, 0.0),
                ("v(s1)", 0, 0.0, 0.0),
                ("v(out)", 0, 4.499550044995501e+00, 0.0),
                ("v(lim)", 0, 1.956032667915977e+00, 0.0),
                ("v(isense)", 0, 1.956032667915977e-01, 0.0),
                ("v(out)", 10, -4.499550044995501e+00, 0.0),
            ],
        },
        // Every source is 0 V at DC: the circuit rests at zero.
        Expectation {
            fixture: "m6_gate",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(in)",
                "i(blim)",
                "i(e.xop.e1)",
                "v(fb)",
                "i(hsense)",
                "v(isense)",
                "i(l1)",
                "i(l2)",
                "v(lim)",
                "v(out)",
                "v(p1)",
                "v(ref)",
                "v(s1)",
                "v(sense)",
                "i(vin)",
                "i(vref)",
                "i(vsense)",
                "v(xop.x)",
            ],
            values: &[("v(out)", 0, 0.0, 0.0), ("v(lim)", 0, 0.0, 0.0)],
        },
        // From rest at t = 0; at 3 ms (three SIN periods) the drive is back
        // at zero and the output small, where the limiter is nearly linear.
        Expectation {
            fixture: "m6_gate",
            plotname: "Transient Analysis",
            flags: PlotFlags::Real,
            points: 327,
            variables: &[
                "time",
                "i(blim)",
                "i(e.xop.e1)",
                "v(fb)",
                "i(hsense)",
                "v(in)",
                "v(isense)",
                "i(l1)",
                "i(l2)",
                "v(lim)",
                "v(out)",
                "v(p1)",
                "v(ref)",
                "v(s1)",
                "v(sense)",
                "i(vin)",
                "i(vref)",
                "i(vsense)",
                "v(xop.x)",
            ],
            values: &[
                ("time", 0, 0.0, 0.0),
                ("v(out)", 0, 0.0, 0.0),
                ("time", 326, 3.000000000000000e-03, 0.0),
                ("v(out)", 326, 7.708097104984421e-02, 0.0),
                ("v(lim)", 326, 7.704282914934442e-02, 0.0),
                ("v(isense)", 326, 7.704282914934442e-03, 0.0),
            ],
        },
    ],
    &[
        // Emitter-coupled BJT Schmitt trigger (#106). ngspice runs the later
        // `.dc` card first: the downward sweep starts with q1 on and q2 off
        // (out at VCC) and holds that state at 2.1 V ...
        Expectation {
            fixture: "m7_conv_bjt_schmitt",
            plotname: "DC transfer characteristic",
            flags: PlotFlags::Real,
            points: 61,
            variables: &[
                "v(v-sweep)",
                "v(b1)",
                "v(b2)",
                "v(c1)",
                "v(e)",
                "v(in)",
                "v(out)",
                "v(vcc)",
                "i(vcc)",
                "i(vin)",
            ],
            values: &[
                ("v(v-sweep)", 0, 3.000000000000000e+00, 0.0),
                ("v(out)", 0, 4.999999990809181e+00, 0.0),
                ("v(v-sweep)", 18, 2.100000000000003e+00, 0.0),
                ("v(out)", 18, 4.999999990670086e+00, 0.0),
                ("v(out)", 60, 1.770312170849794e+00, 0.0),
            ],
        },
        // ... while the upward sweep keeps q2 on (out = 1.77 V) through 2.1 V:
        // the two sweeps differ inside the hysteresis band.
        Expectation {
            fixture: "m7_conv_bjt_schmitt",
            plotname: "DC transfer characteristic",
            flags: PlotFlags::Real,
            points: 61,
            variables: &[
                "v(v-sweep)",
                "v(b1)",
                "v(b2)",
                "v(c1)",
                "v(e)",
                "v(in)",
                "v(out)",
                "v(vcc)",
                "i(vcc)",
                "i(vin)",
            ],
            values: &[
                ("v(v-sweep)", 0, 0.0, 0.0),
                ("v(out)", 0, 1.770312170849776e+00, 0.0),
                ("v(v-sweep)", 42, 2.100000000000001e+00, 0.0),
                ("v(out)", 42, 1.770526226101157e+00, 0.0),
                ("v(out)", 60, 4.999999990809181e+00, 0.0),
            ],
        },
        // The operating point at vin = 2.1 V, reached by CKTop from
        // MODEINITJCT with dynamic gmin, is the third (middle) solution.
        Expectation {
            fixture: "m7_conv_bjt_schmitt",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(vcc)", "v(b1)", "v(b2)", "v(c1)", "v(e)", "v(in)", "v(out)", "i(vcc)", "i(vin)",
            ],
            values: &[
                ("v(in)", 0, 2.100000000000000e+00, 0.0),
                ("v(out)", 0, 2.338543430598761e+00, 0.0),
                ("v(e)", 0, 1.476806727682212e+00, 0.0),
            ],
        },
    ],
    &[
        // CMOS Schmitt trigger (#106): downward from 5 V the output stays low
        // through 2.5 V ...
        Expectation {
            fixture: "m7_conv_cmos_schmitt",
            plotname: "DC transfer characteristic",
            flags: PlotFlags::Real,
            points: 51,
            variables: &[
                "v(v-sweep)",
                "v(a)",
                "v(b)",
                "v(in)",
                "v(out)",
                "v(vdd)",
                "i(vdd)",
                "i(vin)",
            ],
            values: &[
                ("v(v-sweep)", 0, 5.000000000000000e+00, 0.0),
                ("v(v-sweep)", 25, 2.500000000000002e+00, 0.0),
                ("v(out)", 25, 3.929411702970586e-08, 0.0),
                ("v(out)", 50, 4.999999980914284e+00, 0.0),
            ],
        },
        // ... and upward from 0 V it stays high there (hysteresis).
        Expectation {
            fixture: "m7_conv_cmos_schmitt",
            plotname: "DC transfer characteristic",
            flags: PlotFlags::Real,
            points: 51,
            variables: &[
                "v(v-sweep)",
                "v(a)",
                "v(b)",
                "v(in)",
                "v(out)",
                "v(vdd)",
                "i(vdd)",
                "i(vin)",
            ],
            values: &[
                ("v(v-sweep)", 25, 2.500000000000001e+00, 0.0),
                ("v(out)", 25, 4.999999952847062e+00, 0.0),
                ("v(b)", 25, 2.499999927084873e+00, 0.0),
            ],
        },
        // The operating point at vin = 2.5 V lands in the low-output state.
        Expectation {
            fixture: "m7_conv_cmos_schmitt",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(vdd)", "v(a)", "v(b)", "v(in)", "v(out)", "i(vdd)", "i(vin)",
            ],
            values: &[
                ("v(out)", 0, 3.929411702970590e-08, 0.0),
                ("v(a)", 0, 2.500000070751028e+00, 0.0),
            ],
        },
    ],
    &[
        // `.op`, `.ac` and two `.pz` cards: ngspice runs `.ac`, `.op`, then the
        // later `.pz` (zeros) before the earlier (poles). The operating point
        // divides 1 V by r1 + r2 = 1.1 k into the 100 ohm r2.
        Expectation {
            fixture: "multi_analysis_pz",
            plotname: "AC Analysis",
            flags: PlotFlags::Complex,
            points: 3,
            variables: &["frequency", "v(a)", "v(in)", "i(l1)", "v(out)", "i(v1)"],
            values: &[
                ("frequency", 0, 100.0, 0.0),
                ("v(out)", 0, 9.083936024383166e-02, -5.727952383288644e-03),
            ],
        },
        Expectation {
            fixture: "multi_analysis_pz",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &["v(in)", "v(a)", "i(l1)", "v(out)", "i(v1)"],
            values: &[
                ("v(out)", 0, 9.090909090909093e-02, 0.0),
                ("i(v1)", 0, -9.090909090909091e-04, 0.0),
            ],
        },
        // c3 bridges the input to the output: two right-half-plane zeros.
        Expectation {
            fixture: "multi_analysis_pz",
            plotname: "Pole-Zero Analysis",
            flags: PlotFlags::Complex,
            points: 1,
            variables: &["v(zero(1))", "v(zero(2))", "v(zero(3))"],
            values: &[
                ("v(zero(1))", 0, -2.031285288892326e+04, 0.0),
                (
                    "v(zero(2))",
                    0,
                    9.656426444461636e+03,
                    1.997630015595523e+04,
                ),
                (
                    "v(zero(3))",
                    0,
                    9.656426444461636e+03,
                    -1.997630015595523e+04,
                ),
            ],
        },
        Expectation {
            fixture: "multi_analysis_pz",
            plotname: "Pole-Zero Analysis",
            flags: PlotFlags::Complex,
            points: 1,
            variables: &["v(pole(1))", "v(pole(2))", "v(pole(3))"],
            values: &[
                ("v(pole(1))", 0, -7.967878263548421e+04, 0.0),
                (
                    "v(pole(2))",
                    0,
                    -6.115154136803343e+03,
                    9.386629607343893e+03,
                ),
                (
                    "v(pole(3))",
                    0,
                    -6.115154136803343e+03,
                    -9.386629607343893e+03,
                ),
            ],
        },
    ],
    &[
        // `.tf` in a batch (#101): `.op`, then the two `.tf` cards in reverse
        // deck order. tf1 (MOS1): the supply current changes by
        // -gm * ro / (ro + 10k) per gate volt, the supply sees 10k + ro =
        // 241 k and the gate rg = 1 Mohm. tf2 (diode): at 1.2 mA the diode is
        // rs + n Vt / Id = 30.8 ohm against rd = 1k (gain 0.0299, input
        // 1030.8 ohm), and v(d, dm) looks into rd || 30.8 plus 10k || ro.
        Expectation {
            fixture: "m8_tf_batch",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(vdin)", "v(d)", "v(dm)", "v(g)", "i(vd)", "v(vdd)", "i(vdd)", "i(vg)",
            ],
            values: &[
                ("v(d)", 0, 7.978661647232698e-01, 0.0),
                ("v(dm)", 0, 2.722392611844397e+00, 0.0),
                ("i(vg)", 0, -2.0e-06, 0.0),
            ],
        },
        Expectation {
            fixture: "m8_tf_batch",
            plotname: "Transfer Function",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(transfer_function)",
                "v(vdd#output_impedance)",
                "v(vg#input_impedance)",
            ],
            values: &[
                ("v(transfer_function)", 0, -3.638815406120247e-04, 0.0),
                ("v(vdd#output_impedance)", 0, 2.414814278978181e+05, 0.0),
                ("v(vg#input_impedance)", 0, 1.0e+06, 0.0),
            ],
        },
        Expectation {
            fixture: "m8_tf_batch",
            plotname: "Transfer Function",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(transfer_function)",
                "v(output_impedance_at_v(d,dm))",
                "v(vd#input_impedance)",
            ],
            values: &[
                ("v(transfer_function)", 0, 2.989759171726813e-02, 0.0),
                (
                    "v(output_impedance_at_v(d,dm))",
                    0,
                    9.615787070380138e+03,
                    0.0,
                ),
                ("v(vd#input_impedance)", 0, 1.030819005768878e+03, 0.0),
            ],
        },
    ],
    // `.sp` (#105) with `.ac`, `.op` and `.tran` on the same RF ports (50 and
    // 75 ohm). Every analysis keeps each port's z0 in series: the operating
    // point is 1 V behind 50 + 50 ohm into 200 || 75, so v(out) = 0.35294 and
    // v(in) = 0.67647. In `.ac` port 1 drives `ac 1`; because r1 = z0 = 50,
    // its v(out) equals the `.sp` S11 = (Zin - 50)/(Zin + 50). The `.sp` node
    // vectors are port 2's excitation (v(v1#res) = 0). The pulse/pwr setters
    // are overridden by the final `sin(0 1 1meg)`, which is ~0 at 20 us.
    &[
        Expectation {
            fixture: "sp_multi",
            plotname: "AC Analysis",
            flags: PlotFlags::Complex,
            points: 3,
            variables: &[
                "frequency",
                "v(in)",
                "v(out)",
                "i(v1)",
                "v(v1#res)",
                "i(v2)",
                "v(v2#res)",
            ],
            values: &[
                ("v(out)", 0, 3.527676949572184e-01, -7.822958109892253e-03),
                ("v(v1#res)", 2, 1.0, 0.0),
                ("i(v2)", 2, 7.952179178714906e-04, -1.763471130909209e-03),
            ],
        },
        Expectation {
            fixture: "sp_multi",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(in)",
                "v(out)",
                "i(v1)",
                "v(v1#res)",
                "i(v2)",
                "v(v2#res)",
            ],
            values: &[
                ("v(in)", 0, 6.764705882352942e-01, 0.0),
                ("v(out)", 0, 3.529411764705882e-01, 0.0),
                ("i(v1)", 0, -6.470588235294117e-03, 0.0),
                ("i(v2)", 0, 4.705882352941176e-03, 0.0),
            ],
        },
        Expectation {
            fixture: "sp_multi",
            plotname: "Transient Analysis",
            flags: PlotFlags::Real,
            points: 212,
            variables: &[
                "time",
                "v(in)",
                "v(out)",
                "i(v1)",
                "v(v1#res)",
                "i(v2)",
                "v(v2#res)",
            ],
            values: &[
                ("v(out)", 0, 0.0, 0.0),
                ("time", 211, 2.0e-05, 0.0),
                ("v(out)", 211, -1.292445458038272e-01, 0.0),
                ("i(v2)", 211, -1.723260610717697e-03, 0.0),
            ],
        },
        Expectation {
            fixture: "sp_multi",
            plotname: "SP Analysis",
            flags: PlotFlags::Complex,
            points: 4,
            variables: &[
                "frequency",
                "v(rbase)",
                "s_1_1",
                "s_1_2",
                "s_2_1",
                "s_2_2",
                "y_1_1",
                "y_1_2",
                "y_2_1",
                "y_2_2",
                "z_1_1",
                "z_1_2",
                "z_2_1",
                "z_2_2",
                "v(in)",
                "v(out)",
                "i(v1)",
                "v(v1#res)",
                "i(v2)",
                "v(v2#res)",
            ],
            values: &[
                ("s_1_1", 0, 3.527676949572185e-01, -7.822958109892253e-03),
                ("s_2_1", 3, 9.739390665518058e-02, -2.159802223428179e-01),
                ("v(v1#res)", 0, 0.0, 0.0),
                ("v(out)", 0, 4.703569266096246e-01, -1.043061081318967e-02),
                ("v(rbase)", 3, 50.0, 0.0),
            ],
        },
    ],
    &[
        // #100. At 10 Hz the 1 nF capacitor is negligible and the output sees
        // 1k || 2k || 3k || 1.5k = 400 ohm, so r1's thermal density is
        // sqrt(4 k 300.15 K / 1k) * 400 = 1.6285e-9 V/sqrt(Hz); the literal
        // resistors have no flicker noise (KF = 0) and r3 (noisy=0) no columns.
        // r5 (500 ohm under r4 = 1k) reaches the output through the 500/1500
        // divider: sqrt(4 k T / 500) * 400 / 3 = 7.677e-10 V/sqrt(Hz).
        Expectation {
            fixture: "noise_rc",
            plotname: "Noise Spectral Density Curves",
            flags: PlotFlags::Real,
            points: 13,
            variables: &[
                "frequency",
                "inoise_spectrum",
                "onoise_r1",
                "onoise_r1_1overf",
                "onoise_r1_thermal",
                "onoise_r2",
                "onoise_r2_1overf",
                "onoise_r2_thermal",
                "onoise_r4",
                "onoise_r4_1overf",
                "onoise_r4_thermal",
                "onoise_r5_1overf",
                "onoise_r5",
                "onoise_r5_thermal",
                "onoise_spectrum",
            ],
            values: &[
                ("frequency", 0, 1.000000000000000e+01, 0.0),
                ("onoise_r1_thermal", 0, 1.628548611280592e-09, 0.0),
                ("onoise_r1_1overf", 0, 0.000000000000000e+00, 0.0),
                ("onoise_r5_thermal", 0, 7.677051776856275e-10, 0.0),
                ("onoise_spectrum", 12, 1.005489959385753e-10, 0.0),
            ],
        },
        // The integrated noise of the same deck: r2's KF/AF/EF flicker noise
        // dominates the 10 Hz .. 10 MHz total.
        Expectation {
            fixture: "noise_rc",
            plotname: "Integrated Noise",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(onoise_total_r4_thermal)",
                "v(inoise_total_r1_1overf)",
                "v(inoise_total_r1)",
                "v(inoise_total_r1_thermal)",
                "v(inoise_total_r2)",
                "v(inoise_total_r2_1overf)",
                "v(inoise_total_r2_thermal)",
                "v(inoise_total_r4)",
                "v(inoise_total_r4_1overf)",
                "v(inoise_total_r4_thermal)",
                "v(inoise_total_r5_1overf)",
                "v(inoise_total_r5)",
                "v(inoise_total)",
                "v(inoise_total_r5_thermal)",
                "v(onoise_total_r1_1overf)",
                "v(onoise_total_r1)",
                "v(onoise_total_r1_thermal)",
                "v(onoise_total_r2)",
                "v(onoise_total_r2_1overf)",
                "v(onoise_total_r2_thermal)",
                "v(onoise_total_r4_1overf)",
                "v(onoise_total)",
                "v(onoise_total_r4)",
                "v(onoise_total_r5)",
                "v(onoise_total_r5_1overf)",
                "v(onoise_total_r5_thermal)",
            ],
            values: &[
                ("v(onoise_total)", 0, 1.762234501449271e-05, 0.0),
                ("v(inoise_total)", 0, 5.288138992590370e-05, 0.0),
                ("v(onoise_total_r2_1overf)", 0, 1.752056590064329e-05, 0.0),
            ],
        },
    ],
    &[
        // #100. No pts_per_summary: only the circuit spectra.
        Expectation {
            fixture: "noise_diode",
            plotname: "Noise Spectral Density Curves",
            flags: PlotFlags::Real,
            points: 41,
            variables: &["frequency", "inoise_spectrum", "onoise_spectrum"],
            values: &[
                ("frequency", 40, 1.000000000000003e+08, 0.0),
                ("onoise_spectrum", 0, 2.856057133693482e-08, 0.0),
                ("inoise_spectrum", 0, 8.767441824204312e-08, 0.0),
            ],
        },
        // Integrated over 1 Hz .. 100 MHz.
        Expectation {
            fixture: "noise_diode",
            plotname: "Integrated Noise",
            flags: PlotFlags::Real,
            points: 1,
            variables: &["v(onoise_total)", "v(inoise_total)"],
            values: &[
                ("v(onoise_total)", 0, 3.331383551978140e-05, 0.0),
                ("v(inoise_total)", 0, 1.022054778251567e-04, 0.0),
            ],
        },
    ],
    &[
        // #100. A current input refers the input noise to amperes; the RCO-free
        // quasi-saturation generator (rci) is zero.
        Expectation {
            fixture: "noise_bjt",
            plotname: "Noise Spectral Density Curves",
            flags: PlotFlags::Real,
            points: 10,
            variables: &[
                "frequency",
                "inoise_spectrum",
                "onoise_q1_1overf",
                "onoise_q1_ib",
                "onoise_q1_ic",
                "onoise_q1_rb",
                "onoise_q1_rc",
                "onoise_q1_rci",
                "onoise_q1_re",
                "onoise_q1",
                "onoise_rbias",
                "onoise_rbias_1overf",
                "onoise_rbias_thermal",
                "onoise_rc",
                "onoise_rc_1overf",
                "onoise_rc_thermal",
                "onoise_re_1overf",
                "onoise_re",
                "onoise_re_thermal",
                "onoise_spectrum",
            ],
            values: &[
                ("frequency", 9, 5.623413251903506e+07, 0.0),
                ("onoise_q1_rci", 0, 0.000000000000000e+00, 0.0),
                ("onoise_q1_ic", 0, 3.928030366835040e-08, 0.0),
                ("inoise_spectrum", 0, 1.477839734782995e-11, 0.0),
            ],
        },
        // C's `write` puts the first vector it meets first and sorts the rest.
        Expectation {
            fixture: "noise_bjt",
            plotname: "Integrated Noise",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(onoise_total_q1_rc)",
                "i(inoise_total)",
                "i(inoise_total_q1_1overf)",
                "i(inoise_total_q1_ib)",
                "i(inoise_total_q1_ic)",
                "i(inoise_total_q1)",
                "i(inoise_total_q1_rb)",
                "i(inoise_total_q1_rc)",
                "i(inoise_total_q1_rci)",
                "i(inoise_total_q1_re)",
                "i(inoise_total_rbias)",
                "i(inoise_total_rbias_1overf)",
                "i(inoise_total_rbias_thermal)",
                "i(inoise_total_rc_1overf)",
                "i(inoise_total_rc_thermal)",
                "i(inoise_total_rc)",
                "i(inoise_total_re)",
                "i(inoise_total_re_1overf)",
                "i(inoise_total_re_thermal)",
                "v(onoise_total_q1_1overf)",
                "v(onoise_total_q1_ib)",
                "v(onoise_total_q1_ic)",
                "v(onoise_total_q1_rb)",
                "v(onoise_total_q1)",
                "v(onoise_total)",
                "v(onoise_total_q1_rci)",
                "v(onoise_total_q1_re)",
                "v(onoise_total_rbias)",
                "v(onoise_total_rbias_1overf)",
                "v(onoise_total_rbias_thermal)",
                "v(onoise_total_rc)",
                "v(onoise_total_rc_1overf)",
                "v(onoise_total_rc_thermal)",
                "v(onoise_total_re)",
                "v(onoise_total_re_1overf)",
                "v(onoise_total_re_thermal)",
            ],
            values: &[
                ("v(onoise_total)", 0, 3.690648707272827e-04, 0.0),
                ("i(inoise_total)", 0, 5.579948270123649e-08, 0.0),
                ("v(onoise_total_q1_rci)", 0, 0.000000000000000e+00, 0.0),
            ],
        },
    ],
    &[
        // #100. Every NLEV flicker law; NLEV 0's flicker noise dominates m1.
        Expectation {
            fixture: "noise_mos1",
            plotname: "Noise Spectral Density Curves",
            flags: PlotFlags::Real,
            points: 6,
            variables: &[
                "frequency",
                "inoise_spectrum",
                "onoise_m1_1overf",
                "onoise_m1_id",
                "onoise_m1_rd",
                "onoise_m1_rs",
                "onoise_m1",
                "onoise_m2",
                "onoise_m2_1overf",
                "onoise_m2_id",
                "onoise_m2_rd",
                "onoise_m2_rs",
                "onoise_m3",
                "onoise_m3_1overf",
                "onoise_m3_id",
                "onoise_m3_rd",
                "onoise_m3_rs",
                "onoise_m4_1overf",
                "onoise_m4_id",
                "onoise_m4_rd",
                "onoise_m4",
                "onoise_m4_rs",
                "onoise_m5",
                "onoise_m5_1overf",
                "onoise_m5_id",
                "onoise_m5_rd",
                "onoise_m5_rs",
                "onoise_r1",
                "onoise_r1_1overf",
                "onoise_r1_thermal",
                "onoise_r2",
                "onoise_r2_1overf",
                "onoise_r2_thermal",
                "onoise_spectrum",
            ],
            values: &[
                ("frequency", 5, 1.000000000000000e+05, 0.0),
                ("onoise_m1_1overf", 0, 4.454684965572098e-06, 0.0),
                ("onoise_m4_id", 0, 2.104661168490229e-09, 0.0),
                ("onoise_spectrum", 0, 4.473840326116049e-06, 0.0),
            ],
        },
        // The integrated noise of the same deck.
        Expectation {
            fixture: "noise_mos1",
            plotname: "Integrated Noise",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(onoise_total_m5_rd)",
                "v(inoise_total_m1)",
                "v(inoise_total_m1_1overf)",
                "v(inoise_total_m1_id)",
                "v(inoise_total_m1_rd)",
                "v(inoise_total_m1_rs)",
                "v(inoise_total_m2_1overf)",
                "v(inoise_total_m2_id)",
                "v(inoise_total_m2)",
                "v(inoise_total_m2_rd)",
                "v(inoise_total_m2_rs)",
                "v(inoise_total_m3_1overf)",
                "v(inoise_total_m3_id)",
                "v(inoise_total_m3)",
                "v(inoise_total_m3_rd)",
                "v(inoise_total_m3_rs)",
                "v(inoise_total_m4_1overf)",
                "v(inoise_total_m4)",
                "v(inoise_total_m4_id)",
                "v(inoise_total_m4_rd)",
                "v(inoise_total_m4_rs)",
                "v(inoise_total_m5_1overf)",
                "v(inoise_total_m5_id)",
                "v(inoise_total_m5_rd)",
                "v(inoise_total_m5)",
                "v(inoise_total_m5_rs)",
                "v(inoise_total_r1)",
                "v(inoise_total_r1_1overf)",
                "v(inoise_total_r1_thermal)",
                "v(inoise_total_r2)",
                "v(inoise_total)",
                "v(inoise_total_r2_1overf)",
                "v(inoise_total_r2_thermal)",
                "v(onoise_total_m1_1overf)",
                "v(onoise_total_m1_id)",
                "v(onoise_total_m1_rd)",
                "v(onoise_total_m1_rs)",
                "v(onoise_total_m1)",
                "v(onoise_total_m2)",
                "v(onoise_total_m2_1overf)",
                "v(onoise_total_m2_id)",
                "v(onoise_total_m2_rd)",
                "v(onoise_total_m2_rs)",
                "v(onoise_total_m3)",
                "v(onoise_total_m3_1overf)",
                "v(onoise_total_m3_id)",
                "v(onoise_total_m3_rd)",
                "v(onoise_total_m3_rs)",
                "v(onoise_total_m4_1overf)",
                "v(onoise_total_m4)",
                "v(onoise_total)",
                "v(onoise_total_m4_id)",
                "v(onoise_total_m4_rd)",
                "v(onoise_total_m4_rs)",
                "v(onoise_total_m5_1overf)",
                "v(onoise_total_m5)",
                "v(onoise_total_m5_id)",
                "v(onoise_total_m5_rs)",
                "v(onoise_total_r1)",
                "v(onoise_total_r1_1overf)",
                "v(onoise_total_r1_thermal)",
                "v(onoise_total_r2)",
                "v(onoise_total_r2_1overf)",
                "v(onoise_total_r2_thermal)",
            ],
            values: &[
                ("v(onoise_total)", 0, 3.036060712865311e-04, 0.0),
                ("v(inoise_total)", 0, 5.048415191717196e-04, 0.0),
            ],
        },
    ],
    &[
        // #100. `.ac` at 100 Hz: the 1k/2k divider's 2/3 behind 10 nF.
        Expectation {
            fixture: "noise_multi",
            plotname: "AC Analysis",
            flags: PlotFlags::Complex,
            points: 5,
            variables: &[
                "frequency",
                "v(in)",
                "v(out)",
                "i(v1)",
                "i(vs)",
                "v(x)",
                "v(y)",
            ],
            values: &[
                ("frequency", 0, 1.000000000000000e+02, 0.000000000000000e+00),
                ("v(out)", 0, 6.662112875224052e-01, -2.788761996285963e-03),
            ],
        },
        // s1 is closed at the operating point (control 2 V > VT = 1 V), so r3
        // plus RON loads the divider: v(out) = 2 (2k || 1.1k) / (1k + 2k || 1.1k).
        Expectation {
            fixture: "noise_multi",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &["v(in)", "v(out)", "i(v1)", "i(vs)", "v(x)", "v(y)"],
            values: &[("v(out)", 0, 8.301886789006766e-01, 0.0)],
        },
        // The later, single-frequency `.noise` card runs first: one spectrum
        // point, no integrated plot. The switches are open in noise, as in C:
        // the small-signal load copies the zero accepted switch state.
        Expectation {
            fixture: "noise_multi",
            plotname: "Noise Spectral Density Curves",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "frequency",
                "inoise_spectrum",
                "onoise_r1",
                "onoise_r1_1overf",
                "onoise_r1_thermal",
                "onoise_r2_1overf",
                "onoise_r2",
                "onoise_r2_thermal",
                "onoise_r3",
                "onoise_r3_1overf",
                "onoise_r3_thermal",
                "onoise_s1",
                "onoise_spectrum",
                "onoise_w1",
            ],
            values: &[
                ("frequency", 0, 1.000000000000000e+04, 0.0),
                ("onoise_s1", 0, 7.904338498719295e-11, 0.0),
                ("onoise_w1", 0, 7.912242837218013e-14, 0.0),
                ("onoise_spectrum", 0, 3.065418750855006e-09, 0.0),
            ],
        },
        // The decade card's spectrum and integrated noise.
        Expectation {
            fixture: "noise_multi",
            plotname: "Noise Spectral Density Curves",
            flags: PlotFlags::Real,
            points: 13,
            variables: &["frequency", "inoise_spectrum", "onoise_spectrum"],
            values: &[
                ("frequency", 12, 1.000000000000001e+06, 0.0),
                ("onoise_spectrum", 0, 3.323125390241841e-09, 0.0),
            ],
        },
        Expectation {
            fixture: "noise_multi",
            plotname: "Integrated Noise",
            flags: PlotFlags::Real,
            points: 1,
            variables: &["v(onoise_total)", "v(inoise_total)"],
            values: &[
                ("v(onoise_total)", 0, 6.301893945257240e-07, 0.0),
                ("v(inoise_total)", 0, 7.223239499099059e-06, 0.0),
            ],
        },
    ],
    &[
        // #104. The later IM card runs first. Every stage has its own input, so
        // no response vanishes; the IM card steps by half octaves from 1 kHz
        // (13 points to 64 kHz), the harmonic card by thirds of a decade to
        // 10 MHz (13 points).
        Expectation {
            fixture: "disto_diode",
            plotname: "DISTORTION - IM: f1+f2",
            flags: PlotFlags::Complex,
            points: 13,
            variables: &[
                "frequency",
                "v(a)",
                "v(in)",
                "v(k)",
                "v(v)",
                "i(v1)",
                "i(v3)",
                "v(z)",
            ],
            values: &[
                (
                    "frequency",
                    12,
                    6.399999999999993e+04,
                    0.000000000000000e+00,
                ),
                ("v(a)", 0, -1.567418829808580e-04, -9.047995939404058e-05),
                ("v(v)", 0, -1.988424803951513e-06, 2.039837100415841e-06),
                ("v(k)", 0, 4.430307569206585e-05, -7.308197734831027e-08),
            ],
        },
        Expectation {
            fixture: "disto_diode",
            plotname: "DISTORTION - IM: f1-f2",
            flags: PlotFlags::Complex,
            points: 13,
            variables: &[
                "frequency",
                "v(a)",
                "v(in)",
                "v(k)",
                "v(v)",
                "i(v1)",
                "i(v3)",
                "v(z)",
            ],
            values: &[
                (
                    "frequency",
                    12,
                    6.399999999999993e+04,
                    0.000000000000000e+00,
                ),
                ("v(a)", 0, -1.567350424011118e-04, 9.049180886584899e-05),
                ("v(v)", 0, -1.059464424130241e-07, 1.060888659816988e-07),
                ("v(k)", 0, 4.430315083036539e-05, -3.846423122206841e-09),
            ],
        },
        Expectation {
            fixture: "disto_diode",
            plotname: "DISTORTION - IM: 2f1-f2",
            flags: PlotFlags::Complex,
            points: 13,
            variables: &[
                "frequency",
                "v(a)",
                "v(in)",
                "v(k)",
                "v(v)",
                "i(v1)",
                "i(v3)",
                "v(z)",
            ],
            values: &[
                (
                    "frequency",
                    12,
                    6.399999999999993e+04,
                    0.000000000000000e+00,
                ),
                ("v(a)", 0, 7.037168157130438e-07, -4.064099935598489e-07),
                ("v(v)", 0, -2.208652978377869e-08, 2.175926834552194e-10),
                ("v(k)", 0, 1.396870477311623e-06, -2.013712734672575e-09),
            ],
        },
        Expectation {
            fixture: "disto_diode",
            plotname: "DISTORTION - 2nd harmonic",
            flags: PlotFlags::Complex,
            points: 13,
            variables: &[
                "frequency",
                "v(a)",
                "v(in)",
                "v(k)",
                "v(v)",
                "i(v1)",
                "i(v3)",
                "v(z)",
            ],
            values: &[
                (
                    "frequency",
                    12,
                    1.000000000000001e+07,
                    0.000000000000000e+00,
                ),
                ("v(a)", 0, -1.131140200729342e-04, 8.551395921887489e-09),
                ("v(v)", 0, -2.998291506456779e-06, 4.028113588784644e-08),
                ("v(k)", 0, 2.768941672435486e-05, -4.808024254737855e-08),
            ],
        },
        Expectation {
            fixture: "disto_diode",
            plotname: "DISTORTION - 3rd harmonic",
            flags: PlotFlags::Complex,
            points: 13,
            variables: &[
                "frequency",
                "v(a)",
                "v(in)",
                "v(k)",
                "v(v)",
                "i(v1)",
                "i(v3)",
                "v(z)",
            ],
            values: &[
                (
                    "frequency",
                    12,
                    1.000000000000001e+07,
                    0.000000000000000e+00,
                ),
                ("v(a)", 0, 3.386006688482444e-07, -1.170124059489325e-10),
                ("v(v)", 0, -2.914789341558284e-08, -2.762225164276361e-08),
                ("v(k)", 0, 5.820250319825741e-07, -2.288302876279198e-09),
            ],
        },
    ],
    &[
        // #104. Three stages share one input; the IM card runs first and both
        // cards measure 9 decade half-steps from 1 kHz to 10 MHz.
        Expectation {
            fixture: "disto_bjt",
            plotname: "DISTORTION - IM: f1+f2",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(b)",
                "v(b2)",
                "v(b3)",
                "v(c2)",
                "v(c)",
                "v(c3)",
                "v(e)",
                "v(e2)",
                "v(e3)",
                "v(in)",
                "v(sub)",
                "i(vcc)",
                "v(vcc)",
                "v(vee)",
                "i(vee)",
                "i(vin)",
                "i(vsub)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(c)", 0, -6.030982355072556e-07, -3.179317539130566e-06),
                ("v(c2)", 0, -3.177336892846194e-06, -1.521395567593330e-05),
                ("v(c3)", 0, -6.540077408335640e-08, 2.846547640065247e-06),
            ],
        },
        Expectation {
            fixture: "disto_bjt",
            plotname: "DISTORTION - IM: f1-f2",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(b)",
                "v(b2)",
                "v(b3)",
                "v(c2)",
                "v(c)",
                "v(c3)",
                "v(e)",
                "v(e2)",
                "v(e3)",
                "v(in)",
                "v(sub)",
                "i(vcc)",
                "v(vcc)",
                "v(vee)",
                "i(vee)",
                "i(vin)",
                "i(vsub)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(c)", 0, -2.885492274259324e-06, -8.335769424354971e-07),
                ("v(c2)", 0, -1.092821423043636e-05, 5.433184502114853e-06),
                ("v(c3)", 0, 4.429157685981454e-06, 4.867520314019361e-06),
            ],
        },
        Expectation {
            fixture: "disto_bjt",
            plotname: "DISTORTION - IM: 2f1-f2",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(b)",
                "v(b2)",
                "v(b3)",
                "v(c2)",
                "v(c)",
                "v(c3)",
                "v(e)",
                "v(e2)",
                "v(e3)",
                "v(in)",
                "v(sub)",
                "i(vcc)",
                "v(vcc)",
                "v(vee)",
                "i(vee)",
                "i(vin)",
                "i(vsub)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(c)", 0, 9.615947204215468e-09, -8.576384827015130e-09),
                ("v(c2)", 0, 7.108673356945696e-08, -1.001874324299550e-07),
                ("v(c3)", 0, 9.585055239707654e-09, -1.587778128009575e-08),
            ],
        },
        Expectation {
            fixture: "disto_bjt",
            plotname: "DISTORTION - 2nd harmonic",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(b)",
                "v(b2)",
                "v(b3)",
                "v(c2)",
                "v(c)",
                "v(c3)",
                "v(e)",
                "v(e2)",
                "v(e3)",
                "v(in)",
                "v(sub)",
                "i(vcc)",
                "v(vcc)",
                "v(vee)",
                "i(vee)",
                "i(vin)",
                "i(vsub)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(c)", 0, -1.930772790249303e-06, -6.177054945213964e-07),
                ("v(c2)", 0, -9.329195579941177e-06, -2.809493356476752e-06),
                ("v(c3)", 0, 1.556472574061165e-06, 8.600381781667186e-07),
            ],
        },
        Expectation {
            fixture: "disto_bjt",
            plotname: "DISTORTION - 3rd harmonic",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(b)",
                "v(b2)",
                "v(b3)",
                "v(c2)",
                "v(c)",
                "v(c3)",
                "v(e)",
                "v(e2)",
                "v(e3)",
                "v(in)",
                "v(sub)",
                "i(vcc)",
                "v(vcc)",
                "v(vee)",
                "i(vee)",
                "i(vin)",
                "i(vsub)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(c)", 0, 8.288480567294358e-09, 2.715113965852214e-09),
                ("v(c2)", 0, 4.728048186888338e-08, 1.774994698403959e-08),
                ("v(c3)", 0, 7.052947271456242e-09, 2.372880715156196e-09),
            ],
        },
    ],
    &[
        // #104. Saturated, linear-region NMOS and a PMOS stage; the IM card
        // runs first (9 points, 1 kHz to 10 MHz).
        Expectation {
            fixture: "disto_mos1",
            plotname: "DISTORTION - IM: f1+f2",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(d)",
                "v(d2)",
                "v(d3)",
                "v(g)",
                "v(in)",
                "v(s)",
                "v(s2)",
                "v(vdd)",
                "i(vdd)",
                "i(vin)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(d)", 0, -4.694173232832053e-04, -4.697188311553934e-04),
                ("v(d2)", 0, 7.793221970751433e-05, 7.798124215120448e-05),
                ("v(d3)", 0, -1.348483785725486e-04, -1.349343032894649e-04),
            ],
        },
        Expectation {
            fixture: "disto_mos1",
            plotname: "DISTORTION - IM: f1-f2",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(d)",
                "v(d2)",
                "v(d3)",
                "v(g)",
                "v(in)",
                "v(s)",
                "v(s2)",
                "v(vdd)",
                "i(vdd)",
                "i(vin)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(d)", 0, -4.695413228045661e-04, 4.695948785066036e-04),
                ("v(d2)", 0, 7.795219787455694e-05, -7.796127142538520e-05),
                ("v(d3)", 0, -1.348835944108541e-04, 1.348991006882919e-04),
            ],
        },
        Expectation {
            fixture: "disto_mos1",
            plotname: "DISTORTION - IM: 2f1-f2",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(d)",
                "v(d2)",
                "v(d3)",
                "v(g)",
                "v(in)",
                "v(s)",
                "v(s2)",
                "v(vdd)",
                "i(vdd)",
                "i(vin)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(d)", 0, -4.654637045454106e-04, 4.655183459601768e-04),
                ("v(d2)", 0, 7.604792885436849e-05, -7.605745359592716e-05),
                ("v(d3)", 0, -1.369914245777221e-04, 1.370067784069655e-04),
            ],
        },
        Expectation {
            fixture: "disto_mos1",
            plotname: "DISTORTION - 2nd harmonic",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(d)",
                "v(d2)",
                "v(d3)",
                "v(g)",
                "v(in)",
                "v(s)",
                "v(s2)",
                "v(vdd)",
                "i(vdd)",
                "i(vin)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(d)", 0, -5.533913003484259e-04, -1.461072200855848e-07),
                ("v(d2)", 0, 9.187289083226667e-05, 2.354011130150962e-08),
                ("v(d3)", 0, -1.589709739290193e-04, -4.149446415324491e-08),
            ],
        },
        Expectation {
            fixture: "disto_mos1",
            plotname: "DISTORTION - 3rd harmonic",
            flags: PlotFlags::Complex,
            points: 9,
            variables: &[
                "frequency",
                "v(d)",
                "v(d2)",
                "v(d3)",
                "v(g)",
                "v(in)",
                "v(s)",
                "v(s2)",
                "v(vdd)",
                "i(vdd)",
                "i(vin)",
            ],
            values: &[
                ("frequency", 8, 1.000000000000000e+07, 0.000000000000000e+00),
                ("v(d)", 0, 4.243186248596728e-06, 1.684584341394907e-09),
                ("v(d2)", 0, -2.241238035410588e-06, -8.634391334454325e-10),
                ("v(d3)", 0, -2.462044577208023e-06, -9.636741923212753e-10),
            ],
        },
    ],
    &[
        // #104. `.ac`, `.op` and a `lin 3` distortion sweep, which measures
        // pts + 2 = 5 frequencies from 1 kHz to 4 kHz (distoan.c).
        Expectation {
            fixture: "disto_multi",
            plotname: "AC Analysis",
            flags: PlotFlags::Complex,
            points: 5,
            variables: &[
                "frequency",
                "v(a)",
                "v(b)",
                "v(c)",
                "i(e1)",
                "v(e)",
                "v(f)",
                "v(fx)",
                "v(gx)",
                "i(h1)",
                "v(hx)",
                "v(h)",
                "v(in)",
                "i(l1)",
                "i(l2)",
                "v(sx)",
                "i(v1)",
                "i(vsense)",
                "v(wx)",
            ],
            values: &[
                ("frequency", 4, 1.000000000000000e+04, 0.000000000000000e+00),
                ("v(a)", 0, 6.631736263232155e-01, -1.887236191042314e-07),
                ("v(f)", 0, 1.118006146649348e-01, -1.055388576145894e-07),
                ("i(v1)", 0, -6.736527473535691e-04, -3.774472382084629e-10),
            ],
        },
        Expectation {
            fixture: "disto_multi",
            plotname: "Operating Point",
            flags: PlotFlags::Real,
            points: 1,
            variables: &[
                "v(in)",
                "v(a)",
                "v(b)",
                "v(c)",
                "v(e)",
                "i(e1)",
                "v(f)",
                "v(fx)",
                "v(gx)",
                "v(h)",
                "i(h1)",
                "v(hx)",
                "i(l1)",
                "i(l2)",
                "v(sx)",
                "i(v1)",
                "i(vsense)",
                "v(wx)",
            ],
            values: &[
                ("v(a)", 0, 4.516335655959562e-01, 0.0),
                ("v(f)", 0, 6.222834518836875e-01, 0.0),
                ("i(v1)", 0, -4.967328688080878e-04, 0.0),
            ],
        },
        Expectation {
            fixture: "disto_multi",
            plotname: "DISTORTION - 2nd harmonic",
            flags: PlotFlags::Complex,
            points: 5,
            variables: &[
                "frequency",
                "v(a)",
                "v(b)",
                "v(c)",
                "i(e1)",
                "v(e)",
                "v(f)",
                "v(fx)",
                "v(gx)",
                "i(h1)",
                "v(hx)",
                "v(h)",
                "v(in)",
                "i(l1)",
                "i(l2)",
                "v(sx)",
                "i(v1)",
                "i(vsense)",
                "v(wx)",
            ],
            values: &[
                ("frequency", 4, 4.000000000000000e+03, 0.000000000000000e+00),
                ("v(a)", 0, -8.352045825756337e-06, -2.235752136036530e-10),
                ("v(f)", 0, -4.566159373940503e-05, 8.546439719488322e-10),
                ("i(v1)", 0, -1.670409165151267e-08, -4.471504272073060e-13),
            ],
        },
        Expectation {
            fixture: "disto_multi",
            plotname: "DISTORTION - 3rd harmonic",
            flags: PlotFlags::Complex,
            points: 5,
            variables: &[
                "frequency",
                "v(a)",
                "v(b)",
                "v(c)",
                "i(e1)",
                "v(e)",
                "v(f)",
                "v(fx)",
                "v(gx)",
                "i(h1)",
                "v(hx)",
                "v(h)",
                "v(in)",
                "i(l1)",
                "i(l2)",
                "v(sx)",
                "i(v1)",
                "i(vsense)",
                "v(wx)",
            ],
            values: &[
                ("frequency", 4, 4.000000000000000e+03, 0.000000000000000e+00),
                ("v(a)", 0, -7.033690630958422e-07, 5.057062444351502e-13),
                ("v(f)", 0, 1.051169858760331e-06, -3.136298648165327e-11),
                ("i(v1)", 0, -1.406738126191684e-09, 1.011412488870300e-15),
            ],
        },
    ],
];

fn check_plot(expectation: &Expectation, plot: &ngspice_rs::analysis::Plot) {
    assert_eq!(
        plot.plotname, expectation.plotname,
        "{}: plot name",
        expectation.fixture
    );
    assert_eq!(
        plot.flags, expectation.flags,
        "{}: flags",
        expectation.fixture
    );
    assert_eq!(
        plot.point_count(),
        expectation.points,
        "{}: point count",
        expectation.fixture
    );
    let names: Vec<&str> = plot
        .variables
        .iter()
        .map(|variable| variable.name.as_str())
        .collect();
    assert_eq!(
        names, expectation.variables,
        "{}: variables, in rawfile order",
        expectation.fixture
    );

    for &(variable, point, re, im) in expectation.values {
        let actual = plot
            .value(variable, point)
            .unwrap_or_else(|| panic!("{}: no '{variable}'", expectation.fixture));
        let expected = Complex::new(re, im);
        assert!(
            approx_eq(actual.re, expected.re, 1e-15) && approx_eq(actual.im, expected.im, 1e-15),
            "{}: {variable}[{point}] is {actual}, expected {expected}",
            expectation.fixture
        );
    }
}

#[test]
fn fixtures_have_the_expected_shape_and_values() {
    let discovered = fixture_names();
    let mut documented: Vec<String> = EXPECTATIONS
        .iter()
        .map(|expectation| expectation.fixture.to_owned())
        .chain(
            MULTI_EXPECTATIONS
                .iter()
                .map(|plots| plots[0].fixture.to_owned()),
        )
        .collect();
    documented.sort();
    assert_eq!(
        discovered, documented,
        "every fixture must be documented here, so that new goldens get read by a human"
    );

    for expectation in EXPECTATIONS {
        let rawfile = RawFile::parse(&golden_text(expectation.fixture)).expect("parses");
        assert_eq!(
            rawfile.len(),
            1,
            "{}: expected a single plot",
            expectation.fixture
        );
        check_plot(expectation, &rawfile.plots[0].plot);
    }
    for plots in MULTI_EXPECTATIONS {
        let fixture = plots[0].fixture;
        let rawfile = RawFile::parse(&golden_text(fixture)).expect("parses");
        assert_eq!(rawfile.len(), plots.len(), "{fixture}: plot count");
        for (expectation, raw_plot) in plots.iter().zip(&rawfile.plots) {
            assert_eq!(expectation.fixture, fixture);
            check_plot(expectation, &raw_plot.plot);
        }
    }
}

/// A multi-plot golden survives the ASCII and binary writers with its plot
/// order, headers and every value intact.
#[test]
fn multi_plot_goldens_round_trip_through_both_encodings() {
    for plots in MULTI_EXPECTATIONS {
        let fixture = plots[0].fixture;
        let rawfile = RawFile::parse(&golden_text(fixture)).expect("parses");
        let ascii = RawFile::parse(&rawfile.to_ascii()).expect("the ASCII form parses");
        assert_eq!(ascii, rawfile, "{fixture}: ASCII round trip");
        let binary = RawFile::parse_bytes(&rawfile.to_binary().expect("binary encodes"))
            .expect("the binary form parses");
        assert_eq!(binary.len(), rawfile.len(), "{fixture}: binary plot count");
        for (got, want) in binary.plots.iter().zip(&rawfile.plots) {
            assert_eq!(got.title, want.title, "{fixture}");
            assert_eq!(got.command, want.command, "{fixture}");
            assert_eq!(got.plot.plotname, want.plot.plotname, "{fixture}");
            assert_eq!(got.plot.flags, want.plot.flags, "{fixture}");
            assert_eq!(got.plot.point_count(), want.plot.point_count());
            for variable in &want.plot.variables {
                assert_eq!(
                    got.plot.column(&variable.name),
                    want.plot.column(&variable.name),
                    "{fixture}: '{}' survives the binary round trip bit for bit",
                    variable.name
                );
            }
        }
    }
}

/// The transient golden is checked against the closed-form solution, not just
/// against recorded numbers: a golden that is subtly wrong would still be
/// self-consistent.
#[test]
fn the_transient_golden_matches_the_analytic_rc_curve() {
    let rawfile = RawFile::parse(&golden_text("rc_transient")).expect("parses");
    let plot = &rawfile.plots[0].plot;
    // R = 1 k, C = 1 n, driven by a 5 V step.
    let tau = 1e-6;
    let mut worst: Real = 0.0;
    for point in 0..plot.point_count() {
        let time = plot.value("time", point).expect("time").re;
        let expected = 5.0 * (1.0 - (-time / tau).exp());
        let actual = plot.value("v(out)", point).expect("v(out)").re;
        worst = worst.max((actual - expected).abs() / 5.0);
    }
    assert!(
        worst < 1e-2,
        "the transient golden deviates from 5(1 - e^(-t/tau)) by {worst} of full scale"
    );
}

/// The diode golden must be monotonic in the sweep, and the current must agree
/// with the resistor it flows through. Both are cheap invariants that catch a
/// golden being captured from the wrong deck.
#[test]
fn the_diode_golden_is_self_consistent() {
    let rawfile = RawFile::parse(&golden_text("diode_dc")).expect("parses");
    let plot = &rawfile.plots[0].plot;
    let mut previous = Real::NEG_INFINITY;
    for point in 0..plot.point_count() {
        let swept = plot.value("v(v-sweep)", point).expect("sweep").re;
        let output = plot.value("v(out)", point).expect("v(out)").re;
        let current = plot.value("i(v1)", point).expect("i(v1)").re;
        assert!(swept > previous, "the sweep must increase");
        previous = swept;
        // v(out) rises with the sweep and never exceeds the source. At zero
        // sweep it is not exactly zero: the diode's junction leakage leaves
        // 7.3e-29 V at the tap, so the bounds carry a slack rather than being
        // exactly [0, swept].
        const SLACK: Real = 1e-15;
        assert!(
            output <= swept + SLACK,
            "v(out) = {output} above the sweep {swept}"
        );
        assert!(output >= -SLACK, "v(out) = {output} is negative");
        // The diode current is the current through the 1 k resistor.
        let expected_current = -(swept - output) / 1000.0;
        assert!(
            approx_eq(current, expected_current, 1e-9),
            "i(v1) = {current}, expected {expected_current} at {swept} V"
        );
    }
}

/// `maxord` 3..=6 cannot change ngspice's transient (#98): `dctran.c` only
/// ever raises the integration order from 1 to 2, so the C rawfile of the
/// Gear `maxord=6` deck holds exactly the values of the default (`maxord=2`)
/// deck, digit for digit.
#[test]
fn gear_maxord6_golden_is_the_gear2_golden() {
    let maxord6 = RawFile::parse(&golden_text("rlc_series_gear_maxord6_tran")).expect("parses");
    let gear2 = RawFile::parse(&golden_text("rlc_series_gear_tran")).expect("parses");
    assert_eq!(maxord6.plots[0].plot, gear2.plots[0].plot);
}
