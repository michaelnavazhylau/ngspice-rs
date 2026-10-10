//! `.sp` S-parameter analysis and RF port sources (GitHub #105).
//!
//! Analytic two-port checks (series and shunt resistors, matched attenuators,
//! a lossy RC network against closed-form Z/Y/S, a one-port load), the port
//! source in `.op`/`.ac`/`.tran`, C's `vsrcpar.c`/`vsrctemp.c` setter and
//! numbering rules, batch order and every explicit rejection. The committed C
//! goldens `sp_attenuator`, `sp_rc` and `sp_multi` are compared by
//! `cargo xtask golden verify`; `tests/c_sparam_reference.rs` compares against
//! a live C binary.
use std::path::Path;

use ngspice_rs::analysis::batch::schedule;
use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, Complex, Real, SpiceError, SpiceResult};

/// Runs every analysis of `deck` in batch order, as `spice-rs simulate` does.
fn simulate(deck: &str) -> SpiceResult<Vec<Plot>> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(Path::new("sp.cir"), deck))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let mut plots = Vec::new();
    for entry in schedule(&netlist.analyses) {
        let request = config.request_for(&netlist.analyses[entry.card_index])?;
        let mut circuit = config.circuit(&netlist)?;
        plots.push(runner(request.kind)?.run(&mut circuit, &request, &config.context())?);
    }
    Ok(plots)
}

fn sp(body: &str, card: &str) -> SpiceResult<Plot> {
    let mut plots = simulate(&format!("sp test\n{body}\n{card}\n.end\n"))?;
    assert_eq!(plots.len(), 1);
    Ok(plots.remove(0))
}

fn value(plot: &Plot, name: &str, point: usize) -> Complex {
    plot.value(name, point)
        .unwrap_or_else(|| panic!("no {name}[{point}]"))
}

#[track_caller]
fn close(got: Complex, want: Complex) {
    let bound = 1e-12 * want.magnitude() + 1e-14;
    assert!(
        (got - want).magnitude() <= bound,
        "{got} != {want} (bound {bound})"
    );
}

#[track_caller]
fn close_real(plot: &Plot, name: &str, point: usize, want: Real) {
    close(value(plot, name, point), Complex::real(want));
}

/// Two-port `[[a, b], [c, d]]` algebra for the closed forms.
type M2 = [[Complex; 2]; 2];

fn inverse(m: M2) -> M2 {
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    [
        [m[1][1] / det, -m[0][1] / det],
        [-m[1][0] / det, m[0][0] / det],
    ]
}

fn product(a: M2, b: M2) -> M2 {
    let mut out = [[Complex::ZERO; 2]; 2];
    for i in 0..2 {
        for j in 0..2 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j];
        }
    }
    out
}

/// Power-wave S from Z with real reference impedances `r`:
/// `S_ij = sqrt(r_j / r_i) [(Z - R)(Z + R)^-1]_ij`.
fn s_from_z(z: M2, r: [Real; 2]) -> M2 {
    let mut minus = z;
    let mut plus = z;
    for i in 0..2 {
        minus[i][i] = minus[i][i] - Complex::real(r[i]);
        plus[i][i] = plus[i][i] + Complex::real(r[i]);
    }
    let m = product(minus, inverse(plus));
    let mut s = m;
    for i in 0..2 {
        for j in 0..2 {
            s[i][j] = m[i][j] * Complex::real((r[j] / r[i]).sqrt());
        }
    }
    s
}

#[track_caller]
fn matrix(plot: &Plot, prefix: &str, point: usize, want: M2) {
    for (i, row) in want.iter().enumerate() {
        for (j, entry) in row.iter().enumerate() {
            close(
                value(plot, &format!("{prefix}_{}_{}", i + 1, j + 1), point),
                *entry,
            );
        }
    }
}

