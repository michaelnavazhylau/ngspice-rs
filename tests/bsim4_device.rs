//! BSIM4 (`bsim4/`, levels 14/54) end to end against values read from C.
//!
//! Every expected value below is C's: printed by a `-ffp-contract=off` build
//! of the reference ngspice (the FMA-free oracle the port is judged against,
//! see `docs/port/VERIFICATION.md`) from the same deck with `.options
//! sparse`. On macOS arm64, where that oracle was built, the port reproduces
//! them bit for bit; other platforms' `exp`/`log`/`pow` round differently in
//! the last place, so there the comparison is relative (and a transient is
//! compared on C's time grid by interpolation, since its step sequence can
//! differ).
use std::path::Path;

use ngspice_rs::analysis::{Plot, RunConfig, runner};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};

/// Exact on the oracle's platform.
const EXACT: bool = cfg!(all(target_os = "macos", target_arch = "aarch64"));

fn plots(text: &str) -> Vec<Plot> {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("bsim4.cir"), text))
        .unwrap();
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    runner(request.kind)
        .unwrap()
        .run_plots(&mut circuit, &request, &config.context())
        .unwrap()
}

fn same(actual: f64, expected: f64, label: &str) {
    if EXACT {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{label}: Rust {actual:e}, C {expected:e}"
        );
    } else {
        assert!(
            (actual - expected).abs() <= 1e-9 * expected.abs() + 1e-30,
            "{label}: Rust {actual:e}, C {expected:e}"
        );
    }
}

/// Expected samples: `(vector, [(point, re, im)])`.
type Expected<'a> = &'a [(&'a str, &'a [(usize, f64, f64)])];

/// Compare `expected` with `plot`.
fn check(plot: &Plot, expected: Expected<'_>) {
    for (name, points) in expected {
        for &(index, re, im) in *points {
            let value = plot
                .value(name, index)
                .unwrap_or_else(|| panic!("{name}[{index}] missing"));
            same(value.re, re, &format!("{name}[{index}].re"));
            same(value.im, im, &format!("{name}[{index}].im"));
        }
    }
}

#[test]
fn an_amplifier_operating_point_with_gate_and_body_resistance_matches_c() {
    let plots = plots(
        r"BSIM4 nmos amplifier op
vdd dd 0 1.0
rl dd d 1k
vg g 0 dc 0.7 ac 1
vb b 0 dc -0.1
m1 d g 0 b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u
cl d 0 2f
.model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
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
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
.op
.end
",
    );
    check(
        &plots[plots.len() - 1],
        &[
            ("v(d)", &[(0, 0.9999705861861714, 0.0)]),
            ("v(m1#gate)", &[(0, 0.6999998704507737, 0.0)]),
            ("v(m1#body)", &[(0, -0.0999999999977343, 0.0)]),
            ("v(m1#dbody)", &[(0, -0.09999999999061691, 0.0)]),
            ("i(vdd)", &[(0, -2.941381382860417e-08, 0.0)]),
            ("i(vg)", &[(0, -4.957240801184426e-08, 0.0)]),
            ("i(vb)", &[(0, 1.2042311592352917e-12, 0.0)]),
        ],
    );
}

