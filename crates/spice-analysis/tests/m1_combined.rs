//! #22: the combined params+options+globals fixture is simulatable at top level
//! through the production RunConfig/Circuit/runner path (no subcircuits, so no
//! flattening is involved). Parsing D/Q/M or subcircuit decks elsewhere in the
//! gate is syntax only and implies no simulation support.
use std::path::Path;

use spice_analysis::{RunConfig, runner};
use spice_core::SpiceError;
use spice_netlist::{Parser, ast::Netlist, source::parse_deck_text};

fn run_op(n: &Netlist) -> Result<(RunConfig, String), SpiceError> {
    let config = RunConfig::from_netlist(n)?;
    let mut circuit = config.circuit(n)?;
    let card = &n.analyses[0];
    let request = config.request_for(card)?;
    let plot = runner(card.kind)?.run(&mut circuit, &request, &config.context())?;
    Ok((config, format!("{plot:?}")))
}

fn fixture() -> Netlist {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/parser/combined_params_options.cir");
    Parser::new().parse_file(path).unwrap()
}

#[test]
fn combined_fixture_elaborates_and_matches_the_literal_deck() {
    let n = fixture();
    let (config, plot) = run_op(&n).unwrap();
    // Duplicate setters are applied in order: the last reltol/temp win.
    let applied: Vec<_> = config.applied().iter().map(|a| format!("{a:?}")).collect();
    assert_eq!(applied.len(), 4, "{applied:?}");
    assert!(applied[0].contains("reltol") && applied[2].contains("reltol"));
    assert!(applied[1].contains("temp") && applied[3].contains("temp"));
    let literal = parse_deck_text(
        Path::new("lit.cir"),
        "t\n.option reltol=1e-4 temp=30\n.global vdd\n.option reltol=1e-5\nvdd vdd 0 dc 5\nr1 vdd out 2k\nr2 out 0 6k\n.option temp=40\n.op\n.end\n",
    );
    let literal = Parser::new().parse_deck(&literal).unwrap();
    let (_, expected) = run_op(&literal).unwrap();
    assert_eq!(plot, expected);
}

#[test]
fn param_cycles_and_unsupported_options_fail_before_any_result() {
    let parse = |body: &str| {
        Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("n.cir"),
                &format!("t\n{body}\n.op\n.end\n"),
            ))
            .unwrap()
    };
    let cyclic = parse(".param a={b} b={a}\nv1 x 0 {a}\nr1 x 0 1k");
    assert!(run_op(&cyclic).is_err());
    let unsupported = parse(".option gshunt=1e-12\nv1 x 0 1\nr1 x 0 1k");
    assert!(run_op(&unsupported).is_err());
}
