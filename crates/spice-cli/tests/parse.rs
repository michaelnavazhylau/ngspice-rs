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
        ("rlc_series", 4, 0),
        ("diode_dc", 3, 1),
        ("bjt_ce", 4, 1),
        ("mos_inverter", 4, 1),
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
fn unported_fixtures_keep_exit_status_three() {
    for name in ["rc_transient", "subckt_divider"] {
        let output = run(name);
        assert_eq!(output.status.code(), Some(3), "{name}");
        assert!(
            output.stdout.is_empty(),
            "no success output for incomplete decks"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("not yet ported"));
    }
}

#[test]
fn missing_file_keeps_exit_status_two() {
    let output = run("does-not-exist");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