#[test]
fn a_pmos_sweep_with_rdsmod_matches_c() {
    let plots = plots(
        r"BSIM4 pmos dc sweep
vd d 0 -1.0
vg g 0 -0.8
vb b 0 0
m1 d g 0 b pch l=45n w=1u ad=1e-13 as=1e-13 pd=2.2u ps=2.2u
.model pch pmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=0 rbodymod=0 rdsmod=1
+ toxe=1.25e-9 toxp=1e-9 toxm=1.25e-9 epsrox=3.9 xl=-20e-9 lint=3.75e-9 wint=5e-9
+ vth0=-0.49158 k1=0.4 dvt0=1 dvt1=2 dsub=0.1 minv=0.05 dvtp0=1e-9 dvtp1=0.1 xj=1.4e-8
+ ngate=1e23 ndep=3.24e18 nsd=2e20 voff=-0.13 nfactor=1.6 eta0=0.0125 vfb=0.55 u0=0.021
+ ua=6e-10 ub=1.2e-18 vsat=130000 keta=0.04 pclm=0.02 pdiblc1=0.001 pdiblc2=0.001
+ pdiblcb=-0.005 drout=0.5 pvag=1e-20 delta=0.01 pscbe1=8.14e8 pscbe2=1e-7 fprout=0.2
+ pdits=0.08 pditsd=0.23 pditsl=2.3e6 rsh=5 rdsw=155 rsw=80 rdw=80 alpha0=0.074 alpha1=0.005
+ beta0=30 agidl=0.0002 bgidl=2.1e9 cgidl=0.0002 egidl=0.8 aigbacc=0.012 bigbacc=0.0028
+ cigbacc=0.002 aigbinv=0.014 bigbinv=0.004 cigbinv=0.004 eigbinv=1.1 nigbinv=3 aigc=0.012
+ bigc=0.0028 cigc=0.002 aigsd=0.012 bigsd=0.0028 cigsd=0.002 nigc=1 poxedge=1 pigcd=1
+ xrcrg1=12 xrcrg2=5 cgso=1.1e-10 cgdo=1.1e-10 cgbo=2.56e-11 cgdl=2.653e-10 cgsl=2.653e-10
+ ckappas=0.03 ckappad=0.03 kt1=-0.11 kt2=0.022 ute=-1.5 ua1=4.31e-9 ub1=7.61e-18
+ uc1=-5.6e-11 at=33000 jss=1e-4 jsws=1e-11 jswgs=1e-10 cjs=5e-4 cjsws=5e-10 cjswgs=3e-10
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
.dc vd 0 -1.1 -0.1 vg -0.2 -1.0 -0.4
.end
",
    );
    check(
        &plots[plots.len() - 1],
        &[
            (
                "i(vd)",
                &[
                    (0, -4.698249300880435e-13, 0.0),
                    (1, -2.8848082776755657e-13, 0.0),
                    (2, 2.0011162865651855e-13, 0.0),
                    (3, 3.9937450048432854e-12, 0.0),
                    (4, 1.5719142133774344e-11, 0.0),
                    (5, 4.4509299024220184e-11, 0.0),
                    (6, 1.0771938809689408e-10, 0.0),
                    (7, 2.3741278561140167e-10, 0.0),
                    (8, 4.915580269215791e-10, 0.0),
                    (9, 9.731717064143108e-10, 0.0),
                    (10, 1.8627751268568415e-09, 0.0),
                    (11, 3.473029299913555e-09, 0.0),
                    (12, -9.459212684732725e-11, 0.0),
                    (13, -5.0167509252699305e-11, 0.0),
                    (14, -2.942676397693633e-11, 0.0),
                    (15, -2.0499348746261248e-11, 0.0),
                    (16, -1.720797636328708e-11, 0.0),
                    (17, -1.695467117396321e-11, 0.0),
                    (18, -1.629506338890474e-11, 0.0),
                    (19, -1.2085905193304214e-11, 0.0),
                    (20, 6.558868032024989e-13, 0.0),
                    (21, 3.1941706224447586e-11, 0.0),
                    (22, 1.0130547442588345e-10, 0.0),
                    (23, 2.462529867519603e-10, 0.0),
                    (24, -1.4426207061203018e-09, 0.0),
                    (25, -4.602808884902887e-10, 0.0),
                    (26, 4.3758128977916533e-10, 0.0),
                    (27, 1.981040377747878e-09, 0.0),
                    (28, 5.329307298414243e-09, 0.0),
                    (29, 1.3020606438526872e-08, 0.0),
                    (30, 3.0677306498902435e-08, 0.0),
                    (31, 7.048384570614019e-08, 0.0),
                    (32, 1.5803724630857052e-07, 0.0),
                    (33, 3.454860840381574e-07, 0.0),
                    (34, 7.36397070898176e-07, 0.0),
                    (35, 1.5314999925412215e-06, 0.0),
                ],
            ),
            (
                "i(vg)",
                &[
                    (0, 3.395840844062104e-11, 0.0),
                    (1, 3.387718260723604e-11, 0.0),
                    (2, 3.348859323409553e-11, 0.0),
                    (3, 2.9794959765097096e-11, 0.0),
                    (4, 1.8169564120238296e-11, 0.0),
                    (5, -1.0520594123863188e-11, 0.0),
                    (6, -7.363068077722269e-11, 0.0),
                    (7, -2.0322407308918642e-10, 0.0),
                    (8, -4.5726930809313697e-10, 0.0),
                    (9, -9.387829682338297e-10, 0.0),
                    (10, -1.828286337423671e-09, 0.0),
                    (11, -3.4384403893984485e-09, 0.0),
                    (12, 1.8918474803878574e-10, 0.0),
                    (13, 1.446537252601513e-10, 0.0),
                    (14, 1.239935110504209e-10, 0.0),
                    (15, 1.151713789430064e-10, 0.0),
                    (16, 1.1200563010878294e-10, 0.0),
                    (17, 1.1192021113975812e-10, 0.0),
                    (18, 1.1152928761328447e-10, 0.0),
                    (19, 1.0783436570347637e-10, 0.0),
                    (20, 9.620834014743269e-11, 0.0),
                    (21, 6.75180042769319e-11, 0.0),
                    (22, 4.408074310185751e-12, 0.0),
                    (23, -1.2518489543677594e-10, 0.0),
                    (24, 2.8852456140587166e-09, 0.0),
                    (25, 2.2925295873020617e-09, 0.0),
                    (26, 1.971923577036471e-09, 0.0),
                    (27, 1.8011121915060844e-09, 0.0),
                    (28, 1.712733711507052e-09, 0.0),
                    (29, 1.6688662850276985e-09, 0.0),
                    (30, 1.6484356033103489e-09, 0.0),
                    (31, 1.6400068663973062e-09, 0.0),
                    (32, 1.6376349639107245e-09, 0.0),
                    (33, 1.639088540792057e-09, 0.0),
                    (34, 1.6415077902182087e-09, 0.0),
                    (35, 1.642603494944735e-09, 0.0),
                ],
            ),
            (
                "i(vb)",
                &[
                    (0, -3.3018758580444966e-11, 0.0),
                    (1, -3.3118877145261054e-11, 0.0),
                    (2, -3.3218879627333216e-11, 0.0),
                    (3, -3.331887967898806e-11, 0.0),
                    (4, -3.341887967912605e-11, 0.0),
                    (5, -3.3518879676816765e-11, 0.0),
                    (6, -3.3618879671696773e-11, 0.0),
                    (7, -3.371887966119084e-11, 0.0),
                    (8, -3.3818879640603406e-11, 0.0),
                    (9, -3.3918879601585496e-11, 0.0),
                    (10, -3.4018879529530114e-11, 0.0),
                    (11, -3.411887939946352e-11, 0.0),
                    (12, -4.943437839710951e-16, 0.0),
                    (13, -1.0061290626658785e-13, 0.0),
                    (14, -2.006153872705148e-13, 0.0),
                    (15, -3.006154387933681e-13, 0.0),
                    (16, -4.006154398537483e-13, 0.0),
                    (17, -5.006154401796507e-13, 0.0),
                    (18, -6.006154409121922e-13, 0.0),
                    (19, -7.006154427905164e-13, 0.0),
                    (20, -8.006154479171626e-13, 0.0),
                    (21, -9.006154622338383e-13, 0.0),
                    (22, -1.0006163366082205e-12, 0.0),
                    (23, -1.1006638133709037e-12, 0.0),
                    (24, -4.201818251867337e-15, 0.0),
                    (25, -1.04320705983433e-13, 0.0),
                    (26, -2.043245070361024e-13, 0.0),
                    (27, -3.043287077574737e-13, 0.0),
                    (28, -4.043408473434049e-13, 0.0),
                    (29, -5.04374662053732e-13, 0.0),
                    (30, -6.044649655572871e-13, 0.0),
                    (31, -7.046964571322348e-13, 0.0),
                    (32, -8.052656581745423e-13, 0.0),
                    (33, -9.066090448747916e-13, 0.0),
                    (34, -1.0590582320904471e-12, 0.0),
                    (35, -3.3601435771140522e-12, 0.0),
                ],
            ),
        ],
    );
}

