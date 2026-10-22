//! BSIM3v3.3 noise generators, a port of `b3noi.c`.
//!
//! C's `BSIM3noiMod` selects the channel-thermal and flicker laws
//! independently: `1`/`3` use the SPICE2 `2/3 |gm+gds+gmbs|` thermal source,
//! `2`/`4` the BSIM3 `qinv` source, `5`/`6` the SPICE2 source with the
//! linear/saturation fix, and `1`/`4`/`5` the SPICE2 flicker law while
//! `2`/`3`/`6` combine the strong-inversion and weak-inversion flicker
//! densities. Values outside 1..6 leave C's `noizDens` uninitialized, so they
//! are refused here instead of being given a substitute law.
use super::charges::ChargeEvaluation;
use super::dc::DcEvaluation;
use super::defaults::ModelDefaults;
use super::setup::InstanceSetup;
use crate::primitives::{Real, SpiceError, SpiceResult};

/// `const.h` `CHARGE`.
const CHARGE: Real = 1.602_176_620_8e-19;
/// `noisedef.h` `N_MINLOG`.
const N_MINLOG: Real = 1.0e-38;

/// The bias-dependent part of every generator C's `BSIM3noise` emits, with the
/// multiplicity already applied as C applies it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseDensities {
    /// Thermal conductance of the drain series resistance (C `drainConductance
    /// * m`), injected between the drain and its prime node.
    pub drain_conductance: Real,
    /// The same for the source.
    pub source_conductance: Real,
    /// The channel thermal source's conductance argument to `NevalSrc`
    /// (injected between the drain and source prime nodes).
    pub channel: Real,
    /// The flicker source's bias-dependent numerator (C's `noizDens *=
    /// <factor>` after `N_GAIN`), injected between the drain and source prime
    /// nodes.
    pub flicker_numerator: Real,
    /// C's three frequency-law factors after `pow(f, EF)`.
    pub flicker_factors: [Real; 3],
    /// C's `EF`.
    pub flicker_exponent: Real,
}

/// The BSIM3 noise model for one defaulted model and one set-up instance.
#[derive(Debug, Clone, Copy)]
pub struct NoiseModel<'a> {
    /// The defaulted model (`b3set.c`).
    pub model: &'a ModelDefaults,
    /// This instance's effective parameters (`b3temp.c`).
    pub setup: &'a InstanceSetup,
}

