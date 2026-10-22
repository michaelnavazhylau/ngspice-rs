//! Opt-in BSIM4 end-to-end comparison with C: a matrix of model selectors
//! and analyses, run through `NGSPICE_BIN` and the port, compared vector by
//! vector by name. `NGSPICE_BIN` is usually the reference build, which fuses
//! multiply-adds, so this checks agreement to 1e-6 relative; bit-exactness is
//! judged against the FMA-free oracle with `fidelity-tools` (see
//! `docs/port/BSIM.md`) and pinned offline in `bsim4_device.rs`.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ngspice_rs::analysis::{Plot, RawFile, RunConfig, runner};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::source::parse_deck_text;

const NMOS: &str = r".model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
+ toxe=1.25e-9 toxp=1e-9 toxm=1.25e-9 epsrox=3.9 xl=-20e-9 lint=3.75e-9 wint=5e-9
+ vth0=0.46893 k1=0.4 dvt0=1 dvt1=2 dsub=0.1 minv=0.05 dvtp0=1e-9 dvtp1=0.1 xj=1.4e-8
+ ngate=2e23 ndep=3.24e18 nsd=2e20 voff=-0.13 nfactor=1.6 eta0=0.0125 vfb=-0.55 u0=0.049
+ ua=6e-10 ub=1.2e-18 vsat=130000 keta=0.04 pclm=0.02 pdiblc1=0.001 pdiblc2=0.001
+ pdiblcb=-0.005 drout=0.5 pvag=1e-20 delta=0.01 pscbe1=8.14e8 pscbe2=1e-7 fprout=0.2
+ pdits=0.08 pditsd=0.23 pditsl=2.3e6 rsh=5 rdsw=155 rsw=80 rdw=80 alpha0=0.074 alpha1=0.005
+ beta0=30 agidl=0.0002 bgidl=2.1e9 cgidl=0.0002 egidl=0.8 aigbacc=0.012 bigbacc=0.0028
+ cigbacc=0.002 aigbinv=0.014 bigbinv=0.004 cigbinv=0.004 eigbinv=1.1 nigbinv=3 aigc=0.012
+ bigc=0.0028 cigc=0.002 aigsd=0.012 bigsd=0.0028 cigsd=0.002 nigc=1 poxedge=1 pigcd=1
+ xrcrg1=12 xrcrg2=5 cgso=1.1e-10 cgdo=1.1e-10 cgbo=2.56e-11 cgdl=2.653e-10 cgsl=2.653e-10
+ ckappas=0.03 ckappad=0.03 kt1=-0.11 kt2=0.022 ute=-1.5 ua1=4.31e-9 ub1=7.61e-18
+ uc1=-5.6e-11 at=33000 jss=1e-4 jsws=1e-11 jswgs=1e-10 cjs=5e-4 cjsws=5e-10 cjswgs=3e-10
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15";
const PMOS: &str = r".model pch pmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=0 rbodymod=0 rdsmod=1
+ toxe=1.25e-9 toxp=1e-9 toxm=1.25e-9 epsrox=3.9 xl=-20e-9 lint=3.75e-9 wint=5e-9
+ vth0=-0.49158 k1=0.4 dvt0=1 dvt1=2 dsub=0.1 minv=0.05 dvtp0=1e-9 dvtp1=0.1 xj=1.4e-8
+ ngate=2e23 ndep=3.24e18 nsd=2e20 voff=-0.13 nfactor=1.6 eta0=0.0125 vfb=0.55 u0=0.021
+ ua=6e-10 ub=1.2e-18 vsat=130000 keta=0.04 pclm=0.02 pdiblc1=0.001 pdiblc2=0.001
+ pdiblcb=-0.005 drout=0.5 pvag=1e-20 delta=0.01 pscbe1=8.14e8 pscbe2=1e-7 fprout=0.2
+ pdits=0.08 pditsd=0.23 pditsl=2.3e6 rsh=5 rdsw=155 rsw=80 rdw=80 alpha0=0.074 alpha1=0.005
+ beta0=30 agidl=0.0002 bgidl=2.1e9 cgidl=0.0002 egidl=0.8 aigbacc=0.012 bigbacc=0.0028
+ cigbacc=0.002 aigbinv=0.014 bigbinv=0.004 cigbinv=0.004 eigbinv=1.1 nigbinv=3 aigc=0.012
+ bigc=0.0028 cigc=0.002 aigsd=0.012 bigsd=0.0028 cigsd=0.002 nigc=1 poxedge=1 pigcd=1
+ xrcrg1=12 xrcrg2=5 cgso=1.1e-10 cgdo=1.1e-10 cgbo=2.56e-11 cgdl=2.653e-10 cgsl=2.653e-10
+ ckappas=0.03 ckappad=0.03 kt1=-0.11 kt2=0.022 ute=-1.5 ua1=4.31e-9 ub1=7.61e-18
+ uc1=-5.6e-11 at=33000 jss=1e-4 jsws=1e-11 jswgs=1e-10 cjs=5e-4 cjsws=5e-10 cjswgs=3e-10
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15";

/// Model selectors appended to both cards, one deck family each.
const VARIANTS: &[(&str, &str)] = &[
    ("base", ""),
    ("rdsmod1", "rdsmod=1"),
    ("rgate2", "rgatemod=2"),
    ("rgate3", "rgatemod=3"),
    ("rbody0", "rbodymod=0"),
    (
        "rbody2",
        "rbodymod=2 rbps0=50 rbpd0=50 rbsbx0=100 rbsby0=100 rbdbx0=100 rbdby0=100",
    ),
    ("capmod0", "capmod=0"),
    ("capmod1", "capmod=1"),
    ("mob2", "mobmod=2"),
    ("mob4", "mobmod=4"),
    ("mob6", "mobmod=6"),
    ("temp1", "tempmod=1"),
    ("temp3", "tempmod=3"),
    ("mtrl1", "mtrlmod=1 eot=1.2e-9"),
    ("gidl1", "gidlmod=1"),
    ("dio2", "diomod=2"),
    ("nqs", "trnqsmod=1 acnqsmod=1"),
    ("noise", "tnoimod=2 fnoimod=0 kf=1e-26"),
];