#[test]
fn an_amplifier_ac_sweep_matches_c() {
    let plots = plots(
        r"BSIM4 nmos amplifier ac
vdd dd 0 1.0
rl dd d 1k
vg g 0 dc 0.7 ac 1
vb b 0 dc -0.1
m1 d g 0 b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u
cl d 0 2f
.model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
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
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
.ac dec 2 1e3 1e11
.end
",
    );
    check(
        &plots[plots.len() - 1],
        &[
            (
                "v(d)",
                &[
                    (0, -0.0008149422745999804, 9.656407433251895e-10),
                    (1, -0.0008149422745998404, 3.053624150365628e-09),
                    (2, -0.0008149422745984419, 9.656407433251651e-09),
                    (3, -0.0008149422745844554, 3.053624150364846e-08),
                    (4, -0.0008149422744445909, 9.656407433226896e-08),
                    (5, -0.0008149422730459447, 3.053624150286558e-07),
                    (6, -0.0008149422590594834, 9.656407430751204e-07),
                    (7, -0.0008149421191948711, 3.0536241424577277e-06),
                    (8, -0.0008149407205487845, 9.656407183181829e-06),
                    (9, -0.0008149267340915033, 3.05362335957481e-05),
                    (10, -0.0008147868698772298, 9.65638242630851e-05),
                    (11, -0.000813388263587329, 0.00030535450733100243),
                    (12, -0.0007994057850446247, 0.0009653907379792161),
                    (13, -0.0006599385104503515, 0.0030457366636184495),
                    (14, 0.0006998846101100228, 0.009412649141023886),
                    (15, 0.01152921903237344, 0.024254819835180227),
                    (16, 0.04248110279439063, 0.0268943254413958),
                ],
            ),
            (
                "v(m1#gate)",
                &[
                    (0, 0.9999988781449966, -7.409893147379739e-12),
                    (1, 0.9999988781449966, -2.343213956419371e-11),
                    (2, 0.9999988781449966, -7.409893147379736e-11),
                    (3, 0.9999988781449966, -2.343213956419359e-10),
                    (4, 0.9999988781449966, -7.409893147379355e-10),
                    (5, 0.9999988781449963, -2.3432139564181496e-09),
                    (6, 0.9999988781449941, -7.409893147341097e-09),
                    (7, 0.999998878144972, -2.3432139562971636e-08),
                    (8, 0.999998878144751, -7.409893143515178e-08),
                    (9, 0.99999887814254, -2.3432139441985512e-07),
                    (10, 0.9999988781204312, -7.409892760924358e-07),
                    (11, 0.9999988778993476, -2.3432127343686204e-06),
                    (12, 0.9999988756890668, -7.409854511745255e-06),
                    (13, 0.9999988536415069, -2.3430920638006488e-05),
                    (14, 0.9999986385514804, -7.40612612431005e-05),
                    (15, 0.9999969155494833, -0.000233350666665371),
                    (16, 0.9999916376823916, -0.000730222284564066),
                ],
            ),
            (
                "i(vg)",
                &[
                    (0, -4.2928125132402073e-07, -2.8354182961912278e-12),
                    (1, -4.2928125132402073e-07, -8.96637993527821e-12),
                    (2, -4.2928125132402073e-07, -2.8354182961912267e-11),
                    (3, -4.2928125132402073e-07, -8.966379935278162e-11),
                    (4, -4.2928125132402073e-07, -2.835418296191081e-10),
                    (5, -4.292812513795319e-07, -8.966379935273535e-10),
                    (6, -4.292812522677103e-07, -2.835418296176441e-09),
                    (7, -4.292812607054053e-07, -8.966379934810579e-09),
                    (8, -4.2928134530439976e-07, -2.8354182947124414e-08),
                    (9, -4.292821913498557e-07, -8.966379888514867e-08),
                    (10, -4.292906513048145e-07, -2.8354181483128933e-07),
                    (11, -4.293752496331571e-07, -8.966375259063602e-07),
                    (12, -4.3022102036438525e-07, -2.83540351214742e-06),
                    (13, -4.3865758664329846e-07, -8.965913509441262e-06),
                    (14, -5.209624437396698e-07, -2.8339768332819077e-05),
                    (15, -1.180274432410755e-06, -8.929234693827975e-05),
                    (16, -3.1998664317689496e-06, -0.00027942179256278045),
                ],
            ),
        ],
    );
}

