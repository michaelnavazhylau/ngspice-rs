//! TXL (`Y`) transmission lines (#170–#173): the `inp2y.c` card and model
//! front end, the parser-time three-pi fallback expansion, the Padé DC stamp
//! and recursive-convolution transient, and the explicit boundaries of the
//! slices.
//!
//! The reference values below were printed by
//! `ngspice -b`/`-a -r` from `ngspice_test/build/src/ngspice` on the same
//! decks. `cargo xtask golden verify` checks the committed goldens for
//! `m10_txl_{fb_op,fb_dc,fb_tran,op,dc,tran,early_tran,tran_step,lossless_tran}` and
//! `m10_txl3_tran`; the assertions here state that agreement, including the
//! two-branch-current naming C's rawfile uses (`i(y1)` twice) and C's
//! `CKTmaxStep = 0.9*taul` clamp.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, Plot, RunConfig, runner};
use ngspice_rs::devices::Circuit;
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, SpiceError, SpiceResult};
use std::path::Path;

fn deck(body: &str) -> Result<ngspice_rs::netlist::ast::Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("txl.cir"),
        &format!("TXL\n{body}\n.end\n"),
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
/// Runs a deck that carries `.option` cards, which need
/// [`RunConfig::from_netlist`] (its `RunConfig` applies them, the plain
/// [`Circuit::from_netlist`] entry point refuses them).
fn run_with_options(body: &str, kind: AnalysisKind, arguments: &[&str]) -> Plot {
    let netlist = deck(body).unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = AnalysisRequest::with_arguments(kind, arguments.iter().copied());
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap()
}
fn value(plot: &Plot, name: &str, point: usize) -> f64 {
    let index = plot
        .variable_index(name)
        .unwrap_or_else(|| panic!("missing {name}: {:?}", plot.variables));
    plot.points[point][index].re
}
/// The `occurrence`-th variable with this name (`i(y1)` twice).
fn values(plot: &Plot, name: &str, point: usize) -> Vec<f64> {
    plot.variables
        .iter()
        .enumerate()
        .filter(|(_, variable)| variable.name == name)
        .map(|(index, _)| plot.points[point][index].re)
        .collect()
}
/// The TXL instance of a circuit (`v1` and the load come first in deck order).
fn txl(c: &Circuit) -> &dyn ngspice_rs::devices::traits::Device {
    c.devices()
        .iter()
        .map(AsRef::as_ref)
        .find(|device| device.designator() == 'y')
        .expect("a Y instance")
}
fn close(a: f64, b: f64, relative: f64, absolute: f64) {
    assert!(
        (a - b).abs() <= relative * b.abs() + absolute,
        "{a:e} != {b:e}"
    );
}

const LADDER: &str = "v1 1 0 dc 5\ny1 1 0 2 0 ym\nrl 2 0 1k\n\
                      .model ym txl r=1e9 l=1e-2 c=1e-9 g=0 length=3e-11";
const PADE: &str = "v1 1 0 dc 5\ny1 1 0 2 0 ym\nrl 2 0 50\n\
                    .model ym txl r=12.45 l=8.972e-9 c=0.468e-12 g=0 length=16";

