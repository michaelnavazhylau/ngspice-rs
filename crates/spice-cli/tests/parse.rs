//! Process-level checks for the parser's success/gap/failure exit contract.

use std::path::Path;
use std::process::Command;

fn run(name: &str) -> std::process::Output {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/netlists")
        .join(format!("{name}.cir"));
    Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("parse")
        .arg(path)
        .output()
        .expect("run spice-rs")
}

#[test]
fn supported_fixtures_parse_successfully() {
    for (name, devices, models) in [
        ("rc_divider", 3, 0),
        ("rc_lowpass_ac", 3, 0),
        ("rc_transient", 3, 0),
        ("rlc_series", 4, 0),
        ("diode_dc", 3, 1),
        ("bjt_ce", 4, 1),
        ("mos_inverter", 4, 1),
        ("subckt_divider", 3, 0),
    ] {
        let output = run(name);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains(&format!("{devices} device instance(s)")),
            "{stdout}"
        );
        assert!(stdout.contains(&format!("{models} model(s)")), "{stdout}");
        assert!(stdout.contains("1 analysis request(s)"), "{stdout}");
    }
}

#[test]
fn param_fixture_parses_and_evaluates_top_level_values() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/parser/param_expressions.cir");
    let output = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("parse")
        .arg(path)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("6 device instance(s), 2 model(s), 1 subcircuit(s)"),
        "{text}"
    );
    assert!(text.contains("parameters: 8 definition(s)"), "{text}");
}

#[test]
fn param_failures_exit_with_a_located_error() {
    let dir = std::env::temp_dir().join(format!("spice-param-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.cir");
    std::fs::write(&path, "t\n.param a={1/0}\nr1 1 0 {a}\n.end\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("parse")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("division by zero") && stderr.contains("bad.cir:2:"),
        "{stderr}"
    );
}

#[test]
fn subcircuit_parse_success_is_not_flattening() {
    let output = run("subckt_divider");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 subcircuit(s)"));
}

#[test]
fn parse_command_resolves_source_relative_fragments_and_library_sections() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/parser/sources/main.cir");
    let output = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("parse")
        .arg(path)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("3 device instance(s), 1 model(s), 1 subcircuit(s), 1 analysis request(s)"),
        "{text}"
    );
}

#[test]
fn remaining_syntax_gaps_and_source_failures_keep_distinct_exits() {
    let dir = std::env::temp_dir().join(format!(
        "spice-cli-sources-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("deck.cir");
    for (text, status) in [
        // `.plot` is still `.`-card syntax this port does not run yet, so it
        // keeps the "not ported" exit ahead of a missing include. (`.save` and
        // `.print` are parsed and applied; see docs/port/OUTPUT_SELECTION.md.)
        ("title\n.plot dc v(a)\n.include missing.inc\n", 3),
        // `.param` now parses, so the missing source is the first failure.
        ("title\n.param x=1\n.include missing.inc\n", 2),
        ("title\n.include missing.inc\n", 2),
    ] {
        std::fs::write(&path, text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
            .arg("parse")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(status),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_file_keeps_exit_status_two() {
    let output = run("does-not-exist");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

fn run_text(name: &str, text: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!("spice-rs-options-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("deck.cir");
    std::fs::write(&path, text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .arg("parse")
        .arg(&path)
        .output()
        .expect("run spice-rs");
    let _ = std::fs::remove_dir_all(&dir);
    output
}

#[test]
fn deck_options_are_resolved_without_leaking_between_runs() {
    let hot = run_text(
        "hot",
        "t\nr1 a 0 1k\n.options temp=85 tnom=25 reltol=1m\n.global a\n.end\n",
    );
    assert_eq!(hot.status.code(), Some(0));
    let stdout = String::from_utf8(hot.stdout).unwrap();
    assert!(
        stdout.contains("options: 3 setting(s); TEMP = 85 C, TNOM = 25 C; 1 global node card(s)"),
        "{stdout}"
    );
    let plain = run_text("plain", "t\nr1 a 0 1k\n.end\n");
    let stdout = String::from_utf8(plain.stdout).unwrap();
    assert!(
        stdout.contains("0 setting(s); TEMP = 27 C, TNOM = 27 C"),
        "{stdout}"
    );
}

#[test]
fn unknown_and_unimplemented_options_fail_with_distinct_exits() {
    let unknown = run_text("unknown", "t\nr1 a 0 1k\n.options bogus=1\n.end\n");
    assert_eq!(unknown.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown option 'bogus'"));
    let pending = run_text("pending", "t\nr1 a 0 1k\n.options itl4=20\n.end\n");
    assert_eq!(pending.status.code(), Some(3));
}
