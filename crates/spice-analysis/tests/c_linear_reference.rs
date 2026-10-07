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
                "{name} t={t}: diffsol={ours}, C={value}"
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

#[test]
#[ignore = "requires NGSPICE_BIN; coupled-capacitance index-one DAE against C on a common grid"]
fn coupled_capacitance_transient_matches_c_on_requested_samples() {
    let (c_source, waveform) = ramp();
    compare_with_c(
        "coupled",
        "v1 in 0 0\nr1 in a 1k\nc1 a 0 1u\nc12 a b 2u\nc2 b 0 1u\nr2 b 0 1k",
        c_source,
        waveform,
        &["v(a)", "v(b)"],
        2e-5,
    );
}

/// Runs the *same* deck text through the Rust parser/elaboration/diffsol BDF and
/// through C, comparing `v(out)` on Rust's requested grid. Grid points that are
/// source breakpoints (and the samples either side) are compared like any other;
/// C's own points are dense enough around its breakpoints that linear
/// interpolation never spans a jump (these decks are continuous or have the jump
/// strictly between C samples).
fn compare_deck_with_c(tag: &str, source: &str, breakpoints: &[f64]) {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir =
        std::env::temp_dir().join(format!("spice-wave-reference-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let body = format!("{source}\nr1 in out 1k\nc1 out 0 0.1u");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("wave.cir"),
            &format!("WAVE\n{body}\n.end\n"),
        ))
        .unwrap();
    let mut c = Circuit::from_netlist(&netlist).unwrap();
    let request = AnalysisRequest::with_arguments(
        AnalysisKind::Transient,
        ["10u", "8m", "0", "50u", "backend=diffsol", "method=bdf"],
    );
    let got = runner(request.kind)
        .unwrap()
        .run(&mut c, &request, &AnalysisContext::default())
        .unwrap();
    fs::write(
        dir.join("wave.cir"),
        format!(
            "WAVE\n{body}\n.control\nset filetype=ascii\ntran 10u 8m 0 1u\nwrite result.raw\nquit\n.endc\n.end\n"
        ),
    )
    .unwrap();
    let result = Command::new(binary)
        .args(["-b", "wave.cir"])
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
    let samples: Vec<f64> = (0..got.point_count())
        .map(|i| got.value("time", i).unwrap().re)
        .collect();
    for b in breakpoints {
        assert!(
            samples.iter().any(|t| (t - b).abs() < 1e-12),
            "grid lacks breakpoint {b}"
        );
    }
    for (i, t) in samples.iter().enumerate() {
        let upper = time.partition_point(|v| v.re < *t).min(time.len() - 1);
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
            "{tag} t={t}: diffsol={ours}, C={value}"
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; parsed PULSE deck against C on a common grid incl. breakpoints"]
fn parsed_pulse_rc_matches_c_on_requested_samples() {
    // Breakpoints 1m, 1.01m, 3.01m, 3.02m, 5m, ... lie on the 10 us grid.
    compare_deck_with_c(
        "pulse",
        "v1 in 0 pulse(0 1 1m 10u 10u 2m 4m)",
        &[1e-3, 1.01e-3, 3.01e-3, 3.02e-3, 5e-3],
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; parsed PWL deck against C on a common grid incl. breakpoints"]
fn parsed_pwl_rc_matches_c_on_requested_samples() {
    compare_deck_with_c(
        "pwl",
        "v1 in 0 pwl(0 0 1m 0 1.01m 1 3m 1 3.01m 0.25 8m 0.25)",
        &[1e-3, 1.01e-3, 3e-3, 3.01e-3],
    );
}