/// `inp2y.c`'s card: four terminals, then the model, then `len=` alone. The
/// reference terminals are written but the Padé device binds only the first
/// and third (`bindNode(fast, 1, node1)`/`(2, node2)`).
#[test]
fn the_card_and_the_instance_length_setter_are_accepted() {
    let netlist = deck(
        "y1 in 0 out 0 ym\nv1 in 0 1\nrl out 0 1k\n\
         .model ym txl r=1 l=250e-9 c=100e-12 g=0 length=1",
    )
    .unwrap();
    let device = &netlist.devices[0];
    assert_eq!(device.designator, 'y');
    assert_eq!(device.nodes, ["in", "0", "out", "0"]);
    assert_eq!(device.model.as_deref(), Some("ym"));
    let netlist = deck(
        "y1 in 0 out 0 ym len=2\nv1 in 0 1\nrl out 0 1k\n\
         .model ym txl r=1 l=250e-9 c=100e-12 g=0 length=1",
    )
    .unwrap();
    assert_eq!(netlist.devices[0].parameters[0].name, "len");
    // The model's own `length` is the `TXL_LENGTH` setter; only `len` exists
    // on the instance (`INP2Y` probes that token alone).
    let error = deck(
        "y1 in 0 out 0 ym length=2\nv1 in 0 1\nrl out 0 1k\n\
         .model ym txl r=1 l=250e-9 c=100e-12 g=0 length=1",
    )
    .unwrap_err();
    assert!(error.to_string().contains("len=<value>"), "{error}");
    // The netlist writer round-trips the card, its model and the setter.
    let text = ngspice_rs::netlist::writer::write_netlist(&netlist).unwrap();
    assert!(text.contains("y1 in 0 out 0 ym len=2"), "{text}");
    assert!(text.contains("txl"), "{text}");
    // The written deck carries its own title line, so parse it directly
    // rather than through `deck`, which adds one.
    let reparsed = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("txl.cir"), &text))
        .unwrap_or_else(|error| panic!("{error}\n---\n{text}"));
    assert_eq!(reparsed.devices.len(), 3, "the Y card plus v1 and rl");
    assert!(
        reparsed.models.iter().any(|model| model.name == "ym"),
        "{text}"
    );
    assert_eq!(reparsed.devices[0].designator, 'y');
    assert_eq!(reparsed.devices[0].parameters[0].name, "len");
}

/// `TXLsetup` reports five fatal errors, one per missing parameter, and none
/// has a default (`txlinit.c`'s `TXLmPTable` marks each with `IF_SET`).
#[test]
fn every_model_parameter_is_required() {
    for (missing, message) in [
        ("r", "r must be given"),
        ("l", "l must be given"),
        ("g", "g must be given"),
        ("c", "c must be given"),
        ("length", "length must be given"),
    ] {
        let all = [
            ("r", "1"),
            ("l", "250e-9"),
            ("g", "0"),
            ("c", "100e-12"),
            ("length", "1"),
        ];
        let setters: Vec<String> = all
            .iter()
            .filter(|(name, _)| *name != missing)
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        let body = format!(
            "v1 1 0 dc 1\ny1 1 0 2 0 ym\nrl 2 0 50\n.model ym txl {}",
            setters.join(" ")
        );
        let error = circuit(&body).unwrap_err();
        assert!(
            error.to_string().contains(message),
            "{missing}: expected '{message}', got {error}"
        );
    }
    // The bare `txl` flag is accepted and changes nothing (`TXL_MOD_TXL` is
    // consumed by the front end and read by no load).
    assert!(circuit(&format!("{PADE} txl")).is_ok());
    // An unknown setter is refused rather than ignored.
    let error = circuit(
        "y1 a 0 b 0 ym\nv1 a 0 1\nrl b 0 50\n\
         .model ym txl r=1 l=1n c=1p g=0 length=1 foo=1",
    )
    .unwrap_err();
    assert!(error.to_string().contains("foo"), "{error}");
}

/// CPL is a different device (`inp2p.c`, `devices/cpl/`); `INP2Y` hands the
/// card to `INP2P` when the model's type is `CplLines`, which this port does
/// not implement.
#[test]
fn coupled_line_models_are_refused() {
    let error = deck("y1 a 0 b 0 cm\nv1 a 0 1\nrl b 0 50\n.model cm cpl r=1 l=1 c=1 g=0 length=1")
        .unwrap_err();
    assert!(error.to_string().contains("inp2p.c"), "{error}");
}

