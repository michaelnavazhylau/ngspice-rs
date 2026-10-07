//! #13 production file API, bounded resolution and source provenance.
use spice_core::SpiceError;
use spice_netlist::{Parser, SourceLimits, ast::ScopedCardKind, source::parse_deck_text};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "spice-sources-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
    fn parse(&self, name: &str) -> Result<spice_netlist::ast::Netlist, SpiceError> {
        Parser::new().parse_file(self.0.join(name))
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn syntax_only_retains_quoted_paths_without_io() {
    let deck = parse_deck_text(
        Path::new("absent.cir"),
        "title\n.inc 'missing dir/x.inc'\n.lib \"no.lib\" TT\n",
    );
    let n = Parser::new().parse_deck(&deck).unwrap();
    assert_eq!(n.includes[0].path, "missing dir/x.inc");
    assert_eq!(n.includes[0].path_spelling, "'missing dir/x.inc'");
    assert!(n.includes[0].resolved_path.is_none());
    assert_eq!(n.includes[1].section.as_deref(), Some("tt"));
    assert!(n.includes[1].selected_section.is_none());
    assert_eq!(n.cards[1].kind, ScopedCardKind::Include(1));
}

#[test]
fn relative_sources_expand_in_order_and_inherit_the_insertion_scope() {
    let f = Files::new();
    let root = f.write("main.cir", "Title\nr0 a 0 1k\n.subckt block a b\n.include 'dir/body.inc'\n.ends block\nX1 a 0 block\n.end\n.include missing\n");
    let body = f.write(
        "dir/body.inc",
        "Q1 c b e QM\n.include '../models.inc'\nr1 a b 2K\n+ tc1=0.01\n",
    );
    let models = f.write("models.inc", ".model QM NPN\n");
    let n = Parser::new().parse_file(&root).unwrap();
    assert_eq!(n.path, root);
    assert_eq!(
        n.devices
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["r0", "x1"]
    );
    let s = &n.subcircuits[0];
    assert_eq!(
        s.devices
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["q1", "r1"]
    );
    assert!(n.models.is_empty());
    assert_eq!(s.models[0].name, "qm");
    assert_eq!(
        s.cards.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ScopedCardKind::Include(0),
            ScopedCardKind::Device(0),
            ScopedCardKind::Include(1),
            ScopedCardKind::Model(0),
            ScopedCardKind::Device(1),
            ScopedCardKind::Ends
        ]
    );
    assert_eq!(
        s.includes[0].resolved_path.as_ref().unwrap(),
        &fs::canonicalize(body).unwrap()
    );
    assert_eq!(
        s.models[0].location.path(),
        fs::canonicalize(models).unwrap()
    );
    assert_eq!(
        s.models[0].location.line, 1,
        "fragment first line is not a title"
    );
    assert_eq!(
        s.cards[3]
            .include_chain
            .iter()
            .map(|l| l.line)
            .collect::<Vec<_>>(),
        [4, 2]
    );
    assert_eq!(
        s.devices[1].parameters[1].location.line, 3,
        "joined logical-card contract"
    );
    assert_eq!(s.devices[1].parameters[0].value, "2K");
}

#[test]
fn library_selects_only_requested_section_and_preserves_boundaries() {
    let f = Files::new();
    f.write(
        "main.cir",
        "title\n.lib 'libs/process.lib' tT\nr1 a 0 RES\n",
    );
    f.write("libs/process.lib", "* fragment comment\n.lib FF\n.include missing.inc\nmalformed {\n.endl ff\n.lib TT\n.lib 'nested.lib' BASE\n.endl TT\n");
    f.write("libs/nested.lib", ".lib base\n.model RES R RSH=1K\n.endl\n");
    let n = f.parse("main.cir").unwrap();
    assert_eq!(n.models[0].parameters[0].value, "1K");
    assert_eq!(n.devices[0].model.as_deref(), Some("res"));
    assert_eq!(n.includes.len(), 2);
    let section = n.includes[0].selected_section.as_ref().unwrap();
    assert_eq!(section.name, "tt");
    assert_eq!(section.opening.raw, ".lib TT");
    assert_eq!(section.opening.location.line, 6);
    assert_eq!(section.closing.raw, ".endl TT");
    assert_eq!(n.cards[2].include_chain.len(), 2);
}

#[test]
fn repeated_and_diamond_includes_are_not_cycles_or_deduplicated() {
    let f = Files::new();
    f.write(
        "main.cir",
        "title\n.include a.inc\n.include b.inc\n.include a.inc\n",
    );
    f.write("a.inc", ".include shared.inc\n");
    f.write("b.inc", ".include shared.inc\n");
    f.write("shared.inc", "r1 a 0 1K\n");
    let n = f.parse("main.cir").unwrap();
    assert_eq!(n.devices.len(), 3);
    assert_eq!(n.includes.len(), 6);
    assert_eq!(
        n.cards
            .iter()
            .filter(|c| matches!(c.kind, ScopedCardKind::Device(_)))
            .count(),
        3
    );
}