#[test]
fn an_amplifier_noise_spectrum_matches_c() {
    let plots = plots(
        r"BSIM4 nmos amplifier noise
vdd dd 0 1.0
rl dd d 1k
vg g 0 dc 0.7 ac 1
vb b 0 dc -0.1
m1 d g 0 b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u
cl d 0 2f
.model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
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
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
+ fnoimod=1 tnoimod=0
.noise v(d) vg dec 2 1e3 1e10
.end
",
    );
    check(
        &plots[0],
        &[
            (
                "onoise_spectrum",
                &[
                    (0, 4.199670531762844e-09, 0.0),
                    (1, 4.112290825325946e-09, 0.0),
                    (2, 4.084269902376846e-09, 0.0),
                    (3, 4.0753688128962985e-09, 0.0),
                    (4, 4.072549992502159e-09, 0.0),
                    (5, 4.071658197050695e-09, 0.0),
                    (6, 4.071376145455259e-09, 0.0),
                    (7, 4.071286944249942e-09, 0.0),
                    (8, 4.071258690017071e-09, 0.0),
                    (9, 4.071249295921326e-09, 0.0),
                    (10, 4.071241732433363e-09, 0.0),
                    (11, 4.071193413464332e-09, 0.0),
                    (12, 4.07071895273175e-09, 0.0),
                    (13, 4.0659861743993295e-09, 0.0),
                    (14, 4.019549821845343e-09, 0.0),
                ],
            ),
            (
                "inoise_spectrum",
                &[
                    (0, 5.153334981697116e-06, 0.0),
                    (1, 5.046113023546762e-06, 0.0),
                    (2, 5.0117290873180236e-06, 0.0),
                    (3, 5.0008067284439015e-06, 0.0),
                    (4, 4.9973477774088336e-06, 0.0),
                    (5, 4.996253165264318e-06, 0.0),
                    (6, 4.995903994453115e-06, 0.0),
                    (7, 4.995763830989816e-06, 0.0),
                    (8, 4.9954221297406886e-06, 0.0),
                    (9, 4.992343408738236e-06, 0.0),
                    (10, 4.961969853301214e-06, 0.0),
                    (11, 4.685907941066204e-06, 0.0),
                    (12, 3.2477233968782885e-06, 0.0),
                    (13, 1.3047006391897054e-06, 0.0),
                    (14, 4.2586141547643547e-07, 0.0),
                ],
            ),
        ],
    );
}

