//! End-to-end remaining M8 front-end/output behavior.
use ngspice_rs::analysis::{RawFile, RunConfig};
use ngspice_rs::netlist::{Parser, source::parse_deck_text, write_netlist};
use std::{fs, path::Path, process::Command};

fn simulate(tag: &str, deck: &str) -> (RawFile, String) {
    let dir = std::env::temp_dir().join(format!("m8-output-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("deck.cir"), deck).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_spice-rs"))
        .args(["simulate", "--output"])
        .arg(dir.join("result.raw"))
        .arg(dir.join("deck.cir"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = RawFile::load(dir.join("result.raw")).unwrap();
    fs::remove_dir_all(dir).unwrap();
    (raw, String::from_utf8(output.stdout).unwrap())
}

#[test]
fn noise_prints_route_spectrum_and_integrated_vectors_with_retained_bias() {
    let deck = "noise output\nv1 in 0 dc 1 ac 1\nr1 in out 1k\nr2 out 0 1k\n.options keepopinfo\n.noise v(out) v1 dec 2 10 1k\n.print noise onoise_spectrum inoise_spectrum onoise_total\n.end\n";
    let (raw, report) = simulate("noise", deck);
    assert_eq!(raw.plots.len(), 3);
    assert_eq!(raw.plots[0].plot.plotname, "NOISE Operating Point");
    assert_eq!(raw.plots[0].plot.value("v(out)", 0).unwrap().re, 0.5);
    assert!(report.contains("onoise_spectrum"), "{report}");
    assert!(report.contains("onoise_total"), "{report}");
    assert_eq!(raw.plots[1].plot.variables.len(), 3);
    assert_eq!(raw.plots[2].plot.variables.len(), 1);
}

#[test]
fn sp_measurement_uses_frequency_and_named_complex_parameters() {
    let deck = "sp measurement\nv1 a 0 dc 1 portnum 1\nr1 a b 50\nv2 b 0 dc 0 portnum 2\nr2 a 0 100\nr3 b 0 100\n.options keepopinfo\n.sp lin 3 1k 3k 1\n.meas sp transmission max mag(S_2_1)\n.meas sp atfreq find real(S_2_1) at=2k\n.print sp mag(S_2_1) NF\n.end\n";
    let (raw, report) = simulate("sp", deck);
    assert_eq!(raw.plots.len(), 2);
    assert_eq!(raw.plots[0].plot.plotname, "AC Operating Point");
    assert!(report.contains("transmission"), "{report}");
    assert!(report.contains("atfreq"), "{report}");
    assert!(raw.plots[1].plot.variable_index("mag(s_2_1)").is_some());
    assert!(raw.plots[1].plot.variable_index("nf").is_some());
}

#[test]
fn frontend_settings_roundtrip_and_unsupported_commands_fail() {
    let deck = "settings\nv1 in 0 ac 1\nr1 in 0 1k\n.control\nset sqrnoise\nset ngbehavior=s3\n.endc\n.noise v(in) v1 lin 2 10 20\n.end\n";
    let parsed = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("settings.cir"), deck))
        .unwrap();
    let written = write_netlist(&parsed).unwrap();
    assert!(written.contains("set sqrnoise"), "{written}");
    let reparsed = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("settings.cir"), &written))
        .unwrap();
    let config = RunConfig::from_netlist(&reparsed).unwrap();
    let request = config.request_for(&reparsed.analyses[0]).unwrap();
    assert!(request.squared_noise && request.spice3_noise);
    for body in [
        ".control\nresume\n.endc",
        ".control\nset unknown\n.endc",
        ".control\nset sqrnoise",
        ".endc",
        ".control\n.control\n.endc",
        ".options sqrnoise",
    ] {
        let text = format!("bad setting\nv1 in 0 1\nr1 in 0 1k\n{body}\n");
        assert!(
            Parser::new()
                .parse_deck(&parse_deck_text(Path::new("bad.cir"), &text))
                .is_err(),
            "{body}"
        );
    }
}
