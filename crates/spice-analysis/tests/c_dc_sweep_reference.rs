//! Opt-in #35 live C checks; no committed oracle data is overwritten.
use spice_analysis::{RawFile, RunConfig, runner};
use spice_core::AnalysisKind;
use spice_netlist::{Parser, source::parse_deck_text};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs temporary C decks out of process"]
fn typed_resistor_temperature_and_nested_source_sweeps_match_c() {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set absolute NGSPICE_BIN");
    assert!(Path::new(&binary).is_absolute());
    let literal = "v1 in 0 2\nr1 in out 1k\nr2 out 0 1k";
    let modeled =
        "v1 in 0 2\nr1 in out rm scale=2 m=4\nr2 out 0 1k\n.model rm r(r=1k tc1=0.01 tnom=27)";
    let fixed = "v1 in 0 2\nr1 in out rm temp=77 scale=2 m=4\nr2 out 0 1k\n.model rm r(r=1k tc1=0.01 tnom=27)";
    let geometry = "v1 in 0 2\nr1 in out rm l=2u w=1u scale=2 m=4\nr2 out 0 1k\n.model rm r(rsh=100 tc1=0.01 tnom=27)";
    let sources = "v1 in 0 1\ni1 out 0 0\nr1 in out 1k\nr2 out 0 1k";
    for (tag, body, sweep) in [
        ("literal-forward", literal, "r1 1k 2k 500"),
        ("literal-reverse", literal, "r1 2k 1k -500"),
        ("model-r-temp", modeled, "r1 1k 2k 1k temp 27 47 20"),
        ("model-temp-r", modeled, "temp 47 27 -20 r1 2k 1k -1k"),
        ("fixed-instance-temp", fixed, "r1 1k 2k 1k temp 27 47 20"),
        ("geometry-replaced", geometry, "r1 1k 2k 1k temp 27 47 20"),
        ("negative-resistance", modeled, "r1 -3k -4k -500"),
        ("nested-sources", sources, "i1 1m -1m -1m v1 2 1 -1"),
    ] {
        let dir =
            std::env::temp_dir().join(format!("spice-dc-oracle-{}-{tag}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let _cleanup = Cleanup(dir.clone());
        let netlist = Parser::new()
            .parse_deck(&parse_deck_text(
                Path::new("sweep.cir"),
                &format!("DC oracle\n{body}\n.dc {sweep}\n.end\n"),
            ))
            .unwrap();
        let config = RunConfig::from_netlist(&netlist).unwrap();
        let request = config.request_for(&netlist.analyses[0]).unwrap();
        assert_eq!(request.kind, AnalysisKind::DcSweep);
        let mut c = config.circuit(&netlist).unwrap();
        let got = runner(request.kind)
            .unwrap()
            .run(&mut c, &request, &config.context())
            .unwrap();
        fs::write(dir.join("sweep.cir"), format!(
            "DC oracle\n{body}\n.control\nset filetype=ascii\ndc {sweep}\nwrite result.raw\nquit\n.endc\n.end\n"
        )).unwrap();
        let result = Command::new(&binary)
            .args(["-b", "sweep.cir"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{tag}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let raw = RawFile::parse(&fs::read_to_string(dir.join("result.raw")).unwrap()).unwrap();
        let want = &raw.plots[0].plot;
        assert_eq!(
            got.point_count(),
            want.point_count(),
            "{tag}: loop/grid order"
        );
        for (ours, reference) in [
            ("sweep", want.variables[0].name.as_str()),
            ("v(in)", "v(in)"),
            ("v(out)", "v(out)"),
            ("i(v1)", "i(v1)"),
        ] {
            for row in 0..got.point_count() {
                let a = got.value(ours, row).unwrap().re;
                let b = want.value(reference, row).unwrap().re;
                // Static LU/temperature arithmetic, not a relaxed nonlinear bound.
                assert!(
                    (a - b).abs() <= 1e-12 * b.abs() + 1e-15,
                    "{tag}, {ours}, row {row}: Rust={a:e}, C={b:e}"
                );
            }
        }
    }
}
