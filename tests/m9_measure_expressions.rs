//! Scalar measurements: C's two-pass ordering, deck parameters, and atomic errors.
use ngspice_rs::{
    analysis::{Plot, PlotFlags, Variable, measure},
    netlist::{Parser, eval::ParamScope, source::parse_deck_text},
    primitives::{AnalysisKind, Complex},
};
use std::{fs, path::Path, process::Command};

fn evaluate(body: &str) -> ngspice_rs::primitives::SpiceResult<Vec<measure::Measurement>> {
    let parsed =
        Parser::new().parse_deck_with_output(&parse_deck_text(Path::new("scalar.cir"), body))?;
    let scope = ParamScope::for_netlist(&parsed.netlist)?;
    let mut plot = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
    plot.push_variable(Variable::new("time", "time"));
    plot.push_variable(Variable::new("v(out)", "voltage"));
    for t in [0., 0.5, 1.] {
        plot.push_point(vec![Complex::real(t), Complex::real(2. * t)])?;
    }
    measure::resolve_with_scope(&plot, AnalysisKind::Transient, &parsed.measurements, &scope)
}

const DECK: &str = "Scalar\n.param scale=3\n.func twice(x) {2*x}\n.measure tran scaled param='peak*scale'\n.measure tran peak max v(out)\n.measure tran chained expr='twice(scaled)+1'\n.end\n";

#[test]
fn scalar_measurements_use_two_passes_but_return_source_order() {
    let results = evaluate(DECK).unwrap();
    assert_eq!(
        results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["scaled", "peak", "chained"]
    );
    assert_eq!(
        results.iter().map(|r| r.value).collect::<Vec<_>>(),
        [6., 2., 13.]
    );
    assert_eq!(results[0].unit, "scalar");
    for expression in ["missing", "1/0", "sqrt(-1)", "later+1"] {
        assert!(
            evaluate(&format!(
                "Bad\n.measure tran first param='{expression}'\n.measure tran later param=1\n.end\n"
            ))
            .is_err()
        );
    }
}