#[test]
fn the_plot_has_c_names_units_and_order() {
    let plot = sp(
        "v1 1 0 dc 0 ac 1 portnum 1 z0 50\nr1 1 2 50\nv2 2 0 dc 0 portnum 2 z0 50",
        ".sp lin 3 1k 1meg",
    )
    .unwrap();
    assert_eq!(plot.plotname, "SP Analysis");
    assert!(plot.flags.is_complex());
    let names: Vec<_> = plot
        .variables
        .iter()
        .map(|v| (v.name.as_str(), v.unit.as_str(), v.is_real))
        .collect();
    assert_eq!(
        names,
        [
            ("frequency", "frequency", false),
            ("v(1)", "voltage", false),
            ("v(v1#res)", "voltage", false),
            ("v(2)", "voltage", false),
            ("v(v2#res)", "voltage", false),
            ("i(v1)", "current", false),
            ("i(v2)", "current", false),
            ("S_1_1", "s-param", false),
            ("S_1_2", "s-param", false),
            ("S_2_1", "s-param", false),
            ("S_2_2", "s-param", false),
            ("Y_1_1", "admittance", false),
            ("Y_1_2", "admittance", false),
            ("Y_2_1", "admittance", false),
            ("Y_2_2", "admittance", false),
            ("Z_1_1", "impedance", false),
            ("Z_1_2", "impedance", false),
            ("Z_2_1", "impedance", false),
            ("Z_2_2", "impedance", false),
            ("v(Rbase)", "voltage", false),
        ]
    );
    assert_eq!(plot.point_count(), 3);
    close_real(&plot, "frequency", 1, 500.5e3);
    // The rawfile writer and reader keep the S/Y/Z vectors and their types.
    let text = RawFile::single(plot.clone()).to_ascii();
    let back = RawFile::parse(&text).unwrap();
    let back = back.single_plot().unwrap();
    assert_eq!(back.variables, plot.variables);
    assert_eq!(back.points, plot.points);
}

#[test]
fn a_series_resistor_matches_closed_form_and_has_no_z_matrix() {
    let r = 25.;
    let plot = sp(
        &format!("v1 1 0 dc 0 ac 1 portnum 1\nr1 1 2 {r}\nv2 2 0 dc 0 ac 0 portnum 2"),
        ".sp dec 2 1k 100k",
    )
    .unwrap();
    for point in 0..plot.point_count() {
        let s11 = Complex::real(r / (r + 100.));
        let s21 = Complex::real(100. / (r + 100.));
        matrix(&plot, "S", point, [[s11, s21], [s21, s11]]);
        let g = Complex::real(1. / r);
        matrix(&plot, "Y", point, [[g, -g], [-g, g]]);
        // E - S is singular: C's cinverse zero-fills an exactly singular
        // matrix (its rounding-level pivots give huge values instead).
        matrix(&plot, "Z", point, [[Complex::ZERO; 2]; 2]);
        close_real(&plot, "v(Rbase)", point, 50.);
        // The node vectors are port 2's excitation: 1 V behind 50 + 25 + 50.
        close_real(&plot, "v(2)", point, 1. - 50. / 125.);
        close_real(&plot, "v(1)", point, 50. / 125.);
        close_real(&plot, "v(v2#res)", point, 1.);
        close_real(&plot, "v(v1#res)", point, 0.);
        close_real(&plot, "i(v2)", point, -1. / 125.);
        close_real(&plot, "i(v1)", point, 1. / 125.);
    }
}

#[test]
fn a_shunt_resistor_matches_closed_form_and_has_no_y_matrix() {
    let r = 100.;
    let plot = sp(
        &format!("v1 a 0 dc 0 portnum 1 z0 50\nr1 a 0 {r}\nv2 a 0 dc 0 portnum 2 z0 50"),
        ".sp lin 1 1meg 1meg",
    )
    .unwrap();
    let s11 = Complex::real(-50. / (2. * r + 50.));
    let s21 = Complex::real(2. * r / (2. * r + 50.));
    matrix(&plot, "S", 0, [[s11, s21], [s21, s11]]);
    let z = Complex::real(r);
    matrix(&plot, "Z", 0, [[z, z], [z, z]]);
    matrix(&plot, "Y", 0, [[Complex::ZERO; 2]; 2]);
}

