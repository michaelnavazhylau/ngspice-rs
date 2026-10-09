//! `cargo xtask snapshots [--bless]`: token/AST snapshot check and regeneration.
//!
//! Without `--bless` nothing is written: drift is reported and the command
//! fails. With `--bless` created, changed and removed files are written and
//! reported; a second run reports no changes. Needs neither C nor sibling
//! checkouts. See `conformance/snapshots/README.md`.

use std::fs;
use std::path::Path;

use ngspice_rs::netlist::snapshot::{self, SNAPSHOT_DIR};

use crate::workspace_root;

const USAGE: &str = "\
usage: cargo xtask snapshots [--bless]

    (no flag)   compare committed snapshots with freshly generated ones;
                write nothing; exit non-zero on any difference
    --bless     write created/changed snapshots, delete orphans, report each";

pub(crate) fn main(arguments: &[String]) -> Result<(), String> {
    let mut bless = false;
    for argument in arguments {
        match argument.as_str() {
            "--bless" => bless = true,
            other => return Err(format!("usage: unknown option '{other}'\n\n{USAGE}")),
        }
    }
    let conformance = workspace_root().join("conformance");
    let generated = snapshot::generate(&conformance).map_err(|e| format!("generating: {e}"))?;
    let orphans = snapshot::orphans(&conformance, &generated)
        .map_err(|e| format!("scanning snapshots: {e}"))?;
    let base = conformance.join(SNAPSHOT_DIR);

    let (mut created, mut changed, mut unchanged, mut removed) = (0, 0, 0, 0);
    for file in &generated {
        let target = base.join(&file.path);
        let state = match fs::read(&target) {
            Ok(existing) if existing == file.contents.as_bytes() => {
                unchanged += 1;
                "unchanged"
            }
            Ok(_) => {
                changed += 1;
                "changed"
            }
            Err(_) => {
                created += 1;
                "created"
            }
        };
        if state == "unchanged" {
            continue;
        }
        println!("  {state:<9}  {}", file.path);
        if bless {
            write(&target, &file.contents)?;
        }
    }
    for path in &orphans {
        removed += 1;
        println!("  removed    {path}");
        if bless {
            fs::remove_file(base.join(path)).map_err(|e| format!("removing {path}: {e}"))?;
        }
    }
    println!(
        "\n{} snapshot(s): {created} created, {changed} changed, {removed} removed, {unchanged} unchanged{}",
        generated.len(),
        if bless {
            ""
        } else {
            " (dry run; pass --bless to write)"
        }
    );
    if !bless && created + changed + removed > 0 {
        return Err(
            "drift: snapshots differ; review, then run `cargo xtask snapshots --bless`".into(),
        );
    }
    Ok(())
}

fn write(target: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    fs::write(target, contents).map_err(|e| format!("writing {}: {e}", target.display()))
}
