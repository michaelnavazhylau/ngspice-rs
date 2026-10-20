//! BSIM4 `.sens` (DC and AC) against values read from C.
//!
//! Every expected value is C's: printed by the `-ffp-contract=off` build of
//! the reference ngspice (`docs/port/VERIFICATION.md`) from the same deck
//! with `.options sparse`. On macOS arm64, where that oracle was built, the
//! port reproduces them bit for bit. Elsewhere `exp`/`log`/`pow` round
//! differently in the last place, and `.sens` divides differences of loads
//! by perturbations of about 1e-6 relative, which amplifies that rounding
//! about a millionfold, so the comparison there is 1e-4 relative.
//! C's NaN columns (a perturbation that makes `b4temp.c` reject the
//! geometry, and the AC frequencies after TNOM's per-frequency drift
//! overflows) are NaN here too. See `docs/port/SENSITIVITY.md`.
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use ngspice_rs::primitives::SpiceResult;

/// Exact on the oracle's platform.
const EXACT: bool = cfg!(all(target_os = "macos", target_arch = "aarch64"));

/// The PTM-like card: gate resistance, body network, gate tunnelling.
const CARD: &str =
    ".model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
+ toxe=1.25e-9 toxp=1e-9 toxm=1.25e-9 epsrox=3.9 xl=-20e-9 lint=3.75e-9 wint=5e-9
+ vth0=0.46893 k1=0.4 dvt0=1 dvt1=2 dsub=0.1 minv=0.05 dvtp0=1e-9 dvtp1=0.1 xj=1.4e-8
+ ngate=1e23 ndep=3.24e18 nsd=2e20 voff=-0.13 nfactor=1.6 eta0=0.0125 vfb=-0.55 u0=0.049
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

fn run(text: &str) -> SpiceResult<Plot> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(Path::new("bsim4.cir"), text))?;
    let config = RunConfig::from_netlist(&netlist)?;
    let request = config.request_for(&netlist.analyses[0])?;
    let mut circuit = config.circuit(&netlist)?;
    runner(request.kind)?.run(&mut circuit, &request, &config.context())
}

/// C's zero columns are `-0.0` or `0.0` by its own operation order; the
/// sign of a zero is not compared.
fn same(actual: f64, expected: f64, label: &str) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label}: Rust {actual:e}, C NaN");
    } else if EXACT {
        assert_eq!(
            actual.to_bits() & !(1 << 63),
            expected.to_bits() & !(1 << 63),
            "{label}: Rust {actual:e}, C {expected:e}"
        );
    } else {
        assert!(
            (actual - expected).abs() <= 1e-4 * expected.abs() + 1e-30,
            "{label}: Rust {actual:e}, C {expected:e}"
        );
    }
}

const AMPLIFIER: &str = "vdd dd 0 1.0\nrl dd d 1k\nvg g 0 dc 0.7 ac 1\nvs s 0 0\nvb b 0 dc -0.1\n";

