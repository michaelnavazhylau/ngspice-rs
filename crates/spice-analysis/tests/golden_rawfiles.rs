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
#[test]
fn production_linear_drivers_match_c_goldens() {
    for name in ["rc_divider", "rlc_series", "rc_lowpass_ac"] {
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

/// Exactly one token in the whole corpus cannot be re-derived from its own
/// spelling. Pinning it keeps the tolerance above bounded: if a re-capture
/// introduces more of them, this test fails and somebody looks.
#[test]
fn exactly_one_golden_token_is_not_round_trippable() {
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
    assert_eq!(offenders.len(), 1, "{offenders:#?}");
    assert!(
        offenders[0].starts_with("rc_transient:19 "),
        "{offenders:#?}"
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
];

#[test]
fn fixtures_have_the_expected_shape_and_values() {
    let discovered = fixture_names();
    let documented: Vec<String> = EXPECTATIONS
        .iter()
        .map(|expectation| expectation.fixture.to_owned())
        .collect();
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
        let plot = &rawfile.plots[0].plot;

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
                approx_eq(actual.re, expected.re, 1e-15)
                    && approx_eq(actual.im, expected.im, 1e-15),
                "{}: {variable}[{point}] is {actual}, expected {expected}",
                expectation.fixture
            );
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
