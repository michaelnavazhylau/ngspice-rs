//! `.tf` DC transfer-function analysis (GitHub #101) through the production
//! driver: closed-form linear results (dividers, current-source inputs,
//! controlled-source amplifiers, C's same-source and open-circuit rules),
//! nonlinear bias points checked against central differences of `.op`
//! solutions, multi-analysis batch composition and explicit failures.
//!
//! The committed C goldens `m8_tf_*` are compared by `cargo xtask golden
//! verify`; `tests/c_tf_reference.rs` compares further decks with live C.
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, batch, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, SpiceError, SpiceResult};

/// Every analysis of `body` in ngspice batch order, each on a fresh circuit,
/// as `spice-rs simulate` runs them, with C's in-memory plot names.
fn run_all(body: &str) -> SpiceResult<Vec<(String, Plot)>> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("tf.cir"),
        &format!("tf test\n{body}\n.end\n"),
    ))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let mut plots = Vec::new();
    for entry in batch::schedule(&netlist.analyses) {
        let request = config.request_for(&netlist.analyses[entry.card_index])?;
        let mut circuit = config.circuit(&netlist)?;
        let plot = runner(request.kind)?.run(&mut circuit, &request, &config.context())?;
        plots.push((entry.plot_name, plot));
    }
    Ok(plots)
}

/// The single `.tf` plot of `body` with `card` appended.
fn tf(body: &str, card: &str) -> SpiceResult<Plot> {
    let mut plots = run_all(&format!("{body}\n{card}"))?;
    assert_eq!(plots.len(), 1);
    Ok(plots.remove(0).1)
}

/// `(transfer, input resistance, output resistance)` in C's column order.
fn values(plot: &Plot) -> [f64; 3] {
    assert_eq!(plot.plotname, "Transfer Function");
    assert_eq!(plot.point_count(), 1);
    assert_eq!(plot.variable_count(), 3);
    assert!(
        plot.variables
            .iter()
            .all(|v| v.unit == "voltage" && v.is_real)
    );
    assert!(plot.points[0].iter().all(|v| v.im == 0.));
    [0, 1, 2].map(|column| plot.points[0][column].re)
}

fn names(plot: &Plot) -> Vec<&str> {
    plot.variables.iter().map(|v| v.name.as_str()).collect()
}

fn close(got: f64, want: f64, relative: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs() + 1e-12,
        "{got} != {want}"
    );
}

fn parallel(a: f64, b: f64) -> f64 {
    a * b / (a + b)
}

const DIVIDER: &str = "v1 in 0 dc 10\nr1 in out 1k\nr2 out 0 3k";

#[test]
fn a_voltage_divider_has_closed_form_gain_and_resistances() {
    let plot = tf(DIVIDER, ".tf v(out) v1").unwrap();
    assert_eq!(
        names(&plot),
        [
            "v(Transfer_function)",
            "v(v1#Input_impedance)",
            "v(output_impedance_at_V(out))"
        ]
    );
    let [gain, rin, rout] = values(&plot);
    close(gain, 0.75, 1e-14);
    close(rin, 4e3, 1e-14);
    close(rout, parallel(1e3, 3e3), 1e-14);
}

#[test]
fn reactive_elements_are_their_dc_limits() {
    // l1 is a short and c1 open at DC, so this is the divider again.
    let body = "v1 in 0 dc 10 ac 1\nr1 in a 1k\nl1 a out 1m\nr2 out 0 3k\nc1 out 0 1u";
    let [gain, rin, rout] = values(&tf(body, ".tf v(out) v1").unwrap());
    close(gain, 0.75, 1e-14);
    close(rin, 4e3, 1e-14);
    close(rout, 750., 1e-14);
}

#[test]
fn differential_outputs_and_ground_aliases_follow_c() {
    // v(in,out) across r1: the input source is ideal, so the port looks into
    // r1 || r2 from either end: out -> in sees r1 || r2.
    let plot = tf(DIVIDER, ".tf V(In, OUT) v1").unwrap();
    assert_eq!(names(&plot)[2], "v(output_impedance_at_V(in,out))");
    let [gain, rin, rout] = values(&plot);
    close(gain, 0.25, 1e-14);
    close(rin, 4e3, 1e-14);
    close(rout, 750., 1e-14);
    // `gnd` is ground (and named `0`, as C's deck preprocessing writes it).
    let plot = tf(DIVIDER, ".tf v(out,gnd) v1").unwrap();
    assert_eq!(names(&plot)[2], "v(output_impedance_at_V(out,0))");
    close(values(&plot)[0], 0.75, 1e-14);
}