#[test]
fn matched_attenuators_are_reflectionless_with_gain_one_over_k() {
    // T pad for reference impedance z0 and voltage ratio K.
    for (z0, k) in [(50., 3.), (75., 2.), (50., 10.)] {
        let series = z0 * (k - 1.) / (k + 1.);
        let shunt = 2. * z0 * k / (k * k - 1.);
        let plot = sp(
            &format!(
                "v1 in 0 dc 0 portnum 1 z0 {z0}\nr1 in mid {series}\nr2 mid 0 {shunt}\n\
                 r3 mid out {series}\nv2 out 0 dc 0 portnum 2 z0 {z0}"
            ),
            ".sp oct 1 1k 4k",
        )
        .unwrap();
        assert_eq!(plot.point_count(), 3);
        let (zero, through) = (Complex::ZERO, Complex::real(1. / k));
        for point in 0..3 {
            for (name, want) in [
                ("S_1_1", zero),
                ("S_2_2", zero),
                ("S_2_1", through),
                ("S_1_2", through),
            ] {
                let got = value(&plot, name, point);
                assert!((got - want).magnitude() < 1e-14, "{name}: {got} != {want}");
            }
            let z: M2 = [
                [Complex::real(series + shunt), Complex::real(shunt)],
                [Complex::real(shunt), Complex::real(series + shunt)],
            ];
            matrix(&plot, "Z", point, z);
            matrix(&plot, "Y", point, inverse(z));
            close_real(&plot, "v(Rbase)", point, z0);
        }
    }
}

#[test]
fn a_lossy_rc_network_with_unequal_ports_matches_closed_form() {
    let (r1, r2, rp, c) = (100., 20., 1e3, 1e-9);
    let plot = sp(
        &format!(
            "v1 in 0 dc 0 ac 1 portnum 1 z0 50\nr1 in mid {r1}\nc1 mid 0 {c}\nrp mid 0 {rp}\n\
             r2 mid out {r2}\nv2 out 0 dc 0 ac 0 portnum 2 z0 75"
        ),
        ".sp dec 4 10k 10meg",
    )
    .unwrap();
    assert_eq!(plot.point_count(), 13);
    for point in 0..plot.point_count() {
        let f = value(&plot, "frequency", point).re;
        let shunt = Complex::real(1.)
            / (Complex::real(1. / rp) + Complex::imaginary(2. * std::f64::consts::PI * f * c));
        let z: M2 = [
            [Complex::real(r1) + shunt, shunt],
            [shunt, Complex::real(r2) + shunt],
        ];
        matrix(&plot, "Z", point, z);
        matrix(&plot, "Y", point, inverse(z));
        let s = s_from_z(z, [50., 75.]);
        matrix(&plot, "S", point, s);
        // Reciprocal network: S12 = S21 even with unequal references.
        close(value(&plot, "S_1_2", point), value(&plot, "S_2_1", point));
        close_real(&plot, "v(Rbase)", point, 50.);
    }
}

#[test]
fn a_one_port_reflects_its_load() {
    let plot = sp(
        "v1 1 0 dc 0 portnum 1 z0 50\nr1 1 2 25\nc1 2 0 1n",
        ".sp dec 1 1e5 1e7",
    )
    .unwrap();
    assert!(plot.variable_index("S_1_2").is_none());
    for point in 0..plot.point_count() {
        let f = value(&plot, "frequency", point).re;
        let load = Complex::real(25.)
            + Complex::real(1.) / Complex::imaginary(2e-9 * std::f64::consts::PI * f);
        close(
            value(&plot, "S_1_1", point),
            (load - Complex::real(50.)) / (load + Complex::real(50.)),
        );
        close(value(&plot, "Z_1_1", point), load);
        close(value(&plot, "Y_1_1", point), Complex::real(1.) / load);
    }
}

#[test]
fn ports_are_ordered_by_number_not_by_deck_order() {
    let plot = sp(
        "vb b 0 dc 0 portnum 2 z0 75\nra a 0 100\nrb b 0 300\nva a 0 dc 0 portnum 1 z0 50",
        ".sp lin 1 1k 1k",
    )
    .unwrap();
    // Two isolated loads: port 1 sees 100 ohm against 50, port 2 300 against 75.
    close_real(&plot, "S_1_1", 0, 50. / 150.);
    close_real(&plot, "S_2_2", 0, 225. / 375.);
    close_real(&plot, "S_1_2", 0, 0.);
    close_real(&plot, "v(Rbase)", 0, 50.);
    close_real(&plot, "Z_2_2", 0, 300.);
}