#[test]
fn scalar_measurements_round_trip_and_malformed_tails_fail() {
    let parser = Parser::new();
    let parsed = parser
        .parse_deck_with_output(&parse_deck_text(Path::new("scalar.cir"), DECK))
        .unwrap();
    let written = ngspice_rs::netlist::write_netlist(&parsed.netlist).unwrap();
    let again = parser
        .parse_deck_with_output(&parse_deck_text(Path::new("again.cir"), &written))
        .unwrap();
    assert!(ngspice_rs::netlist::semantic_eq(
        &parsed.netlist,
        &again.netlist
    ));
    for card in ["param=", "param='1+2' garbage", "param='1+'"] {
        assert!(
            parser
                .parse_deck_with_output(&parse_deck_text(
                    Path::new("bad.cir"),
                    &format!("Bad\n.meas tran x {card}\n.end\n")
                ))
                .is_err()
        );
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN"]
fn scalar_measurements_match_live_c() {
    let binary = std::env::var("NGSPICE_BIN").unwrap();
    assert!(Path::new(&binary).is_absolute());
    let dir = std::env::temp_dir().join(format!("m9-meas-expr-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let deck = dir.join("deck.cir");
    let raw = dir.join("out.raw");
    fs::write(
        &deck,
        DECK.replace("chained expr=", "chained param=").replace(
            "Scalar\n",
            "Scalar\nV1 out 0 pwl(0 0 1 2)\nR1 out 0 1k\n.tran .01 1\n",
        ),
    )
    .unwrap();
    let c = Command::new(binary).arg("-b").arg(&deck).output().unwrap();
    let stdout = String::from_utf8_lossy(&c.stdout);
    assert!(
        c.status.success(),
        "{stdout} {}",
        String::from_utf8_lossy(&c.stderr)
    );
    let ours = ngspice_rs::cli::simulate::run(&deck, &raw, true).unwrap();
    for result in &ours.plots.last().unwrap().measurements {
        let value: f64 = stdout
            .lines()
            .find_map(|line| {
                let (name, tail) = line.split_once('=')?;
                (name.trim() == result.name)
                    .then(|| tail.split_whitespace().next()?.parse().ok())
                    .flatten()
            })
            .unwrap_or_else(|| panic!("missing {}: {stdout}", result.name));
        assert!(
            (result.value - value).abs() < 1e-5,
            "{}: {} vs {value}",
            result.name,
            result.value
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_scalar_expression_preserves_existing_rawfile() {
    let dir = std::env::temp_dir().join(format!("m9-meas-atomic-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let deck = dir.join("deck.cir");
    let raw = dir.join("out.raw");
    fs::write(&raw, b"previous output").unwrap();
    fs::write(&deck, "Bad scalar\nV1 out 0 1\nR1 out 0 1k\n.tran .01 1\n.measure tran invalid param='missing+1'\n.end\n").unwrap();
    assert!(ngspice_rs::cli::simulate::run(&deck, &raw, true).is_err());
    assert_eq!(fs::read(&raw).unwrap(), b"previous output");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn numeric_setter_expressions_resolve_parameters_and_prior_results() {
    let deck = "Setters\n.param half=.5 one=1\n.measure tran peak max v(out)\n.measure tran sample find v(out) at={half}\n.measure tran crossing when v(out)='peak/2' rise={one} td={half/2}\n.measure tran area integ v(out) from={half} to={one}\n.end\n";
    let results = evaluate(deck).unwrap();
    assert_eq!(
        results.iter().map(|r| r.value).collect::<Vec<_>>(),
        [2., 1., 0.5, 0.75]
    );
    let parser = Parser::new();
    let parsed = parser
        .parse_deck_with_output(&parse_deck_text(Path::new("setters.cir"), deck))
        .unwrap();
    let written = ngspice_rs::netlist::write_netlist(&parsed.netlist).unwrap();
    let again = parser
        .parse_deck_with_output(&parse_deck_text(Path::new("again.cir"), &written))
        .unwrap();
    assert!(ngspice_rs::netlist::semantic_eq(
        &parsed.netlist,
        &again.netlist
    ));
    for setter in ["at={missing}", "at={1/0}", "at={sqrt(-1)}"] {
        assert!(evaluate(&format!("Bad\n.meas tran x find v(out) {setter}\n.end\n")).is_err());
    }
    assert!(evaluate("Bad count\n.meas tran x when v(out)={1} rise={1.5}\n.end\n").is_err());
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN"]
fn numeric_setter_integration_expressions_match_live_c() {
    let binary = std::env::var("NGSPICE_BIN").unwrap();
    assert!(Path::new(&binary).is_absolute());
    let dir = std::env::temp_dir().join(format!("m9-setters-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let deck = dir.join("deck.cir");
    let raw = dir.join("out.raw");
    fs::write(&deck, "Setters\n.param half=.5 one=1\nV1 out 0 pwl(0 0 1 2)\nR1 out 0 1k\n.tran .01 1\n.measure tran sample find v(out) at={half}\n.measure tran crossing when v(out)={one} rise={one} td={half/2}\n.measure tran area integ v(out) from={half} to={one}\n.end\n").unwrap();
    let c = Command::new(binary).arg("-b").arg(&deck).output().unwrap();
    let stdout = String::from_utf8_lossy(&c.stdout);
    assert!(
        c.status.success(),
        "{stdout} {}",
        String::from_utf8_lossy(&c.stderr)
    );
    let ours = ngspice_rs::cli::simulate::run(&deck, &raw, true).unwrap();
    for result in &ours.plots.last().unwrap().measurements {
        let value: f64 = stdout
            .lines()
            .find_map(|line| {
                let (name, tail) = line.split_once('=')?;
                (name.trim() == result.name)
                    .then(|| tail.split_whitespace().next()?.parse().ok())
                    .flatten()
            })
            .unwrap_or_else(|| panic!("missing {}: {stdout}", result.name));
        assert!(
            (result.value - value).abs() < 1e-5,
            "{}: {} vs {value}",
            result.name,
            result.value
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn numeric_setter_expressions_share_the_measurement_scope() {
    let results = evaluate("Setters\n.param lo=.25 hi=.75 target=1 count=1\n.meas tran peak max v(out) from={lo} to='hi'\n.meas tran cross when v(out)={target} rise={count} td={lo}\n.meas tran at_cross find v(out) at={cross}\n.meas tran delay trig v(out) val={target} rise='count' td={lo} targ at={hi}\n.end\n").unwrap();
    assert_eq!(
        results.iter().map(|r| r.value).collect::<Vec<_>>(),
        [1., 0.5, 1., 0.25]
    );
    for setter in ["rise={0}", "rise={1.5}", "rise={missing}", "td={1/0}"] {
        assert!(
            evaluate(&format!(
                "Bad\n.meas tran x when v(out)={{1}} {setter}\n.end\n"
            ))
            .is_err()
        );
    }
    let text = "Round trip\n.meas tran x max v(out) from={.2} to='.8'\n.end\n";
    let parser = Parser::new();
    let parsed = parser
        .parse_deck_with_output(&parse_deck_text(Path::new("a.cir"), text))
        .unwrap();
    let written = ngspice_rs::netlist::write_netlist(&parsed.netlist).unwrap();
    let again = parser
        .parse_deck_with_output(&parse_deck_text(Path::new("b.cir"), &written))
        .unwrap();
    assert!(ngspice_rs::netlist::semantic_eq(
        &parsed.netlist,
        &again.netlist
    ));
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN"]
fn numeric_setter_expressions_match_live_c() {
    let binary = std::env::var("NGSPICE_BIN").unwrap();
    assert!(Path::new(&binary).is_absolute());
    let dir = std::env::temp_dir().join(format!("m9-meas-setters-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let deck = dir.join("deck.cir");
    let raw = dir.join("out.raw");
    fs::write(&deck,"Setters\n.param lo=.25 hi=.75 target=1 count=1\nV1 out 0 pwl(0 0 1 2)\nR1 out 0 1k\n.tran .01 1\n.meas tran peak max v(out) from={lo} to='hi'\n.meas tran cross when v(out)={target} rise={count} td={lo}\n.meas tran at_cross find v(out) at={target/2}\n.meas tran delay trig v(out) val={target} rise='count' td={lo} targ at={hi}\n.end\n").unwrap();
    let c = Command::new(binary).arg("-b").arg(&deck).output().unwrap();
    let stdout = String::from_utf8_lossy(&c.stdout);
    assert!(
        c.status.success(),
        "{stdout} {}",
        String::from_utf8_lossy(&c.stderr)
    );
    let ours = ngspice_rs::cli::simulate::run(&deck, &raw, true).unwrap();
    for result in &ours.plots[0].measurements {
        let value: f64 = stdout
            .lines()
            .find_map(|line| {
                let (name, tail) = line.split_once('=')?;
                (name.trim() == result.name)
                    .then(|| tail.split_whitespace().next()?.parse().ok())
                    .flatten()
            })
            .unwrap_or_else(|| panic!("missing {}: {stdout}", result.name));
        assert!(
            (result.value - value).abs() < 1e-5,
            "{}: {} vs {value}",
            result.name,
            result.value
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