/// `inp2y.c`'s fallback: `lenval && rval && lval && rval/lval > 1.6e10`
/// replaces the `Y` instance with three resistors `3/(R*length)` and four
/// capacitors `c*length/6` of `c, 2c, 2c, c` to the **input** reference node.
/// C assigns `3/(R*length)` to the *resistance* setter (a dimensional quirk of
/// the original code) and the port reproduces it literally.
#[test]
fn the_high_resistance_fallback_expands_to_the_c_ladder() {
    let mut c = circuit(LADDER).unwrap();
    let names: Vec<&str> = c.devices().iter().map(|d| d.name()).collect();
    assert_eq!(
        names,
        [
            "v1",
            "txlres1y1",
            "txlres2y1",
            "txlres3y1",
            "txlcap1y1",
            "txlcap2y1",
            "txlcap3y1",
            "txlcap4y1",
            "rl"
        ],
        "the Y instance itself is not built on this path"
    );
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    // Three 100 ohm sections into the 1 kohm load: 5/1300 A.
    assert_eq!(
        plot.variables
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["v(1)", "v(2)", "v(txlnd1y1)", "v(txlnd2y1)", "i(v1)"]
    );
    close(value(&plot, "v(1)", 0), 5.0, 1e-12, 1e-12);
    close(
        value(&plot, "v(txlnd1y1)", 0),
        4.615_384_615_384_617,
        1e-12,
        1e-12,
    );
    close(
        value(&plot, "v(txlnd2y1)", 0),
        4.230_769_230_769_233,
        1e-12,
        1e-12,
    );
    close(value(&plot, "v(2)", 0), 3.846_153_846_153_848, 1e-12, 1e-12);
    close(
        value(&plot, "i(v1)", 0),
        -3.846_153_846_153_834e-3,
        1e-12,
        1e-15,
    );
    // The resistor values are `3/(r*len)` and the capacitors `c*len/6`.
    match c.devices()[1].resistor_metadata() {
        Some(meta) => close(
            c.devices()[1]
                .resistor_effective(meta.supplied, &Default::default())
                .unwrap(),
            100.0,
            1e-12,
            1e-12,
        ),
        None => panic!("the generated element is a resistor"),
    }
    let mut c = circuit(&LADDER.replace("dc 5", "dc 0")).unwrap();
    let plot = run(&mut c, AnalysisKind::DcSweep, &["v1", "0", "5", "0.5"]);
    assert_eq!(plot.point_count(), 11);
    close(
        value(&plot, "v(2)", 10),
        3.846_153_846_153_848,
        1e-12,
        1e-12,
    );
    close(
        value(&plot, "i(v1)", 10),
        -3.846_153_846_153_834e-3,
        1e-12,
        1e-15,
    );
}

/// The trigger is `r/l > 1.6e10`, so a lower ratio keeps the Padé device, and
/// a name collision on a generated node is refused rather than reusing a deck
/// node (`CKTmkVolt` reports `E_EXISTS`).
#[test]
fn the_fallback_requires_c_ratio_and_fresh_node_names() {
    // r/l = 1e9/1e-2 = 1e11 triggers; the same `r` with `l=1` (1e9) does not.
    let pade = circuit(&LADDER.replace("l=1e-2", "l=1")).unwrap();
    assert!(pade.devices().iter().any(|d| d.name() == "y1"));
    let error = circuit(&format!("{LADDER}\nrt txlnd1y1 0 1k")).unwrap_err();
    assert!(error.to_string().contains("collision"), "{error}");
}

/// `ReadTxL`'s Padé path and `TXLload`'s `MODEDC` stamp: two branch currents
/// `<name>#branch1`/`#branch2`, `Ibr2 - Ibr1 = R*length` and C's rawfile
/// naming, which prints both as `i(y1)`.
#[test]
fn the_pade_dc_stamp_matches_the_reference_binary() {
    let mut c = circuit(PADE).unwrap();
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    assert_eq!(
        plot.variables
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["v(1)", "v(2)", "i(y1)", "i(y1)", "i(v1)"],
        "both branch currents are named after the device; the committed golden \
         holds C's own column order (`i(v1)` second), which the comparison \
         matches by name"
    );
    // v(p) - v(n) = R*length*i1, i.e. 5 - 1.003210272849201 = 12.45*16*i1.
    let current = values(&plot, "i(y1)", 0);
    assert_eq!(current.len(), 2);
    let v1 = value(&plot, "v(1)", 0);
    let v2 = value(&plot, "v(2)", 0);
    close(current[0], 2.006_420_545_758_433e-2, 1e-12, 1e-15);
    close(current[1], -2.006_420_545_758_433e-2, 1e-12, 1e-15);
    close(v2, 1.003_210_272_849_201, 1e-12, 1e-15);
    close((v1 - v2) / (12.45 * 16.0), current[0], 1e-12, 1e-15);
    close(
        value(&plot, "i(v1)", 0),
        -2.006_420_545_818_466e-2,
        1e-12,
        1e-15,
    );
    // `TXLask` reports the *instance* `TXLlength`, which stays zero unless
    // `len=` was written (measured on the reference binary).
    assert_eq!(
        txl(&c)
            .observation_parameter("length", &Default::default())
            .unwrap(),
        Some(0.0)
    );
    let mut c = circuit(&PADE.replace("ym\n", "ym len=2\n")).unwrap();
    assert_eq!(
        txl(&c)
            .observation_parameter("length", &Default::default())
            .unwrap(),
        Some(2.0)
    );
    let plot = run(&mut c, AnalysisKind::OperatingPoint, &[]);
    // `len=2` really is the line length here: `ReadTxL` reads the instance
    // field when it was given, so the DC line becomes `12.45*2` ohms and
    // C prints v(2) = 3.337783711601629 V with i1 = 6.675567423286632e-2 A.
    close(value(&plot, "v(2)", 0), 3.337_783_711_601_629, 1e-12, 1e-15);
    close(
        values(&plot, "i(y1)", 0)[0],
        6.675_567_423_286_632e-2,
        1e-12,
        1e-15,
    );
    close(
        value(&plot, "i(v1)", 0),
        -6.675_567_423_370_01e-2,
        1e-12,
        1e-15,
    );
}

