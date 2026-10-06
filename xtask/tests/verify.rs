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
    assert!(text.contains("3 verified fixture(s), 5 unsupported fixture(s), 0 failure(s)"));
    for name in ["rc_divider", "RC_LOWPASS_AC.cir", "rlc_series"] {
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
fn explicit_unsupported_requests_and_usage_errors_exit_nonzero() {
    for name in [
        "diode_dc",
        "bjt_ce",
        "mos_inverter",
        "rc_transient",
        "subckt_divider",
    ] {
        let output = verify(&["--netlist", name]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("requested unsupported fixture")
        );
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("0 verified fixture(s), 1 unsupported fixture(s)")
        );
    }
    for args in [
        vec!["--netlist", "unknown"],
        vec!["--netlist"],
        vec!["--ngspice", "ignored"],
        vec!["--netlist", "rc_divider", "--netlist", "rlc_series"],
    ] {
        assert!(!verify(&args).status.success(), "{args:?}");
    }
}
