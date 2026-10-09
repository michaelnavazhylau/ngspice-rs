//! Discovery and generation of the committed token/AST snapshot set.
//!
//! Shared by the `snapshots` integration test (which only compares) and
//! `cargo xtask snapshots` (which can bless). Nothing here writes files.
//! Layout and procedure: `conformance/snapshots/README.md`.

use std::fs;
use std::io;
use std::path::Path;

use crate::netlist::dump::{dump_ast_file, dump_tokens_file};
use crate::netlist::parser::Parser;

/// Directories (relative to the conformance root) searched recursively for
/// `*.cir` inputs.
pub const INPUT_DIRS: &[&str] = &["netlists", "parser", "cases"];
/// Snapshot output directory, relative to the conformance root.
pub const SNAPSHOT_DIR: &str = "snapshots";

/// One generated snapshot file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotFile {
    /// Path relative to `<conformance>/snapshots`, with `/` separators.
    pub path: String,
    /// Exact file contents (`\n` line endings).
    pub contents: String,
}

fn walk(dir: &Path, prefix: &str, found: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = format!("{prefix}/{name}");
        if entry.file_type()?.is_dir() {
            walk(&entry.path(), &relative, found)?;
        } else if name.ends_with(".cir") {
            found.push(relative);
        }
    }
    Ok(())
}

/// Sorted `/`-separated paths (relative to `conformance`) of every input deck.
///
/// # Errors
///
/// Fails if an input directory cannot be read.
pub fn discover_inputs(conformance: &Path) -> io::Result<Vec<String>> {
    let mut found = Vec::new();
    for dir in INPUT_DIRS {
        walk(&conformance.join(dir), dir, &mut found)?;
    }
    found.sort();
    Ok(found)
}

/// Generates every expected snapshot, sorted by path.
///
/// # Errors
///
/// Fails if an input cannot be read. Parse failures are not errors: they are
/// snapshotted as positioned error dumps.
pub fn generate(conformance: &Path) -> io::Result<Vec<SnapshotFile>> {
    let root = conformance.canonicalize()?;
    let parser = Parser::new();
    let mut files = Vec::new();
    for input in discover_inputs(&root)? {
        let path = root.join(&input);
        let stem = input.strip_suffix(".cir").unwrap_or(&input);
        let tokens = dump_tokens_file(&path, &root).map_err(io::Error::other)?;
        files.push(SnapshotFile {
            path: format!("tokens/{stem}.tokens"),
            contents: tokens,
        });
        files.push(SnapshotFile {
            path: format!("ast/{stem}.ast"),
            contents: dump_ast_file(&parser, &path, &root),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn collect_existing(dir: &Path, prefix: &str, found: &mut Vec<String>) -> io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type()?.is_dir() {
            collect_existing(&entry.path(), &relative, found)?;
        } else {
            found.push(relative);
        }
    }
    Ok(())
}

/// Committed snapshot files (under `tokens/` and `ast/`) that no input
/// generates any more.
///
/// # Errors
///
/// Fails if the snapshot directory cannot be read.
pub fn orphans(conformance: &Path, generated: &[SnapshotFile]) -> io::Result<Vec<String>> {
    let base = conformance.join(SNAPSHOT_DIR);
    let mut existing = Vec::new();
    for sub in ["tokens", "ast"] {
        collect_existing(&base.join(sub), sub, &mut existing)?;
    }
    existing.sort();
    existing.retain(|path| !generated.iter().any(|g| &g.path == path));
    Ok(existing)
}
