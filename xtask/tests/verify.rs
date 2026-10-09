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
    assert!(text.contains("86 verified fixture(s), 0 unsupported fixture(s), 0 failure(s)"));
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
        "m7_mos1_inverter_tran",
        "m7_mos1_ring_tran",
        "m7_mos1_meyer_ac",
        "m7_mos1_process_dc",
        "rc_sin_tran",
        "rc_exp_tran",
        "rc_sffm_am_tran",
        "rc_pwl_repeat_tran",
        "rc_pulse_count_tran",
        "options_gmin_dc",
        "options_xmu_tran",
        "multi_analysis_rc",
        "m6_gate",
        "controlled_op",
        "controlled_ac",
        "controlled_tran",
        "transformer_ac",
        "transformer_tran",
        "transformer_ic_uic_tran",
        "transformer_model_uic_tran",
        "switch_op",
        "switch_dc",
        "switch_tran",
        "switch_w_tran",
        "switch_dc_decimal",
        "switch_ac",
        "bsource_op",
        "bsource_dc",
        "bsource_ac",
        "bsource_tran",
        "evalue_op",
        "gtable_dc",
        "epoly_dc",
        "bsource_zero_op",
        "bsource_zero_dc",
        "bsource_zero_tran",
        "m7_zener_dc",
        "m7_zener_tran",
        "m7_diode_physics_dc",
        "m7_diode_temp_dc",
        "m7_diode_temp_ac",
        "m7_bjt_gummel",
        "m7_bjt_output",
        "m7_bjt_temp",
        "m7_bjt_amp_ac",
        "m7_bjt_amp_tran",
        "m7_conv_latch_op",
        "m7_conv_latch_gillespie_op",
        "m7_conv_latch_spice3_gmin_op",
        "m7_conv_latch_spice3_src_op",
        "m7_conv_latch_tran",
        "m7_conv_bjt_schmitt",
        "m7_conv_cmos_schmitt",
        "m7_ic_diode_uic_tran",
        "m7_ic_bjt_flipflop_tran",
        "m7_ic_bjt_off_tran",
        "m7_ic_mos1_uic_tran",
        "m7_ic_latch_nodeset_op",
        "m7_ic_latch_mos1_ic_op",
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
