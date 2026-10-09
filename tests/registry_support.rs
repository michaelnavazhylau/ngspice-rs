//! Pins the registry's support table (`spice-rs devices`, #117) against what
//! deck elaboration actually builds, so the table cannot drift from the
//! factories again: every designator gets a representative deck, `ported` and
//! `bounded` entries must elaborate it, `pending` entries must refuse it with
//! `NotYetPorted`, and only `ported` entries build from the card alone.

use std::path::Path;

use ngspice_rs::devices::{Circuit, DeviceSupport, Registry};
use ngspice_rs::netlist::{Parser, RawCard, source::parse_deck_text};
use ngspice_rs::primitives::{NodeTable, SpiceError};

/// A deck whose first line is one instance of `designator`.
fn deck(designator: char) -> &'static str {
    match designator {
        'r' => "r1 a 0 1k\nv1 a 0 1",
        'c' => "c1 a 0 1p\nv1 a 0 1",
        'l' => "l1 a b 1u\nv1 a 0 1\nr1 b 0 1k",
        'v' => "v1 a 0 1\nr1 a 0 1k",
        'i' => "i1 a 0 1m\nr1 a 0 1k",
        'e' => "e1 b 0 a 0 2\nv1 a 0 1\nr1 b 0 1k",
        'f' => "f1 b 0 v1 2\nv1 a 0 1\nr1 a 0 1k\nr2 b 0 1k",
        'g' => "g1 b 0 a 0 1m\nv1 a 0 1\nr1 b 0 1k",
        'h' => "h1 b 0 v1 2\nv1 a 0 1\nr1 a 0 1k\nr2 b 0 1k",
        'b' => "b1 b 0 v=v(a)*2\nv1 a 0 1\nr1 b 0 1k",
        'k' => "k1 l1 l2 0.5\nl1 a c 1u\nl2 b 0 1u\nv1 a 0 1\nr1 c 0 1k\nr2 b 0 1k",
        'd' => "d1 a 0 dm\n.model dm d\nv1 a 0 1",
        'q' => "q1 c b 0 qm\n.model qm npn\nv1 c 0 1\nv2 b 0 0.7",
        'm' => "m1 d g 0 0 mm\n.model mm nmos\nv1 d 0 1\nv2 g 0 1",
        's' => "s1 a 0 c 0 sm\n.model sm sw\nv1 c 0 1\nr1 a 0 1k",
        'w' => "w1 a 0 v1 wm\n.model wm csw\nv1 c 0 1\nr1 c 0 1k\nr2 a 0 1k",
        'x' => "x1 a b div\n.subckt div i o\nr1 i o 1k\n.ends\nv1 a 0 1\nr1 b 0 1k",
        'j' => "j1 d g 0 jm\n.model jm njf\nv1 d 0 1",
        'z' => "z1 d g 0 zm\n.model zm nmf\nv1 d 0 1",
        't' => "t1 a 0 b 0 z0=50 td=1n\nv1 a 0 1\nr1 b 0 50",
        'o' => "o1 a 0 b 0 lm\n.model lm ltra r=1 l=1n c=1p len=1\nv1 a 0 1\nr1 b 0 50",
        'y' => "y1 a 0 b 0 ym\n.model ym txl r=1 l=1n c=1p length=1\nv1 a 0 1\nr1 b 0 50",
        'u' => "u1 a b 0 um l=1m\n.model um urc\nv1 a 0 1\nr1 b 0 1k",
        'n' => "n1 a 0 nm\n.model nm nport\nv1 a 0 1",
        'p' => "p1 a 0 b 0 0 0 pm\n.model pm cpl\nv1 a 0 1",
        'a' => "a1 a b am\n.model am gain\nv1 a 0 1\nr1 b 0 1k",
        other => panic!("no representative deck for designator '{other}'"),
    }
}

/// Parses and elaborates `body`; a parse failure is reported like an
/// elaboration failure (unported cards are refused by the parser).
fn elaborate(body: &str) -> Result<Circuit, SpiceError> {
    let text = format!("registry support\n{body}\n.end\n");
    let netlist = Parser::new().parse_deck(&parse_deck_text(Path::new("support.cir"), &text))?;
    Circuit::from_netlist(&netlist)
}

fn first_card(body: &str) -> RawCard {
    let text = format!("registry support\n{body}\n");
    RawCard::parse(&parse_deck_text(Path::new("card.cir"), &text).lines[0]).expect("tokenizes")
}

#[test]
fn every_designator_is_built_exactly_as_the_registry_reports() {
    let registry = Registry::with_builtins();
    assert_eq!(registry.len(), 26);
    for entry in registry.entries() {
        let designator = entry.designator;
        let body = deck(designator);
        let elaborated = elaborate(body);
        let mut nodes = NodeTable::new();
        let from_card = registry.instantiate(&first_card(body), &mut nodes);
        match entry.support {
            DeviceSupport::Ported => {
                elaborated.unwrap_or_else(|error| panic!("'{designator}' ported: {error}"));
                from_card.unwrap_or_else(|error| panic!("'{designator}' card: {error}"));
            }
            DeviceSupport::Bounded { scope } => {
                assert!(!scope.is_empty(), "'{designator}'");
                elaborated.unwrap_or_else(|error| panic!("'{designator}' bounded: {error}"));
                let Err(error) = from_card else {
                    panic!("'{designator}': a bounded card needs its deck")
                };
                assert!(
                    matches!(&error, SpiceError::Unsupported { feature, .. } if feature.contains("from_netlist")),
                    "'{designator}': {error}"
                );
                assert!(nodes.is_empty(), "'{designator}'");
            }
            DeviceSupport::Pending => {
                let Err(error) = elaborated else {
                    panic!("'{designator}': a pending device is refused")
                };
                assert!(error.is_not_yet_ported(), "'{designator}': {error}");
                let Err(error) = from_card else {
                    panic!("'{designator}': a pending card is refused")
                };
                assert!(error.is_not_yet_ported(), "'{designator}': {error}");
                assert!(
                    error.to_string().contains(entry.c_reference),
                    "'{designator}': {error}"
                );
            }
        }
    }
}

#[test]
fn the_k_entry_cites_the_mutual_inductance_sources() {
    let entry = *Registry::with_builtins().get('k').expect("k");
    assert!(entry.c_reference.contains("devices/ind/mut"), "{entry:?}");
    assert!(!entry.c_reference.contains("cpl"), "{entry:?}");
}
