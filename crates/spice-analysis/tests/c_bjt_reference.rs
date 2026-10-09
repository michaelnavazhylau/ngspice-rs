//! Opt-in live C validation of the Gummel-Poon BJT (#87) beyond the committed
//! `m7_bjt_*` goldens: substrate forward bias and lateral geometry, reverse
//! activity, saturation, IBE/IBC, NKF, AREAB/AREAC, TLEV/TLEVC variants and
//! temperature extremes. `NGSPICE_BIN` must name the external reference
//! binary; run with `-- --ignored`.
//!
//! Rust and C run the same deck text with `.options reltol=1e-8` (C's default
//! reltol with bypass stops about 4e-4 from the root of these sweeps). Every
//! node voltage and branch current C writes is compared by name at
//! `1e-6 |C| + 1e-12` per component, the `xtask::compare::NONLINEAR` bound.
use spice_analysis::{Plot, RawFile, RunConfig, runner};
use spice_netlist::{Parser, source::parse_deck_text};
use std::{fs, path::Path, process::Command};

fn run_c(tag: &str, deck: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    let dir = std::env::temp_dir().join(format!("spice-bjt-ref-{}-{tag}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    fs::write(
        dir.join("c.cir"),
        format!("{deck}\n.control\nset filetype=ascii\nrun\nwrite result.raw\nquit\n.endc\n.end\n"),
    )
    .unwrap();
    let result = Command::new(binary)
        .args(["-b", "c.cir"])
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
    raw.plots[0].plot.clone()
}

fn run_rust(deck: &str) -> Plot {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("c.cir"),
            &format!("{deck}\n.end\n"),
        ))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap()
}

fn compare(tag: &str, deck: &str) {
    let c = run_c(tag, deck);
    let rust = run_rust(deck);
    assert_eq!(rust.point_count(), c.point_count(), "{tag}: point counts");
    let mut compared = 0;
    for variable in &c.variables {
        let name = variable.name.to_ascii_lowercase();
        if !(name.starts_with("v(") || name.starts_with("i(")) || name.contains("-sweep") {
            continue;
        }
        compared += 1;
        for point in 0..c.point_count() {
            let ours = rust
                .value(&name, point)
                .unwrap_or_else(|| panic!("{tag}: Rust has no {name}"));
            let theirs = c.value(&name, point).unwrap();
            for (a, b) in [(ours.re, theirs.re), (ours.im, theirs.im)] {
                let bound = 1e-6 * b.abs() + 1e-12;
                assert!(
                    (a - b).abs() <= bound,
                    "{tag}: {name}[{point}] Rust {ours:?}, C {theirs:?}, bound {bound:e}"
                );
            }
        }
    }
    assert!(compared >= 2, "{tag}: nothing compared");
}

#[test]
#[ignore = "requires NGSPICE_BIN; Gummel-Poon substrate, geometry and reverse/saturation regions against live C"]
fn substrate_geometry_and_regions_match_c() {
    // Lateral PNP (default SUBS) whose substrate junction is swept into
    // forward bias, with AREAB/AREAC and ISS; then a vertical NPN in reverse
    // activity and saturation with IBE/IBC and the NKF knee.
    compare(
        "lateral",
        "lateral pnp substrate\nve e 0 0\nvb b 0 -0.7\nvc c 0 -2\nvs s 0 0\nq1 c b e s qp area=2 areab=3 areac=0.5\n.model qp pnp(is=1e-15 bf=50 br=2 vaf=30 ikf=5m iss=1e-16 ns=1.2 isc=1e-14 rb=50 rc=10 re=2)\n.options reltol=1e-8\n.dc vs -1 0.2 0.05",
    );
    compare(
        "reverse",
        "reverse and saturation\nvc c 0 0.2\nvb b 0 0.75\nve e 0 0\nq1 c b e qn\n.model qn npn(is=1e-15 ibe=2e-15 ibc=4e-15 bf=80 br=6 vaf=40 var=6 ikf=10m ikr=2m nkf=0.8 ise=1e-14 isc=2e-14 nc=1.6 rb=150 rbm=10 irb=50u re=1 rc=8)\n.options reltol=1e-8\n.dc vc -2 1 0.05",
    );
}

#[test]
#[ignore = "requires NGSPICE_BIN; Gummel-Poon temperature models against live C"]
fn temperature_models_match_c() {
    let bias = "vcc vcc 0 5\nrb vcc b 220k\nrc vcc c 2k\nre e 0 100";
    for (tag, model) in [
        (
            "tlev0",
            "is=1e-15 bf=150 xtb=1.8 xti=4 eg=1.15 ise=1e-14 ne=1.7 isc=1e-14 vaf=70 ikf=20m rb=100 trb1=3m trb2=1e-5 trm1=1m rbm=10 rc=5 trc1=2m re=1 tre2=1e-5 tnom=50",
        ),
        (
            "tlev1",
            "is=1e-15 bf=150 xtb=5m tlev=1 tbr1=2m ise=1e-14 tne1=1e-3 tnf1=2e-4 tnr2=1e-6 tvaf2=1e-6 tikf1=-2m ikf=20m vaf=70",
        ),
        (
            "tlev3",
            "is=1e-15 bf=150 tlev=3 tis1=-4e-4 tis2=1e-7 ise=1e-14 tise1=-3e-4 isc=1e-14 tisc2=1e-7 iss=1e-17 tiss1=-3e-4 tbf1=3m tbf2=-1e-5 ikr=1m tikr2=1e-6",
        ),
    ] {
        compare(
            tag,
            &format!(
                "temperature {tag}\n{bias}\nq1 c b e qn\n.model qn npn({model})\n.options reltol=1e-8\n.dc temp -55 150 25"
            ),
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; Gummel-Poon small-signal charges against live C"]
fn small_signal_charges_match_c() {
    // TLEVC=0 and TLEVC=1 capacitance temperature laws, XCJC, the substrate
    // charge of a lateral PNP and the bias-dependent transit time.
    for (tag, temp, model) in [
        (
            "tlevc0",
            "-20",
            "is=1e-15 bf=60 vaf=30 ikf=5m rb=80 rbm=10 irb=0.2m rc=10 re=2 cje=4p vje=0.8 mje=0.4 cjc=2p vjc=0.65 mjc=0.45 xcjc=0.3 cjs=3p vjs=0.6 mjs=0.4 tf=0.5n xtf=3 vtf=4 itf=20m tr=30n fc=0.65 tmje1=1e-3 tmjc2=1e-6",
        ),
        (
            "tlevc1",
            "120",
            "is=1e-15 bf=60 vaf=30 ikf=5m rb=80 rc=10 re=2 cje=4p cte=2m vje=0.8 tvje=1m cjc=2p ctc=1m tvjc=1m cjs=3p cts=2m tvjs=1m mjs=0.4 tlevc=1 tf=0.5n ttf1=1m xtf=3 itf=20m titf1=2m tr=30n ttr1=-1m",
        ),
    ] {
        compare(
            tag,
            &format!(
                "small signal {tag}\nvin in 0 dc 0 ac 1\nvcc vcc 0 5\nrs in b 1k\nrb b 0 100k\nre vcc e 4.3k\nrc c 0 2k\nvs s 0 -6\nq1 c b e s qp\n.model qp pnp({model})\n.options temp={temp} reltol=1e-8\n.ac dec 5 1k 10g"
            ),
        );
    }
}