const DC_COUNT: usize = 941;
const DC: &[(&str, f64)] = &[
    ("v(m1:toxe)", 300767668.68206185),
    ("v(m1:vth0)", 908961.6346485153),
    ("v(m1:k1)", -2299.5843037409963),
    ("v(m1:u0)", -0.22623318688961666),
    ("v(m1:tnom)", 33.767219415605595),
    ("v(m1:ndep)", 4.279842792646033e-18),
    ("v(m1:lint)", f64::NAN),
    ("v(m1:cgso)", 0.0),
    ("v(m1_l)", 283891.5009555118),
    ("v(m1_w)", -8543.279994266533),
    ("v(m1_nf)", 0.002012861297494201),
    ("v(m1_delvto)", 0.1704877353501178),
    ("v(m1_icvbs)", 0.0),
];
const AC_COUNT: usize = 956;
const AC: &[(&str, usize, f64, f64)] = &[
    (
        "v(m1:toxe)",
        0,
        -7.87269453211122e-11,
        -0.053975864309095324,
    ),
    ("v(m1:toxe)", 1, 0.0, -0.0),
    ("v(m1:cgdo)", 0, 4.490509393805432e-09, 3.078731490598824),
    ("v(m1:cgdo)", 1, 4.4903949573958985e-08, 9.735638408610413),
    ("v(m1:cf)", 0, 4.490509393681365e-09, 3.0787314905137633),
    ("v(m1:cf)", 1, 4.4903949571835215e-08, 9.73563840814996),
    ("v(m1:lcf)", 0, 2.566005368181313e-07, 175.92751376855787),
    ("v(m1:lcf)", 1, 2.5659399758311377e-06, 556.3221948159699),
    (
        "v(m1:dwc)",
        0,
        -4.341596352405108e-12,
        -0.0029766354409719235,
    ),
    (
        "v(m1:dwc)",
        1,
        -4.3414857292662925e-11,
        -0.009412787876634968,
    ),
    ("v(m1_w)", 0, 2.170798176202554e-12, 0.0014883177204859615),
    ("v(m1_w)", 1, 2.1707428553188725e-11, 0.0047063939181231805),
    ("v(m1:vth0)", 0, 0.0, -0.0),
    ("v(m1:vth0)", 1, 0.0, -0.0),
];

#[test]
fn dc_sensitivity_of_a_default_card_matches_c() {
    let plot = run(
        "BSIM4 sens\nvdd dd 0 1.0\nrl dd d 1k\nvg g 0 dc 0.7\nvb b 0 dc -0.1\n\
         m1 d g 0 b nch l=0.1u w=1u\n.model nch nmos level=54\n.sens v(d)\n.end\n",
    )
    .unwrap();
    // 869 askable model parameters, 33 instance ones, then the sources'.
    assert_eq!(plot.variable_count(), DC_COUNT);
    assert_eq!(plot.variable_index("v(m1:eot)"), Some(0));
    for &(name, value) in DC {
        same(plot.value(name, 0).unwrap().re, value, name);
    }
}

#[test]
fn ac_sensitivity_matches_c() {
    let plot = run(&format!(
        "BSIM4 sens ac\n{AMPLIFIER}\
         m1 d g s b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u\n{CARD}\n\
         + rgatemod=0 rbodymod=0 rsh=0\n.sens v(d) ac dec 2 1e3 1e9\n.end\n"
    ))
    .unwrap();
    assert_eq!(plot.variable_count(), AC_COUNT);
    for &(name, point, re, im) in AC {
        let value = plot.value(name, point).unwrap();
        same(value.re, re, &format!("{name}[{point}].re"));
        same(value.im, im, &format!("{name}[{point}].im"));
    }
}

/// C exits (`controlled_exit`) where a `BSIM4setup` re-run creates a node
/// (`cktsens.c`) and where `CKTunsetup` cannot delete the internal nodes a
/// re-run aliased; the port fails with C's message at the same points.
#[test]
fn cs_fatal_node_errors_are_reproduced() {
    let trnqs = run(&format!(
        "BSIM4 sens nqs\n{AMPLIFIER}\
         m1 d g s b nch l=45n w=0.5u\n{CARD}\n+ trnqsmod=1 rgatemod=0 rbodymod=0\n\
         .sens v(d)\n.end\n"
    ))
    .unwrap_err()
    .to_string();
    assert!(trnqs.contains("node allocation in DEVsetup()"), "{trnqs}");
    let gate = run(&format!(
        "BSIM4 sens rgate ac\n{AMPLIFIER}\
         m1 d g s b nch l=45n w=0.5u\n{CARD}\n.sens v(d) ac dec 1 1e3 1e5\n.end\n"
    ))
    .unwrap_err()
    .to_string();
    assert!(gate.contains("incomplete CKTunsetup()"), "{gate}");
}