const ANALYSES: &[(&str, &str)] = &[
    ("op", ".op"),
    ("dc", ".dc vg 0 1.1 0.1"),
    ("ac", ".ac dec 3 1e3 1e11"),
    ("noise", ".noise v(d) vg dec 3 1e3 1e10"),
    ("pz", ".pz g 0 d 0 vol pz"),
];

fn amplifier(extra: &str, analysis: &str) -> String {
    format!(
        "BSIM4 amplifier\nvdd dd 0 1.0\nrl dd d 1k\nvg g 0 dc 0.7 ac 1\nvb b 0 dc -0.1\n         m1 d g 0 b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u nf=2\n         cl d 0 2f\n{NMOS}\n+ {extra}\n{analysis}\n.end\n"
    )
}

fn inverter(extra: &str) -> String {
    format!(
        "BSIM4 inverter\nvdd dd 0 1.0\nvin in 0 pulse(0 1 0.1n 20p 20p 0.4n 1n)\n         mp out in dd dd pch l=45n w=1u ad=1e-13 as=1e-13 pd=2.2u ps=2.2u\n         mn out in 0 0 nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u\n         cl out 0 2f\n{NMOS}\n+ {extra}\n{PMOS}\n+ {extra}\n.tran 10p 1.2n\n.end\n"
    )
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn c_plots(binary: &Path, deck: &str, label: &str) -> Vec<Plot> {
    let dir =
        Scratch(std::env::temp_dir().join(format!("bsim4-dev-{}-{label}", std::process::id())));
    fs::create_dir_all(&dir.0).unwrap();
    // The reference build defaults to KLU; the port is SPARSE.
    fs::write(
        dir.0.join("c.cir"),
        deck.replace("\n.end\n", "\n.options sparse\n.end\n"),
    )
    .unwrap();
    let output = Command::new(binary)
        .args(["-b", "-r", "c.raw", "c.cir"])
        .current_dir(&dir.0)
        .output()
        .unwrap();
    let raw = RawFile::load(dir.0.join("c.raw")).unwrap_or_else(|e| {
        panic!(
            "{label}: C wrote no rawfile ({e}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    raw.plots.into_iter().map(|p| p.plot).collect()
}

fn rust_plots(deck: &str) -> Vec<Plot> {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("bsim4.cir"), deck))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run_plots(&mut circuit, &request, &config.context())
        .unwrap()
}

/// Every C vector against the port's of the same name; a transient is
/// compared on C's time grid by interpolation.
fn compare(c: &Plot, rust: &Plot, label: &str) {
    let transient = c.variable_index("time").is_some();
    let grid: Vec<f64> = rust
        .column("time")
        .map_or_else(Vec::new, |t| t.iter().map(|v| v.re).collect());
    for variable in &c.variables {
        let name = variable.name.as_str();
        let expected = c.column(name).unwrap();
        let actual = rust
            .column(name)
            .unwrap_or_else(|| panic!("{label}: port has no {name}"));
        let scale = expected
            .iter()
            .map(|v| v.re.abs().max(v.im.abs()))
            .fold(0.0, f64::max);
        // Picoamp branch currents are gmin-level cancellations, which the
        // FMA build moves by ~1e-18 A.
        let floor = 1e-6 * scale + if name.starts_with("i(") { 1e-15 } else { 1e-30 };
        if transient {
            let times = c.column("time").unwrap();
            for (time, value) in times.iter().zip(&expected) {
                let k = grid
                    .partition_point(|&t| t < time.re)
                    .clamp(1, grid.len() - 1);
                let (t0, t1) = (grid[k - 1], grid[k]);
                let at = actual[k - 1].re
                    + (actual[k].re - actual[k - 1].re) * (time.re - t0) / (t1 - t0);
                assert!(
                    (at - value.re).abs() <= 1e-3 * value.re.abs() + 1e-6 * scale + 1e-30,
                    "{label} {name} at {:e}: port {at:e}, C {:e}",
                    time.re,
                    value.re
                );
            }
            continue;
        }
        assert_eq!(actual.len(), expected.len(), "{label} {name}: point count");
        for (k, (a, e)) in actual.iter().zip(&expected).enumerate() {
            for (x, y, part) in [(a.re, e.re, "re"), (a.im, e.im, "im")] {
                if name == "frequency" && part == "im" {
                    continue;
                }
                assert!(
                    (x - y).abs() <= 1e-6 * y.abs() + floor,
                    "{label} {name}[{k}].{part}: port {x:e}, C {y:e}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires absolute NGSPICE_BIN; BSIM4 mode x analysis matrix"]
fn bsim4_mode_and_analysis_matrix_matches_c() {
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    assert!(binary.is_absolute());
    for (variant, extra) in VARIANTS {
        for (analysis, card) in ANALYSES {
            let label = format!("{variant}-{analysis}");
            let deck = amplifier(extra, card);
            let c = c_plots(&binary, &deck, &label);
            let rust = rust_plots(&deck);
            assert_eq!(c.len(), rust.len(), "{label}: plot count");
            for (c, rust) in c.iter().zip(&rust) {
                compare(c, rust, &label);
            }
        }
        let label = format!("{variant}-tran");
        let deck = inverter(extra);
        let c = c_plots(&binary, &deck, &label);
        compare(&c[0], &rust_plots(&deck)[0], &label);
    }
}