/// `txlload.c`'s recursive convolution. The committed goldens fix C's own
/// accepted points: 214 for the 10 ns runs and 495 for the three-line deck,
/// which the port reproduces with last-bit differences in nonlinear steps.
#[test]
fn the_pade_transient_reproduces_cs_accepted_points() {
    let plot = run_with_options(
        "v1 1 0 dc 0 pulse(0 1 5n 0.1n 0.1n 100n 200n)\ny1 1 0 2 0 ym\nrl 2 0 50\n\
         .model ym txl r=1 l=250e-9 c=100e-12 g=0 length=1\n\
         .options reltol=1e-6 vntol=1e-9 abstol=1e-14",
        AnalysisKind::Transient,
        &["0.05n", "10n"],
    );
    assert_eq!(plot.point_count(), 214);
    close(value(&plot, "time", 107), 5.0e-9, 1e-12, 1e-18);
    // The far end has not been reached inside this run; C prints -2.5e-12.
    close(
        value(&plot, "v(2)", 213),
        -2.512_257_462_215_495e-12,
        1e-6,
        1e-15,
    );
    close(
        value(&plot, "i(v1)", 213),
        -1.980_427_568_658_008e-2,
        1e-9,
        1e-12,
    );
    // Applying the step before the first delay elapses transmits it.
    let plot = run_with_options(
        "v1 1 0 dc 0 pulse(0 1 1n 0.1n 0.1n 100n 200n)\ny1 1 0 2 0 ym\nrl 2 0 50\n\
         .model ym txl r=1 l=250e-9 c=100e-12 g=0 length=1\n\
         .options reltol=1e-6 vntol=1e-9 abstol=1e-14",
        AnalysisKind::Transient,
        &["0.05n", "10n"],
    );
    assert_eq!(plot.point_count(), 214);
    close(
        value(&plot, "v(2)", 213),
        9.862_218_900_058_257e-1,
        1e-9,
        1e-12,
    );
}

/// `TXLload`'s step control: a load whose `h` exceeds `taul` prints "your time
/// step is too large for TXL tau." and sets `CKTmaxStep = 0.9*taul`, so the
/// *first* oversized step is taken and the following ones are bounded. Here
/// `tmax = 10 ns` against `taul = 5 ns`: C accepts 34 points, including
/// one 7.168 ns gap followed by steps bounded at 4.5 ns.
#[test]
fn the_step_bound_follows_c_after_one_oversized_step() {
    let plot = run_with_options(
        "v1 1 0 dc 0 pulse(0 1 2n 0.1n 0.1n 100n 200n)\ny1 1 0 2 0 ym\nrl 2 0 50\n\
         .model ym txl r=1 l=250e-9 c=100e-12 g=0 length=1\n\
         .options reltol=1e-6 vntol=1e-9 abstol=1e-14",
        AnalysisKind::Transient,
        &["0.5n", "50n", "0", "10n"],
    );
    assert_eq!(plot.point_count(), 34);
    let steps: Vec<f64> = (1..5)
        .map(|i| value(&plot, "time", i) - value(&plot, "time", i - 1))
        .collect();
    for (step, expected) in steps.iter().zip([5e-12, 5e-12, 10e-12, 20e-12]) {
        close(*step, expected, 1e-6, 1e-18);
    }
    close(value(&plot, "time", 33), 50e-9, 1e-9, 1e-18);
    let gap = |i| value(&plot, "time", i) - value(&plot, "time", i - 1);
    close(gap(25), 7.168e-9, 1e-9, 1e-18);
    for i in 26..plot.point_count() {
        assert!(gap(i) <= 4.5e-9 + 1e-18, "unclamped step {i}");
    }
    close(
        value(&plot, "v(2)", 33),
        9.803_138_913_247_859e-1,
        1e-9,
        1e-12,
    );
}

