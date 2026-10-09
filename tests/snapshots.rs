//! #21: committed token/AST dumps are compared byte for byte. This test never
//! writes; regenerate intentionally with `cargo xtask snapshots --bless`.
use std::fs;
use std::path::{Path, PathBuf};

use ngspice_rs::netlist::dump::{AST_DUMP_HEADER, TOKEN_DUMP_HEADER};
use ngspice_rs::netlist::snapshot::{self, SNAPSHOT_DIR};

fn conformance() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

const HINT: &str = "If the change is intentional, review it and run `cargo xtask snapshots --bless` \
(bump the dump version constant if the schema changed; see conformance/snapshots/README.md).";

fn first_difference(expected: &str, actual: &str) -> String {
    for (n, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
        if e != a {
            return format!("line {}:\n  committed: {e}\n  generated: {a}", n + 1);
        }
    }
    format!(
        "line counts differ (committed {}, generated {})",
        expected.lines().count(),
        actual.lines().count()
    )
}

#[test]
fn committed_snapshots_match_generated_byte_for_byte() {
    let root = conformance();
    let generated = snapshot::generate(&root).unwrap();
    assert!(
        generated.len() >= 40,
        "too few snapshots: {}",
        generated.len()
    );
    let mut problems = Vec::new();
    for file in &generated {
        let target = root.join(SNAPSHOT_DIR).join(&file.path);
        match fs::read(&target) {
            Ok(bytes) if bytes == file.contents.as_bytes() => {}
            Ok(bytes) => problems.push(format!(
                "{} differs at {}",
                file.path,
                first_difference(&String::from_utf8_lossy(&bytes), &file.contents)
            )),
            Err(_) => problems.push(format!("{} is missing", file.path)),
        }
    }
    for orphan in snapshot::orphans(&root, &generated).unwrap() {
        problems.push(format!("{orphan} is orphaned (no input generates it)"));
    }
    assert!(
        problems.is_empty(),
        "snapshot drift:\n{}\n{HINT}",
        problems.join("\n")
    );
}

#[test]
fn generation_is_deterministic_and_headers_are_versioned() {
    let root = conformance();
    let first = snapshot::generate(&root).unwrap();
    assert_eq!(first, snapshot::generate(&root).unwrap());
    for file in &first {
        let header = if file.path.starts_with("tokens/") {
            TOKEN_DUMP_HEADER
        } else {
            AST_DUMP_HEADER
        };
        assert!(
            file.contents.starts_with(&format!("{header}\n")),
            "{}",
            file.path
        );
        assert!(file.contents.ends_with('\n') && !file.contents.contains('\r'));
        assert!(
            file.contents.lines().all(|line| line == line.trim_end()),
            "trailing whitespace in {}",
            file.path
        );
    }
}

#[test]
fn dumps_contain_no_absolute_or_host_specific_paths() {
    let root = conformance().canonicalize().unwrap();
    let root_text = root.to_string_lossy().replace('\\', "/");
    for file in snapshot::generate(&root).unwrap() {
        let text = file.contents.replace('\\', "/");
        assert!(!text.contains(&root_text), "{} leaks the root", file.path);
        assert!(
            !text.contains("/Users/") && !text.contains("/home/") && !text.contains(":/"),
            "{}",
            file.path
        );
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn relocating_the_fixture_root_does_not_change_dumps() {
    let scratch = std::env::temp_dir().join(format!("ngspice-rs-snapshots-{}", std::process::id()));
    let copy = scratch.join("a b").join("conformance");
    let _ = fs::remove_dir_all(&scratch);
    for dir in snapshot::INPUT_DIRS {
        copy_tree(&conformance().join(dir), &copy.join(dir));
    }
    let moved = snapshot::generate(&copy).unwrap();
    let _ = fs::remove_dir_all(&scratch);
    assert_eq!(moved, snapshot::generate(&conformance()).unwrap());
}
