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
        ("title\n.param x=1\n.include missing.inc\n", 3),
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
