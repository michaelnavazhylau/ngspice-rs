//! `b4check.c` (`BSIM4checkModel`): fatal parameter errors and warnings,
//! and the parameter resets C makes while checking.
//!
//! Ported statement by statement from BSIM4 4.8.3 (`b4check.c`, Copyright
//! (c) 2025 University of California, Educational Community License 2.0).
#![allow(non_snake_case)]

use super::fields::{Instance, Model, SizeParam};
use super::temp::c_g;

/// C `fabs`.
fn fabs(x: f64) -> f64 {
    x.abs()
}

/// C `strcmp(a, b) != 0`.
fn strcmp(a: &str, b: &str) -> bool {
    a != b
}

/// C `strncmp(a, b, n) != 0`: the first `n` bytes (up to each string's end)
/// differ.
fn strncmp(a: &str, b: &str, n: usize) -> bool {
    let a = &a.as_bytes()[..a.len().min(n)];
    let b = &b.as_bytes()[..b.len().min(n)];
    a != b
}

/// `BSIM4checkModel` for one instance: appends C's messages to `messages`
/// in C order and returns C's `Fatal_Flag`. Like C, it also resets out-of-
/// range model, size-dependent and instance values (`m`, `p`, `h`), which
/// later computations read. `temperature` is `CKTtemp` (K), `model_name`
/// C's `BSIM4modName`.
///
/// Message spelling: each entry is one C `wl_append_word` word (or one of
/// the two `printf` lines of the version warning, which C prints to stdout
/// immediately as well as appending a slightly different text to the list),
/// with its trailing newline removed; the first entry is C's
/// `"\nChecking parameters for BSIM 4.8 model %s"` header. C only emits
/// the list when something follows the header: it writes the words to the
/// file `bsim4.out` (in the current directory) and copies them to
/// **stderr**. This function returns the header too only when another
/// message was produced, so `messages` is empty for a clean model.
///
/// What the caller must reproduce (cross-instance state C keeps):
/// - `ng_nomodcheck`: when that front-end variable is set, C skips the
///   whole check (returns 0, no resets).
/// - A function-`static` `modname` makes C check each model *name* once
///   per process: C skips the check when
///   `strncmp(modname, BSIM4modName, strlen(BSIM4modName)) == 0`, i.e. when
///   the previously checked name *starts with* this model's name (so after
///   checking `nch2`, a model named `nch` is skipped too; names of
///   `BSIZE_SP` bytes or more are always checked). The static survives
///   every later `BSIM4temp` call, so a second temperature pass checks
///   nothing. C runs `BSIM4temp`'s instance loop over the model's
///   instances in its list order (last-parsed first), so the checked
///   instance is the first one in that order; its model resets persist
///   into every later instance and pass, its size-dependent resets only
///   into instances sharing its (l, w, nf) parameter set in that pass.
#[allow(
    clippy::too_many_lines,
    clippy::if_same_then_else,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    unused_parens
)] // A statement-by-statement port of C.
pub(crate) fn check_model(
    m: &mut Model,
    h: &mut Instance,
    p: &mut SizeParam,
    temperature: f64,
    model_name: &str,
    messages: &mut Vec<String>,
) -> bool {
    let mut Fatal_Flag: i32 = 0;
    let start = messages.len();
    messages.push(format!(
        "\nChecking parameters for BSIM 4.8 model {model_name}"
    ));
    if (strcmp(&m.version, "4.8.0"))
        && (strncmp(&m.version, "4.80", 4))
        && (strncmp(&m.version, "4.8", 3))
        && (strcmp(&m.version, "4.8.1"))
        && (strncmp(&m.version, "4.81", 4))
        && (strcmp(&m.version, "4.8.3"))
        && (strncmp(&m.version, "4.83", 4))
    {
        messages.push("Warning: This model supports BSIM4 version 4.8".to_string());
        messages
            .push("You specified a wrong version number. Working now with BSIM4.8.3".to_string());
        messages.push("Warning: This model supports BSIM4 version 4.8".to_string());
        messages
            .push("You specified a wrong version number. Working now with BSIM4.8.3.".to_string());
    }
    if (h.rgateMod == 2) || (h.rgateMod == 3) {
        if (h.trnqsMod == 1) || (h.acnqsMod == 1) {
            messages.push(
                "Warning: You've selected both Rg and charge deficit NQS; select one only."
                    .to_string(),
            );
        }
    }
    if m.toxe <= 0.0 {
        messages.push(format!("Fatal: Toxe = {} is not positive.", c_g(m.toxe)));
        Fatal_Flag = 1;
    }
    if h.toxp <= 0.0 {
        messages.push(format!("Fatal: Toxp = {} is not positive.", c_g(h.toxp)));
        Fatal_Flag = 1;
    }
    if m.eot <= 0.0 {
        messages.push(format!("Fatal: EOT = {} is not positive.", c_g(m.eot)));
        Fatal_Flag = 1;
    }
    if m.tempeot <= 0.0 {
        messages.push(format!(
            "Fatal: TEMPEOT = {} is not positive.",
            c_g(m.tempeot)
        ));
        Fatal_Flag = 1;
    }
    if m.epsrgate < 0.0 {
        messages.push(format!(
            "Fatal: Epsrgate = {} is not positive.",
            c_g(m.epsrgate)
        ));
        Fatal_Flag = 1;
    }
    if m.epsrsub < 0.0 {
        messages.push(format!(
            "Fatal: Epsrsub = {} is not positive.",
            c_g(m.epsrsub)
        ));
        Fatal_Flag = 1;
    }
    if m.easub < 0.0 {
        messages.push(format!("Fatal: Easub = {} is not positive.", c_g(m.easub)));
        Fatal_Flag = 1;
    }
    if m.ni0sub <= 0.0 {
        messages.push(format!("Fatal: Easub = {} is not positive.", c_g(m.ni0sub)));
        Fatal_Flag = 1;
    }
    if m.toxm <= 0.0 {
        messages.push(format!("Fatal: Toxm = {} is not positive.", c_g(m.toxm)));
        Fatal_Flag = 1;
    }
    if m.toxref <= 0.0 {
        messages.push(format!(
            "Fatal: Toxref = {} is not positive.",
            c_g(m.toxref)
        ));
        Fatal_Flag = 1;
    }
    if p.lpe0 < -p.leff {
        messages.push(format!("Fatal: Lpe0 = {} is less than -Leff.", c_g(p.lpe0)));
        Fatal_Flag = 1;
    }
    if m.lintnoi > p.leff / 2.0 {
        messages.push(format!(
            "Fatal: Lintnoi = {} is too large - Leff for noise is negative.",
            c_g(m.lintnoi)
        ));
        Fatal_Flag = 1;
    }
    if p.lpeb < -p.leff {
        messages.push(format!("Fatal: Lpeb = {} is less than -Leff.", c_g(p.lpeb)));
        Fatal_Flag = 1;
    }
    if p.ndep <= 0.0 {
        messages.push(format!("Fatal: Ndep = {} is not positive.", c_g(p.ndep)));
        Fatal_Flag = 1;
    }
    if p.phi <= 0.0 {
        messages.push(format!(
            "Fatal: Phi = {} is not positive. Please check Phin and Ndep",
            c_g(p.phi)
        ));
        messages.push(format!(
            "       Phin = {}  Ndep = {} ",
            c_g(p.phin),
            c_g(p.ndep)
        ));
        Fatal_Flag = 1;
    }
    if p.nsub <= 0.0 {
        messages.push(format!("Fatal: Nsub = {} is not positive.", c_g(p.nsub)));
        Fatal_Flag = 1;
    }
    if p.ngate < 0.0 {
        messages.push(format!(
            "Fatal: Ngate = {} Ngate is not positive.",
            c_g(p.ngate)
        ));
        Fatal_Flag = 1;
    }
    if p.ngate > 1.0e25 {
        messages.push(format!("Fatal: Ngate = {} Ngate is too high", c_g(p.ngate)));
        Fatal_Flag = 1;
    }
    if p.xj <= 0.0 {
        messages.push(format!("Fatal: Xj = {} is not positive.", c_g(p.xj)));
        Fatal_Flag = 1;
    }
    if p.dvt1 < 0.0 {
        messages.push(format!("Fatal: Dvt1 = {} is negative.", c_g(p.dvt1)));
        Fatal_Flag = 1;
    }
    if p.dvt1w < 0.0 {
        messages.push(format!("Fatal: Dvt1w = {} is negative.", c_g(p.dvt1w)));
        Fatal_Flag = 1;
    }
    if p.w0 == -p.weff {
        messages.push("Fatal: (W0 + Weff) = 0 causing divided-by-zero.".to_string());
        Fatal_Flag = 1;
    }
    if p.dsub < 0.0 {
        messages.push(format!("Fatal: Dsub = {} is negative.", c_g(p.dsub)));
        Fatal_Flag = 1;
    }
    if p.b1 == -p.weff {
        messages.push("Fatal: (B1 + Weff) = 0 causing divided-by-zero.".to_string());
        Fatal_Flag = 1;
    }
    if h.u0temp <= 0.0 {
        messages.push(format!(
            "Fatal: u0 at current temperature = {} is not positive.",
            c_g(h.u0temp)
        ));
        Fatal_Flag = 1;
    }
    if p.delta < 0.0 {
        messages.push(format!(
            "Fatal: Delta = {} is less than zero.",
            c_g(p.delta)
        ));
        Fatal_Flag = 1;
    }
    if h.vsattemp <= 0.0 {
        messages.push(format!(
            "Fatal: Vsat at current temperature = {} is not positive.",
            c_g(h.vsattemp)
        ));
        Fatal_Flag = 1;
    }
    if p.pclm <= 0.0 {
        messages.push(format!("Fatal: Pclm = {} is not positive.", c_g(p.pclm)));
        Fatal_Flag = 1;
    }
    if p.drout < 0.0 {
        messages.push(format!("Fatal: Drout = {} is negative.", c_g(p.drout)));
        Fatal_Flag = 1;
    }
    if h.m <= 0.0 {
        messages.push(format!("Fatal: multiplier = {} is not positive.", c_g(h.m)));
        Fatal_Flag = 1;
    }
    if h.nf < 1.0 {
        messages.push(format!(
            "Fatal: Number of finger = {} is smaller than one.",
            c_g(h.nf)
        ));
        Fatal_Flag = 1;
    }
    if (h.sa > 0.0) && (h.sb > 0.0) && ((h.nf == 1.0) || ((h.nf > 1.0) && (h.sd > 0.0))) {
        if m.saref <= 0.0 {
            messages.push(format!("Fatal: SAref = {} is not positive.", c_g(m.saref)));
            Fatal_Flag = 1;
        }
        if m.sbref <= 0.0 {
            messages.push(format!("Fatal: SBref = {} is not positive.", c_g(m.sbref)));
            Fatal_Flag = 1;
        }
    }
    if (h.l + m.xl) <= m.xgl {
        messages.push("Fatal: The parameter xgl must be smaller than Ldrawn+XL.".to_string());
        Fatal_Flag = 1;
    }
    if h.ngcon < 1.0 {
        messages.push("Fatal: The parameter ngcon cannot be smaller than one.".to_string());
        Fatal_Flag = 1;
    }
    if (h.ngcon != 1.0) && (h.ngcon != 2.0) {
        h.ngcon = 1.0;
        messages.push("Warning: Ngcon must be equal to one or two; reset to 1.0.".to_string());
    }
    if m.gbmin < 1.0e-20 {
        messages.push(format!("Warning: Gbmin = {} is too small.", c_g(m.gbmin)));
    }
    if p.fprout < 0.0 {
        messages.push(format!("Fatal: fprout = {} is negative.", c_g(p.fprout)));
        Fatal_Flag = 1;
    }
    if p.pdits < 0.0 {
        messages.push(format!("Fatal: pdits = {} is negative.", c_g(p.pdits)));
        Fatal_Flag = 1;
    }
    if m.pditsl < 0.0 {
        messages.push(format!("Fatal: pditsl = {} is negative.", c_g(m.pditsl)));
        Fatal_Flag = 1;
    }
    if m.igbMod != 0 {
        if p.nigbinv <= 0.0 {
            messages.push(format!(
                "Fatal: nigbinv = {} is non-positive.",
                c_g(p.nigbinv)
            ));
            Fatal_Flag = 1;
        }
        if p.nigbacc <= 0.0 {
            messages.push(format!(
                "Fatal: nigbacc = {} is non-positive.",
                c_g(p.nigbacc)
            ));
            Fatal_Flag = 1;
        }
    }
    if m.igcMod != 0 {
        if p.nigc <= 0.0 {
            messages.push(format!("Fatal: nigc = {} is non-positive.", c_g(p.nigc)));
            Fatal_Flag = 1;
        }
        if p.poxedge <= 0.0 {
            messages.push(format!(
                "Fatal: poxedge = {} is non-positive.",
                c_g(p.poxedge)
            ));
            Fatal_Flag = 1;
        }
        if p.pigcd <= 0.0 {
            messages.push(format!("Fatal: pigcd = {} is non-positive.", c_g(p.pigcd)));
            Fatal_Flag = 1;
        }
    }
    if p.clc < 0.0 {
        messages.push(format!("Fatal: Clc = {} is negative.", c_g(p.clc)));
        Fatal_Flag = 1;
    }
    if p.ckappas < 0.02 {
        messages.push(format!(
            "Warning: ckappas = {} is too small.",
            c_g(p.ckappas)
        ));
        p.ckappas = 0.02;
    }
    if p.ckappad < 0.02 {
        messages.push(format!(
            "Warning: ckappad = {} is too small.",
            c_g(p.ckappad)
        ));
        p.ckappad = 0.02;
    }
    if m.vtss < 0.0 {
        messages.push(format!("Fatal: Vtss = {} is negative.", c_g(m.vtss)));
        Fatal_Flag = 1;
    }
    if m.vtsd < 0.0 {
        messages.push(format!("Fatal: Vtsd = {} is negative.", c_g(m.vtsd)));
        Fatal_Flag = 1;
    }
    if m.vtssws < 0.0 {
        messages.push(format!("Fatal: Vtssws = {} is negative.", c_g(m.vtssws)));
        Fatal_Flag = 1;
    }
    if m.vtsswd < 0.0 {
        messages.push(format!("Fatal: Vtsswd = {} is negative.", c_g(m.vtsswd)));
        Fatal_Flag = 1;
    }
    if m.vtsswgs < 0.0 {
        messages.push(format!("Fatal: Vtsswgs = {} is negative.", c_g(m.vtsswgs)));
        Fatal_Flag = 1;
    }
    if m.vtsswgd < 0.0 {
        messages.push(format!("Fatal: Vtsswgd = {} is negative.", c_g(m.vtsswgd)));
        Fatal_Flag = 1;
    }
    if m.paramChk == 1 {
        if p.leff <= 1.0e-9 {
            messages.push(format!(
                "Warning: Leff = {} <= 1.0e-9. Recommended Leff >= 1e-8 ",
                c_g(p.leff)
            ));
        }
        if p.leffCV <= 1.0e-9 {
            messages.push(format!(
                "Warning: Leff for CV = {} <= 1.0e-9. Recommended LeffCV >=1e-8 ",
                c_g(p.leffCV)
            ));
        }
        if p.weff <= 1.0e-9 {
            messages.push(format!(
                "Warning: Weff = {} <= 1.0e-9. Recommended Weff >=1e-7 ",
                c_g(p.weff)
            ));
        }
        if p.weffCV <= 1.0e-9 {
            messages.push(format!(
                "Warning: Weff for CV = {} <= 1.0e-9. Recommended WeffCV >= 1e-7 ",
                c_g(p.weffCV)
            ));
        }
        if m.toxe < 1.0e-10 {
            messages.push(format!(
                "Warning: Toxe = {} is less than 1A. Recommended Toxe >= 5A",
                c_g(m.toxe)
            ));
        }
        if h.toxp < 1.0e-10 {
            messages.push(format!(
                "Warning: Toxp = {} is less than 1A. Recommended Toxp >= 5A",
                c_g(h.toxp)
            ));
        }
        if m.toxm < 1.0e-10 {
            messages.push(format!(
                "Warning: Toxm = {} is less than 1A. Recommended Toxm >= 5A",
                c_g(m.toxm)
            ));
        }
        if p.ndep <= 1.0e12 {
            messages.push(format!("Warning: Ndep = {} may be too small.", c_g(p.ndep)));
        } else if p.ndep >= 1.0e21 {
            messages.push(format!("Warning: Ndep = {} may be too large.", c_g(p.ndep)));
        }
        if p.nsub <= 1.0e14 {
            messages.push(format!("Warning: Nsub = {} may be too small.", c_g(p.nsub)));
        } else if p.nsub >= 1.0e21 {
            messages.push(format!("Warning: Nsub = {} may be too large.", c_g(p.nsub)));
        }
        if (p.ngate > 0.0) && (p.ngate <= 1.0e18) {
            messages.push(format!(
                "Warning: Ngate = {} is less than 1.E18cm^-3.",
                c_g(p.ngate)
            ));
        }
        if p.dvt0 < 0.0 {
            messages.push(format!("Warning: Dvt0 = {} is negative.", c_g(p.dvt0)));
        }
        if fabs(1.0e-8 / (p.w0 + p.weff)) > 10.0 {
            messages.push("Warning: (W0 + Weff) may be too small.".to_string());
        }
        if p.nfactor < 0.0 {
            messages.push(format!(
                "Warning: Nfactor = {} is negative.",
                c_g(p.nfactor)
            ));
        }
        if p.cdsc < 0.0 {
            messages.push(format!("Warning: Cdsc = {} is negative.", c_g(p.cdsc)));
        }
        if p.cdscd < 0.0 {
            messages.push(format!("Warning: Cdscd = {} is negative.", c_g(p.cdscd)));
        }
        if h.eta0 < 0.0 {
            messages.push(format!("Warning: Eta0 = {} is negative.", c_g(h.eta0)));
        }
        if m.gidlclamp >= 0.0 {
            messages.push(format!(
                "Warning: gidlclamp = {} is zero or positive.",
                c_g(m.gidlclamp)
            ));
        }
        if fabs(1.0e-8 / (p.b1 + p.weff)) > 10.0 {
            messages.push("Warning: (B1 + Weff) may be too small.".to_string());
        }
        if p.a2 < 0.01 {
            messages.push(format!(
                "Warning: A2 = {} is too small. Set to 0.01.",
                c_g(p.a2)
            ));
            p.a2 = 0.01;
        } else if p.a2 > 1.0 {
            messages.push(format!(
                "Warning: A2 = {} is larger than 1. A2 is set to 1 and A1 is set to 0.",
                c_g(p.a2)
            ));
            p.a2 = 1.0;
            p.a1 = 0.0;
        }
        if p.prwg < 0.0 {
            messages.push(format!(
                "Warning: Prwg = {} is negative. Set to zero.",
                c_g(p.prwg)
            ));
            p.prwg = 0.0;
        }
        if p.rdsw < 0.0 {
            messages.push(format!(
                "Warning: Rdsw = {} is negative. Set to zero.",
                c_g(p.rdsw)
            ));
            p.rdsw = 0.0;
            p.rds0 = 0.0;
        }
        if p.rds0 < 0.0 {
            messages.push(format!(
                "Warning: Rds at current temperature = {} is negative. Set to zero.",
                c_g(p.rds0)
            ));
            p.rds0 = 0.0;
        }
        if p.rdswmin < 0.0 {
            messages.push(format!(
                "Warning: Rdswmin at current temperature = {} is negative. Set to zero.",
                c_g(p.rdswmin)
            ));
            p.rdswmin = 0.0;
        }
        if p.pscbe2 <= 0.0 {
            messages.push(format!(
                "Warning: Pscbe2 = {} is not positive.",
                c_g(p.pscbe2)
            ));
        }
        if p.vsattemp < 1.0e3 {
            messages.push(format!(
                "Warning: Vsat at current temperature = {} may be too small.",
                c_g(p.vsattemp)
            ));
        }
        if (m.lambdaGiven) && (p.lambda > 0.0) {
            if p.lambda > 1.0e-9 {
                messages.push(format!(
                    "Warning: Lambda = {} may be too large.",
                    c_g(p.lambda)
                ));
            }
        }
        if (m.vtlGiven) && (p.vtl > 0.0) {
            if p.vtl < 6.0e4 {
                messages.push(format!(
                    "Warning: Thermal velocity vtl = {} may be too small.",
                    c_g(p.vtl)
                ));
            }
            if p.xn < 3.0 {
                messages.push(format!(
                    "Warning: back scattering coeff xn = {} is too small. Reset to 3.0 ",
                    c_g(p.xn)
                ));
                p.xn = 3.0;
            }
            if m.lc < 0.0 {
                messages.push(format!(
                    "Warning: back scattering coeff lc = {} is too small. Reset to 0.0",
                    c_g(m.lc)
                ));
                p.lc = 0.0;
            }
        }
        if p.pdibl1 < 0.0 {
            messages.push(format!("Warning: Pdibl1 = {} is negative.", c_g(p.pdibl1)));
        }
        if p.pdibl2 < 0.0 {
            messages.push(format!("Warning: Pdibl2 = {} is negative.", c_g(p.pdibl2)));
        }
        if (h.sa > 0.0) && (h.sb > 0.0) && ((h.nf == 1.0) || ((h.nf > 1.0) && (h.sd > 0.0))) {
            if m.lodk2 <= 0.0 {
                messages.push(format!(
                    "Warning: LODK2 = {} is not positive.",
                    c_g(m.lodk2)
                ));
            }
            if m.lodeta0 <= 0.0 {
                messages.push(format!(
                    "Warning: LODETA0 = {} is not positive.",
                    c_g(m.lodeta0)
                ));
            }
        }
        if h.rgateMod == 1 {
            if m.rshg <= 0.0 {
                messages.push("Warning: rshg should be positive for rgateMod = 1.".to_string());
            }
        } else if h.rgateMod == 2 {
            if m.rshg <= 0.0 {
                messages.push("Warning: rshg <= 0.0 for rgateMod = 2.".to_string());
            } else if p.xrcrg1 <= 0.0 {
                messages.push("Warning: xrcrg1 <= 0.0 for rgateMod = 2.".to_string());
            }
        }
        if h.rgateMod == 3 {
            if m.rshg <= 0.0 {
                messages.push("Warning: rshg should be positive for rgateMod = 3.".to_string());
            } else if p.xrcrg1 <= 0.0 {
                messages.push("Warning: xrcrg1 should be positive for rgateMod = 3.".to_string());
            }
        }
        if m.rbps0 <= 0.0 {
            messages.push(format!("Fatal: RBPS0 = {} is not positive.", c_g(m.rbps0)));
            Fatal_Flag = 1;
        }
        if m.rbpd0 <= 0.0 {
            messages.push(format!("Fatal: RBPD0 = {} is not positive.", c_g(m.rbpd0)));
            Fatal_Flag = 1;
        }
        if m.rbpbx0 <= 0.0 {
            messages.push(format!(
                "Fatal: RBPBX0 = {} is not positive.",
                c_g(m.rbpbx0)
            ));
            Fatal_Flag = 1;
        }
        if m.rbpby0 <= 0.0 {
            messages.push(format!(
                "Fatal: RBPBY0 = {} is not positive.",
                c_g(m.rbpby0)
            ));
            Fatal_Flag = 1;
        }
        if m.rbdbx0 <= 0.0 {
            messages.push(format!(
                "Fatal: RBDBX0 = {} is not positive.",
                c_g(m.rbdbx0)
            ));
            Fatal_Flag = 1;
        }
        if m.rbdby0 <= 0.0 {
            messages.push(format!(
                "Fatal: RBDBY0 = {} is not positive.",
                c_g(m.rbdby0)
            ));
            Fatal_Flag = 1;
        }
        if m.rbsbx0 <= 0.0 {
            messages.push(format!(
                "Fatal: RBSBX0 = {} is not positive.",
                c_g(m.rbsbx0)
            ));
            Fatal_Flag = 1;
        }
        if m.rbsby0 <= 0.0 {
            messages.push(format!(
                "Fatal: RBSBY0 = {} is not positive.",
                c_g(m.rbsby0)
            ));
            Fatal_Flag = 1;
        }
        if p.noff < 0.1 {
            messages.push(format!("Warning: Noff = {} is too small.", c_g(p.noff)));
        }
        if p.voffcv < -0.5 {
            messages.push(format!("Warning: Voffcv = {} is too small.", c_g(p.voffcv)));
        }
        if p.moin < 5.0 {
            messages.push(format!("Warning: Moin = {} is too small.", c_g(p.moin)));
        }
        if p.moin > 25.0 {
            messages.push(format!("Warning: Moin = {} is too large.", c_g(p.moin)));
        }
        if m.capMod == 2 {
            if p.acde < 0.1 {
                messages.push(format!("Warning: Acde = {} is too small.", c_g(p.acde)));
            }
            if p.acde > 1.6 {
                messages.push(format!("Warning: Acde = {} is too large.", c_g(p.acde)));
            }
        }
        if m.cgdo < 0.0 {
            messages.push(format!(
                "Warning: cgdo = {} is negative. Set to zero.",
                c_g(m.cgdo)
            ));
            m.cgdo = 0.0;
        }
        if m.cgso < 0.0 {
            messages.push(format!(
                "Warning: cgso = {} is negative. Set to zero.",
                c_g(m.cgso)
            ));
            m.cgso = 0.0;
        }
        if m.cgbo < 0.0 {
            messages.push(format!(
                "Warning: cgbo = {} is negative. Set to zero.",
                c_g(m.cgbo)
            ));
            m.cgbo = 0.0;
        }
        if m.tnoiMod == 1 {
            messages.push(format!(
                "Warning: TNOIMOD={} is not supported and may be removed from future version.",
                m.tnoiMod
            ));
        }
        if m.idovvdsc <= 0.0 {
            messages.push(format!(
                "Warning: idovvdsc = {} is zero or negative.",
                c_g(m.idovvdsc)
            ));
        }
        if (strcmp(&m.version, "4.8.1"))
            && (strncmp(&m.version, "4.81", 4))
            && (strncmp(&m.version, "4.8", 3))
            && (strcmp(&m.version, "4.8.2"))
            && (strncmp(&m.version, "4.82", 4))
            && (strcmp(&m.version, "4.8.3"))
            && (strncmp(&m.version, "4.83", 4))
        {
            if m.tnoiMod == 1 || m.tnoiMod == 2 {
                if m.tnoia < 0.0 {
                    messages.push(format!(
                        "Warning: tnoia = {} is negative. Set to zero.",
                        c_g(m.tnoia)
                    ));
                    m.tnoia = 0.0;
                }
                if m.tnoib < 0.0 {
                    messages.push(format!(
                        "Warning: tnoib = {} is negative. Set to zero.",
                        c_g(m.tnoib)
                    ));
                    m.tnoib = 0.0;
                }
                if m.rnoia < 0.0 {
                    messages.push(format!(
                        "Warning: rnoia = {} is negative. Set to zero.",
                        c_g(m.rnoia)
                    ));
                    m.rnoia = 0.0;
                }
                if m.rnoib < 0.0 {
                    messages.push(format!(
                        "Warning: rnoib = {} is negative. Set to zero.",
                        c_g(m.rnoib)
                    ));
                    m.rnoib = 0.0;
                }
            }
            if m.tnoiMod == 2 {
                if m.tnoic < 0.0 {
                    messages.push(format!(
                        "Warning: tnoic = {} is negative. Set to zero.",
                        c_g(m.tnoic)
                    ));
                    m.tnoic = 0.0;
                }
                if m.rnoic < 0.0 {
                    messages.push(format!(
                        "Warning: rnoic = {} is negative. Set to zero.",
                        c_g(m.rnoic)
                    ));
                    m.rnoic = 0.0;
                }
            }
        } else {
            if m.tnoiMod == 1 {
                if m.tnoia < 0.0 {
                    messages.push(format!(
                        "Warning: tnoia = {} is negative. Set to zero.",
                        c_g(m.tnoia)
                    ));
                    m.tnoia = 0.0;
                }
                if m.tnoib < 0.0 {
                    messages.push(format!(
                        "Warning: tnoib = {} is negative. Set to zero.",
                        c_g(m.tnoib)
                    ));
                    m.tnoib = 0.0;
                }
                if m.rnoia < 0.0 {
                    messages.push(format!(
                        "Warning: rnoia = {} is negative. Set to zero.",
                        c_g(m.rnoia)
                    ));
                    m.rnoia = 0.0;
                }
                if m.rnoib < 0.0 {
                    messages.push(format!(
                        "Warning: rnoib = {} is negative. Set to zero.",
                        c_g(m.rnoib)
                    ));
                    m.rnoib = 0.0;
                }
            }
        }
        if m.SjctEmissionCoeff < 0.1 {
            messages.push(format!(
                "Warning: Njs = {} is less than 0.1. Setting Njs to 0.1.",
                c_g(m.SjctEmissionCoeff)
            ));
            m.SjctEmissionCoeff = 0.1;
        } else if m.SjctEmissionCoeff < 0.7 {
            messages.push(format!(
                "Warning: Njs = {} is less than 0.7.",
                c_g(m.SjctEmissionCoeff)
            ));
        }
        if m.DjctEmissionCoeff < 0.1 {
            messages.push(format!(
                "Warning: Njd = {} is less than 0.1. Setting Njd to 0.1.",
                c_g(m.DjctEmissionCoeff)
            ));
            m.DjctEmissionCoeff = 0.1;
        } else if m.DjctEmissionCoeff < 0.7 {
            messages.push(format!(
                "Warning: Njd = {} is less than 0.7.",
                c_g(m.DjctEmissionCoeff)
            ));
        }
        if m.njtsstemp < 0.0 {
            messages.push(format!(
                "Warning: Njts = {} is negative at temperature = {}.",
                c_g(m.njtsstemp),
                c_g(temperature)
            ));
        }
        if m.njtsswstemp < 0.0 {
            messages.push(format!(
                "Warning: Njtssw = {} is negative at temperature = {}.",
                c_g(m.njtsswstemp),
                c_g(temperature)
            ));
        }
        if m.njtsswgstemp < 0.0 {
            messages.push(format!(
                "Warning: Njtsswg = {} is negative at temperature = {}.",
                c_g(m.njtsswgstemp),
                c_g(temperature)
            ));
        }
        if m.njtsdGiven && m.njtsdtemp < 0.0 {
            messages.push(format!(
                "Warning: Njtsd = {} is negative at temperature = {}.",
                c_g(m.njtsdtemp),
                c_g(temperature)
            ));
        }
        if m.njtsswdGiven && m.njtsswdtemp < 0.0 {
            messages.push(format!(
                "Warning: Njtsswd = {} is negative at temperature = {}.",
                c_g(m.njtsswdtemp),
                c_g(temperature)
            ));
        }
        if m.njtsswgdGiven && m.njtsswgdtemp < 0.0 {
            messages.push(format!(
                "Warning: Njtsswgd = {} is negative at temperature = {}.",
                c_g(m.njtsswgdtemp),
                c_g(temperature)
            ));
        }
        if m.ntnoi < 0.0 {
            messages.push(format!(
                "Warning: ntnoi = {} is negative. Set to zero.",
                c_g(m.ntnoi)
            ));
            m.ntnoi = 0.0;
        }
        if m.SbulkJctBotGradingCoeff >= 0.99 {
            messages.push(format!(
                "Warning: MJS = {} is too big. Set to 0.99.",
                c_g(m.SbulkJctBotGradingCoeff)
            ));
            m.SbulkJctBotGradingCoeff = 0.99;
        }
        if m.SbulkJctSideGradingCoeff >= 0.99 {
            messages.push(format!(
                "Warning: MJSWS = {} is too big. Set to 0.99.",
                c_g(m.SbulkJctSideGradingCoeff)
            ));
            m.SbulkJctSideGradingCoeff = 0.99;
        }
        if m.SbulkJctGateSideGradingCoeff >= 0.99 {
            messages.push(format!(
                "Warning: MJSWGS = {} is too big. Set to 0.99.",
                c_g(m.SbulkJctGateSideGradingCoeff)
            ));
            m.SbulkJctGateSideGradingCoeff = 0.99;
        }
        if m.DbulkJctBotGradingCoeff >= 0.99 {
            messages.push(format!(
                "Warning: MJD = {} is too big. Set to 0.99.",
                c_g(m.DbulkJctBotGradingCoeff)
            ));
            m.DbulkJctBotGradingCoeff = 0.99;
        }
        if m.DbulkJctSideGradingCoeff >= 0.99 {
            messages.push(format!(
                "Warning: MJSWD = {} is too big. Set to 0.99.",
                c_g(m.DbulkJctSideGradingCoeff)
            ));
            m.DbulkJctSideGradingCoeff = 0.99;
        }
        if m.DbulkJctGateSideGradingCoeff >= 0.99 {
            messages.push(format!(
                "Warning: MJSWGD = {} is too big. Set to 0.99.",
                c_g(m.DbulkJctGateSideGradingCoeff)
            ));
            m.DbulkJctGateSideGradingCoeff = 0.99;
        }
        if m.wpemod == 1.0 {
            if m.scref <= 0.0 {
                messages.push(format!(
                    "Warning: SCREF = {} is not positive. Set to 1e-6.",
                    c_g(m.scref)
                ));
                m.scref = 1e-6;
            }
        }
    }
    if messages.len() == start + 1 {
        messages.truncate(start);
    }
    Fatal_Flag != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_compares_follow_c() {
        assert!(!strncmp("4.8.2", "4.8", 3));
        assert!(strncmp("4.7", "4.8", 3));
        assert!(strncmp("4", "4.8", 3));
        assert!(!strcmp("4.8.0", "4.8.0"));
    }

    #[test]
    fn an_unset_model_is_fatal_with_c_messages() {
        let mut m = Model {
            version: "4.8.2".into(),
            ..Model::default()
        };
        let (mut h, mut p) = (Instance::default(), SizeParam::default());
        let mut messages = Vec::new();
        assert!(check_model(
            &mut m,
            &mut h,
            &mut p,
            300.15,
            "nn",
            &mut messages
        ));
        assert_eq!(messages[0], "\nChecking parameters for BSIM 4.8 model nn");
        assert_eq!(messages[1], "Fatal: Toxe = 0 is not positive.");
    }
}