#[test]
fn the_ac_excitation_of_ordinary_sources_is_off() {
    let plain = "v1 1 0 dc 0 portnum 1\nr1 1 2 50\nr2 2 0 100\nv2 2 0 dc 0 portnum 2";
    let reference = sp(plain, ".sp lin 2 1k 2k").unwrap();
    // A non-port V source with an AC phasor and a current source with a real
    // one change nothing (MODESP loads no V excitation; span.c drops the real
    // RHS of current sources).
    let driven = sp(
        &format!("{plain}\nvx 3 0 dc 0 ac 1 45\nrx 3 2 1k\nix 0 2 dc 0 ac 2"),
        ".sp lin 2 1k 2k",
    )
    .unwrap();
    // rx loads node 2, so compare against the same network without phasors.
    let quiet = sp(
        &format!("{plain}\nvx 3 0 dc 0\nrx 3 2 1k\nix 0 2 dc 0"),
        ".sp lin 2 1k 2k",
    )
    .unwrap();
    assert_eq!(driven.points, quiet.points);
    assert_ne!(reference.points, quiet.points);
}

#[test]
fn port_sources_are_thevenin_sources_in_op_ac_and_tran() {
    let plots = simulate(
        "ports everywhere\n\
         v1 in 0 dc 1 ac 1 portnum 1 z0 50\nr1 in out 50\nr2 out 0 100\n\
         v2 out 0 dc 0 portnum 2 z0 75\n.op\n.ac lin 1 1k 1k\n.tran 1u 5u\n.sp lin 1 1k 1k\n.end\n",
    )
    .unwrap();
    let names: Vec<_> = plots.iter().map(|plot| plot.plotname.as_str()).collect();
    assert_eq!(
        names,
        [
            "AC Analysis",
            "Operating Point",
            "Transient Analysis",
            "SP Analysis"
        ]
    );
    // 1 V behind 50 ohm, 50 ohm, then 100 || 75 to ground.
    let load = 100. * 75. / 175.;
    let current = 1. / (100. + load);
    for (plot, point) in [(&plots[0], 0), (&plots[1], 0), (&plots[2], 5)] {
        close_real(plot, "v(in)", point, 1. - 50. * current);
        close_real(plot, "v(out)", point, load * current);
        close_real(plot, "v(v1#res)", point, 1.);
        close_real(plot, "i(v1)", point, -current);
        close_real(plot, "v(v2#res)", point, 0.);
        close_real(plot, "i(v2)", point, load * current / 75.);
    }
    let names: Vec<_> = schedule(
        &Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("sp.cir"),
                "x\nv1 1 0 portnum 1\n.sp lin 1 1 1\n.op\n.sp lin 1 2 2\n.ac lin 1 1 1\n.end\n",
            ))
            .unwrap()
            .analyses,
    )
    .into_iter()
    .map(|entry| (entry.kind, entry.plot_name))
    .collect();
    assert_eq!(
        names,
        [
            (AnalysisKind::Ac, "ac1".to_owned()),
            (AnalysisKind::OperatingPoint, "op1".to_owned()),
            (AnalysisKind::SParameter, "sp1".to_owned()),
            (AnalysisKind::SParameter, "sp2".to_owned()),
        ]
    );
}

#[test]
fn port_setters_follow_c_order_and_defaults() {
    let op = |source: &str| sp(&format!("{source}\nr1 1 0 50"), ".op");
    // Default z0 = 50; portnum rounds like INPgetValue(IF_INTEGER).
    for source in [
        "v1 1 0 dc 1 portnum 1",
        "v1 1 0 dc 1 portnum=1.4 z0=50",
        "v1 1 0 dc 1 z0 0 portnum 1",
        "v1 1 0 dc 1 z0 -3 portnum 1",
        "v1 1 0 dc 1 portnum 1 phase 30",
    ] {
        let plot = op(source).unwrap();
        close_real(&plot, "v(1)", 0, 0.5);
        close_real(&plot, "v(v1#res)", 0, 1.);
    }
    let plot = op("v1 1 0 dc 1 portnum 1 z0 150").unwrap();
    close_real(&plot, "v(1)", 0, 0.25);
    // Not a port: z0 set to zero after portnum, portnum zero, or no portnum.
    for source in [
        "v1 1 0 dc 1 portnum 1 z0 0",
        "v1 1 0 dc 1 portnum 0",
        "v1 1 0 dc 1 z0 75",
        "v1 1 0 dc 1 phase 10",
    ] {
        let plot = op(source).unwrap();
        close_real(&plot, "v(1)", 0, 1.);
        assert!(plot.variable_index("v(v1#res)").is_none(), "{source}");
    }
}