#[test]
fn correlated_induced_gate_noise_and_fnoimod0_match_c() {
    let plots = plots(
        r"BSIM4 nmos amplifier noise tnoimod=2 fnoimod=0
vdd dd 0 1.0
rl dd d 1k
vg g 0 dc 0.7 ac 1
vb b 0 dc -0.1
m1 d g 0 b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u
cl d 0 2f
.model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
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
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
+ fnoimod=0 tnoimod=2 kf=1e-26 af=1.2
.noise v(d) vg dec 2 1e3 1e10
.end
",
    );
    check(
        &plots[0],
        &[
            (
                "onoise_spectrum",
                &[
                    (0, 3.661757067955003e-08, 0.0),
                    (1, 2.0864895753813926e-08, 0.0),
                    (2, 1.2206506792550163e-08, 0.0),
                    (3, 7.645164962976936e-09, 0.0),
                    (4, 5.460223496980085e-09, 0.0),
                    (5, 4.556206158633518e-09, 0.0),
                    (6, 4.23031486871428e-09, 0.0),
                    (7, 4.1219001979014346e-09, 0.0),
                    (8, 4.087017957665427e-09, 0.0),
                    (9, 4.075924638306615e-09, 0.0),
                    (10, 4.072405740798044e-09, 0.0),
                    (11, 4.071246410360117e-09, 0.0),
                    (12, 4.070420592803762e-09, 0.0),
                    (13, 4.06557717752874e-09, 0.0),
                    (14, 4.01911080163919e-09, 0.0),
                ],
            ),
            (
                "inoise_spectrum",
                &[
                    (0, 4.493271710256814e-05, 0.0),
                    (1, 2.5602912505566974e-05, 0.0),
                    (2, 1.4978369845530384e-05, 0.0),
                    (3, 9.381234961099819e-06, 0.0),
                    (4, 6.700135248683372e-06, 0.0),
                    (5, 5.59083261413214e-06, 0.0),
                    (6, 5.190934513382048e-06, 0.0),
                    (7, 5.0578699574857536e-06, 0.0),
                    (8, 5.014758703599811e-06, 0.0),
                    (9, 4.9980765174339315e-06, 0.0),
                    (10, 4.96338852966438e-06, 0.0),
                    (11, 4.685968940028816e-06, 0.0),
                    (12, 3.247485357718604e-06, 0.0),
                    (13, 1.3045693995701894e-06, 0.0),
                    (14, 4.2581490236558934e-07, 0.0),
                ],
            ),
        ],
    );
}

#[test]
fn an_instance_width_sweep_matches_c() {
    let plots = plots(
        r"BSIM4 nmos width sweep
vdd dd 0 1.0
rl dd d 1k
vg g 0 dc 0.7 ac 1
vb b 0 dc -0.1
m1 d g 0 b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u
cl d 0 2f
.model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
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
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
.dc @m1[w] 0.3u 1.2u 0.3u
.end
",
    );
    check(
        &plots[plots.len() - 1],
        &[
            (
                "v(d)",
                &[
                    (0, 0.9999838404829531, 0.0),
                    (1, 0.99996338429206, 0.0),
                    (2, 0.9999397329553387, 0.0),
                    (3, 0.9999133970827645, 0.0),
                ],
            ),
            (
                "i(vdd)",
                &[
                    (0, -1.6159517046959032e-08, 0.0),
                    (1, -3.6615707940061e-08, 0.0),
                    (2, -6.026704466141793e-08, 0.0),
                    (3, -8.660291723550442e-08, 0.0),
                ],
            ),
            (
                "i(vb)",
                &[
                    (0, 1.2023923523507563e-12, 0.0),
                    (1, 1.2051956654879348e-12, 0.0),
                    (2, 1.208321637191645e-12, 0.0),
                    (3, 1.2117043479697998e-12, 0.0),
                ],
            ),
        ],
    );
}

