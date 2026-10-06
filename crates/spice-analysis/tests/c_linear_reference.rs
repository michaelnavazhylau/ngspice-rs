//! Opt-in live C validation. NGSPICE_BIN must name the external reference binary.
use spice_analysis::{AnalysisContext, AnalysisRequest, RawFile, runner};
use spice_core::{AnalysisKind, Complex};
use spice_devices::{Circuit, IndependentSource, Waveform};
use spice_netlist::{Parser, source::parse_deck_text};
use std::{fs, path::Path, process::Command};

#[test]
#[ignore = "requires NGSPICE_BIN; compares different integrators on a common sample grid"]
fn rc_pwl_transient_matches_c_on_requested_samples() {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir = std::env::temp_dir().join(format!("spice-diffsol-reference-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let body = "v1 in 0 dc 0\nr1 in out 1k\nc1 out 0 1u";
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("rc.cir"),
            &format!("RC\n{body}\n.end\n"),
        ))
        .unwrap();
    let mut c = Circuit::from_netlist(&netlist).unwrap();
    let nodes = c.devices()[0].terminals();
    let nodes = [nodes[0], nodes[1]];
    c.devices_mut()[0] = Box::new(
        IndependentSource::new(
            "v1",
            nodes,
            true,
            0.,
            Complex::ZERO,
            Waveform::Pwl(vec![(0., 0.), (0.001, 0.), (0.00101, 1.), (0.006, 1.)]),
        )
        .unwrap(),
    );
    let request = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        [
            "0.0001",
            "0.006",
            "0",
            "0.00005",
            "backend=diffsol",
            "method=bdf",
        ],
    );
    let got = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &AnalysisContext::default())
        .unwrap();
    let c_deck = format!(
        "RC\n{}\n.control\nset filetype=ascii\ntran 10u 6m 0 1u\nwrite result.raw\nquit\n.endc\n.end\n",
        body.replace("v1 in 0 dc 0", "v1 in 0 PWL(0 0 1m 0 1.01m 1 6m 1)")
    );
    fs::write(dir.join("rc.cir"), c_deck).unwrap();
    let result = Command::new(binary)
        .args(["-b", "rc.cir"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let raw = RawFile::parse(&fs::read_to_string(dir.join("result.raw")).unwrap()).unwrap();
    let want = &raw.plots[0].plot;
    let time = want.column("time").unwrap();
    let voltage = want.column("v(out)").unwrap();
    for i in 0..got.point_count() {
        let t = got.value("time", i).unwrap().re;
        let upper = time.partition_point(|v| v.re < t).min(time.len() - 1);
        let lower = upper.saturating_sub(1);
        let value = if upper == lower {
            voltage[upper].re
        } else {
            let f = (t - time[lower].re) / (time[upper].re - time[lower].re);
            (1. - f) * voltage[lower].re + f * voltage[upper].re
        };
        let ours = got.value("v(out)", i).unwrap().re;
        assert!(
            (ours - value).abs() < 2e-5,
            "t={t}: diffsol={ours}, C={value}"
        );
    }
}