fn message(result: SpiceResult<impl std::fmt::Debug>) -> (bool, String) {
    let error = result.expect_err("must fail");
    (error.is_not_yet_ported(), error.to_string())
}

#[test]
fn port_numbering_must_be_one_to_n_in_every_analysis() {
    for (body, expected) in [
        ("v1 1 0 portnum 2\nr1 1 0 1", "incorrect port ordering"),
        (
            "v1 1 0 portnum 1\nv2 2 0 portnum 1\nr1 1 2 1",
            "duplicate port Index",
        ),
        (
            "v1 1 0 portnum 1\nv2 2 0 portnum 3\nr1 1 2 1",
            "incorrect port ordering",
        ),
    ] {
        for card in [".op", ".sp lin 1 1k 1k"] {
            let (pending, text) = message(sp(body, card));
            assert!(!pending, "{text}");
            assert!(text.contains(expected), "{body} {card}: {text}");
        }
    }
}

#[test]
fn unsupported_sp_requests_are_explicit() {
    let ports = "v1 1 0 dc 0 portnum 1\nr1 1 2 50\nv2 2 0 dc 0 portnum 2";
    assert!(
        sp(ports, ".sp lin 2 1k 2k 1")
            .unwrap()
            .variable_index("i(Cy_1_1)")
            .is_some()
    );
    // donoise values other than 1 are C's "no noise".
    for flag in ["0", "2", "0.4"] {
        sp(ports, &format!(".sp lin 2 1k 2k {flag}")).unwrap();
    }
    for card in [
        ".sp lin 2 1k",
        ".sp lin 2 1k 2k 0 0",
        ".sp log 2 1k 2k",
        ".sp lin 0 1k 2k",
        ".sp lin 2 2k 1k",
        ".sp lin 2 1k 2k x",
    ] {
        let (pending, text) = message(sp(ports, card));
        assert!(!pending, "{card}: {text}");
    }
    let (pending, text) = message(sp("v1 1 0 dc 1\nr1 1 0 50", ".sp lin 1 1k 1k"));
    assert!(!pending && text.contains("no RF port"), "{text}");
    let (pending, text) = message(sp(&format!("{ports}\ni1 0 2 ac 1 90"), ".sp lin 1 1k 1k"));
    assert!(
        !pending && text.contains("i1") && text.contains("imaginary"),
        "{text}"
    );
    sp(ports, ".sp lin 2 1k 2k\n.meas sp m1 max v(2)").unwrap();
}

#[test]
fn the_port_power_function_runs_where_c_uses_it() {
    let tail = "r1 1 2 50\nv2 2 0 dc 0 portnum 2";
    // With an explicit DC value, pwr/freq do not affect DC or small-signal
    // analyses.
    let plain = sp(
        &format!("v1 1 0 dc 0 ac 1 portnum 1\n{tail}"),
        ".sp lin 2 1k 2k",
    )
    .unwrap();
    let powered = sp(
        &format!("v1 1 0 dc 0 ac 1 portnum 1 pwr 0.002 freq 2.4g\n{tail}"),
        ".sp lin 2 1k 2k",
    )
    .unwrap();
    assert_eq!(plain.points, powered.points);
    sp(&format!("v1 1 0 dc 1 portnum 1 pwr 1m\n{tail}"), ".op").unwrap();
    for backend in ["", " backend=diffsol method=bdf"] {
        sp(
            &format!("v1 1 0 dc 1 portnum 1 freq 1meg\nc1 1 0 1n\n{tail}"),
            &format!(".tran 1u 10u{backend}"),
        )
        .unwrap();
    }
    // A later waveform setter replaces the PORT function, as in vsrcpar.c.
    let plots = sp(
        &format!("v1 1 0 dc 0 portnum 1 pwr 1m sin(0 1 100k)\n{tail}"),
        ".tran 1u 10u",
    )
    .unwrap();
    assert!(plots.point_count() > 10);
    // Without a DC value C loads the PORT function in the operating point.
    sp(
        &format!("v1 1 0 ac 1 portnum 1 pwr 1m\n{tail}"),
        ".sp lin 1 1k 1k",
    )
    .unwrap();
    sp("v1 1 0 dc 1 freq 1k\nr1 1 0 50", ".op").unwrap();
    // Current sources have no port parameters.
    let error = sp("i1 1 0 dc 1 portnum 1\nr1 1 0 1", ".op").unwrap_err();
    assert!(matches!(error, SpiceError::NotYetPorted { .. }), "{error}");
}