#[test]
fn a_temperature_sweep_checks_the_model_once_as_c_does() {
    let plots = plots(
        r"BSIM4 nmos temperature sweep
vdd dd 0 1.0
rl dd d 1k
vg g 0 dc 0.7 ac 1
vb b 0 dc -0.1
m1 d g 0 b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u 
cl d 0 2f
.model nch nmos level=54 version=4.8.3 binunit=1 paramchk=1 mobmod=0 capmod=2 igcmod=1 igbmod=1 geomod=1 diomod=1 rdsmod=0 rbodymod=1 rgatemod=1 permod=1 acnqsmod=0 trnqsmod=0 tnom=27 toxe=1.25e-9 toxp=1e-9 toxm=1.25e-9 dtox=0.25e-9 epsrox=3.9 wint=5e-9 lint=3.75e-9 ll=0 wl=0 lln=1 wln=1 lw=0 ww=0 lwn=1 wwn=1 lwl=0 wwl=0 xpart=0 toxref=1.25e-9 xl=-20e-9 vth0=0.46893 k1=0.4 k2=0 k3=0 k3b=0 w0=2.5e-6 dvt0=1 dvt1=2 dvt2=0 dvt0w=0 dvt1w=0 dvt2w=0 dsub=0.1 minv=0.05 voffl=0 dvtp0=1.0e-9 dvtp1=0.1 lpe0=0 lpeb=0 xj=1.4e-8 ngate=1e23 ndep=3.24e18 nsd=2e20 phin=0 cdsc=0 cdscb=0 cdscd=0 cit=0 voff=-0.13 nfactor=1.6 eta0=0.0125 etab=0 vfb=-0.55 u0=0.049 ua=6e-10 ub=1.2e-18 uc=0 vsat=130000 a0=1.0 ags=0 a1=0 a2=1.0 b0=0 b1=0 keta=0.04 dwg=0 dwb=0 pclm=0.02 pdiblc1=0.001 pdiblc2=0.001 pdiblcb=-0.005 drout=0.5 pvag=1e-20 delta=0.01 pscbe1=8.14e8 pscbe2=1e-7 fprout=0.2 pdits=0.08 pditsd=0.23 pditsl=2.3e6 rsh=5 rdsw=155 rsw=80 rdw=80 rdswmin=0 rdwmin=0 rswmin=0 prwg=0 prwb=0 wr=1 alpha0=0.074 alpha1=0.005 beta0=30 agidl=0.0002 bgidl=2.1e9 cgidl=0.0002 egidl=0.8 aigbacc=0.012 bigbacc=0.0028 cigbacc=0.002 nigbacc=1 aigbinv=0.014 bigbinv=0.004 cigbinv=0.004 eigbinv=1.1 nigbinv=3 aigc=0.012 bigc=0.0028 cigc=0.002 aigsd=0.012 bigsd=0.0028 cigsd=0.002 nigc=1 poxedge=1 pigcd=1 ntox=1 xrcrg1=12 xrcrg2=5 cgso=1.1e-10 cgdo=1.1e-10 cgbo=2.56e-11 cgdl=2.653e-10 cgsl=2.653e-10 ckappas=0.03 ckappad=0.03 acde=1 moin=15 noff=0.9 voffcv=0.02 kt1=-0.11 kt1l=0 kt2=0.022 ute=-1.5 ua1=4.31e-9 ub1=7.61e-18 uc1=-5.6e-11 prt=0 at=33000 fnoimod=1 tnoimod=0 jss=0.0001 jsws=1e-11 jswgs=1e-10 njs=1 ijthsfwd=0.01 ijthsrev=0.001 bvs=10 xjbvs=1 jsd=0.0001 jswd=1e-11 jswgd=1e-10 njd=1 ijthdfwd=0.01 ijthdrev=0.001 bvd=10 xjbvd=1 pbs=1 cjs=0.0005 mjs=0.5 pbsws=1 cjsws=5e-10 mjsws=0.33 pbswgs=1 cjswgs=3e-10 mjswgs=0.33 pbd=1 cjd=0.0005 mjd=0.5 pbswd=1 cjswd=5e-10 mjswd=0.33 pbswgd=1 cjswgd=5e-10 mjswgd=0.33 tpb=0.005 tcj=0.001 tpbsw=0.005 tcjsw=0.001 tpbswg=0.005 tcjswg=0.001 xtis=3 xtid=3 dmcg=0 dmci=0 dmdg=0 dmcgt=0 dwj=0 xgw=0 xgl=0 rshg=0.4 gbmin=1e-10 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15 ngcon=1
+ tempmod=2
.dc temp -40 125 15
.end
",
    );
    check(
        &plots[plots.len() - 1],
        &[
            (
                "v(d)",
                &[
                    (0, 0.43812264624363184, 0.0),
                    (1, 0.4414985646293975, 0.0),
                    (2, 0.44469205406362333, 0.0),
                    (3, 0.44771842561716907, 0.0),
                    (4, 0.4505981808617301, 0.0),
                    (5, 0.4253135347103079, 0.0),
                    (6, 0.42883467742299974, 0.0),
                    (7, 0.43302369053135387, 0.0),
                    (8, 0.43663560466754225, 0.0),
                    (9, 0.4401135436409742, 0.0),
                    (10, 0.443474136185905, 0.0),
                    (11, 0.44673165277067584, 0.0),
                ],
            ),
            (
                "i(vdd)",
                &[
                    (0, -0.0005618773537563682, 0.0),
                    (1, -0.0005585014353706025, 0.0),
                    (2, -0.0005553079459363766, 0.0),
                    (3, -0.000552281574382831, 0.0),
                    (4, -0.0005494018191382699, 0.0),
                    (5, -0.0005746864652896921, 0.0),
                    (6, -0.0005711653225770003, 0.0),
                    (7, -0.0005669763094686461, 0.0),
                    (8, -0.0005633643953324578, 0.0),
                    (9, -0.0005598864563590258, 0.0),
                    (10, -0.0005565258638140951, 0.0),
                    (11, -0.0005532683472293242, 0.0),
                ],
            ),
            (
                "i(vg)",
                &[
                    (0, -4.512612616913714e-08, 0.0),
                    (1, -4.5377883894914817e-08, 0.0),
                    (2, -4.563003819235689e-08, 0.0),
                    (3, -4.5881131116587426e-08, 0.0),
                    (4, -4.613110010653898e-08, 0.0),
                    (5, -4.471196196620397e-08, 0.0),
                    (6, -4.497362215971634e-08, 0.0),
                    (7, -4.5278107263513334e-08, 0.0),
                    (8, -4.555715032994456e-08, 0.0),
                    (9, -4.583229457066906e-08, 0.0),
                    (10, -4.610268539151363e-08, 0.0),
                    (11, -4.6367549522141616e-08, 0.0),
                ],
            ),
        ],
    );
}