/// The AC family is refused because C has no frequency-domain stamp: `txlinit.c` sets
/// `DEVacLoad = TXLload`, which reads `CKTdelta` (never assigned by
/// `niaciter.c`/`acan.c`), so C's answer is frequency-independent residue.
/// `DEVpzLoad`/`DEVnoise`/`DEVdisto` are `NULL` and `.sens` has no `DEVsen*`.
/// `.tf` is *not* refused: `tfanal.c` reuses the operating point's factors, so
/// it answers the DC line, and the port matches C there.
#[test]
fn the_ac_family_is_refused_and_tf_uses_the_dc_line() {
    let body = "v1 a 0 dc 1 ac 1\nrd a 0 50\ny1 a 0 b 0 ym\nrl2 b 0 50\n\
                .model ym txl r=1 l=250e-9 c=100e-12 g=0 length=1";
    for (kind, arguments) in [
        (AnalysisKind::Ac, vec!["dec", "2", "1k", "1meg"]),
        (
            AnalysisKind::PoleZero,
            vec!["a", "0", "b", "0", "vol", "pz"],
        ),
        (
            AnalysisKind::Noise,
            vec!["v(b)", "v1", "dec", "2", "1k", "1meg"],
        ),
        (AnalysisKind::Distortion, vec!["dec", "2", "1k", "1meg"]),
    ] {
        let mut c = circuit(body).unwrap();
        let error = attempt(&mut c, kind, &arguments).unwrap_err();
        assert!(error.is_not_yet_ported(), "{kind:?}: {error}");
        assert!(error.to_string().contains("TXL"), "{kind:?}: {error}");
    }
    let mut rf = circuit(&body.replace("ac 1", "ac 1 portnum=1")).unwrap();
    let error = attempt(&mut rf, AnalysisKind::SParameter, &["lin", "2", "1k", "2k"]).unwrap_err();
    assert!(
        error.is_not_yet_ported() && error.to_string().contains("TXL"),
        "{error}"
    );
    let mut c = circuit(body).unwrap();
    let error = attempt(&mut c, AnalysisKind::Sensitivity, &["v", "(", "b", ")"]).unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    let mut c = circuit(body).unwrap();
    let plot = run(
        &mut c,
        AnalysisKind::TransferFunction,
        &["v", "(", "b", ")", "v1"],
    );
    assert_eq!(plot.point_count(), 1);
    // The line's DC stamp is a `R*length = 1` ohm series resistance into the
    // 50 ohm load, so the transfer is 50/51 and the input impedance is 51.
    close(
        value(&plot, "v(Transfer_function)", 0),
        9.803_921_568_625_51e-1,
        1e-9,
        1e-12,
    );
    // `rd 50` is in parallel with the source, so the input impedance is
    // `50 || (R*length + 50)` = 25.2475... ohms.
    close(
        value(&plot, "v(v1#Input_impedance)", 0),
        2.524_752_475_222_528e1,
        1e-9,
        1e-12,
    );
}

