//! `b3noi.c` (`BSIM3noise`, `N_CALC`/`N_DENS`): the instance's noise
//! generators at one frequency, from the values the `MODEINITSMSIG` load
//! leaves on the instance.
//!
//! The analysis owns gains, `Nintegrate` and the output columns; this file
//! reproduces what C passes to `NevalSrc` and the flicker factor it
//! multiplies the `N_GAIN` gain by, in C's order and arithmetic.
#![allow(non_snake_case)] // C's member and local names.

use super::fields::{Instance, Model, SizeParam};
use super::host::state;
use crate::devices::bsim4::NoiseSource as Source;
use crate::devices::noise::CHARGE;
use crate::primitives::{SpiceError, SpiceResult};

/// `N_MINLOG` (`noisedef.h`).
const N_MINLOG: f64 = 1E-38;

/// C's `MAX` macro.
fn MAX(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

/// C's `MIN` macro.
fn MIN(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

/// `BSIM3nNames`, in C's order (the instance total `""` follows).
pub(crate) const NAMES: [&str; 4] = [".rd", ".rs", ".id", ".1overf"];

/// `StrongInversionNoiseEval`: the strong-inversion flicker density.
fn strong_inversion_noise_eval(
    Vds: f64,
    model: &Model,
    here: &Instance,
    pParam: &SizeParam,
    freq: f64,
    temp: f64,
) -> f64 {
    let cd = here.cd.abs();
    let Leff = pParam.leff - 2.0 * model.lintnoi;
    let Leffsq = Leff * Leff;
    let esat = 2.0 * pParam.vsattemp / here.ueff;
    let mut DelClm;
    if model.em <= 0.0 {
        DelClm = 0.0;
    } else {
        let T0 = (((Vds - here.Vdseff) / pParam.litl) + model.em) / esat;
        DelClm = pParam.litl * MAX(T0, N_MINLOG).ln();
        if DelClm < 0.0 {
            DelClm = 0.0; // bugfix
        }
    }
    let EffFreq = freq.powf(model.ef);
    let T1 = CHARGE * CHARGE * 8.62e-5 * cd * temp * here.ueff;
    let T2 = 1.0e8 * EffFreq * here.Abulk * model.cox * Leffsq;
    let N0 = model.cox * here.Vgsteff / CHARGE;
    let Nl = model.cox * here.Vgsteff * (1.0 - here.AbovVgst2Vtm * here.Vdseff) / CHARGE;

    let T3 = model.oxideTrapDensityA * MAX((N0 + 2.0e14) / (Nl + 2.0e14), N_MINLOG).ln();
    let T4 = model.oxideTrapDensityB * (N0 - Nl);
    let T5 = model.oxideTrapDensityC * 0.5 * (N0 * N0 - Nl * Nl);

    let T6 = 8.62e-5 * temp * cd * cd;
    let T7 = 1.0e8 * EffFreq * Leffsq * pParam.weff;
    let T8 =
        model.oxideTrapDensityA + model.oxideTrapDensityB * Nl + model.oxideTrapDensityC * Nl * Nl;
    let T9 = (Nl + 2.0e14) * (Nl + 2.0e14);

    T1 / T2 * (T3 + T4 + T5) + T6 / T7 * DelClm * T8 / T9
}

/// The `.rd`, `.rs`, `.id` and `.1overf` generators at `freq` (Hz) and
/// `CKTtemp` = `temp` (K), reading `CKTstates[0]` through `state0`.
///
/// # Errors
/// A `noiMod` outside 1..=6, for which C's switch evaluates no channel
/// generator.
pub(crate) fn generators(
    model: &Model,
    here: &Instance,
    pParam: &SizeParam,
    state0: &dyn Fn(usize) -> f64,
    freq: f64,
    temp: f64,
) -> SpiceResult<[Source; 4]> {
    let m = here.m;
    let rd = Source::Thermal(here.dNodePrime, here.dNode, here.drainConductance * m);
    let rs = Source::Thermal(here.sNodePrime, here.sNode, here.sourceConductance * m);
    let id = match model.noiMod {
        1 | 3 => Source::Thermal(
            here.dNodePrime,
            here.sNodePrime,
            2.0 * (here.gm + here.gds + here.gmbs).abs() / 3.0 * m,
        ),
        5 | 6 => {
            let vds = MIN(state0(state::VDS), here.vdsat);
            Source::Thermal(
                here.dNodePrime,
                here.sNodePrime,
                (3.0 - vds / here.vdsat) * (here.gm + here.gds + here.gmbs).abs() / 3.0 * m,
            )
        }
        2 | 4 => Source::Thermal(
            here.dNodePrime,
            here.sNodePrime,
            m * here.ueff * here.qinv.abs()
                / (pParam.leff * pParam.leff + here.ueff * here.qinv.abs() * here.rds),
        ),
        other => {
            return Err(SpiceError::not_yet_ported(
                format!("BSIM3 noise with noiMod={other}"),
                "src/spicelib/devices/bsim3/b3noi.c (no case for this noiMod)",
            ));
        }
    };
    let factor = match model.noiMod {
        1 | 4 | 5 => {
            m * model.kf * (model.af * MAX(here.cd.abs(), N_MINLOG).ln()).exp()
                / (freq.powf(model.ef) * pParam.leff * pParam.leff * model.cox)
        }
        _ => {
            let mut vds = state0(state::VDS);
            if vds < 0.0 {
                vds = -vds;
            }
            let Ssi = strong_inversion_noise_eval(vds, model, here, pParam, freq, temp);
            let T10 = model.oxideTrapDensityA * 8.62e-5 * temp;
            let T11 = pParam.weff * pParam.leff * freq.powf(model.ef) * 4.0e36;
            let Swi = T10 / T11 * here.cd * here.cd;
            let T1 = Swi + Ssi;
            if T1 > 0.0 { m * (Ssi * Swi) / T1 } else { 0.0 }
        }
    };
    let flicker = Source::Gain(here.dNodePrime, here.sNodePrime, factor);
    Ok([rd, rs, id, flicker])
}
