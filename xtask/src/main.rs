//! Workspace automation for the port.
//!
//! Run it as `cargo xtask <command>` (the alias is defined in
//! `.cargo/config.toml`).
//!
//! The important command is `golden`, which drives the C `ngspice` binary to
//! produce the comparison data the port is verified against. Nothing in the
//! simulator depends on this crate; it is a development tool. See
//! `docs/port/VERIFICATION.md`.

mod ci;
mod golden;
mod ngspice;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
cargo xtask — workspace automation for the Rust ngspice port

USAGE:
    cargo xtask golden capture [OPTIONS]   run the C ngspice binary on every fixture
                                           and write conformance/golden/*.raw
    cargo xtask golden check [OPTIONS]     re-capture and report drift, without writing
    cargo xtask golden list                describe the committed fixtures
    cargo xtask ci                         fmt --check, clippy -D warnings, test
    cargo xtask help

OPTIONS (golden capture/check):
    --ngspice <PATH>   the C ngspice binary to drive
                       (default: $NGSPICE_BIN, then build/src/ngspice, then PATH)
    --netlist <NAME>   only this fixture, e.g. --netlist rc_divider
    --verbose          echo each ngspice invocation
";

/// The workspace root: the parent of this crate's directory.
#[must_use]
pub(crate) fn workspace_root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives directly under the workspace root")
        .to_path_buf()
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some("golden") => golden::main(&arguments[1..]),
        Some("ci") => ci::main(),
        Some(other) => Err(format!("unknown command '{other}'")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            if !message.starts_with("drift") && !message.starts_with("usage") {
                eprintln!();
                eprintln!("{USAGE}");
            }
            ExitCode::FAILURE
        }
    }
}