#[test]
fn a_current_source_input_gives_a_transimpedance() {
    // 1 A into `in` (C: rhs[n+] -= 1, rhs[n-] += 1 for `i1 0 in`): the input
    // resistance is v(n-) - v(n+), here rp || (r1 + r2).
    let body = "i1 0 in dc 1m\nrp in 0 6k\nr1 in out 1k\nr2 out 0 2k";
    let plot = tf(body, ".tf v(out) i1").unwrap();
    assert_eq!(names(&plot)[1], "v(i1#Input_impedance)");
    let [transfer, rin, rout] = values(&plot);
    let rin_want = parallel(6e3, 3e3);
    close(rin, rin_want, 1e-14);
    close(transfer, rin_want * 2. / 3., 1e-14);
    close(rout, parallel(2e3, 7e3), 1e-14);
    // Reversing the source flips the transfer sign and the voltage the input
    // resistance is read with: the resistance stays positive.
    let body = "i1 in 0 dc 1m\nrp in 0 6k\nr1 in out 1k\nr2 out 0 2k";
    let [transfer, rin, _] = values(&tf(body, ".tf v(out) i1").unwrap());
    close(transfer, -rin_want * 2. / 3., 1e-14);
    close(rin, rin_want, 1e-14);
}

#[test]
fn current_outputs_use_the_sensing_branch() {
    // i(vm) through a zero-volt ammeter in series with r2.
    let body = "v1 in 0 dc 1\nr1 in out 1k\nr2 out m 3k\nvm m 0 0\nr3 out 0 6k";
    let plot = tf(body, ".tf i(vm) v1").unwrap();
    assert_eq!(names(&plot)[2], "v(vm#Output_impedance)");
    let [transfer, rin, rout] = values(&plot);
    let rin_want = 1e3 + parallel(3e3, 6e3);
    close(rin, rin_want, 1e-14);
    // Divider to `out`, then the share through r2.
    close(transfer, parallel(3e3, 6e3) / rin_want / 3e3, 1e-14);
    // A unit voltage in vm's branch looks into r2 + (r1 || r3), and C reports
    // -1/i with the source's own current sign: a positive resistance.
    close(rout, 3e3 + parallel(1e3, 6e3), 1e-14);
}

#[test]
fn the_input_source_current_copies_the_input_resistance() {
    // C: `TFoutIsI && TFoutSrc == TFinSrc` skips the second solve.
    let [transfer, rin, rout] = values(&tf(DIVIDER, ".tf i(V1) v1").unwrap());
    close(transfer, -1. / 4e3, 1e-14);
    close(rin, 4e3, 1e-14);
    assert_eq!(rout, rin);
}

#[test]
fn an_unloaded_input_reports_c_open_resistance() {
    // Nothing loads v1: |i| < 1e-20, so TFanal() reports 1e20 ohm.
    let body = "v1 in 0 dc 1\ne1 out 0 in 0 2\nrl out 0 1k";
    let [gain, rin, rout] = values(&tf(body, ".tf v(out) v1").unwrap());
    close(gain, 2., 1e-14);
    assert_eq!(rin, 1e20);
    // An ideal E output has zero output resistance.
    assert_eq!(rout, 0.);
    // The E branch current is findable (CKTfndBranch): a unit voltage in the
    // output branch drives 1 V into rl, i = -1 mA.
    let plot = tf(body, ".tf i(e1) v1").unwrap();
    assert_eq!(names(&plot)[2], "v(e1#Output_impedance)");
    let [transfer, _, rout] = values(&plot);
    close(transfer, -2e-3, 1e-14);
    close(rout, 1e3, 1e-14);
}