impl NoiseModel<'_> {
    /// C's `BSIM3noiMod`.
    fn noi_mod(&self) -> i32 {
        self.model.get("noimod").unwrap_or(1.).round() as i32
    }

    /// Evaluate the generator laws at `dc`/`charge` and the circuit
    /// temperature `circuit_kelvin` (C `ckt->CKTtemp`).
    ///
    /// # Errors
    /// A `noiMod` outside 1..6, or a nonfinite result.
    ///
    /// # Panics
    /// If a field or parameter this port names is absent from the model or
    /// instance setup: a schema bug in the port, not a deck error.
    pub fn densities(
        &self,
        dc: &DcEvaluation,
        charge: &ChargeEvaluation,
        circuit_kelvin: Real,
    ) -> SpiceResult<NoiseDensities> {
        let noi_mod = self.noi_mod();
        if !(1..=6).contains(&noi_mod) {
            return Err(SpiceError::not_yet_ported(
                format!(
                    "BSIM3 noiMod={noi_mod} noise (C leaves its densities \
                     uninitialized; only noiMod 1..6 are defined)"
                ),
                "src/spicelib/devices/bsim3/b3noi.c",
            ));
        }
        let p = self.setup;
        let m = self.model;
        let number = |name: &str| -> Real {
            m.get(name)
                .unwrap_or_else(|| panic!("BSIM3 noise model field {name}"))
        };
        let param = |name: &str| -> Real {
            p.pparam(name)
                .unwrap_or_else(|| panic!("BSIM3 noise instance parameter {name}"))
        };
        let multiplier = p.multiplier;
        let cox = m.oxide_capacitance;
        let leff = param("leff");
        let weff = param("weff");
        let ueff = dc.ueff;
        let cd = dc.ids;

        // Channel thermal noise (`b3noi.c`'s `BSIM3IDNOIZ`).
        let channel = match noi_mod {
            1 | 3 => 2.0 * (dc.gm + dc.gds + dc.gmbs).abs() / 3.0 * multiplier,
            5 | 6 => {
                // C clamps the drain voltage it reads from state by `vdsat`.
                let vds = dc.vds.min(dc.vdsat);
                (3.0 - vds / dc.vdsat) * (dc.gm + dc.gds + dc.gmbs).abs() / 3.0 * multiplier
            }
            _ => {
                // `m * ueff * |qinv| / (leff^2 + ueff * |qinv| * rds)`.
                let qinv = charge.qinv.abs();
                multiplier * ueff * qinv / (leff * leff + ueff * qinv * dc.rds)
            }
        };

        // Flicker noise (`b3noi.c`'s `BSIM3FLNOIZ`).
        let (flicker_numerator, flicker_factors) = match noi_mod {
            1 | 4 | 5 => {
                let noise_current = cd.abs().max(N_MINLOG);
                let numerator =
                    multiplier * number("kf") * (number("af") * noise_current.ln()).exp();
                (numerator, [leff, leff, cox])
            }
            _ => {
                // C's strong-inversion/weak-inversion blend. Both densities
                // carry one `pow(f, EF)` in their denominators, so the blend
                // keeps the same law and the numerator below is the
                // frequency-free part.
                let lintnoi = number("lintnoi");
                let leff_noi = leff - 2.0 * lintnoi;
                let leff_noi_sq = leff_noi * leff_noi;
                let vgsteff = dc.vgsteff;
                let vdseff = dc.vdseff;
                let abulk = dc.abulk;
                let em = number("em");
                let vsattemp = param("vsattemp");
                let litl = param("litl");
                let esat = 2.0 * vsattemp / ueff;
                let del_clm = if em <= 0.0 {
                    0.0
                } else {
                    let t0 = ((dc.vds - vdseff) / litl + em) / esat;
                    (litl * t0.max(N_MINLOG).ln()).max(0.0)
                };
                let n0 = cox * vgsteff / CHARGE;
                let nl = cox * vgsteff * (1.0 - dc.abulk_over_vgst2vtm * vdseff) / CHARGE;
                let noia = number("noia");
                let noib = number("noib");
                let noic = number("noic");
                let thermal = 8.62e-5 * circuit_kelvin;
                // `Ssi = A / pow(f,EF)`, `Swi = C / pow(f,EF)`.
                let a = CHARGE
                    * CHARGE
                    * thermal
                    * cd
                    * ueff
                    * (noia * (((n0 + 2.0e14) / (nl + 2.0e14)).max(N_MINLOG)).ln()
                        + noib * (n0 - nl)
                        + noic * 0.5 * (n0 * n0 - nl * nl))
                    / (1.0e8 * abulk * cox * leff_noi_sq);
                let b = thermal * cd * cd / (1.0e8 * leff_noi_sq * weff)
                    * del_clm
                    * (noia + noib * nl + noic * nl * nl)
                    / ((nl + 2.0e14) * (nl + 2.0e14));
                let c = noia * thermal * cd * cd / (weff * leff * 4.0e36);
                let sum = a + b + c;
                let numerator = if sum > 0.0 {
                    multiplier * (a + b) * c / sum
                } else {
                    0.0
                };
                (numerator, [1.0, 1.0, 1.0])
            }
        };

        let result = NoiseDensities {
            drain_conductance: p.drain_conductance * multiplier,
            source_conductance: p.source_conductance * multiplier,
            channel,
            flicker_numerator,
            flicker_factors,
            flicker_exponent: number("ef"),
        };
        for (name, value) in [
            ("drain conductance", result.drain_conductance),
            ("source conductance", result.source_conductance),
            ("channel", result.channel),
            ("flicker numerator", result.flicker_numerator),
            ("flicker exponent", result.flicker_exponent),
        ] {
            if !value.is_finite() {
                return Err(SpiceError::Numerical {
                    context: "BSIM3 noise evaluation (bsim3/b3noi.c)".into(),
                    message: format!("{name} is not finite"),
                });
            }
        }
        Ok(result)
    }
}
