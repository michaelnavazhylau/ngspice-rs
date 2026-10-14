//! Opt-in live C validation. NGSPICE_BIN must name the external reference binary.
use ngspice_rs::analysis::{AnalysisContext, AnalysisRequest, RawFile, runner};
use ngspice_rs::devices::{Circuit, IndependentSource, Waveform};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::{AnalysisKind, Complex};
use std::{fs, path::Path, process::Command};

/// Runs `body` in C with `v1` replaced by `c_source`, and in Rust with the
/// equivalent `rust_waveform`, then compares `vectors` on Rust's requested
/// sample grid (C is linearly interpolated between its own points).
fn compare_with_c(
    tag: &str,
    body: &str,
    c_source: &str,
    rust_waveform: Waveform,
    vectors: &[&str],
    tol: f64,
) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir =
        std::env::temp_dir().join(format!("spice-dae-reference-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("dae.cir"),
            &format!("DAE\n{body}\n.end\n"),
        ))
        .unwrap();
    let mut c = Circuit::from_netlist(&netlist).unwrap();
    let nodes = c.devices()[0].terminals();
    let nodes = [nodes[0], nodes[1]];
    c.devices_mut()[0] = Box::new(
        IndependentSource::new("v1", nodes, true, 0., Complex::ZERO, rust_waveform).unwrap(),
    );
    let request = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["0.0001", "0.006", "0", "0.00005"],
    );
    let got = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &AnalysisContext::default())
        .unwrap();
    let c_body = body.replacen("v1 in 0 0", &format!("v1 in 0 {c_source}"), 1);
    fs::write(
        dir.join("dae.cir"),
        format!(
            "DAE\n{c_body}\n.control\nset filetype=ascii\ntran 10u 6m 0 1u\nwrite result.raw\nquit\n.endc\n.end\n"
        ),
    )
    .unwrap();
    let result = Command::new(binary)
        .args(["-b", "dae.cir"])
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
    for name in vectors {
        let column = want.column(name).unwrap();
        for i in 0..got.point_count() {
            let t = got.value("time", i).unwrap().re;
            let upper = time.partition_point(|v| v.re < t).min(time.len() - 1);
            let lower = upper.saturating_sub(1);
            let value = if upper == lower {
                column[upper].re
            } else {
                let f = (t - time[lower].re) / (time[upper].re - time[lower].re);
                (1. - f) * column[lower].re + f * column[upper].re
            };
            let ours = got.value(name, i).unwrap().re;
            assert!(
                (ours - value).abs() < tol,
                "{name} t={t}: rust={ours}, C={value}"
            );
        }
    }
}

fn ramp() -> (&'static str, Waveform) {
    (
        "PWL(0 0 1m 0 1.01m 1 6m 1)",
        Waveform::Pwl(vec![(0., 0.), (0.001, 0.), (0.00101, 1.), (0.006, 1.)]),
    )
}

#[test]
#[ignore = "requires NGSPICE_BIN; floating-capacitor index-one DAE against C on a common grid"]
fn floating_capacitor_transient_matches_c_on_requested_samples() {
    let (c_source, waveform) = ramp();
    compare_with_c(
        "floating",
        "v1 in 0 0\nr1 in a 1k\nc1 a b 1u\nr2 b 0 1k",
        c_source,
        waveform,
        &["v(a)", "v(b)"],
        2e-5,
    );
}
