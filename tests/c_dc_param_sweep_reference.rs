//! Opt-in live C validation of `.dc @instance[parameter]` sweeps (#97,
//! `dctrcurv.c` `DCTfindInstParam`/`DCTsetInstParam`) beyond the committed
//! `m8_dc_param_*` goldens. `NGSPICE_BIN` must name the external reference
//! binary; run with `-- --ignored`. No committed oracle data is written.
//!
//! Rust and C run the same deck text. The swept scale (C `param-sweep`, or
//! `res-sweep`/`temp-sweep`/`v-sweep` for the other axis kinds), every node
//! voltage and every branch current C writes are compared by name. Linear
//! decks use the static-LU bound `1e-12 |C| + 1e-15`; nonlinear decks set
//! `.options reltol=1e-8` (C's default reltol with bypass stops a warm-started
//! sweep point up to ~4e-4 from its root) and use the `xtask::compare::NONLINEAR`
//! bound `1e-6 |C| + 1e-12`.
use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use std::{fs, path::Path, process::Command};

fn run_c(tag: &str, deck: &str) -> Plot {
    let binary = std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN");
    assert!(
        Path::new(&binary).is_absolute(),
        "NGSPICE_BIN must be absolute"
    );
    let dir = std::env::temp_dir().join(format!("spice-dc-param-{}-{tag}", std::process::id()));
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
        "{tag}: {}\n{}",
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

/// `(relative, absolute)` comparison bound.
const LINEAR: (f64, f64) = (1e-12, 1e-15);
const NONLINEAR: (f64, f64) = (1e-6, 1e-12);

fn compare(tag: &str, deck: &str, (relative, absolute): (f64, f64)) {
    let c = run_c(tag, deck);
    let rust = run_rust(deck);
    assert_eq!(rust.point_count(), c.point_count(), "{tag}: point counts");
    let scale = c.variables[0].name.clone();
    let mut compared = 0;
    for variable in &c.variables {
        let name = variable.name.to_ascii_lowercase();
        let ours_name = if name == scale.to_ascii_lowercase() {
            "sweep".to_owned()
        } else if name.starts_with("v(") || name.starts_with("i(") {
            name.clone()
        } else {
            continue;
        };
        compared += 1;
        for point in 0..c.point_count() {
            let ours = rust
                .value(&ours_name, point)
                .unwrap_or_else(|| panic!("{tag}: Rust has no {ours_name}"))
                .re;
            let theirs = c.value(&variable.name, point).unwrap().re;
            assert!(
                (ours - theirs).abs() <= relative * theirs.abs() + absolute,
                "{tag}: {name} at point {point}: Rust {ours:e}, C {theirs:e}"
            );
        }
    }
    assert!(compared >= 2, "{tag}: nothing compared");
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs temporary C decks out of process"]
fn diode_instance_parameter_sweeps_match_c() {
    let diode = "Diode parameter sweep\n.options reltol=1e-8\n\
                 v1 in 0 2\nr1 in a 1k\nd1 a 0 dx area=2 pj=1\n\
                 .model dx d(is=1e-14 rs=10 jsw=1e-15 cjo=1p cjsw=0.2p n=1.1)";
    for (tag, sweep) in [
        ("area", "@d1[area] 0.5 4 0.5"),
        ("area-temp", "@d1[area] 1 3 1 temp -20 80 50"),
        ("temp-area", "temp 80 -20 -50 @d1[area] 3 1 -1"),
        ("perim-m", "@d1[perim] 0 4 1 @d1[m] 1 3 1"),
        ("instance-temp", "@d1[temp] -40 125 15"),
        ("dtemp-source", "@d1[dtemp] 0 50 10 v1 1 3 1"),
    ] {
        compare(tag, &format!("{diode}\n.dc {sweep}"), NONLINEAR);
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs temporary C decks out of process"]
fn bjt_and_mos1_instance_parameter_sweeps_match_c() {
    let bjt = "BJT parameter sweep\n.options reltol=1e-8\n\
               vcc c 0 5\nvb b 0 0.75\nrc c cc 1k\nq1 cc b 0 qx areac=2\n\
               .model qx npn(is=1e-15 bf=100 br=2 rb=50 rc=5 re=1 ikf=10m ise=1e-14 isc=1e-14 vaf=50)";
    for (tag, sweep) in [
        // AREAB keeps bjtsetup.c's copy of the card AREA; AREAC is given.
        ("bjt-area", "@q1[area] 1 3 0.5"),
        ("bjt-areab-m", "@q1[areab] 1 2 0.5 @q1[m] 1 2 1"),
        ("bjt-dtemp", "@q1[dtemp] -20 40 20 vb 0.7 0.8 0.05"),
    ] {
        compare(tag, &format!("{bjt}\n.dc {sweep}"), NONLINEAR);
    }
    let mos = "MOS1 parameter sweep\n.options reltol=1e-8\n\
               vdd d 0 3\nvg g 0 1.5\nrd d dd 1k\nm1 dd g 0 0 nx w=10u l=2u nrd=2 nrs=1 ad=10p as=10p pd=12u ps=12u\n\
               .model nx nmos(vto=0.7 kp=50u gamma=0.4 phi=0.6 lambda=0.02 rsh=20 cj=1e-4 cjsw=1e-10 ld=0.1u)";
    for (tag, sweep) in [
        ("mos-w-l", "@m1[w] 5u 25u 5u @m1[l] 1u 3u 1u"),
        ("mos-m-nrd", "@m1[m] 1 3 1 @m1[nrd] 1 5 2"),
        ("mos-temp", "@m1[temp] -40 110 30"),
    ] {
        compare(tag, &format!("{mos}\n.dc {sweep}"), NONLINEAR);
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; runs temporary C decks out of process"]
fn linear_instance_parameter_sweeps_match_c() {
    // G/F: `m` given anywhere on the card multiplies a swept gain
    // (`VCCSparam`/`CCCSparam`); E/H store it as is.
    let controlled = "Controlled-source gain sweep\n\
                      v1 in 0 1\nr1 in 0 1k\n\
                      e1 eo 0 in 0 2\nre eo 0 1k\n\
                      g1 0 go in 0 1m m=3\nrg go 0 1k\n\
                      f1 0 fo v1 2 m=2\nrf fo 0 1k\n\
                      h1 ho 0 v1 100\nrh ho 0 1k";
    for (tag, sweep) in [
        ("vcvs", "@e1[gain] -2 2 0.5"),
        ("vccs-m", "@g1[gain] 1m 3m 1m"),
        ("cccs-m", "@f1[gain] 1 3 1 @h1[gain] 100 300 100"),
    ] {
        compare(tag, &format!("{controlled}\n.dc {sweep}"), LINEAR);
    }
    let passive = "Source and resistor parameters\n\
                   v1 in 0 2\ni1 0 out 1m\nr1 in out rm scale=2 m=4\nr2 out 0 1k\n\
                   .model rm r(r=1k tc1=0.01 tnom=27)";
    for (tag, sweep) in [
        ("vsrc-dc-isrc-c", "@v1[dc] 1 3 1 @i1[c] 0 2m 1m"),
        ("res-r-temp", "@r1[r] 1k 2k 500 temp 27 47 20"),
        ("res-resistance", "@r1[resistance] 2k 1k -250"),
    ] {
        compare(tag, &format!("{passive}\n.dc {sweep}"), LINEAR);
    }
}
