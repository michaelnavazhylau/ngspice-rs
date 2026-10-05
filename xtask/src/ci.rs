//! `cargo xtask ci`: the checks that must pass before anything is committed.

use std::process::Command;

use crate::workspace_root;

const STEPS: &[(&str, &[&str])] = &[
    ("cargo", &["fmt", "--all", "--check"]),
    (
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ),
    ("cargo", &["test", "--workspace"]),
];

/// Runs every check in order, stopping at the first failure.
pub(crate) fn main() -> Result<(), String> {
    for (program, arguments) in STEPS {
        run(program, arguments)?;
    }
    println!("\nall checks passed");
    Ok(())
}

fn run(program: &str, arguments: &[&str]) -> Result<(), String> {
    println!("\n$ {program} {}", arguments.join(" "));
    let status = Command::new(program)
        .args(arguments)
        .current_dir(workspace_root())
        .status()
        .map_err(|error| format!("running {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{program} {}` failed with {status}",
            arguments.join(" ")
        ))
    }
}