#[test]
fn pole_zero_with_cs_rerun_node_aliasing_matches_c() {
    let plots = plots(
        r"BSIM4 nmos amplifier pole-zero
vdd dd 0 1.0
rl dd d 1k
vg g 0 dc 0.7 ac 1
vb b 0 dc -0.1
m1 d g 0 b nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u
cl d 0 2f
.model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
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
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
+ rdsmod=1
.pz g 0 d 0 vol pz
.end
",
    );
    check(
        &plots[plots.len() - 1],
        &[
            ("v(pole(1))", &[(0, -6053142460299076.0, 0.0)]),
            ("v(pole(2))", &[(0, -355301372814519.06, 0.0)]),
            ("v(pole(3))", &[(0, -225165653823292.66, 0.0)]),
            ("v(pole(4))", &[(0, -390468895610.38446, 0.0)]),
            ("v(zero(1))", &[(0, -6470055569291622.0, 0.0)]),
            ("v(zero(2))", &[(0, -892897503598708.8, 0.0)]),
            ("v(zero(3))", &[(0, -224753488017596.1, 0.0)]),
            ("v(zero(4))", &[(0, -12051800326981.66, 0.0)]),
            ("v(zero(5))", &[(0, 5351248617.656635, 0.0)]),
        ],
    );
}