#[test]
fn canonical_file_and_section_dependencies_detect_cycles() {
    let f = Files::new();
    f.write("main.cir", "title\n.include a.inc\n");
    f.write("a.inc", ".include ./a.inc\n");
    assert!(
        f.parse("main.cir")
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    f.write("a.inc", ".include b.inc\n");
    f.write("b.inc", ".include a.inc\n");
    let e = f.parse("main.cir").unwrap_err();
    assert!(e.to_string().contains("b.inc:1:1"), "{e}");
    f.write("main.cir", "title\n.lib lib.inc a\n");
    f.write(
        "lib.inc",
        ".lib a\n.lib lib.inc b\n.endl a\n.lib b\nr1 a 0 1k\n.endl b\n",
    );
    assert_eq!(
        f.parse("main.cir").unwrap().devices.len(),
        1,
        "distinct sections can share a file"
    );
    f.write(
        "lib.inc",
        ".lib a\n.lib lib.inc b\n.endl a\n.lib b\n.lib lib.inc a\n.endl b\n",
    );
    assert!(
        f.parse("main.cir")
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
}

#[cfg(unix)]
#[test]
fn symlink_aliases_cannot_bypass_cycle_detection() {
    let f = Files::new();
    f.write("main.cir", "title\n.include a.inc\n");
    let a = f.write("a.inc", ".include alias.inc\n");
    std::os::unix::fs::symlink(a, f.0.join("alias.inc")).unwrap();
    assert!(
        f.parse("main.cir")
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
}

#[test]
fn library_boundary_and_missing_source_errors_are_positioned() {
    let f = Files::new();
    f.write("main.cir", "title\n.lib parts.lib tt\n");
    for (text, message) in [
        (".lib other\n.endl\n", "not found"),
        (".lib tt\n", "missing .endl"),
        (".lib tt\n.endl ff\n", "mismatched"),
        (".endl tt\n", "unmatched"),
        (".lib tt\n.lib nested\n.endl\n.endl\n", "nested"),
        (".lib tt\n.endl\n.lib TT\n.endl\n", "duplicate"),
    ] {
        f.write("parts.lib", text);
        let e = f.parse("main.cir").unwrap_err();
        assert!(matches!(e, SpiceError::Parse { .. }), "{e}");
        assert!(e.to_string().contains(message), "{e}");
    }
    f.write("main.cir", "title\n.include missing.inc\n");
    let e = f.parse("main.cir").unwrap_err();
    assert!(e.to_string().contains("main.cir:2:1"), "{e}");
    f.write("missing.inc", "r1 a 0 'unknown'\n");
    let e = f.parse("main.cir").unwrap_err();
    assert!(e.is_not_yet_ported());
    assert!(e.to_string().contains("missing.inc:1:"), "{e}");
}

#[test]
fn file_depth_byte_and_card_work_limits_include_repeated_sources() {
    let f = Files::new();
    let root = f.write("main.cir", "title\n.include a.inc\n.include a.inc\n");
    f.write("a.inc", "r1 a 0 1k\n");
    let defaults = SourceLimits::default();
    for (limits, message) in [
        (
            SourceLimits {
                max_depth: 0,
                ..defaults
            },
            "depth limit",
        ),
        (
            SourceLimits {
                max_files: 2,
                ..defaults
            },
            "file work limit",
        ),
        (
            SourceLimits {
                max_bytes: 36,
                ..defaults
            },
            "byte work limit",
        ),
        (
            SourceLimits {
                max_cards: 3,
                ..defaults
            },
            "card work limit",
        ),
        (
            SourceLimits {
                max_depth: 65,
                ..defaults
            },
            "at most 64",
        ),
        (
            SourceLimits {
                max_files: 0,
                ..defaults
            },
            "file work limit",
        ),
    ] {
        let e = Parser::new()
            .parse_file_with_limits(&root, limits)
            .unwrap_err();
        assert!(e.to_string().contains(message), "{e}");
    }
    let limits = SourceLimits {
        max_depth: 1,
        max_files: 3,
        max_bytes: 56,
        max_cards: 4,
    };
    assert_eq!(
        Parser::new()
            .parse_file_with_limits(root, limits)
            .unwrap()
            .devices
            .len(),
        2
    );
    f.write("a.inc", ".include b.inc\n");
    f.write("b.inc", "r1 a 0 1k\n");
    let e = Parser::new()
        .parse_file_with_limits(
            f.0.join("main.cir"),
            SourceLimits {
                max_depth: 1,
                ..defaults
            },
        )
        .unwrap_err();
    assert!(e.to_string().contains("depth limit"), "{e}");
    // A bounded read that cuts a multibyte token reports the byte budget,
    // rather than misreporting the truncated buffer as invalid UTF-8.
    let unicode = f.write("unicode.cir", "title\nÖ");
    let e = Parser::new()
        .parse_file_with_limits(
            unicode,
            SourceLimits {
                max_bytes: 7,
                ..defaults
            },
        )
        .unwrap_err();
    assert!(e.to_string().contains("byte work limit"), "{e}");
    fs::write(f.0.join("a.inc"), [0xff]).unwrap();
    let e = f.parse("main.cir").unwrap_err();
    assert!(e.to_string().contains("not UTF-8"), "{e}");
}

#[test]
fn end_and_error_order_survive_preprocessing() {
    let f = Files::new();
    f.write("main.cir", "title\n.save v(a)\n.include absent\n");
    let e = f.parse("main.cir").unwrap_err();
    assert!(e.is_not_yet_ported());
    assert!(e.to_string().contains("main.cir:2:1"));
    f.write(
        "main.cir",
        "title\n.include a.inc\n.include absent\nmalformed {\n",
    );
    f.write("a.inc", "r1 a 0 1k\n.end\n.include absent\n");
    assert_eq!(f.parse("main.cir").unwrap().devices.len(), 1);
    f.write("main.cir", "title\n.subckt s\n.include a.inc\n.ends\n");
    assert!(
        f.parse("main.cir")
            .unwrap_err()
            .to_string()
            .contains(".end before .ends")
    );
}

#[test]
fn malformed_source_cards_require_complete_consumption() {
    for body in [
        ".include",
        ".include ''",
        ".include a b",
        ".lib",
        ".lib a ''",
        ".lib a tt extra",
        ".endl",
        ".lib tt",
    ] {
        let deck = parse_deck_text(Path::new("bad.cir"), &format!("title\n{body}\n"));
        assert!(Parser::new().parse_deck(&deck).is_err(), "{body}");
    }
}
