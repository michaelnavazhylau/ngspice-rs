//! `spice-rs` — the command-line entry point.
//!
//! Thin by design: argument parsing, command dispatch and all output live in
//! [`ngspice_rs::cli::args`], which is re-exported as [`ngspice_rs::cli`] so that
//! it can be tested. This binary only turns a
//! [`ngspice_rs::primitives::SpiceError`] into a process exit status.

use std::process::ExitCode;

use ngspice_rs::cli::{Args, exit_code, run, usage};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    let args = match Args::parse(arguments) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("spice-rs: {message}");
            eprintln!();
            eprintln!("{}", usage());
            return exit_code_for(exit_code::USAGE);
        }
    };

    match run(&args) {
        Ok(()) => exit_code_for(exit_code::SUCCESS),
        Err(error) => {
            eprintln!("spice-rs: {error}");
            let status = if error.is_not_yet_ported() {
                exit_code::NOT_YET_PORTED
            } else {
                exit_code::FAILURE
            };
            exit_code_for(status)
        }
    }
}

fn exit_code_for(status: i32) -> ExitCode {
    ExitCode::from(u8::try_from(status).unwrap_or(1))
}