#[test]
fn a_finite_gain_non_inverting_amplifier_matches_its_closed_form() {
    let a = 1e4;
    let (rs, rin, rout, r1, r2, rl) = (1e3, 100e3, 75., 1e3, 9e3, 10e3);
    let body = format!(
        "vin in 0 dc 1\nrs in p {rs}\nrin p n {rin}\ne1 oi 0 p n {a}\nrout oi out {rout}\n\
         r1 n 0 {r1}\nr2 out n {r2}\nrl out 0 {rl}"
    );
    let [gain, input, output] = values(&tf(&body, ".tf v(out) vin").unwrap());
    // Nodal reference solution of the same network for a 1 V input.
    let solve = |vin: f64, iout: f64| {
        // Unknowns p, n, out; e1 drives oi = a (p - n) behind rout.
        let g = |r: f64| 1. / r;
        let m = [
            [g(rs) + g(rin), -g(rin), 0.],
            [-g(rin), g(rin) + g(r1) + g(r2), -g(r2)],
            [-a * g(rout), a * g(rout) - g(r2), g(r2) + g(rl) + g(rout)],
        ];
        let b = [vin * g(rs), 0., iout];
        let det = |m: [[f64; 3]; 3]| {
            m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
        };
        let mut columns = [m; 3];
        let mut x = [0.; 3];
        for (k, column) in columns.iter_mut().enumerate() {
            for (row, value) in b.iter().enumerate() {
                column[row][k] = *value;
            }
            x[k] = det(*column) / det(m);
        }
        x
    };
    let driven = solve(1., 0.);
    close(gain, driven[2], 1e-9);
    close(input, rs / (1. - driven[0]), 1e-9);
    close(output, solve(0., 1.)[2], 1e-9);
}

/// `d/dV(insrc)` of an operating-point quantity by central differences.
fn derivative(body: &str, value: f64, read: &str) -> f64 {
    let at = |v: f64| {
        let deck = body.replace("BIAS", &format!("dc {v}"));
        let plots = run_all(&format!("{deck}\n.op")).unwrap();
        plots[0].1.value(read, 0).unwrap().re
    };
    let h = 1e-4;
    (at(value + h) - at(value - h)) / (2. * h)
}

#[test]
fn nonlinear_bias_points_linearise_at_the_operating_point() {
    // A forward-biased diode with series resistance, a CE BJT stage and a
    // MOS1 common-source stage. Gains and input conductances agree with
    // central differences of the operating point (O(h^2) ~ 1e-8 relative).
    let cases = [
        (
            "vd vdin 0 BIAS\nrd vdin d 1k\nd1 d 0 dmod\n.model dmod d(is=1e-14 n=1.2 rs=5)",
            "vd",
            2.,
            "v(d)",
            "i(vd)",
        ),
        (
            "vin in 0 BIAS\nvcc vcc 0 9\nrs in nb 600\nrb1 vcc nb 82k\nrb2 nb 0 15k\n\
             rc vcc nc 3.3k\nre ne 0 330\nq1 nc nb ne 0 qamp\n\
             .model qamp npn(is=5e-16 bf=180 vaf=70 ikf=40m ise=1e-14 ne=1.5 rb=120 re=0.8 rc=15)",
            "vin",
            1.4,
            "v(nc)",
            "i(vin)",
        ),
        (
            "vg g 0 BIAS\nvdd vdd 0 5\nrdm vdd dm 10k\nm1 dm g 0 0 nmos w=10u l=2u\nrgs g 0 1meg\n\
             .model nmos nmos(level=1 vto=0.8 kp=60u lambda=0.02 gamma=0.4)",
            "vg",
            2.,
            "v(dm)",
            "i(vg)",
        ),
    ];
    for (circuit, source, bias, output, current) in cases {
        let body = format!("{circuit}\n.options reltol=1e-9");
        let plot = tf(
            &body.replace("BIAS", &format!("dc {bias}")),
            &format!(".tf {output} {source}"),
        )
        .unwrap();
        let [gain, rin, _] = values(&plot);
        let want = derivative(&body, bias, output);
        close(gain, want, 1e-6);
        let conductance = -derivative(&body, bias, current);
        close(1. / rin, conductance, 1e-6);
    }
}

