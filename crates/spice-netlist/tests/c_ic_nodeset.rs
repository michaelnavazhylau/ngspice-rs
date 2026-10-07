//! Opt-in syntax-parity probe for `.ic`/`.nodeset` against the C front end
//! (`inppas3.c`). Run with
//! `NGSPICE_BIN=... cargo test -p spice-netlist --test c_ic_nodeset -- --ignored`.
//!
//! The probe checks the forms where both implementations must agree (C
//! reports ` Error: .ic syntax error.` and Rust rejects, or both accept) and
//! pins the deliberately stricter Rust forms: C silently accepts ground
//! nodes, missing values, empty cards and non-finite values.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use spice_netlist::{Parser, source::parse_deck_text};

struct Probe {
    card: &'static str,
    c_syntax_error: bool,
    rust_accepts: bool,
}

const PROBES: &[Probe] = &[
    // Parity.
    Probe {
        card: ".ic v(b)=2",
        c_syntax_error: false,
        rust_accepts: true,
    },
    Probe {
        card: ".ic V(a)=1 v(b)=2",
        c_syntax_error: false,
        rust_accepts: true,
    },
    Probe {
        card: ".ic v(b) 2",
        c_syntax_error: false,
        rust_accepts: true,
    },
    Probe {
        card: ".nodeset v(b)=2",
        c_syntax_error: false,
        rust_accepts: true,
    },
    Probe {
        card: ".ic i(v1)=2",
        c_syntax_error: true,
        rust_accepts: false,
    },
    Probe {
        card: ".ic v(a,b)=2",
        c_syntax_error: true,
        rust_accepts: false,
    },
    Probe {
        card: ".ic b=1",
        c_syntax_error: true,
        rust_accepts: false,
    },
    // Rust is stricter than C's silent acceptance.
    Probe {
        card: ".ic v(0)=1",
        c_syntax_error: false,
        rust_accepts: false,
    },
    Probe {
        card: ".ic v(b)",
        c_syntax_error: false,
        rust_accepts: false,
    },
    Probe {
        card: ".ic",
        c_syntax_error: false,
        rust_accepts: false,
    },
    Probe {
        card: ".ic v(b)=1e999",
        c_syntax_error: false,
        rust_accepts: false,
    },
    // C accepts, Rust reports not-yet-ported.
    Probe {
        card: ".nodeset all=1",
        c_syntax_error: false,
        rust_accepts: false,
    },
];

#[test]
#[ignore = "requires NGSPICE_BIN"]
fn ic_nodeset_syntax_matches_or_is_documented_stricter_than_c() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    for probe in PROBES {
        let deck = format!(
            "T\nv1 a 0 1\nr1 a b 1k\nr2 b 0 1k\n{}\n.op\n.end\n",
            probe.card
        );
        let rust = Parser::new().parse_deck(&parse_deck_text(Path::new("p.cir"), &deck));
        assert_eq!(rust.is_ok(), probe.rust_accepts, "Rust on {:?}", probe.card);

        let dir = std::env::temp_dir().join(format!(
            "spice-ic-probe-{}-{}",
            std::process::id(),
            probe.card.len()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("probe.cir"), &deck).unwrap();
        let output = Command::new(&binary)
            .args(["-b", "probe.cir"])
            .current_dir(&dir)
            .output()
            .expect("run C ngspice");
        let _ = fs::remove_dir_all(&dir);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            text.contains("syntax error"),
            probe.c_syntax_error,
            "C on {:?}:\n{text}",
            probe.card
        );
    }
}
