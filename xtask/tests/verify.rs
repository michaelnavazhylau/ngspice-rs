//! Process-level success/failure contract; deliberately unavailable C binary.
use std::process::{Command, Output};

fn verify(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["golden", "verify"])
        .args(args)
        .env("NGSPICE_BIN", "/definitely/unavailable/ngspice")
        .output()
        .expect("run xtask")
}

#[test]
fn default_and_selected_verification_need_no_c_binary() {
    let output = verify(&[]);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("137 verified fixture(s), 0 unsupported fixture(s), 0 failure(s)"));
    // The default run above already verifies every fixture. Selection is
    // checked on a few representatives (exact name, case-insensitive name with
    // extension, a multi-analysis batch deck, an initial-condition deck), so
    // new fixtures do not need to be added here.
    for name in [
        "rc_divider",
        "RC_LOWPASS_AC.cir",
        "multi_analysis_rc",
        "m7_ic_latch_nodeset_op",
    ] {
        let output = verify(&["--netlist", name]);
        assert!(output.status.success(), "{output:?}");
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("1 verified fixture(s), 0 unsupported fixture(s)")
        );
    }
}

#[test]
fn explicit_bad_requests_and_usage_errors_exit_nonzero() {
    // No committed fixture is excluded any more, so an unknown name is the
    // remaining "not verifiable" case, and it must never pass silently.
    for args in [
        vec!["--netlist", "unknown"],
        vec!["--netlist"],
        vec!["--ngspice", "ignored"],
        vec!["--netlist", "rc_divider", "--netlist", "rlc_series"],
    ] {
        assert!(!verify(&args).status.success(), "{args:?}");
    }
    let output = verify(&["--netlist", "unknown"]);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no fixture named 'unknown'"), "{stderr}");
}