#[test]
fn a_bias_dependent_base_resistance_uses_c_matrix_like_ac() {
    // RB/RBM/IRB: C's bjtload.c (and bjtacld.c) stamp the base resistance as
    // the conductance gx only, without d(gx)/dV. `.tf` solves with that
    // matrix, so its gain equals the low-frequency `.ac` gain of the same
    // capacitor-free stage, and differs measurably from the exact derivative
    // the port's Newton Jacobian would give.
    let body = "vin in 0 BIAS ac 1\nvcc vcc 0 9\nrs in nb 600\nrb1 vcc nb 82k\nrb2 nb 0 15k\n\
                rc vcc nc 3.3k\nre ne 0 330\nq1 nc nb ne 0 qamp\n\
                .model qamp npn(is=5e-16 bf=180 vaf=70 ikf=40m rb=500 rbm=12 irb=0.1m)\n\
                .options reltol=1e-9";
    let deck = body.replace("BIAS", "dc 1.4");
    let plots = run_all(&format!("{deck}\n.tf v(nc) vin\n.ac lin 1 1 1")).unwrap();
    let (ac, tf) = (&plots[0].1, &plots[1].1);
    let [gain, _, _] = values(tf);
    let low_frequency = ac.value("v(nc)", 0).unwrap();
    close(gain, low_frequency.re, 1e-10);
    assert_eq!(low_frequency.im, 0.);
    let exact = derivative(body, 1.4, "v(nc)");
    assert!(
        (gain - exact).abs() > 1e-5 * exact.abs(),
        "gx-only gain {gain} should differ from the exact derivative {exact}"
    );
}

#[test]
fn tf_plots_compose_with_other_analyses_in_batch_order() {
    // ngspice runs .op before .tf whatever the deck order, and two .tf cards
    // in reverse deck order: op1, tf1 (the i(v1) card), tf2.
    let deck = format!("{DIVIDER}\n.tf v(out) v1\n.ac lin 1 1k 1k\n.tf i(v1) v1\n.op");
    let plots = run_all(&deck).unwrap();
    let summary: Vec<(&str, &str)> = plots
        .iter()
        .map(|(name, plot)| (name.as_str(), plot.plotname.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            ("ac1", "AC Analysis"),
            ("op1", "Operating Point"),
            ("tf1", "Transfer Function"),
            ("tf2", "Transfer Function"),
        ]
    );
    assert_eq!(names(&plots[2].1)[2], "v(v1#Output_impedance)");
    assert_eq!(names(&plots[3].1)[2], "v(output_impedance_at_V(out))");
    // The .tf solves never disturb the operating point of a later analysis.
    close(plots[1].1.value("v(out)", 0).unwrap().re, 7.5, 1e-14);
}

#[test]
fn the_bias_point_honours_deck_dc_options() {
    // `.option itl1=1` cannot converge a diode from zero: the .tf bias point
    // receives the deck's DC options exactly as .op does, and fails the same
    // way instead of linearising an unconverged point (C ignores CKTop's
    // result here).
    let body = "vd vdin 0 dc 2\nrd vdin d 1k\nd1 d 0 dmod\n.model dmod d(is=1e-14)\n\
                .options itl1=1 noopiter gminsteps=0 srcsteps=0";
    let tf_error = tf(body, ".tf v(d) vd").expect_err("no convergence");
    let op_error = run_all(&format!("{body}\n.op")).expect_err("no convergence");
    assert!(
        matches!(tf_error, SpiceError::Numerical { .. }),
        "{tf_error}"
    );
    assert_eq!(
        std::mem::discriminant(&tf_error),
        std::mem::discriminant(&op_error)
    );
}

#[test]
fn invalid_and_unsupported_cards_fail_explicitly() {
    for (card, message) in [
        (".tf v(out) vx", "source vx not in circuit"),
        (".tf v(out) r1", "not of proper type"),
        (".tf v(nope) v1", "output node nope is not in the circuit"),
        (
            ".tf v(out,nope) v1",
            "output node nope is not in the circuit",
        ),
        (".tf i(r1) v1", "no findable branch current"),
        (".tf i(vx) v1", "output source vx is not in the circuit"),
        (".tf v(out)", "missing the input source"),
        (".tf out v1", "neither a voltage"),
        (".tf v(out) v1 extra", "unexpected argument 'extra'"),
        (".tf v out v1", "expected '('"),
    ] {
        let error = tf(DIVIDER, card).expect_err(card);
        assert!(error.to_string().contains(message), "{card}: {error}");
        assert!(!error.is_not_yet_ported(), "{card}: {error}");
    }
}

#[test]
fn the_driver_is_registered() {
    let driver = runner(AnalysisKind::TransferFunction).unwrap();
    assert_eq!(driver.kind(), AnalysisKind::TransferFunction);
    assert_eq!(driver.name(), "transfer function");
}
