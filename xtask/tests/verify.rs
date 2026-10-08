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
    assert!(text.contains("42 verified fixture(s), 0 unsupported fixture(s), 0 failure(s)"));
    for name in [
        "rc_divider",
        "RC_LOWPASS_AC.cir",
        "rc_transient",
        "rlc_series",
        "subckt_divider",
        "func_quotes",
        "rl_pulse_tran",
        "rc_gear_tran",
        "rc_pwl_tran",
        "rlc_series_tran",
        "rlc_series_gear_tran",
        "floating_cap_tran",
        "coupled_cap_tran",
        "rc_ic_uic_tran",
        "rlc_ic_uic_tran",
        "rc_ic_node_tran",
        "floating_cap_ic_tran",
        "rlc_series_ac",
        "diode_dc",
        "bjt_ce",
        "mos_inverter",
        "m4_diode_ac",
        "m4_diode_tran",
        "m4_bjt_ac",
        "m4_bjt_tran",
        "m4_mos1_ac",
        "m4_mos1_tran",
        "rc_sin_tran",
        "rc_exp_tran",
        "rc_sffm_am_tran",
        "rc_pwl_repeat_tran",
        "rc_pulse_count_tran",
        "options_gmin_dc",
        "options_xmu_tran",
        "multi_analysis_rc",
        "controlled_op",
        "controlled_ac",
        "controlled_tran",
        "transformer_ac",
        "transformer_tran",
        "transformer_ic_uic_tran",
        "transformer_model_uic_tran",
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
