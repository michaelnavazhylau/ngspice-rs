//! `b3check.c` (`BSIM3checkModel`): fatal parameter errors and warnings,
//! and the parameter resets C makes while checking.
//!
//! Ported statement by statement from BSIM3v3.3.0 (`b3check.c`).
#![allow(non_snake_case)] // C's member names.
#![allow(clippy::float_cmp)] // C compares floating point exactly.
#![allow(clippy::nonminimal_bool)] // C's conditions.
#![allow(clippy::double_parens)] // C's parenthesisation is kept verbatim.
#![allow(unused_parens)] // C's parenthesisation is kept verbatim.
#![allow(clippy::collapsible_if, clippy::collapsible_else_if)] // C's if nesting.

use super::fields::{Instance, Model, SizeParam};
use crate::devices::bsim4::c_g;

/// C `fabs`.
fn fabs(x: f64) -> f64 {
    x.abs()
}

/// C `strncmp(a, b, n) != 0`: the first `n` bytes (up to each string's end)
/// differ.
fn strncmp(a: &str, b: &str, n: usize) -> bool {
    let a = &a.as_bytes()[..a.len().min(n)];
    let b = &b.as_bytes()[..b.len().min(n)];
    a != b
}

/// `BSIM3checkModel` for `here`'s new size-dependent set `pParam`: appends
/// C's check-list messages to `messages` in C order (each one wordlist entry
/// without its trailing newline, and without the
/// `"Checking parameters for BSIM 3.3 model"` header C writes before them)
/// and returns C's `Fatal_Flag`. Like C, it resets out-of-range model and
/// size-dependent values, which later computations read.
///
/// C prints the list to `b3v33check.log` and stderr only when an entry
/// follows the header; the version warning is also printed to stdout. The
/// `ng_nomodcheck` front-end variable, which skips the whole check in C, has
/// no port equivalent.
#[allow(clippy::too_many_lines)] // One C function.
pub(crate) fn check_model(
    model: &mut Model,
    here: &Instance,
    pParam: &mut SizeParam,
    messages: &mut Vec<String>,
) -> bool {
    let mut Fatal_Flag: i32 = 0;
    if (strncmp(&model.version, "3.3.0", 5))
        && (strncmp(&model.version, "3.30", 4))
        && (strncmp(&model.version, "3.3", 3))
    {
        messages.push(
            "Warning: This model is BSIM3v3.3.0; you specified a wrong version number.".to_string(),
        );
    }
    if pParam.nlx < -pParam.leff {
        messages.push(format!(
            "Fatal: Nlx = {} is less than -Leff.",
            c_g(pParam.nlx)
        ));
        Fatal_Flag = 1;
    }
    if model.tox <= 0.0 {
        messages.push(format!("Fatal: Tox = {} is not positive.", c_g(model.tox)));
        Fatal_Flag = 1;
    }
    if model.toxm <= 0.0 {
        messages.push(format!(
            "Fatal: Toxm = {} is not positive.",
            c_g(model.toxm)
        ));
        Fatal_Flag = 1;
    }
    if model.lintnoi > pParam.leff / 2.0 {
        messages.push(format!(
            "Fatal: Lintnoi = {} is too large - Leff for noise is negative.",
            c_g(model.lintnoi)
        ));
        Fatal_Flag = 1;
    }
    if pParam.npeak <= 0.0 {
        messages.push(format!(
            "Fatal: Nch = {} is not positive.",
            c_g(pParam.npeak)
        ));
        Fatal_Flag = 1;
    }
    if pParam.nsub <= 0.0 {
        messages.push(format!(
            "Fatal: Nsub = {} is not positive.",
            c_g(pParam.nsub)
        ));
        Fatal_Flag = 1;
    }
    if pParam.ngate < 0.0 {
        messages.push(format!(
            "Fatal: Ngate = {} is not positive.",
            c_g(pParam.ngate)
        ));
        Fatal_Flag = 1;
    }
    if pParam.ngate > 1.0e25 {
        messages.push(format!("Fatal: Ngate = {} is too high.", c_g(pParam.ngate)));
        Fatal_Flag = 1;
    }
    if pParam.xj <= 0.0 {
        messages.push(format!("Fatal: Xj = {} is not positive.", c_g(pParam.xj)));
        Fatal_Flag = 1;
    }
    if pParam.dvt1 < 0.0 {
        messages.push(format!("Fatal: Dvt1 = {} is negative.", c_g(pParam.dvt1)));
        Fatal_Flag = 1;
    }
    if pParam.dvt1w < 0.0 {
        messages.push(format!("Fatal: Dvt1w = {} is negative.", c_g(pParam.dvt1w)));
        Fatal_Flag = 1;
    }
    if pParam.w0 == -pParam.weff {
        messages.push("Fatal: (W0 + Weff) = 0 causing divided-by-zero.".to_string());
        Fatal_Flag = 1;
    }
    if pParam.dsub < 0.0 {
        messages.push(format!("Fatal: Dsub = {} is negative.", c_g(pParam.dsub)));
        Fatal_Flag = 1;
    }
    if pParam.b1 == -pParam.weff {
        messages.push("Fatal: (B1 + Weff) = 0 causing divided-by-zero.".to_string());
        Fatal_Flag = 1;
    }
    if pParam.u0temp <= 0.0 {
        messages.push(format!(
            "Fatal: u0 at current temperature = {} is not positive.",
            c_g(pParam.u0temp)
        ));
        Fatal_Flag = 1;
    }
    if pParam.delta < 0.0 {
        messages.push(format!(
            "Fatal: Delta = {} is less than zero.",
            c_g(pParam.delta)
        ));
        Fatal_Flag = 1;
    }
    if pParam.vsattemp <= 0.0 {
        messages.push(format!(
            "Fatal: Vsat at current temperature = {} is not positive.",
            c_g(pParam.vsattemp)
        ));
        Fatal_Flag = 1;
    }
    if pParam.pclm <= 0.0 {
        messages.push(format!(
            "Fatal: Pclm = {} is not positive.",
            c_g(pParam.pclm)
        ));
        Fatal_Flag = 1;
    }
    if pParam.drout < 0.0 {
        messages.push(format!("Fatal: Drout = {} is negative.", c_g(pParam.drout)));
        Fatal_Flag = 1;
    }
    if pParam.pscbe2 <= 0.0 {
        messages.push(format!(
            "Warning: Pscbe2 = {} is not positive.",
            c_g(pParam.pscbe2)
        ));
    }
    if model.acmMod == 0 {
        if model.unitLengthSidewallJctCap > 0.0 || model.unitLengthGateSidewallJctCap > 0.0 {
            if here.drainPerimeter < pParam.weff {
                messages.push(format!(
                    "Warning: Pd = {} is less than W.",
                    c_g(here.drainPerimeter)
                ));
            }
            if here.sourcePerimeter < pParam.weff {
                messages.push(format!(
                    "Warning: Ps = {} is less than W.",
                    c_g(here.sourcePerimeter)
                ));
            }
        }
    }
    if (model.calcacm > 0) && (model.acmMod != 12) {
        messages.push(format!(
            "Warning: CALCACM = {} is wrong. Set back to 0.",
            model.calcacm
        ));
        model.calcacm = 0;
    }
    if pParam.noff < 0.1 {
        messages.push(format!(
            "Warning: Noff = {} is too small.",
            c_g(pParam.noff)
        ));
    }
    if pParam.noff > 4.0 {
        messages.push(format!(
            "Warning: Noff = {} is too large.",
            c_g(pParam.noff)
        ));
    }
    if pParam.voffcv < -0.5 {
        messages.push(format!(
            "Warning: Voffcv = {} is too small.",
            c_g(pParam.voffcv)
        ));
    }
    if pParam.voffcv > 0.5 {
        messages.push(format!(
            "Warning: Voffcv = {} is too large.",
            c_g(pParam.voffcv)
        ));
    }
    if model.ijth < 0.0 {
        messages.push(format!(
            "Fatal: Ijth = {} cannot be negative.",
            c_g(model.ijth)
        ));
        Fatal_Flag = 1;
    }
    if pParam.clc < 0.0 {
        messages.push(format!("Fatal: Clc = {} is negative.", c_g(pParam.clc)));
        Fatal_Flag = 1;
    }
    if pParam.moin < 5.0 {
        messages.push(format!(
            "Warning: Moin = {} is too small.",
            c_g(pParam.moin)
        ));
    }
    if pParam.moin > 25.0 {
        messages.push(format!(
            "Warning: Moin = {} is too large.",
            c_g(pParam.moin)
        ));
    }
    if model.capMod == 3 {
        if pParam.acde < 0.4 {
            messages.push(format!(
                "Warning:  Acde = {} is too small.",
                c_g(pParam.acde)
            ));
        }
        if pParam.acde > 1.6 {
            messages.push(format!(
                "Warning:  Acde = {} is too large.",
                c_g(pParam.acde)
            ));
        }
    }
    if model.paramChk == 1 {
        if pParam.leff <= 5.0e-8 {
            messages.push(format!(
                "Warning: Leff = {} may be too small.",
                c_g(pParam.leff)
            ));
        }
        if pParam.leffCV <= 5.0e-8 {
            messages.push(format!(
                "Warning: Leff for CV = {} may be too small.",
                c_g(pParam.leffCV)
            ));
        }
        if pParam.weff <= 1.0e-7 {
            messages.push(format!(
                "Warning: Weff = {} may be too small.",
                c_g(pParam.weff)
            ));
        }
        if pParam.weffCV <= 1.0e-7 {
            messages.push(format!(
                "Warning: Weff for CV = {} may be too small.",
                c_g(pParam.weffCV)
            ));
        }
        if pParam.nlx < 0.0 {
            messages.push(format!("Warning: Nlx = {} is negative.", c_g(pParam.nlx)));
        }
        if model.tox < 1.0e-9 {
            messages.push(format!(
                "Warning: Tox = {} is less than 10A.",
                c_g(model.tox)
            ));
        }
        if pParam.npeak <= 1.0e15 {
            messages.push(format!(
                "Warning: Nch = {} may be too small.",
                c_g(pParam.npeak)
            ));
        } else if pParam.npeak >= 1.0e21 {
            messages.push(format!(
                "Warning: Nch = {} may be too large.",
                c_g(pParam.npeak)
            ));
        }
        if pParam.nsub <= 1.0e14 {
            messages.push(format!(
                "Warning: Nsub = {} may be too small.",
                c_g(pParam.nsub)
            ));
        } else if pParam.nsub >= 1.0e21 {
            messages.push(format!(
                "Warning: Nsub = {} may be too large.",
                c_g(pParam.nsub)
            ));
        }
        if (pParam.ngate > 0.0) && (pParam.ngate <= 1.0e18) {
            messages.push(format!(
                "Warning: Ngate = {} is less than 1.E18cm^-3.",
                c_g(pParam.ngate)
            ));
        }
        if pParam.dvt0 < 0.0 {
            messages.push(format!("Warning: Dvt0 = {} is negative.", c_g(pParam.dvt0)));
        }
        if fabs(1.0e-6 / (pParam.w0 + pParam.weff)) > 10.0 {
            messages.push("Warning: (W0 + Weff) may be too small.".to_string());
        }
        if pParam.nfactor < 0.0 {
            messages.push(format!(
                "Warning: Nfactor = {} is negative.",
                c_g(pParam.nfactor)
            ));
        }
        if pParam.cdsc < 0.0 {
            messages.push(format!("Warning: Cdsc = {} is negative.", c_g(pParam.cdsc)));
        }
        if pParam.cdscd < 0.0 {
            messages.push(format!(
                "Warning: Cdscd = {} is negative.",
                c_g(pParam.cdscd)
            ));
        }
        if pParam.eta0 < 0.0 {
            messages.push(format!("Warning: Eta0 = {} is negative.", c_g(pParam.eta0)));
        }
        if fabs(1.0e-6 / (pParam.b1 + pParam.weff)) > 10.0 {
            messages.push("Warning: (B1 + Weff) may be too small.".to_string());
        }
        if pParam.a2 < 0.01 {
            messages.push(format!(
                "Warning: A2 = {} is too small. Set to 0.01.",
                c_g(pParam.a2)
            ));
            pParam.a2 = 0.01;
        } else if pParam.a2 > 1.0 {
            messages.push(format!(
                "Warning: A2 = {} is larger than 1. A2 is set to 1 and A1 is set to 0.",
                c_g(pParam.a2)
            ));
            pParam.a2 = 1.0;
            pParam.a1 = 0.0;
        }
        if pParam.rdsw < 0.0 {
            messages.push(format!(
                "Warning: Rdsw = {} is negative. Set to zero.",
                c_g(pParam.rdsw)
            ));
            pParam.rdsw = 0.0;
            pParam.rds0 = 0.0;
        } else if (pParam.rds0 > 0.0) && (pParam.rds0 < 0.001) {
            messages.push(format!(
                "Warning: Rds at current temperature = {} is less than 0.001 ohm. Set to zero.",
                c_g(pParam.rds0)
            ));
            pParam.rds0 = 0.0;
        }
        if pParam.vsattemp < 1.0e3 {
            messages.push(format!(
                "Warning: Vsat at current temperature = {} may be too small.",
                c_g(pParam.vsattemp)
            ));
        }
        if pParam.pdibl1 < 0.0 {
            messages.push(format!(
                "Warning: Pdibl1 = {} is negative.",
                c_g(pParam.pdibl1)
            ));
        }
        if pParam.pdibl2 < 0.0 {
            messages.push(format!(
                "Warning: Pdibl2 = {} is negative.",
                c_g(pParam.pdibl2)
            ));
        }
        if model.cgdo < 0.0 {
            messages.push(format!(
                "Warning: cgdo = {} is negative. Set to zero.",
                c_g(model.cgdo)
            ));
            model.cgdo = 0.0;
        }
        if model.cgso < 0.0 {
            messages.push(format!(
                "Warning: cgso = {} is negative. Set to zero.",
                c_g(model.cgso)
            ));
            model.cgso = 0.0;
        }
        if model.cgbo < 0.0 {
            messages.push(format!(
                "Warning: cgbo = {} is negative. Set to zero.",
                c_g(model.cgbo)
            ));
            model.cgbo = 0.0;
        }
    }
    Fatal_Flag != 0
}
