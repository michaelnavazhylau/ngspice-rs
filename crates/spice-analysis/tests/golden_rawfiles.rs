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

use spice_analysis::RawFile;
use spice_analysis::results::PlotFlags;
use spice_core::{Complex, Real, approx_eq, format_spice_number};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
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
        let deck = spice_netlist::source::parse_deck_text(Path::new(name), &netlist_text(name));
        let netlist = spice_netlist::Parser::new().parse_deck(&deck).unwrap();
        let mut circuit = spice_devices::Circuit::from_netlist(&netlist).unwrap();
        let request = spice_analysis::AnalysisRequest::from(&netlist.analyses[0]);
        let got = spice_analysis::runner(request.kind)
            .unwrap()
            .run(
                &mut circuit,
                &request,
                &spice_analysis::AnalysisContext::default(),
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
    },
];

/// Multi-analysis fixtures (#96): one [`Expectation`] per plot, in rawfile
/// order, which is ngspice's batch order (`.ac`, `.dc`, `.op`, `.tran`), not
/// the deck order.
#[allow(clippy::excessive_precision)]
const MULTI_EXPECTATIONS: &[&[Expectation]] = &[&[
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
]];

fn check_plot(expectation: &Expectation, plot: &spice_analysis::Plot) {
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
