//! Diode physics (#86) through the production analysis paths, checked against
//! independent closed forms rather than C. C-golden comparisons of the same
//! physics (`m7_*` fixtures) live in `xtask golden verify`.
use spice_analysis::{Plot, RunConfig, runner};
use spice_netlist::{Parser, source::parse_deck_text};
use std::path::Path;

const K_OVER_Q: f64 = 1.38064852e-23 / 1.6021766208e-19;
const GMIN: f64 = 1e-12;

fn simulate(body: &str) -> Plot {
    let n = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("diode_physics.cir"),
            &format!("diode physics\n{body}\n.end\n"),
        ))
        .unwrap();
    let config = RunConfig::from_netlist(&n).unwrap();
    let mut circuit = config.circuit(&n).unwrap();
    let request = config.request_for(&n.analyses[0]).unwrap();
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap()
}
fn real(plot: &Plot, name: &str, point: usize) -> f64 {
    plot.value(name, point).unwrap().re
}

/// `diotemp.c` breakdown matching, stopped like C at its default RELTOL.
fn matched_breakdown(bv: f64, ibv: f64, is: f64, vt: f64, nbv: f64) -> f64 {
    let mut x = bv - nbv * vt * (1. + ibv / is).ln();
    for _ in 0..25 {
        x = bv - nbv * vt * (ibv / is + 1. - x / vt).ln();
        let matched = is * (((bv - x) / (nbv * vt)).exp() - 1. + x / vt);
        if (matched - ibv).abs() <= 1e-3 * ibv {
            break;
        }
    }
    x
}

#[test]
fn zener_regulator_operating_point_satisfies_the_breakdown_law() {
    let p = simulate(
        "vin in 0 dc 12\nrs in out 100\ndz 0 out dz\nrl out 0 2k\n\
         .model dz d(is=5e-14 n=1.05 bv=5.1 ibv=5m nbv=1.3)\n.op",
    );
    let vout = real(&p, "v(out)", 0);
    assert!((5.1..5.4).contains(&vout), "{vout}");
    // Current from anode (ground) to cathode (out) through the junction.
    let id = -((12. - vout) / 100. - vout / 2000.);
    let vt = K_OVER_Q * 300.15;
    let xbv = matched_breakdown(5.1, 5e-3, 5e-14, vt, 1.3);
    let vd = -vout;
    let law = -5e-14 * (-(xbv + vd) / (1.3 * vt)).exp() + GMIN * vd;
    assert!((id - law).abs() <= 1e-7 * law.abs(), "{id} vs {law}");
    // The knee: the junction carries IBV at V = -BV.
    let at_bv = -5e-14 * (-(xbv - 5.1) / (1.3 * vt)).exp();
    assert!((at_bv + 5e-3).abs() < 1e-6 * 5e-3, "{at_bv}");
}

#[test]
fn temperature_sweep_follows_the_saturation_current_law() {
    // A forward diode on a 1 mA current source with series resistance, TNOM
    // 35 C, EG/XTI and TRS: v = N Vt ln(I/IS(T) + 1) + I RS(T).
    let p = simulate(
        "i1 0 a dc 1m\nd1 a 0 dm\n\
         .model dm d(is=2e-14 n=1.05 eg=1.2 xti=2.5 tnom=35 rs=5 trs1=3m)\n\
         .dc temp -40 125 15",
    );
    assert_eq!(p.point_count(), 12);
    let tn = 35. + 273.15;
    let mut previous = f64::INFINITY;
    for point in 0..p.point_count() {
        let celsius = -40. + 15. * point as f64;
        let t = celsius + 273.15;
        let vte = 1.05 * K_OVER_Q * t;
        let is = 2e-14 * ((t / tn - 1.) * 1.2 / vte + 2.5 / 1.05 * (t / tn).ln()).exp();
        let rs = 5. * (1. + 3e-3 * (t - tn));
        let v = real(&p, "v(a)", point);
        // Solve the junction (with gmin) for its voltage independently.
        let current = 1e-3;
        let mut vd = vte * (current / is + 1.).ln();
        for _ in 0..50 {
            let f = is * ((vd / vte).exp() - 1.) + GMIN * vd - current;
            vd -= f / (is * (vd / vte).exp() / vte + GMIN);
        }
        let expected = vd + current * rs;
        assert!(
            (v - expected).abs() < 1e-7,
            "{celsius} C: {v} vs {expected}"
        );
        assert!(v < previous, "forward voltage falls when heated");
        previous = v;
    }
}

#[test]
fn sidewall_current_and_charge_add_like_a_wider_bottom_junction() {
    // With a shared characteristic, JSW*PJ adds to IS*AREA and CJSW*PJ (same
    // potential and grading) to CJO*AREA, at any temperature.
    for temperature in ["27", "100"] {
        let p = simulate(&format!(
            ".options temp={temperature}\n\
             i1 0 a dc 1m ac 1\nd1 a 0 dsw pj=10\n\
             i2 0 b dc 1m ac 1\nd2 b 0 dsum\n\
             .model dsw d(is=1e-14 jsw=1e-15 cjo=10p cjsw=1p vj=0.8 vjsw=0.8 m=0.4 mjsw=0.4 \
             fc=0.5 fcs=0.5)\n\
             .model dsum d(is=2e-14 cjo=20p vj=0.8 m=0.4 fc=0.5)\n\
             .ac dec 3 1k 1g",
        ));
        for point in 0..p.point_count() {
            let (a, b) = (
                p.value("v(a)", point).unwrap(),
                p.value("v(b)", point).unwrap(),
            );
            assert!((a - b).magnitude() <= 1e-9 * b.magnitude(), "{a} vs {b}");
        }
    }
}

#[test]
fn transient_conserves_the_temperature_adjusted_depletion_charge() {
    // A reverse ramp charges only depletion charge (TLEVC 1 linear laws at
    // 77 C). The source's integrated current equals the closed-form charge
    // change of the bottom and sidewall junctions (leakage is negligible).
    let p = simulate(
        ".options temp=77\n\
         v1 in 0 pwl(0 0 1u 3 5u 3)\nr1 in k 1k\nd1 0 k dc pj=4\n\
         .model dc d(is=1e-20 cjo=20p vj=0.8 m=0.45 tlevc=1 cta=1.5m tpb=1.8m \
         cjsw=1p vjsw=0.6 mjsw=0.3 ctp=2m tphp=1m)\n\
         .tran 10n 5u 0 10n",
    );
    let time = p.column("time").unwrap();
    let current = p.column("i(v1)").unwrap();
    let mut delivered = 0.;
    for k in 1..time.len() {
        delivered += 0.5 * (current[k].re + current[k - 1].re) * (time[k].re - time[k - 1].re);
    }
    let dt = 77. + 273.15 - 300.15;
    let charge =
        |v: f64, c: f64, p: f64, m: f64| c * p * (1. - (1. - v / p).powf(1. - m)) / (1. - m);
    let vd = -real(&p, "v(k)", p.point_count() - 1);
    let bottom = charge(vd, 20e-12 * (1. + 1.5e-3 * dt), 0.8 - 1.8e-3 * dt, 0.45);
    let sidewall = charge(vd, 4e-12 * (1. + 2e-3 * dt), 0.6 - 1e-3 * dt, 0.3);
    // i(v1) flows from + through the source: the source delivers -i(v1).
    // The junction anode is ground, so its charge is Q(-v(k)).
    let expected = -(bottom + sidewall);
    assert!(
        (-delivered - expected).abs() < 2e-3 * expected.abs(),
        "{delivered} vs {expected}"
    );
}