#[test]
fn a_cmos_inverter_with_nqs_transient_matches_c() {
    let plots = plots(
        r"BSIM4 cmos inverter transient
vdd dd 0 1.0
vin in 0 pulse(0 1 0.1n 20p 20p 0.4n 1n)
mp out in dd dd pch l=45n w=1u ad=1e-13 as=1e-13 pd=2.2u ps=2.2u
mn out in 0 0 nch l=45n w=0.5u ad=5e-14 as=5e-14 pd=1.2u ps=1.2u
cl out 0 2f
.model nch nmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=1 rbodymod=1
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
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
+ trnqsmod=1
.model pch pmos level=54 version=4.8.3 capmod=2 igcmod=1 igbmod=1 rgatemod=0 rbodymod=0 rdsmod=1
+ toxe=1.25e-9 toxp=1e-9 toxm=1.25e-9 epsrox=3.9 xl=-20e-9 lint=3.75e-9 wint=5e-9
+ vth0=-0.49158 k1=0.4 dvt0=1 dvt1=2 dsub=0.1 minv=0.05 dvtp0=1e-9 dvtp1=0.1 xj=1.4e-8
+ ngate=1e23 ndep=3.24e18 nsd=2e20 voff=-0.13 nfactor=1.6 eta0=0.0125 vfb=0.55 u0=0.021
+ ua=6e-10 ub=1.2e-18 vsat=130000 keta=0.04 pclm=0.02 pdiblc1=0.001 pdiblc2=0.001
+ pdiblcb=-0.005 drout=0.5 pvag=1e-20 delta=0.01 pscbe1=8.14e8 pscbe2=1e-7 fprout=0.2
+ pdits=0.08 pditsd=0.23 pditsl=2.3e6 rsh=5 rdsw=155 rsw=80 rdw=80 alpha0=0.074 alpha1=0.005
+ beta0=30 agidl=0.0002 bgidl=2.1e9 cgidl=0.0002 egidl=0.8 aigbacc=0.012 bigbacc=0.0028
+ cigbacc=0.002 aigbinv=0.014 bigbinv=0.004 cigbinv=0.004 eigbinv=1.1 nigbinv=3 aigc=0.012
+ bigc=0.0028 cigc=0.002 aigsd=0.012 bigsd=0.0028 cigsd=0.002 nigc=1 poxedge=1 pigcd=1
+ xrcrg1=12 xrcrg2=5 cgso=1.1e-10 cgdo=1.1e-10 cgbo=2.56e-11 cgdl=2.653e-10 cgsl=2.653e-10
+ ckappas=0.03 ckappad=0.03 kt1=-0.11 kt2=0.022 ute=-1.5 ua1=4.31e-9 ub1=7.61e-18
+ uc1=-5.6e-11 at=33000 jss=1e-4 jsws=1e-11 jswgs=1e-10 cjs=5e-4 cjsws=5e-10 cjswgs=3e-10
+ rshg=0.4 rbpb=5 rbpd=15 rbps=15 rbdb=15 rbsb=15
.tran 10p 1.5n
.end
",
    );
    let plot = &plots[plots.len() - 1];
    let expected: Expected<'_> = &[
        (
            "v(out)",
            &[
                (0, 0.4326136200914745, 0.0),
                (14, 0.4326136200914692, 0.0),
                (28, 0.5850160562735105, 0.0),
                (42, 0.5846000131132125, 0.0),
                (56, 0.5841934390363397, 0.0),
                (70, 0.4317991778197369, 0.0),
                (84, 0.43151709940748423, 0.0),
                (98, 0.4315333412352194, 0.0),
                (112, 0.431548704471911, 0.0),
                (126, 0.4315633814439762, 0.0),
                (140, 0.5839553160109582, 0.0),
                (154, 0.5835620001169497, 0.0),
                (168, 0.5831776047445385, 0.0),
            ],
        ),
        (
            "i(vdd)",
            &[
                (0, -2.491891044298499e-08, 0.0),
                (14, -2.4918910440101965e-08, 0.0),
                (28, -1.7600865334162664e-07, 0.0),
                (42, -6.642591862508502e-08, 0.0),
                (56, -1.6288591572752634e-08, 0.0),
                (70, -3.5142250085139375e-05, 0.0),
                (84, -2.0264532047705027e-08, 0.0),
                (98, -3.150622044888738e-08, 0.0),
                (112, -3.672219834932513e-08, 0.0),
                (126, -3.8436128852936875e-08, 0.0),
                (140, -1.7577919346778187e-07, 0.0),
                (154, -6.625078752319023e-08, 0.0),
                (168, -1.6139970647656302e-08, 0.0),
            ],
        ),
    ];
    // C's accepted time points at those indices.
    let indices = [0, 14, 28, 42, 56, 70, 84, 98, 112, 126, 140, 154, 168];
    let times = [
        0.0,
        7.28e-11,
        1.5499999999999998e-10,
        2.95e-10,
        4.3500000000000026e-10,
        5.4e-10,
        6.549999999999996e-10,
        7.949999999999991e-10,
        9.349999999999987e-10,
        1.0749999999999983e-09,
        1.155e-09,
        1.2949999999999996e-09,
        1.434999999999999e-09,
    ];
    if EXACT {
        for (&index, &time) in indices.iter().zip(&times) {
            same(plot.value("time", index).unwrap().re, time, "time");
        }
        check(plot, expected);
        return;
    }
    // Elsewhere: compare on C's grid by interpolating the port's waveform.
    let grid: Vec<f64> = (0..)
        .map_while(|i| plot.value("time", i).map(|v| v.re))
        .collect();
    for (name, points) in expected {
        for (&(_, value, _), &time) in points.iter().zip(&times) {
            let k = grid.partition_point(|&t| t < time).clamp(1, grid.len() - 1);
            let (t0, t1) = (grid[k - 1], grid[k]);
            let (v0, v1) = (
                plot.value(name, k - 1).unwrap().re,
                plot.value(name, k).unwrap().re,
            );
            let at = v0 + (v1 - v0) * (time - t0) / (t1 - t0);
            assert!(
                (at - value).abs() <= 1e-3 * value.abs() + 1e-6,
                "{name} at {time:e}: {at:e} vs {value:e}"
            );
        }
    }
}