#[test]
fn fallback_rc_first_moment_matches_the_analytic_ladder() {
    let plot = run_with_options(
        "v1 1 0 dc 0 pulse(0 1 1n 0.1n 0.1n 100n 200n)\ny1 1 0 2 0 ym\nrl 2 0 1k\n\
         .model ym txl r=1e9 l=1e-2 c=0.2 g=0 length=3e-11\n\
         .options reltol=1e-6 vntol=1e-9 abstol=1e-14",
        AnalysisKind::Transient,
        &["0.01n", "20n"],
    );
    // Three 100 ohm sections; the unknown-node capacitances are 2p, 2p, 1p.
    // With x = s*1e-10, nodal elimination gives
    // H(s) = 1/(1.3 + 9.8*x + 12.4*x^2 + 4*x^3).
    // The normalized step deficit integrates to (9.8/1.3)*1e-10 s.
    // The source's linear 0.1 ns rise adds half its rise time.
    let dc_gain = 10. / 13.;
    let mut area = 0.;
    for i in 1..plot.point_count() {
        let a = value(&plot, "time", i - 1);
        let b = value(&plot, "time", i);
        if a >= 1e-9 - 1e-18 {
            let fa = 1. - value(&plot, "v(2)", i - 1) / dc_gain;
            let fb = 1. - value(&plot, "v(2)", i) / dc_gain;
            area += (b - a) * 0.5 * (fa + fb);
        }
    }
    // 0.01 ns max step resolves the ~0.75 ns RC moment; the bound includes
    // trapezoidal quadrature and companion discretization, not C fitting.
    close(area, (9.8 / 1.3) * 1e-10 + 0.05e-9, 1e-3, 1e-15);
    close(
        value(&plot, "v(2)", plot.point_count() - 1),
        dc_gain,
        1e-6,
        1e-9,
    );
}

#[test]
fn the_lossless_shortcut_transmits_the_matched_wave() {
    let plot = run_with_options(
        "v1 1 0 dc 0 pulse(0 1 1n 0.1n 0.1n 100n 200n)\ny1 1 0 2 0 ym\nrl 2 0 50\n\
         .model ym txl r=0 l=250e-9 c=100e-12 g=0 length=1\n\
         .options reltol=1e-6 vntol=1e-9 abstol=1e-14",
        AnalysisKind::Transient,
        &["0.05n", "12n"],
    );
    assert_eq!(plot.point_count(), 254);
    let last = plot.point_count() - 1;
    close(value(&plot, "v(2)", last), 1., 1e-9, 1e-12);
    close(values(&plot, "i(y1)", last)[1], -0.02, 1e-9, 1e-12);
}

#[test]
fn invalid_domains_overflow_and_uic_are_explicit_errors() {
    for (from, to) in [
        ("r=12.45", "r=-1"),
        ("l=8.972e-9", "l=-1"),
        ("c=0.468e-12", "c=0"),
        ("g=0", "g=-1"),
        ("length=16", "length=0"),
    ] {
        assert!(circuit(&PADE.replace(from, to)).is_err(), "{to}");
    }
    // ReadTxL's lossless shortcut must not bypass finite derived-value checks.
    for model in [
        "r=0 l=1e308 c=1e308 g=0 length=1",
        "r=0 l=1e-12 c=1e308 g=0 length=1",
        "r=0 l=1e-12 c=1e-300 g=0 length=1e-300",
    ] {
        let error = circuit(&format!(
            "v1 a 0 1\ny1 a 0 b 0 ym\nrl b 0 50\n.model ym txl {model}"
        ))
        .unwrap_err();
        assert!(error.to_string().contains("admittance/delay"), "{error}");
    }
    let mut c = circuit(PADE).unwrap();
    let mut request = AnalysisRequest::with_arguments(AnalysisKind::Transient, ["1n", "10n"]);
    request.uic = true;
    let error = runner(AnalysisKind::Transient)
        .unwrap()
        .run(&mut c, &request, &AnalysisContext::default())
        .unwrap_err();
    assert!(error.is_not_yet_ported(), "{error}");
    assert!(error.to_string().contains("uic"), "{error}");
}

/// Every unsupported path names its C reference instead of simulating
/// nothing, and the `y` designator is a bounded capability now.
#[test]
fn the_registry_reports_a_bounded_y() {
    let registry = ngspice_rs::devices::registry::Registry::with_builtins();
    let entry = registry.get('y').expect("registered");
    assert!(entry.c_reference.contains("inp2y.c"), "{entry:?}");
    assert!(matches!(
        entry.support,
        ngspice_rs::devices::registry::DeviceSupport::Bounded { .. }
    ));
    assert!(!entry.description.is_empty());
}