#[test]
fn deck_dc_options_reach_the_sp_operating_point() {
    // `.sp` solves its operating point like `.ac`, so the same DC options apply.
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("sp.cir"),
            "opts\n.options reltol=1e-4 itl1=77 gminsteps=3\nv1 1 0 dc 1 portnum 1\n\
             r1 1 0 50\n.sp lin 1 1k 1k\n.ac lin 1 1k 1k\n.end\n",
        ))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let named = |index: usize| {
        let request = config.request_for(&netlist.analyses[index]).unwrap();
        let named: Vec<String> = request
            .arguments
            .into_iter()
            .filter(|argument| argument.contains('='))
            .collect();
        (request.kind, named)
    };
    let (kind, sp_options) = named(0);
    assert_eq!(kind, AnalysisKind::SParameter);
    assert!(
        sp_options.contains(&"rtol=1e-4".to_owned()),
        "{sp_options:?}"
    );
    assert_eq!(sp_options, named(1).1);
    let plots = simulate(
        "opts\n.options reltol=1e-4\nv1 1 0 dc 1 portnum 1\nr1 1 0 50\n.sp lin 1 1k 1k\n.end\n",
    )
    .unwrap();
    close_real(&plots[0], "S_1_1", 0, 0.);
}

#[test]
fn reactive_ladder_runs_ac_and_sp_across_the_residual_regression_band() {
    let body = "v1 in 0 dc 0 ac 1 portnum 1 z0 50\nc1 in 0 318.3n\nl1 in out 1.592m\nc2 out 0 318.3n\nv2 out 0 dc 0 ac 0 portnum 2 z0 50";
    for kind in ["ac", "sp"] {
        let plot = sp(body, &format!(".{kind} dec 100 1 1e6")).unwrap();
        assert_eq!(plot.point_count(), 601);
        assert!(plot.points.iter().flatten().all(|v| v.is_finite()));
    }
}

#[test]
fn port_cosines_accumulate_in_c_load_order_on_both_backends() {
    let body = "vnon n 0 freq 10\nv1 a 0 portnum 1 pwr 2m freq 2k phase 73\nv2 b 0 dc 0.2 portnum 2 pwr 1m freq 1k\nvbase base 0 pwl(0 0.2 100u 0.3)\nrn n 0 100\nr1 a 0 50\nr2 b 0 50\nrb base 0 100\nc1 a 0 1u";
    let bias = sp(body, ".op").unwrap();
    close(
        value(&bias, "v(v1#res)", 0),
        Complex::real(0.2 + (0.4_f64).sqrt()),
    );
    close(value(&bias, "v(n)", 0), value(&bias, "v(v1#res)", 0));
    for backend in ["", " backend=diffsol method=bdf"] {
        let plot = sp(body, &format!(".tran 1u 100u{backend}")).unwrap();
        for point in 0..plot.point_count() {
            let t = value(&plot, "time", point).re;
            let baseline = 0.2 + 1000. * t;
            let two = baseline + 0.2_f64.sqrt() * (2. * std::f64::consts::PI * 1e3 * t).cos();
            let one = two + 0.4_f64.sqrt() * (2. * std::f64::consts::PI * 2e3 * t).cos();
            for (name, expected) in [("v(v2#res)", two), ("v(v1#res)", one), ("v(n)", one)] {
                let got = value(&plot, name, point).re;
                assert!(
                    (got - expected).abs() < 1e-9,
                    "{backend} {name} t={t}: {got} != {expected}"
                );
            }
        }
    }
}
