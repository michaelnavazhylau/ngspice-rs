//! `b3ld.c` (`BSIM3load`): the DC/transient load of one instance.
//!
//! The load follows C statement by statement over C's own struct members and
//! local names, so the arithmetic (operand order, grouping, `exp`/`log`/`pow`
//! forms) is C's. C's labels become structured control flow:
//!
//! - `goto finished` (C-V skipped when `xpart < 0` or no charge is needed)
//!   only leaves its if/else chain, which ends at `finished:`;
//! - `goto line850`/`goto line1000` out of the charge block are breaks out of
//!   the labelled `'charges` block with flags, and leaving that block
//!   normally is C's `goto line860`; `line850:` and `line860:` are the two
//!   arms of an `if`, both followed by `line900:`'s stamps, all skipped for
//!   `line1000` (`MODEINITSMSIG`);
//! - C's bypass branch (`goto line755`/`line850` when `CKTbypass` is set) is
//!   not compiled: the port runs with `bypass = 0`, C's default, so `ByPass`
//!   stays 0, and `NEWCONV` is defined as in C (`macros.h`).
//!
//! The ACM junction models (`acmMod > 0`) are refused explicitly.
#![allow(non_snake_case)] // C's member and local names.
#![allow(unused_assignments, unused_variables, unused_mut)] // C assigns locals that some paths never read.
#![allow(non_upper_case_globals)] // C's `Charge_q`.
#![allow(unused_parens)] // C's parenthesisation is kept verbatim.
#![allow(clippy::assign_op_pattern)] // `x = x * y` is kept as C writes it.
#![allow(clippy::collapsible_else_if)] // C's if/else nesting.
#![allow(clippy::collapsible_if)] // C's if nesting.
#![allow(clippy::double_parens)] // C's parenthesisation is kept verbatim.
#![allow(clippy::eq_op)] // C's expressions are kept verbatim.
#![allow(clippy::excessive_precision)] // C's literals.
#![allow(clippy::float_cmp)] // C compares floating point exactly.
#![allow(clippy::if_same_then_else)] // C's branches.
#![allow(clippy::many_single_char_names)] // C's locals.
#![allow(clippy::misrefactored_assign_op)] // C's `T0 *= T0 * 4.0e-16`.
#![allow(clippy::manual_range_contains)] // C's range tests.
#![allow(clippy::neg_multiply)] // C's `-1 * x` forms.
#![allow(clippy::nonminimal_bool)] // C's conditions.
#![allow(clippy::precedence)] // C's expressions are kept verbatim.
#![allow(clippy::self_assignment)] // C's statements.
#![allow(clippy::too_many_lines)] // One C function.
#![allow(clippy::cognitive_complexity)] // One C function.

use super::fields::{Instance, Model, SizeParam};
use super::host::{LoadCkt, mode, state};
use crate::devices::limiting::{fetlim, limvds, pnjlim};
use crate::primitives::{SpiceError, SpiceResult};

const MAX_EXP: f64 = 5.834617425e14;
const MIN_EXP: f64 = 1.713908431e-15;
const EXP_THRESHOLD: f64 = 34.0;
const EPSSI: f64 = 1.03594e-10;
const Charge_q: f64 = 1.60219e-19;
const DELTA_1: f64 = 0.02;
const DELTA_3: f64 = 0.02;
const DELTA_4: f64 = 0.02;
/// `CONSTvt0 = CONSTboltz * REFTEMP / CHARGE` (`main.c`), REFTEMP = 27 C.
const CONSTvt0: f64 = 1.38064852e-23 * (27.0 + 273.15) / 1.6021766208e-19;

fn exp(x: f64) -> f64 {
    x.exp()
}
fn log(x: f64) -> f64 {
    x.ln()
}
fn sqrt(x: f64) -> f64 {
    x.sqrt()
}
fn fabs(x: f64) -> f64 {
    x.abs()
}

/// `BSIM3load` for one instance: reads `CKTmode`, the previous solution and
/// the state vectors through `ckt`, writes this instance's members and
/// `CKTstate0`, and adds its matrix and right-hand-side terms.
///
/// # Errors
/// `NIintegrate` failures, and `acmMod > 0` (not ported).
pub(crate) fn load(
    model: &Model,
    here: &mut Instance,
    pParam: &SizeParam,
    ckt: &mut dyn LoadCkt,
) -> SpiceResult<()> {
    let mut SourceSatCurrent: f64 = 0.0;
    let mut DrainSatCurrent: f64 = 0.0;
    let mut ag0: f64 = 0.0;
    let mut qgd: f64 = 0.0;
    let mut qgs: f64 = 0.0;
    let mut qgb: f64 = 0.0;
    let mut von: f64 = 0.0;
    let mut cbhat: f64 = 0.0;
    let mut VgstNVt: f64 = 0.0;
    let mut ExpVgst: f64 = 0.0;
    let mut cdrain: f64 = 0.0;
    let mut cdhat: f64 = 0.0;
    let mut cdreq: f64 = 0.0;
    let mut ceqbd: f64 = 0.0;
    let mut ceqbs: f64 = 0.0;
    let mut ceqqb: f64 = 0.0;
    let mut ceqqd: f64 = 0.0;
    let mut ceqqg: f64 = 0.0;
    let mut ceq: f64 = 0.0;
    let mut geq: f64 = 0.0;
    let mut czbd: f64 = 0.0;
    let mut czbdsw: f64 = 0.0;
    let mut czbdswg: f64 = 0.0;
    let mut czbs: f64 = 0.0;
    let mut czbssw: f64 = 0.0;
    let mut czbsswg: f64 = 0.0;
    let mut evbd: f64 = 0.0;
    let mut evbs: f64 = 0.0;
    let mut arg: f64 = 0.0;
    let mut sarg: f64 = 0.0;
    let mut delvbd: f64 = 0.0;
    let mut delvbs: f64 = 0.0;
    let mut delvds: f64 = 0.0;
    let mut delvgd: f64 = 0.0;
    let mut delvgs: f64 = 0.0;
    let mut Vfbeff: f64 = 0.0;
    let mut dVfbeff_dVg: f64 = 0.0;
    let mut dVfbeff_dVb: f64 = 0.0;
    let mut V3: f64 = 0.0;
    let mut V4: f64 = 0.0;
    let mut gcbdb: f64 = 0.0;
    let mut gcbgb: f64 = 0.0;
    let mut gcbsb: f64 = 0.0;
    let mut gcddb: f64 = 0.0;
    let mut gcdgb: f64 = 0.0;
    let mut gcdsb: f64 = 0.0;
    let mut gcgdb: f64 = 0.0;
    let mut gcggb: f64 = 0.0;
    let mut gcgsb: f64 = 0.0;
    let mut gcsdb: f64 = 0.0;
    let mut gcsgb: f64 = 0.0;
    let mut gcssb: f64 = 0.0;
    let mut MJ: f64 = 0.0;
    let mut MJSW: f64 = 0.0;
    let mut MJSWG: f64 = 0.0;
    let mut vbd: f64 = 0.0;
    let mut vbs: f64 = 0.0;
    let mut vds: f64 = 0.0;
    let mut vgb: f64 = 0.0;
    let mut vgd: f64 = 0.0;
    let mut vgs: f64 = 0.0;
    let mut vgdo: f64 = 0.0;
    let mut xfact: f64 = 0.0;
    let mut qgate: f64 = 0.0;
    let mut qbulk: f64 = 0.0;
    let mut qdrn: f64 = 0.0;
    let mut qsrc: f64 = 0.0;
    let mut qinoi: f64 = 0.0;
    let mut cqgate: f64 = 0.0;
    let mut cqbulk: f64 = 0.0;
    let mut cqdrn: f64 = 0.0;
    let mut Vds: f64 = 0.0;
    let mut Vgs: f64 = 0.0;
    let mut Vbs: f64 = 0.0;
    let mut Gmbs: f64 = 0.0;
    let mut FwdSum: f64 = 0.0;
    let mut RevSum: f64 = 0.0;
    let mut Vgs_eff: f64 = 0.0;
    let mut Vfb: f64 = 0.0;
    let mut Phis: f64 = 0.0;
    let mut dPhis_dVb: f64 = 0.0;
    let mut sqrtPhis: f64 = 0.0;
    let mut dsqrtPhis_dVb: f64 = 0.0;
    let mut Vth: f64 = 0.0;
    let mut dVth_dVb: f64 = 0.0;
    let mut dVth_dVd: f64 = 0.0;
    let mut Vgst: f64 = 0.0;
    let mut dVgst_dVg: f64 = 0.0;
    let mut dVgst_dVb: f64 = 0.0;
    let mut dVgs_eff_dVg: f64 = 0.0;
    let mut Nvtm: f64 = 0.0;
    let mut Vtm: f64 = 0.0;
    let mut n: f64 = 0.0;
    let mut dn_dVb: f64 = 0.0;
    let mut dn_dVd: f64 = 0.0;
    let mut voffcv: f64 = 0.0;
    let mut noff: f64 = 0.0;
    let mut dnoff_dVd: f64 = 0.0;
    let mut dnoff_dVb: f64 = 0.0;
    let mut ExpArg: f64 = 0.0;
    let mut V0: f64 = 0.0;
    let mut CoxWLcen: f64 = 0.0;
    let mut QovCox: f64 = 0.0;
    let mut LINK: f64 = 0.0;
    let mut DeltaPhi: f64 = 0.0;
    let mut dDeltaPhi_dVg: f64 = 0.0;
    let mut VgDP: f64 = 0.0;
    let mut dVgDP_dVg: f64 = 0.0;
    let mut Cox: f64 = 0.0;
    let mut Tox: f64 = 0.0;
    let mut Tcen: f64 = 0.0;
    let mut dTcen_dVg: f64 = 0.0;
    let mut dTcen_dVd: f64 = 0.0;
    let mut dTcen_dVb: f64 = 0.0;
    let mut Ccen: f64 = 0.0;
    let mut Coxeff: f64 = 0.0;
    let mut dCoxeff_dVg: f64 = 0.0;
    let mut dCoxeff_dVd: f64 = 0.0;
    let mut dCoxeff_dVb: f64 = 0.0;
    let mut Denomi: f64 = 0.0;
    let mut dDenomi_dVg: f64 = 0.0;
    let mut dDenomi_dVd: f64 = 0.0;
    let mut dDenomi_dVb: f64 = 0.0;
    let mut ueff: f64 = 0.0;
    let mut dueff_dVg: f64 = 0.0;
    let mut dueff_dVd: f64 = 0.0;
    let mut dueff_dVb: f64 = 0.0;
    let mut Esat: f64 = 0.0;
    let mut Vdsat: f64 = 0.0;
    let mut EsatL: f64 = 0.0;
    let mut dEsatL_dVg: f64 = 0.0;
    let mut dEsatL_dVd: f64 = 0.0;
    let mut dEsatL_dVb: f64 = 0.0;
    let mut dVdsat_dVg: f64 = 0.0;
    let mut dVdsat_dVb: f64 = 0.0;
    let mut dVdsat_dVd: f64 = 0.0;
    let mut Vasat: f64 = 0.0;
    let mut dAlphaz_dVg: f64 = 0.0;
    let mut dAlphaz_dVb: f64 = 0.0;
    let mut dVasat_dVg: f64 = 0.0;
    let mut dVasat_dVb: f64 = 0.0;
    let mut dVasat_dVd: f64 = 0.0;
    let mut Va: f64 = 0.0;
    let mut dVa_dVd: f64 = 0.0;
    let mut dVa_dVg: f64 = 0.0;
    let mut dVa_dVb: f64 = 0.0;
    let mut Vbseff: f64 = 0.0;
    let mut dVbseff_dVb: f64 = 0.0;
    let mut VbseffCV: f64 = 0.0;
    let mut dVbseffCV_dVb: f64 = 0.0;
    let mut Arg1: f64 = 0.0;
    let mut One_Third_CoxWL: f64 = 0.0;
    let mut Two_Third_CoxWL: f64 = 0.0;
    let mut Alphaz: f64 = 0.0;
    let mut CoxWL: f64 = 0.0;
    let mut T0: f64 = 0.0;
    let mut dT0_dVg: f64 = 0.0;
    let mut dT0_dVd: f64 = 0.0;
    let mut dT0_dVb: f64 = 0.0;
    let mut T1: f64 = 0.0;
    let mut dT1_dVg: f64 = 0.0;
    let mut dT1_dVd: f64 = 0.0;
    let mut dT1_dVb: f64 = 0.0;
    let mut T2: f64 = 0.0;
    let mut dT2_dVg: f64 = 0.0;
    let mut dT2_dVd: f64 = 0.0;
    let mut dT2_dVb: f64 = 0.0;
    let mut T3: f64 = 0.0;
    let mut dT3_dVg: f64 = 0.0;
    let mut dT3_dVd: f64 = 0.0;
    let mut dT3_dVb: f64 = 0.0;
    let mut T4: f64 = 0.0;
    let mut T5: f64 = 0.0;
    let mut T6: f64 = 0.0;
    let mut T7: f64 = 0.0;
    let mut T8: f64 = 0.0;
    let mut T9: f64 = 0.0;
    let mut T10: f64 = 0.0;
    let mut T11: f64 = 0.0;
    let mut T12: f64 = 0.0;
    let mut tmp: f64 = 0.0;
    let mut Abulk: f64 = 0.0;
    let mut dAbulk_dVb: f64 = 0.0;
    let mut Abulk0: f64 = 0.0;
    let mut dAbulk0_dVb: f64 = 0.0;
    let mut VACLM: f64 = 0.0;
    let mut dVACLM_dVg: f64 = 0.0;
    let mut dVACLM_dVd: f64 = 0.0;
    let mut dVACLM_dVb: f64 = 0.0;
    let mut VADIBL: f64 = 0.0;
    let mut dVADIBL_dVg: f64 = 0.0;
    let mut dVADIBL_dVd: f64 = 0.0;
    let mut dVADIBL_dVb: f64 = 0.0;
    let mut Xdep: f64 = 0.0;
    let mut dXdep_dVb: f64 = 0.0;
    let mut lt1: f64 = 0.0;
    let mut dlt1_dVb: f64 = 0.0;
    let mut ltw: f64 = 0.0;
    let mut dltw_dVb: f64 = 0.0;
    let mut Delt_vth: f64 = 0.0;
    let mut dDelt_vth_dVb: f64 = 0.0;
    let mut Theta0: f64 = 0.0;
    let mut dTheta0_dVb: f64 = 0.0;
    let mut TempRatio: f64 = 0.0;
    let mut tmp1: f64 = 0.0;
    let mut tmp2: f64 = 0.0;
    let mut tmp3: f64 = 0.0;
    let mut tmp4: f64 = 0.0;
    let mut DIBL_Sft: f64 = 0.0;
    let mut dDIBL_Sft_dVd: f64 = 0.0;
    let mut Lambda: f64 = 0.0;
    let mut dLambda_dVg: f64 = 0.0;
    let mut Idtot: f64 = 0.0;
    let mut Ibtot: f64 = 0.0;
    let mut a1: f64 = 0.0;
    let mut ScalingFactor: f64 = 0.0;
    let mut Vgsteff: f64 = 0.0;
    let mut dVgsteff_dVg: f64 = 0.0;
    let mut dVgsteff_dVd: f64 = 0.0;
    let mut dVgsteff_dVb: f64 = 0.0;
    let mut Vdseff: f64 = 0.0;
    let mut dVdseff_dVg: f64 = 0.0;
    let mut dVdseff_dVd: f64 = 0.0;
    let mut dVdseff_dVb: f64 = 0.0;
    let mut VdseffCV: f64 = 0.0;
    let mut dVdseffCV_dVg: f64 = 0.0;
    let mut dVdseffCV_dVd: f64 = 0.0;
    let mut dVdseffCV_dVb: f64 = 0.0;
    let mut diffVds: f64 = 0.0;
    let mut dAbulk_dVg: f64 = 0.0;
    let mut beta: f64 = 0.0;
    let mut dbeta_dVg: f64 = 0.0;
    let mut dbeta_dVd: f64 = 0.0;
    let mut dbeta_dVb: f64 = 0.0;
    let mut gche: f64 = 0.0;
    let mut dgche_dVg: f64 = 0.0;
    let mut dgche_dVd: f64 = 0.0;
    let mut dgche_dVb: f64 = 0.0;
    let mut fgche1: f64 = 0.0;
    let mut dfgche1_dVg: f64 = 0.0;
    let mut dfgche1_dVd: f64 = 0.0;
    let mut dfgche1_dVb: f64 = 0.0;
    let mut fgche2: f64 = 0.0;
    let mut dfgche2_dVg: f64 = 0.0;
    let mut dfgche2_dVd: f64 = 0.0;
    let mut dfgche2_dVb: f64 = 0.0;
    let mut Idl: f64 = 0.0;
    let mut dIdl_dVg: f64 = 0.0;
    let mut dIdl_dVd: f64 = 0.0;
    let mut dIdl_dVb: f64 = 0.0;
    let mut Idsa: f64 = 0.0;
    let mut dIdsa_dVg: f64 = 0.0;
    let mut dIdsa_dVd: f64 = 0.0;
    let mut dIdsa_dVb: f64 = 0.0;
    let mut Ids: f64 = 0.0;
    let mut Gm: f64 = 0.0;
    let mut Gds: f64 = 0.0;
    let mut Gmb: f64 = 0.0;
    let mut Isub: f64 = 0.0;
    let mut Gbd: f64 = 0.0;
    let mut Gbg: f64 = 0.0;
    let mut Gbb: f64 = 0.0;
    let mut VASCBE: f64 = 0.0;
    let mut dVASCBE_dVg: f64 = 0.0;
    let mut dVASCBE_dVd: f64 = 0.0;
    let mut dVASCBE_dVb: f64 = 0.0;
    let mut CoxWovL: f64 = 0.0;
    let mut Rds: f64 = 0.0;
    let mut dRds_dVg: f64 = 0.0;
    let mut dRds_dVb: f64 = 0.0;
    let mut WVCox: f64 = 0.0;
    let mut WVCoxRds: f64 = 0.0;
    let mut Vgst2Vtm: f64 = 0.0;
    let mut VdsatCV: f64 = 0.0;
    let mut dVdsatCV_dVg: f64 = 0.0;
    let mut dVdsatCV_dVb: f64 = 0.0;
    let mut Leff: f64 = 0.0;
    let mut Weff: f64 = 0.0;
    let mut dWeff_dVg: f64 = 0.0;
    let mut dWeff_dVb: f64 = 0.0;
    let mut AbulkCV: f64 = 0.0;
    let mut dAbulkCV_dVb: f64 = 0.0;
    let mut qgdo: f64 = 0.0;
    let mut qgso: f64 = 0.0;
    let mut cgdo: f64 = 0.0;
    let mut cgso: f64 = 0.0;
    let mut qcheq: f64 = 0.0;
    let mut qdef: f64 = 0.0;
    let mut gqdef: f64 = 0.0;
    let mut cqdef: f64 = 0.0;
    let mut cqcheq: f64 = 0.0;
    let mut gtau_diff: f64 = 0.0;
    let mut gtau_drift: f64 = 0.0;
    let mut gcqdb: f64 = 0.0;
    let mut gcqsb: f64 = 0.0;
    let mut gcqgb: f64 = 0.0;
    let mut gcqbb: f64 = 0.0;
    let mut dxpart: f64 = 0.0;
    let mut sxpart: f64 = 0.0;
    let mut ggtg: f64 = 0.0;
    let mut ggtd: f64 = 0.0;
    let mut ggts: f64 = 0.0;
    let mut ggtb: f64 = 0.0;
    let mut ddxpart_dVd: f64 = 0.0;
    let mut ddxpart_dVg: f64 = 0.0;
    let mut ddxpart_dVb: f64 = 0.0;
    let mut ddxpart_dVs: f64 = 0.0;
    let mut dsxpart_dVd: f64 = 0.0;
    let mut dsxpart_dVg: f64 = 0.0;
    let mut dsxpart_dVb: f64 = 0.0;
    let mut dsxpart_dVs: f64 = 0.0;
    let mut gbspsp: f64 = 0.0;
    let mut gbbdp: f64 = 0.0;
    let mut gbbsp: f64 = 0.0;
    let mut gbspg: f64 = 0.0;
    let mut gbspb: f64 = 0.0;
    let mut gbspdp: f64 = 0.0;
    let mut gbdpdp: f64 = 0.0;
    let mut gbdpg: f64 = 0.0;
    let mut gbdpb: f64 = 0.0;
    let mut gbdpsp: f64 = 0.0;
    let mut Cgg: f64 = 0.0;
    let mut Cgd: f64 = 0.0;
    let mut Cgb: f64 = 0.0;
    let mut Cdg: f64 = 0.0;
    let mut Cdd: f64 = 0.0;
    let mut Cds: f64 = 0.0;
    let mut Csg: f64 = 0.0;
    let mut Csd: f64 = 0.0;
    let mut Css: f64 = 0.0;
    let mut Csb: f64 = 0.0;
    let mut Cbg: f64 = 0.0;
    let mut Cbd: f64 = 0.0;
    let mut Cbb: f64 = 0.0;
    let mut Cgg1: f64 = 0.0;
    let mut Cgb1: f64 = 0.0;
    let mut Cgd1: f64 = 0.0;
    let mut Cbg1: f64 = 0.0;
    let mut Cbb1: f64 = 0.0;
    let mut Cbd1: f64 = 0.0;
    let mut Qac0: f64 = 0.0;
    let mut Qsub0: f64 = 0.0;
    let mut dQac0_dVg: f64 = 0.0;
    let mut dQac0_dVb: f64 = 0.0;
    let mut dQsub0_dVg: f64 = 0.0;
    let mut dQsub0_dVd: f64 = 0.0;
    let mut dQsub0_dVb: f64 = 0.0;
    let mut m: f64 = 0.0;
    let mut ByPass: i32 = 0;
    let mut Check: i32 = 0;
    let mut ChargeComputationNeeded: i32 = 0;
    let mut error: i32 = 0;
    ScalingFactor = 1.0e-9;
    ChargeComputationNeeded = if ((ckt.mode()
        & (mode::MODEDCTRANCURVE | mode::MODEAC | mode::MODETRAN | mode::MODEINITSMSIG))
        != 0)
        || (((ckt.mode() & mode::MODETRANOP) != 0) && ((ckt.mode() & mode::MODEUIC) != 0))
    {
        1
    } else {
        0
    };
    Check = 1;
    ByPass = 0;
    if (ckt.mode() & mode::MODEINITSMSIG) != 0 {
        vbs = ckt.state0(state::VBS);
        vgs = ckt.state0(state::VGS);
        vds = ckt.state0(state::VDS);
        qdef = ckt.state0(state::QDEF);
    } else if (ckt.mode() & mode::MODEINITTRAN) != 0 {
        vbs = ckt.state1(state::VBS);
        vgs = ckt.state1(state::VGS);
        vds = ckt.state1(state::VDS);
        qdef = ckt.state1(state::QDEF);
    } else if ((ckt.mode() & mode::MODEINITJCT) != 0) && !(here.off != 0) {
        vds = f64::from(model.r#type) * here.icVDS;
        vgs = f64::from(model.r#type) * here.icVGS;
        vbs = f64::from(model.r#type) * here.icVBS;
        qdef = 0.0;
        if (vds == 0.0)
            && (vgs == 0.0)
            && (vbs == 0.0)
            && (((ckt.mode()
                & (mode::MODETRAN | mode::MODEAC | mode::MODEDCOP | mode::MODEDCTRANCURVE))
                != 0)
                || (!((ckt.mode() & mode::MODEUIC) != 0)))
        {
            vbs = 0.0;
            vgs = f64::from(model.r#type) * here.vth0 + 0.1;
            vds = 0.1;
        }
    } else if ((ckt.mode() & (mode::MODEINITJCT | mode::MODEINITFIX)) != 0) && ((here.off) != 0) {
        vds = 0.0;
        vgs = vds;
        vbs = vgs;
        qdef = vbs;
    } else {
        if (ckt.mode() & mode::MODEINITPRED) != 0 {
            xfact = ckt.delta() / ckt.delta_old(1);
            ckt.set_state0(state::VBS, ckt.state1(state::VBS));
            vbs = (1.0 + xfact) * (ckt.state1(state::VBS)) - (xfact * (ckt.state2(state::VBS)));
            ckt.set_state0(state::VGS, ckt.state1(state::VGS));
            vgs = (1.0 + xfact) * (ckt.state1(state::VGS)) - (xfact * (ckt.state2(state::VGS)));
            ckt.set_state0(state::VDS, ckt.state1(state::VDS));
            vds = (1.0 + xfact) * (ckt.state1(state::VDS)) - (xfact * (ckt.state2(state::VDS)));
            ckt.set_state0(state::VBD, ckt.state0(state::VBS) - ckt.state0(state::VDS));
            ckt.set_state0(state::QDEF, ckt.state1(state::QDEF));
            qdef = (1.0 + xfact) * (ckt.state1(state::QDEF)) - (xfact * (ckt.state2(state::QDEF)));
        } else {
            vbs =
                f64::from(model.r#type) * (ckt.rhs_old(here.bNode) - ckt.rhs_old(here.sNodePrime));
            vgs =
                f64::from(model.r#type) * (ckt.rhs_old(here.gNode) - ckt.rhs_old(here.sNodePrime));
            vds = f64::from(model.r#type)
                * (ckt.rhs_old(here.dNodePrime) - ckt.rhs_old(here.sNodePrime));
            qdef = f64::from(model.r#type) * (ckt.rhs_old(here.qNode));
        }
        vbd = vbs - vds;
        vgd = vgs - vds;
        vgdo = ckt.state0(state::VGS) - ckt.state0(state::VDS);
        delvbs = vbs - ckt.state0(state::VBS);
        delvbd = vbd - ckt.state0(state::VBD);
        delvgs = vgs - ckt.state0(state::VGS);
        delvds = vds - ckt.state0(state::VDS);
        delvgd = vgd - vgdo;
        if here.mode >= 0 {
            Idtot = here.cd + here.csub - here.cbd;
            cdhat = Idtot - here.gbd * delvbd
                + (here.gmbs + here.gbbs) * delvbs
                + (here.gm + here.gbgs) * delvgs
                + (here.gds + here.gbds) * delvds;
            Ibtot = here.cbs + here.cbd - here.csub;
            cbhat = Ibtot + here.gbd * delvbd + (here.gbs - here.gbbs) * delvbs
                - here.gbgs * delvgs
                - here.gbds * delvds;
        } else {
            Idtot = here.cd - here.cbd;
            cdhat = Idtot - (here.gbd - here.gmbs) * delvbd + here.gm * delvgd - here.gds * delvds;
            Ibtot = here.cbs + here.cbd - here.csub;
            cbhat = Ibtot + here.gbs * delvbs + (here.gbd - here.gbbs) * delvbd
                - here.gbgs * delvgd
                + here.gbds * delvds;
        }
        // Port-only: an exact (unlimited) load evaluates the CKTrhsOld
        // voltages unchanged and reports no limiting; C always limits here.
        if ckt.limiting() {
            von = here.von;
            if ckt.state0(state::VDS) >= 0.0 {
                vgs = fetlim(vgs, ckt.state0(state::VGS), von);
                vds = vgs - vgd;
                vds = limvds(vds, ckt.state0(state::VDS));
                vgd = vgs - vds;
            } else {
                vgd = fetlim(vgd, vgdo, von);
                vds = vgs - vgd;
                vds = -limvds(-vds, -(ckt.state0(state::VDS)));
                vgs = vgd + vds;
            }
            if vds >= 0.0 {
                let limited = pnjlim(vbs, ckt.state0(state::VBS), CONSTvt0, model.vcrit);
                vbs = limited.value;
                Check = i32::from(limited.limited);
                vbd = vbs - vds;
            } else {
                let limited = pnjlim(vbd, ckt.state0(state::VBD), CONSTvt0, model.vcrit);
                vbd = limited.value;
                Check = i32::from(limited.limited);
                vbs = vbd + vds;
            }
        } else {
            // Nothing was limited, so the load does not report
            // nonconvergence (C's `DEVpnjlim` would clear `Check`).
            Check = 0;
        }
    }
    vbd = vbs - vds;
    vgd = vgs - vds;
    vgb = vgs - vbs;
    Nvtm = model.vtm * model.jctEmissionCoeff;
    if model.acmMod == 0 {
        if (here.sourceArea <= 0.0) && (here.sourcePerimeter <= 0.0) {
            SourceSatCurrent = 1.0e-14;
        } else {
            SourceSatCurrent = here.sourceArea * model.jctTempSatCurDensity
                + here.sourcePerimeter * model.jctSidewallTempSatCurDensity;
        }
        if (here.drainArea <= 0.0) && (here.drainPerimeter <= 0.0) {
            DrainSatCurrent = 1.0e-14;
        } else {
            DrainSatCurrent = here.drainArea * model.jctTempSatCurDensity
                + here.drainPerimeter * model.jctSidewallTempSatCurDensity;
        }
    } else {
        return Err(SpiceError::not_yet_ported(
            format!("BSIM3 acmMod={} junction saturation currents", model.acmMod),
            "src/spicelib/devices/bsim3/b3ld.c (ACM_saturationCurrents)",
        ));
    }
    if SourceSatCurrent <= 0.0 {
        here.gbs = ckt.gmin();
        here.cbs = here.gbs * vbs;
    } else {
        if model.ijth == 0.0 {
            evbs = exp(vbs / Nvtm);
            here.gbs = SourceSatCurrent * evbs / Nvtm + ckt.gmin();
            here.cbs = SourceSatCurrent * (evbs - 1.0) + ckt.gmin() * vbs;
        } else {
            if vbs < here.vjsm {
                evbs = exp(vbs / Nvtm);
                here.gbs = SourceSatCurrent * evbs / Nvtm + ckt.gmin();
                here.cbs = SourceSatCurrent * (evbs - 1.0) + ckt.gmin() * vbs;
            } else {
                T0 = here.IsEvjsm / Nvtm;
                here.gbs = T0 + ckt.gmin();
                here.cbs =
                    here.IsEvjsm - SourceSatCurrent + T0 * (vbs - here.vjsm) + ckt.gmin() * vbs;
            }
        }
    }
    if DrainSatCurrent <= 0.0 {
        here.gbd = ckt.gmin();
        here.cbd = here.gbd * vbd;
    } else {
        if model.ijth == 0.0 {
            evbd = exp(vbd / Nvtm);
            here.gbd = DrainSatCurrent * evbd / Nvtm + ckt.gmin();
            here.cbd = DrainSatCurrent * (evbd - 1.0) + ckt.gmin() * vbd;
        } else {
            if vbd < here.vjdm {
                evbd = exp(vbd / Nvtm);
                here.gbd = DrainSatCurrent * evbd / Nvtm + ckt.gmin();
                here.cbd = DrainSatCurrent * (evbd - 1.0) + ckt.gmin() * vbd;
            } else {
                T0 = here.IsEvjdm / Nvtm;
                here.gbd = T0 + ckt.gmin();
                here.cbd =
                    here.IsEvjdm - DrainSatCurrent + T0 * (vbd - here.vjdm) + ckt.gmin() * vbd;
            }
        }
    }
    if vds >= 0.0 {
        here.mode = 1;
        Vds = vds;
        Vgs = vgs;
        Vbs = vbs;
    } else {
        here.mode = -1;
        Vds = -vds;
        Vgs = vgd;
        Vbs = vbd;
    }
    T0 = Vbs - pParam.vbsc - 0.001;
    T1 = sqrt(T0 * T0 - 0.004 * pParam.vbsc);
    Vbseff = pParam.vbsc + 0.5 * (T0 + T1);
    dVbseff_dVb = 0.5 * (1.0 + T0 / T1);
    if Vbseff < Vbs {
        Vbseff = Vbs;
    }
    if Vbseff > 0.0 {
        T0 = pParam.phi / (pParam.phi + Vbseff);
        Phis = pParam.phi * T0;
        dPhis_dVb = -T0 * T0;
        sqrtPhis = pParam.phis3 / (pParam.phi + 0.5 * Vbseff);
        dsqrtPhis_dVb = -0.5 * sqrtPhis * sqrtPhis / pParam.phis3;
    } else {
        Phis = pParam.phi - Vbseff;
        dPhis_dVb = -1.0;
        sqrtPhis = sqrt(Phis);
        dsqrtPhis_dVb = -0.5 / sqrtPhis;
    }
    Xdep = pParam.Xdep0 * sqrtPhis / pParam.sqrtPhi;
    dXdep_dVb = (pParam.Xdep0 / pParam.sqrtPhi) * dsqrtPhis_dVb;
    Leff = pParam.leff;
    Vtm = model.vtm;
    T3 = sqrt(Xdep);
    V0 = pParam.vbi - pParam.phi;
    T0 = pParam.dvt2 * Vbseff;
    if T0 >= -0.5 {
        T1 = 1.0 + T0;
        T2 = pParam.dvt2;
    } else {
        T4 = 1.0 / (3.0 + 8.0 * T0);
        T1 = (1.0 + 3.0 * T0) * T4;
        T2 = pParam.dvt2 * T4 * T4;
    }
    lt1 = model.factor1 * T3 * T1;
    dlt1_dVb = model.factor1 * (0.5 / T3 * T1 * dXdep_dVb + T3 * T2);
    T0 = pParam.dvt2w * Vbseff;
    if T0 >= -0.5 {
        T1 = 1.0 + T0;
        T2 = pParam.dvt2w;
    } else {
        T4 = 1.0 / (3.0 + 8.0 * T0);
        T1 = (1.0 + 3.0 * T0) * T4;
        T2 = pParam.dvt2w * T4 * T4;
    }
    ltw = model.factor1 * T3 * T1;
    dltw_dVb = model.factor1 * (0.5 / T3 * T1 * dXdep_dVb + T3 * T2);
    T0 = -0.5 * pParam.dvt1 * Leff / lt1;
    if T0 > -EXP_THRESHOLD {
        T1 = exp(T0);
        Theta0 = T1 * (1.0 + 2.0 * T1);
        dT1_dVb = -T0 / lt1 * T1 * dlt1_dVb;
        dTheta0_dVb = (1.0 + 4.0 * T1) * dT1_dVb;
    } else {
        T1 = MIN_EXP;
        Theta0 = T1 * (1.0 + 2.0 * T1);
        dTheta0_dVb = 0.0;
    }
    here.thetavth = pParam.dvt0 * Theta0;
    Delt_vth = here.thetavth * V0;
    dDelt_vth_dVb = pParam.dvt0 * dTheta0_dVb * V0;
    T0 = -0.5 * pParam.dvt1w * pParam.weff * Leff / ltw;
    if T0 > -EXP_THRESHOLD {
        T1 = exp(T0);
        T2 = T1 * (1.0 + 2.0 * T1);
        dT1_dVb = -T0 / ltw * T1 * dltw_dVb;
        dT2_dVb = (1.0 + 4.0 * T1) * dT1_dVb;
    } else {
        T1 = MIN_EXP;
        T2 = T1 * (1.0 + 2.0 * T1);
        dT2_dVb = 0.0;
    }
    T0 = pParam.dvt0w * T2;
    T2 = T0 * V0;
    dT2_dVb = pParam.dvt0w * dT2_dVb * V0;
    TempRatio = ckt.temp() / model.tnom - 1.0;
    T0 = sqrt(1.0 + pParam.nlx / Leff);
    T1 = pParam.k1ox * (T0 - 1.0) * pParam.sqrtPhi
        + (pParam.kt1 + pParam.kt1l / Leff + pParam.kt2 * Vbseff) * TempRatio;
    tmp2 = model.tox * pParam.phi / (pParam.weff + pParam.w0);
    T3 = pParam.eta0 + pParam.etab * Vbseff;
    if T3 < 1.0e-4 {
        T9 = 1.0 / (3.0 - 2.0e4 * T3);
        T3 = (2.0e-4 - T3) * T9;
        T4 = T9 * T9;
    } else {
        T4 = 1.0;
    }
    dDIBL_Sft_dVd = T3 * pParam.theta0vb0;
    DIBL_Sft = dDIBL_Sft_dVd * Vds;
    Vth = f64::from(model.r#type) * here.vth0 - pParam.k1 * pParam.sqrtPhi + pParam.k1ox * sqrtPhis
        - pParam.k2ox * Vbseff
        - Delt_vth
        - T2
        + (pParam.k3 + pParam.k3b * Vbseff) * tmp2
        + T1
        - DIBL_Sft;
    here.von = Vth;
    dVth_dVb = pParam.k1ox * dsqrtPhis_dVb - pParam.k2ox - dDelt_vth_dVb - dT2_dVb
        + pParam.k3b * tmp2
        - pParam.etab * Vds * pParam.theta0vb0 * T4
        + pParam.kt2 * TempRatio;
    dVth_dVd = -dDIBL_Sft_dVd;
    tmp2 = pParam.nfactor * EPSSI / Xdep;
    tmp3 = pParam.cdsc + pParam.cdscb * Vbseff + pParam.cdscd * Vds;
    tmp4 = (tmp2 + tmp3 * Theta0 + pParam.cit) / model.cox;
    if tmp4 >= -0.5 {
        n = 1.0 + tmp4;
        dn_dVb =
            (-tmp2 / Xdep * dXdep_dVb + tmp3 * dTheta0_dVb + pParam.cdscb * Theta0) / model.cox;
        dn_dVd = pParam.cdscd * Theta0 / model.cox;
    } else {
        T0 = 1.0 / (3.0 + 8.0 * tmp4);
        n = (1.0 + 3.0 * tmp4) * T0;
        T0 *= T0;
        dn_dVb = (-tmp2 / Xdep * dXdep_dVb + tmp3 * dTheta0_dVb + pParam.cdscb * Theta0)
            / model.cox
            * T0;
        dn_dVd = pParam.cdscd * Theta0 / model.cox * T0;
    }
    T0 = here.vfb + pParam.phi;
    if (pParam.ngate > 1.0e18) && (pParam.ngate < 1.0e25) && (Vgs > T0) {
        T1 = 1.0e6 * Charge_q * EPSSI * pParam.ngate / (model.cox * model.cox);
        T4 = sqrt(1.0 + 2.0 * (Vgs - T0) / T1);
        T2 = T1 * (T4 - 1.0);
        T3 = 0.5 * T2 * T2 / T1;
        T7 = 1.12 - T3 - 0.05;
        T6 = sqrt(T7 * T7 + 0.224);
        T5 = 1.12 - 0.5 * (T7 + T6);
        Vgs_eff = Vgs - T5;
        dVgs_eff_dVg = 1.0 - (0.5 - 0.5 / T4) * (1.0 + T7 / T6);
    } else {
        Vgs_eff = Vgs;
        dVgs_eff_dVg = 1.0;
    }
    Vgst = Vgs_eff - Vth;
    T10 = 2.0 * n * Vtm;
    VgstNVt = Vgst / T10;
    ExpArg = (2.0 * pParam.voff - Vgst) / T10;
    if VgstNVt > EXP_THRESHOLD {
        Vgsteff = Vgst;
        dVgsteff_dVg = dVgs_eff_dVg;
        dVgsteff_dVd = -dVth_dVd;
        dVgsteff_dVb = -dVth_dVb;
    } else if ExpArg > EXP_THRESHOLD {
        T0 = (Vgst - pParam.voff) / (n * Vtm);
        ExpVgst = exp(T0);
        Vgsteff = Vtm * pParam.cdep0 / model.cox * ExpVgst;
        dVgsteff_dVg = Vgsteff / (n * Vtm);
        dVgsteff_dVd = -dVgsteff_dVg * (dVth_dVd + T0 * Vtm * dn_dVd);
        dVgsteff_dVb = -dVgsteff_dVg * (dVth_dVb + T0 * Vtm * dn_dVb);
        dVgsteff_dVg *= dVgs_eff_dVg;
    } else {
        ExpVgst = exp(VgstNVt);
        T1 = T10 * log(1.0 + ExpVgst);
        dT1_dVg = ExpVgst / (1.0 + ExpVgst);
        dT1_dVb = -dT1_dVg * (dVth_dVb + Vgst / n * dn_dVb) + T1 / n * dn_dVb;
        dT1_dVd = -dT1_dVg * (dVth_dVd + Vgst / n * dn_dVd) + T1 / n * dn_dVd;
        dT2_dVg = -model.cox / (Vtm * pParam.cdep0) * exp(ExpArg);
        T2 = 1.0 - T10 * dT2_dVg;
        dT2_dVd = -dT2_dVg * (dVth_dVd - 2.0 * Vtm * ExpArg * dn_dVd) + (T2 - 1.0) / n * dn_dVd;
        dT2_dVb = -dT2_dVg * (dVth_dVb - 2.0 * Vtm * ExpArg * dn_dVb) + (T2 - 1.0) / n * dn_dVb;
        Vgsteff = T1 / T2;
        T3 = T2 * T2;
        dVgsteff_dVg = (T2 * dT1_dVg - T1 * dT2_dVg) / T3 * dVgs_eff_dVg;
        dVgsteff_dVd = (T2 * dT1_dVd - T1 * dT2_dVd) / T3;
        dVgsteff_dVb = (T2 * dT1_dVb - T1 * dT2_dVb) / T3;
    }
    here.Vgsteff = Vgsteff;
    T9 = sqrtPhis - pParam.sqrtPhi;
    Weff = pParam.weff - 2.0 * (pParam.dwg * Vgsteff + pParam.dwb * T9);
    dWeff_dVg = -2.0 * pParam.dwg;
    dWeff_dVb = -2.0 * pParam.dwb * dsqrtPhis_dVb;
    if Weff < 2.0e-8 {
        T0 = 1.0 / (6.0e-8 - 2.0 * Weff);
        Weff = 2.0e-8 * (4.0e-8 - Weff) * T0;
        T0 *= T0 * 4.0e-16;
        dWeff_dVg *= T0;
        dWeff_dVb *= T0;
    }
    T0 = pParam.prwg * Vgsteff + pParam.prwb * T9;
    if T0 >= -0.9 {
        Rds = pParam.rds0 * (1.0 + T0);
        dRds_dVg = pParam.rds0 * pParam.prwg;
        dRds_dVb = pParam.rds0 * pParam.prwb * dsqrtPhis_dVb;
    } else {
        T1 = 1.0 / (17.0 + 20.0 * T0);
        Rds = pParam.rds0 * (0.8 + T0) * T1;
        T1 *= T1;
        dRds_dVg = pParam.rds0 * pParam.prwg * T1;
        dRds_dVb = pParam.rds0 * pParam.prwb * dsqrtPhis_dVb * T1;
    }
    here.rds = Rds;
    T1 = 0.5 * pParam.k1ox / sqrtPhis;
    dT1_dVb = -T1 / sqrtPhis * dsqrtPhis_dVb;
    T9 = sqrt(pParam.xj * Xdep);
    tmp1 = Leff + 2.0 * T9;
    T5 = Leff / tmp1;
    tmp2 = pParam.a0 * T5;
    tmp3 = pParam.weff + pParam.b1;
    tmp4 = pParam.b0 / tmp3;
    T2 = tmp2 + tmp4;
    dT2_dVb = -T9 / tmp1 / Xdep * dXdep_dVb;
    T6 = T5 * T5;
    T7 = T5 * T6;
    Abulk0 = 1.0 + T1 * T2;
    dAbulk0_dVb = T1 * tmp2 * dT2_dVb + T2 * dT1_dVb;
    T8 = pParam.ags * pParam.a0 * T7;
    dAbulk_dVg = -T1 * T8;
    Abulk = Abulk0 + dAbulk_dVg * Vgsteff;
    dAbulk_dVb = dAbulk0_dVb - T8 * Vgsteff * (dT1_dVb + 3.0 * T1 * dT2_dVb);
    if Abulk0 < 0.1 {
        T9 = 1.0 / (3.0 - 20.0 * Abulk0);
        Abulk0 = (0.2 - Abulk0) * T9;
        dAbulk0_dVb *= T9 * T9;
    }
    if Abulk < 0.1 {
        T9 = 1.0 / (3.0 - 20.0 * Abulk);
        Abulk = (0.2 - Abulk) * T9;
        T10 = T9 * T9;
        dAbulk_dVb *= T10;
        dAbulk_dVg *= T10;
    }
    here.Abulk = Abulk;
    T2 = pParam.keta * Vbseff;
    if T2 >= -0.9 {
        T0 = 1.0 / (1.0 + T2);
        dT0_dVb = -pParam.keta * T0 * T0;
    } else {
        T1 = 1.0 / (0.8 + T2);
        T0 = (17.0 + 20.0 * T2) * T1;
        dT0_dVb = -pParam.keta * T1 * T1;
    }
    dAbulk_dVg *= T0;
    dAbulk_dVb = dAbulk_dVb * T0 + Abulk * dT0_dVb;
    dAbulk0_dVb = dAbulk0_dVb * T0 + Abulk0 * dT0_dVb;
    Abulk *= T0;
    Abulk0 *= T0;
    if model.mobMod == 1 {
        T0 = Vgsteff + Vth + Vth;
        T2 = pParam.ua + pParam.uc * Vbseff;
        T3 = T0 / model.tox;
        T5 = T3 * (T2 + pParam.ub * T3);
        dDenomi_dVg = (T2 + 2.0 * pParam.ub * T3) / model.tox;
        dDenomi_dVd = dDenomi_dVg * 2.0 * dVth_dVd;
        dDenomi_dVb = dDenomi_dVg * 2.0 * dVth_dVb + pParam.uc * T3;
    } else if model.mobMod == 2 {
        T5 = Vgsteff / model.tox
            * (pParam.ua + pParam.uc * Vbseff + pParam.ub * Vgsteff / model.tox);
        dDenomi_dVg =
            (pParam.ua + pParam.uc * Vbseff + 2.0 * pParam.ub * Vgsteff / model.tox) / model.tox;
        dDenomi_dVd = 0.0;
        dDenomi_dVb = Vgsteff * pParam.uc / model.tox;
    } else {
        T0 = Vgsteff + Vth + Vth;
        T2 = 1.0 + pParam.uc * Vbseff;
        T3 = T0 / model.tox;
        T4 = T3 * (pParam.ua + pParam.ub * T3);
        T5 = T4 * T2;
        dDenomi_dVg = (pParam.ua + 2.0 * pParam.ub * T3) * T2 / model.tox;
        dDenomi_dVd = dDenomi_dVg * 2.0 * dVth_dVd;
        dDenomi_dVb = dDenomi_dVg * 2.0 * dVth_dVb + pParam.uc * T4;
    }
    if T5 >= -0.8 {
        Denomi = 1.0 + T5;
    } else {
        T9 = 1.0 / (7.0 + 10.0 * T5);
        Denomi = (0.6 + T5) * T9;
        T9 *= T9;
        dDenomi_dVg *= T9;
        dDenomi_dVd *= T9;
        dDenomi_dVb *= T9;
    }
    ueff = here.u0temp / Denomi;
    here.ueff = ueff;
    T9 = -ueff / Denomi;
    dueff_dVg = T9 * dDenomi_dVg;
    dueff_dVd = T9 * dDenomi_dVd;
    dueff_dVb = T9 * dDenomi_dVb;
    WVCox = Weff * pParam.vsattemp * model.cox;
    WVCoxRds = WVCox * Rds;
    Esat = 2.0 * pParam.vsattemp / ueff;
    EsatL = Esat * Leff;
    T0 = -EsatL / ueff;
    dEsatL_dVg = T0 * dueff_dVg;
    dEsatL_dVd = T0 * dueff_dVd;
    dEsatL_dVb = T0 * dueff_dVb;
    a1 = pParam.a1;
    if a1 == 0.0 {
        Lambda = pParam.a2;
        dLambda_dVg = 0.0;
    } else if a1 > 0.0 {
        T0 = 1.0 - pParam.a2;
        T1 = T0 - pParam.a1 * Vgsteff - 0.0001;
        T2 = sqrt(T1 * T1 + 0.0004 * T0);
        Lambda = pParam.a2 + T0 - 0.5 * (T1 + T2);
        dLambda_dVg = 0.5 * pParam.a1 * (1.0 + T1 / T2);
    } else {
        T1 = pParam.a2 + pParam.a1 * Vgsteff - 0.0001;
        T2 = sqrt(T1 * T1 + 0.0004 * pParam.a2);
        Lambda = 0.5 * (T1 + T2);
        dLambda_dVg = 0.5 * pParam.a1 * (1.0 + T1 / T2);
    }
    Vgst2Vtm = Vgsteff + 2.0 * Vtm;
    here.AbovVgst2Vtm = Abulk / Vgst2Vtm;
    if Rds > 0.0 {
        tmp2 = dRds_dVg / Rds + dWeff_dVg / Weff;
        tmp3 = dRds_dVb / Rds + dWeff_dVb / Weff;
    } else {
        tmp2 = dWeff_dVg / Weff;
        tmp3 = dWeff_dVb / Weff;
    }
    if (Rds == 0.0) && (Lambda == 1.0) {
        T0 = 1.0 / (Abulk * EsatL + Vgst2Vtm);
        tmp1 = 0.0;
        T1 = T0 * T0;
        T2 = Vgst2Vtm * T0;
        T3 = EsatL * Vgst2Vtm;
        Vdsat = T3 * T0;
        dT0_dVg = -(Abulk * dEsatL_dVg + EsatL * dAbulk_dVg + 1.0) * T1;
        dT0_dVd = -(Abulk * dEsatL_dVd) * T1;
        dT0_dVb = -(Abulk * dEsatL_dVb + dAbulk_dVb * EsatL) * T1;
        dVdsat_dVg = T3 * dT0_dVg + T2 * dEsatL_dVg + EsatL * T0;
        dVdsat_dVd = T3 * dT0_dVd + T2 * dEsatL_dVd;
        dVdsat_dVb = T3 * dT0_dVb + T2 * dEsatL_dVb;
    } else {
        tmp1 = dLambda_dVg / (Lambda * Lambda);
        T9 = Abulk * WVCoxRds;
        T8 = Abulk * T9;
        T7 = Vgst2Vtm * T9;
        T6 = Vgst2Vtm * WVCoxRds;
        T0 = 2.0 * Abulk * (T9 - 1.0 + 1.0 / Lambda);
        dT0_dVg = 2.0 * (T8 * tmp2 - Abulk * tmp1 + (2.0 * T9 + 1.0 / Lambda - 1.0) * dAbulk_dVg);
        dT0_dVb =
            2.0 * (T8 * (2.0 / Abulk * dAbulk_dVb + tmp3) + (1.0 / Lambda - 1.0) * dAbulk_dVb);
        dT0_dVd = 0.0;
        T1 = Vgst2Vtm * (2.0 / Lambda - 1.0) + Abulk * EsatL + 3.0 * T7;
        dT1_dVg = (2.0 / Lambda - 1.0) - 2.0 * Vgst2Vtm * tmp1
            + Abulk * dEsatL_dVg
            + EsatL * dAbulk_dVg
            + 3.0 * (T9 + T7 * tmp2 + T6 * dAbulk_dVg);
        dT1_dVb = Abulk * dEsatL_dVb + EsatL * dAbulk_dVb + 3.0 * (T6 * dAbulk_dVb + T7 * tmp3);
        dT1_dVd = Abulk * dEsatL_dVd;
        T2 = Vgst2Vtm * (EsatL + 2.0 * T6);
        dT2_dVg = EsatL + Vgst2Vtm * dEsatL_dVg + T6 * (4.0 + 2.0 * Vgst2Vtm * tmp2);
        dT2_dVb = Vgst2Vtm * (dEsatL_dVb + 2.0 * T6 * tmp3);
        dT2_dVd = Vgst2Vtm * dEsatL_dVd;
        T3 = sqrt(T1 * T1 - 2.0 * T0 * T2);
        Vdsat = (T1 - T3) / T0;
        dT3_dVg = (T1 * dT1_dVg - 2.0 * (T0 * dT2_dVg + T2 * dT0_dVg)) / T3;
        dT3_dVd = (T1 * dT1_dVd - 2.0 * (T0 * dT2_dVd + T2 * dT0_dVd)) / T3;
        dT3_dVb = (T1 * dT1_dVb - 2.0 * (T0 * dT2_dVb + T2 * dT0_dVb)) / T3;
        dVdsat_dVg =
            (dT1_dVg - (T1 * dT1_dVg - dT0_dVg * T2 - T0 * dT2_dVg) / T3 - Vdsat * dT0_dVg) / T0;
        dVdsat_dVb =
            (dT1_dVb - (T1 * dT1_dVb - dT0_dVb * T2 - T0 * dT2_dVb) / T3 - Vdsat * dT0_dVb) / T0;
        dVdsat_dVd = (dT1_dVd - (T1 * dT1_dVd - T0 * dT2_dVd) / T3) / T0;
    }
    here.vdsat = Vdsat;
    T1 = Vdsat - Vds - pParam.delta;
    dT1_dVg = dVdsat_dVg;
    dT1_dVd = dVdsat_dVd - 1.0;
    dT1_dVb = dVdsat_dVb;
    T2 = sqrt(T1 * T1 + 4.0 * pParam.delta * Vdsat);
    T0 = T1 / T2;
    T3 = 2.0 * pParam.delta / T2;
    dT2_dVg = T0 * dT1_dVg + T3 * dVdsat_dVg;
    dT2_dVd = T0 * dT1_dVd + T3 * dVdsat_dVd;
    dT2_dVb = T0 * dT1_dVb + T3 * dVdsat_dVb;
    Vdseff = Vdsat - 0.5 * (T1 + T2);
    dVdseff_dVg = dVdsat_dVg - 0.5 * (dT1_dVg + dT2_dVg);
    dVdseff_dVd = dVdsat_dVd - 0.5 * (dT1_dVd + dT2_dVd);
    dVdseff_dVb = dVdsat_dVb - 0.5 * (dT1_dVb + dT2_dVb);
    if Vds == 0.0 {
        Vdseff = 0.0;
        dVdseff_dVg = 0.0;
        dVdseff_dVb = 0.0;
    }
    tmp4 = 1.0 - 0.5 * Abulk * Vdsat / Vgst2Vtm;
    T9 = WVCoxRds * Vgsteff;
    T8 = T9 / Vgst2Vtm;
    T0 = EsatL + Vdsat + 2.0 * T9 * tmp4;
    T7 = 2.0 * WVCoxRds * tmp4;
    dT0_dVg = dEsatL_dVg + dVdsat_dVg + T7 * (1.0 + tmp2 * Vgsteff)
        - T8 * (Abulk * dVdsat_dVg - Abulk * Vdsat / Vgst2Vtm + Vdsat * dAbulk_dVg);
    dT0_dVb = dEsatL_dVb + dVdsat_dVb + T7 * tmp3 * Vgsteff
        - T8 * (dAbulk_dVb * Vdsat + Abulk * dVdsat_dVb);
    dT0_dVd = dEsatL_dVd + dVdsat_dVd - T8 * Abulk * dVdsat_dVd;
    T9 = WVCoxRds * Abulk;
    T1 = 2.0 / Lambda - 1.0 + T9;
    dT1_dVg = -2.0 * tmp1 + WVCoxRds * (Abulk * tmp2 + dAbulk_dVg);
    dT1_dVb = dAbulk_dVb * WVCoxRds + T9 * tmp3;
    Vasat = T0 / T1;
    dVasat_dVg = (dT0_dVg - Vasat * dT1_dVg) / T1;
    dVasat_dVb = (dT0_dVb - Vasat * dT1_dVb) / T1;
    dVasat_dVd = dT0_dVd / T1;
    if Vdseff > Vds {
        Vdseff = Vds;
    }
    diffVds = Vds - Vdseff;
    here.Vdseff = Vdseff;
    if (pParam.pclm > 0.0) && (diffVds > 1.0e-10) {
        T0 = 1.0 / (pParam.pclm * Abulk * pParam.litl);
        dT0_dVb = -T0 / Abulk * dAbulk_dVb;
        dT0_dVg = -T0 / Abulk * dAbulk_dVg;
        T2 = Vgsteff / EsatL;
        T1 = Leff * (Abulk + T2);
        dT1_dVg = Leff * ((1.0 - T2 * dEsatL_dVg) / EsatL + dAbulk_dVg);
        dT1_dVb = Leff * (dAbulk_dVb - T2 * dEsatL_dVb / EsatL);
        dT1_dVd = -T2 * dEsatL_dVd / Esat;
        T9 = T0 * T1;
        VACLM = T9 * diffVds;
        dVACLM_dVg = T0 * dT1_dVg * diffVds - T9 * dVdseff_dVg + T1 * diffVds * dT0_dVg;
        dVACLM_dVb = (dT0_dVb * T1 + T0 * dT1_dVb) * diffVds - T9 * dVdseff_dVb;
        dVACLM_dVd = T0 * dT1_dVd * diffVds + T9 * (1.0 - dVdseff_dVd);
    } else {
        VACLM = MAX_EXP;
        dVACLM_dVb = 0.0;
        dVACLM_dVg = dVACLM_dVb;
        dVACLM_dVd = dVACLM_dVg;
    }
    if pParam.thetaRout > 0.0 {
        T8 = Abulk * Vdsat;
        T0 = Vgst2Vtm * T8;
        dT0_dVg = Vgst2Vtm * Abulk * dVdsat_dVg + T8 + Vgst2Vtm * Vdsat * dAbulk_dVg;
        dT0_dVb = Vgst2Vtm * (dAbulk_dVb * Vdsat + Abulk * dVdsat_dVb);
        dT0_dVd = Vgst2Vtm * Abulk * dVdsat_dVd;
        T1 = Vgst2Vtm + T8;
        dT1_dVg = 1.0 + Abulk * dVdsat_dVg + Vdsat * dAbulk_dVg;
        dT1_dVb = Abulk * dVdsat_dVb + dAbulk_dVb * Vdsat;
        dT1_dVd = Abulk * dVdsat_dVd;
        T9 = T1 * T1;
        T2 = pParam.thetaRout;
        VADIBL = (Vgst2Vtm - T0 / T1) / T2;
        dVADIBL_dVg = (1.0 - dT0_dVg / T1 + T0 * dT1_dVg / T9) / T2;
        dVADIBL_dVb = (-dT0_dVb / T1 + T0 * dT1_dVb / T9) / T2;
        dVADIBL_dVd = (-dT0_dVd / T1 + T0 * dT1_dVd / T9) / T2;
        T7 = pParam.pdiblb * Vbseff;
        if T7 >= -0.9 {
            T3 = 1.0 / (1.0 + T7);
            VADIBL *= T3;
            dVADIBL_dVg *= T3;
            dVADIBL_dVb = (dVADIBL_dVb - VADIBL * pParam.pdiblb) * T3;
            dVADIBL_dVd *= T3;
        } else {
            T4 = 1.0 / (0.8 + T7);
            T3 = (17.0 + 20.0 * T7) * T4;
            dVADIBL_dVg *= T3;
            dVADIBL_dVb = dVADIBL_dVb * T3 - VADIBL * pParam.pdiblb * T4 * T4;
            dVADIBL_dVd *= T3;
            VADIBL *= T3;
        }
    } else {
        VADIBL = MAX_EXP;
        dVADIBL_dVb = 0.0;
        dVADIBL_dVg = dVADIBL_dVb;
        dVADIBL_dVd = dVADIBL_dVg;
    }
    T8 = pParam.pvag / EsatL;
    T9 = T8 * Vgsteff;
    if T9 > -0.9 {
        T0 = 1.0 + T9;
        dT0_dVg = T8 * (1.0 - Vgsteff * dEsatL_dVg / EsatL);
        dT0_dVb = -T9 * dEsatL_dVb / EsatL;
        dT0_dVd = -T9 * dEsatL_dVd / EsatL;
    } else {
        T1 = 1.0 / (17.0 + 20.0 * T9);
        T0 = (0.8 + T9) * T1;
        T1 *= T1;
        dT0_dVg = T8 * (1.0 - Vgsteff * dEsatL_dVg / EsatL) * T1;
        T9 *= T1 / EsatL;
        dT0_dVb = -T9 * dEsatL_dVb;
        dT0_dVd = -T9 * dEsatL_dVd;
    }
    tmp1 = VACLM * VACLM;
    tmp2 = VADIBL * VADIBL;
    tmp3 = VACLM + VADIBL;
    T1 = VACLM * VADIBL / tmp3;
    tmp3 *= tmp3;
    dT1_dVg = (tmp1 * dVADIBL_dVg + tmp2 * dVACLM_dVg) / tmp3;
    dT1_dVd = (tmp1 * dVADIBL_dVd + tmp2 * dVACLM_dVd) / tmp3;
    dT1_dVb = (tmp1 * dVADIBL_dVb + tmp2 * dVACLM_dVb) / tmp3;
    Va = Vasat + T0 * T1;
    dVa_dVg = dVasat_dVg + T1 * dT0_dVg + T0 * dT1_dVg;
    dVa_dVd = dVasat_dVd + T1 * dT0_dVd + T0 * dT1_dVd;
    dVa_dVb = dVasat_dVb + T1 * dT0_dVb + T0 * dT1_dVb;
    if pParam.pscbe2 > 0.0 {
        if diffVds > pParam.pscbe1 * pParam.litl / EXP_THRESHOLD {
            T0 = pParam.pscbe1 * pParam.litl / diffVds;
            VASCBE = Leff * exp(T0) / pParam.pscbe2;
            T1 = T0 * VASCBE / diffVds;
            dVASCBE_dVg = T1 * dVdseff_dVg;
            dVASCBE_dVd = -T1 * (1.0 - dVdseff_dVd);
            dVASCBE_dVb = T1 * dVdseff_dVb;
        } else {
            VASCBE = MAX_EXP * Leff / pParam.pscbe2;
            dVASCBE_dVb = 0.0;
            dVASCBE_dVd = dVASCBE_dVb;
            dVASCBE_dVg = dVASCBE_dVd;
        }
    } else {
        VASCBE = MAX_EXP;
        dVASCBE_dVb = 0.0;
        dVASCBE_dVd = dVASCBE_dVb;
        dVASCBE_dVg = dVASCBE_dVd;
    }
    CoxWovL = model.cox * Weff / Leff;
    beta = ueff * CoxWovL;
    dbeta_dVg = CoxWovL * dueff_dVg + beta * dWeff_dVg / Weff;
    dbeta_dVd = CoxWovL * dueff_dVd;
    dbeta_dVb = CoxWovL * dueff_dVb + beta * dWeff_dVb / Weff;
    T0 = 1.0 - 0.5 * Abulk * Vdseff / Vgst2Vtm;
    dT0_dVg =
        -0.5 * (Abulk * dVdseff_dVg - Abulk * Vdseff / Vgst2Vtm + Vdseff * dAbulk_dVg) / Vgst2Vtm;
    dT0_dVd = -0.5 * Abulk * dVdseff_dVd / Vgst2Vtm;
    dT0_dVb = -0.5 * (Abulk * dVdseff_dVb + dAbulk_dVb * Vdseff) / Vgst2Vtm;
    fgche1 = Vgsteff * T0;
    dfgche1_dVg = Vgsteff * dT0_dVg + T0;
    dfgche1_dVd = Vgsteff * dT0_dVd;
    dfgche1_dVb = Vgsteff * dT0_dVb;
    T9 = Vdseff / EsatL;
    fgche2 = 1.0 + T9;
    dfgche2_dVg = (dVdseff_dVg - T9 * dEsatL_dVg) / EsatL;
    dfgche2_dVd = (dVdseff_dVd - T9 * dEsatL_dVd) / EsatL;
    dfgche2_dVb = (dVdseff_dVb - T9 * dEsatL_dVb) / EsatL;
    gche = beta * fgche1 / fgche2;
    dgche_dVg = (beta * dfgche1_dVg + fgche1 * dbeta_dVg - gche * dfgche2_dVg) / fgche2;
    dgche_dVd = (beta * dfgche1_dVd + fgche1 * dbeta_dVd - gche * dfgche2_dVd) / fgche2;
    dgche_dVb = (beta * dfgche1_dVb + fgche1 * dbeta_dVb - gche * dfgche2_dVb) / fgche2;
    T0 = 1.0 + gche * Rds;
    T9 = Vdseff / T0;
    Idl = gche * T9;
    dIdl_dVg = (gche * dVdseff_dVg + T9 * dgche_dVg) / T0 - Idl * gche / T0 * dRds_dVg;
    dIdl_dVd = (gche * dVdseff_dVd + T9 * dgche_dVd) / T0;
    dIdl_dVb = (gche * dVdseff_dVb + T9 * dgche_dVb - Idl * dRds_dVb * gche) / T0;
    T9 = diffVds / Va;
    T0 = 1.0 + T9;
    Idsa = Idl * T0;
    dIdsa_dVg = T0 * dIdl_dVg - Idl * (dVdseff_dVg + T9 * dVa_dVg) / Va;
    dIdsa_dVd = T0 * dIdl_dVd + Idl * (1.0 - dVdseff_dVd - T9 * dVa_dVd) / Va;
    dIdsa_dVb = T0 * dIdl_dVb - Idl * (dVdseff_dVb + T9 * dVa_dVb) / Va;
    T9 = diffVds / VASCBE;
    T0 = 1.0 + T9;
    Ids = Idsa * T0;
    Gm = T0 * dIdsa_dVg - Idsa * (dVdseff_dVg + T9 * dVASCBE_dVg) / VASCBE;
    Gds = T0 * dIdsa_dVd + Idsa * (1.0 - dVdseff_dVd - T9 * dVASCBE_dVd) / VASCBE;
    Gmb = T0 * dIdsa_dVb - Idsa * (dVdseff_dVb + T9 * dVASCBE_dVb) / VASCBE;
    Gds += Gm * dVgsteff_dVd;
    Gmb += Gm * dVgsteff_dVb;
    Gm *= dVgsteff_dVg;
    Gmb *= dVbseff_dVb;
    tmp = pParam.alpha0 + pParam.alpha1 * Leff;
    if (tmp <= 0.0) || (pParam.beta0 <= 0.0) {
        Gbg = 0.0;
        Gbb = Gbg;
        Gbd = Gbb;
        Isub = Gbd;
    } else {
        T2 = tmp / Leff;
        if diffVds > pParam.beta0 / EXP_THRESHOLD {
            T0 = -pParam.beta0 / diffVds;
            T1 = T2 * diffVds * exp(T0);
            T3 = T1 / diffVds * (T0 - 1.0);
            dT1_dVg = T3 * dVdseff_dVg;
            dT1_dVd = T3 * (dVdseff_dVd - 1.0);
            dT1_dVb = T3 * dVdseff_dVb;
        } else {
            T3 = T2 * MIN_EXP;
            T1 = T3 * diffVds;
            dT1_dVg = -T3 * dVdseff_dVg;
            dT1_dVd = T3 * (1.0 - dVdseff_dVd);
            dT1_dVb = -T3 * dVdseff_dVb;
        }
        Isub = T1 * Idsa;
        Gbg = T1 * dIdsa_dVg + Idsa * dT1_dVg;
        Gbd = T1 * dIdsa_dVd + Idsa * dT1_dVd;
        Gbb = T1 * dIdsa_dVb + Idsa * dT1_dVb;
        Gbd += Gbg * dVgsteff_dVd;
        Gbb += Gbg * dVgsteff_dVb;
        Gbg *= dVgsteff_dVg;
        Gbb *= dVbseff_dVb;
    }
    cdrain = Ids;
    here.gds = Gds;
    here.gm = Gm;
    here.gmbs = Gmb;
    here.gbbs = Gbb;
    here.gbgs = Gbg;
    here.gbds = Gbd;
    here.csub = Isub;
    if (model.xpart < 0.0) || (!(ChargeComputationNeeded != 0)) {
        qbulk = 0.0;
        qsrc = qbulk;
        qdrn = qsrc;
        qgate = qdrn;
        here.cgdb = 0.0;
        here.cgsb = here.cgdb;
        here.cggb = here.cgsb;
        here.cddb = 0.0;
        here.cdsb = here.cddb;
        here.cdgb = here.cdsb;
        here.cbdb = 0.0;
        here.cbsb = here.cbdb;
        here.cbgb = here.cbsb;
        here.cqbb = 0.0;
        here.cqgb = here.cqbb;
        here.cqsb = here.cqgb;
        here.cqdb = here.cqsb;
        here.gtau = 0.0;
        // C's `goto finished`: the end of this if/else chain.
    } else if model.capMod == 0 {
        if Vbseff < 0.0 {
            Vbseff = Vbs;
            dVbseff_dVb = 1.0;
        } else {
            Vbseff = pParam.phi - Phis;
            dVbseff_dVb = -dPhis_dVb;
        }
        Vfb = pParam.vfbcv;
        Vth = Vfb + pParam.phi + pParam.k1ox * sqrtPhis;
        Vgst = Vgs_eff - Vth;
        dVth_dVb = pParam.k1ox * dsqrtPhis_dVb;
        dVgst_dVb = -dVth_dVb;
        dVgst_dVg = dVgs_eff_dVg;
        CoxWL = model.cox * pParam.weffCV * pParam.leffCV;
        Arg1 = Vgs_eff - Vbseff - Vfb;
        if Arg1 <= 0.0 {
            qgate = CoxWL * Arg1;
            qbulk = -qgate;
            qdrn = 0.0;
            here.cggb = CoxWL * dVgs_eff_dVg;
            here.cgdb = 0.0;
            here.cgsb = CoxWL * (dVbseff_dVb - dVgs_eff_dVg);
            here.cdgb = 0.0;
            here.cddb = 0.0;
            here.cdsb = 0.0;
            here.cbgb = -CoxWL * dVgs_eff_dVg;
            here.cbdb = 0.0;
            here.cbsb = -here.cgsb;
            here.qinv = 0.0;
        } else if Vgst <= 0.0 {
            T1 = 0.5 * pParam.k1ox;
            T2 = sqrt(T1 * T1 + Arg1);
            qgate = CoxWL * pParam.k1ox * (T2 - T1);
            qbulk = -qgate;
            qdrn = 0.0;
            T0 = CoxWL * T1 / T2;
            here.cggb = T0 * dVgs_eff_dVg;
            here.cgdb = 0.0;
            here.cgsb = T0 * (dVbseff_dVb - dVgs_eff_dVg);
            here.cdgb = 0.0;
            here.cddb = 0.0;
            here.cdsb = 0.0;
            here.cbgb = -here.cggb;
            here.cbdb = 0.0;
            here.cbsb = -here.cgsb;
            here.qinv = 0.0;
        } else {
            One_Third_CoxWL = CoxWL / 3.0;
            Two_Third_CoxWL = 2.0 * One_Third_CoxWL;
            AbulkCV = Abulk0 * pParam.abulkCVfactor;
            dAbulkCV_dVb = pParam.abulkCVfactor * dAbulk0_dVb;
            Vdsat = Vgst / AbulkCV;
            dVdsat_dVg = dVgs_eff_dVg / AbulkCV;
            dVdsat_dVb = -(Vdsat * dAbulkCV_dVb + dVth_dVb) / AbulkCV;
            if model.xpart > 0.5 {
                if Vdsat <= Vds {
                    T1 = Vdsat / 3.0;
                    qgate = CoxWL * (Vgs_eff - Vfb - pParam.phi - T1);
                    T2 = -Two_Third_CoxWL * Vgst;
                    qbulk = -(qgate + T2);
                    qdrn = 0.0;
                    here.cggb = One_Third_CoxWL * (3.0 - dVdsat_dVg) * dVgs_eff_dVg;
                    T2 = -One_Third_CoxWL * dVdsat_dVb;
                    here.cgsb = -(here.cggb + T2);
                    here.cgdb = 0.0;
                    here.cdgb = 0.0;
                    here.cddb = 0.0;
                    here.cdsb = 0.0;
                    here.cbgb = -(here.cggb - Two_Third_CoxWL * dVgs_eff_dVg);
                    T3 = -(T2 + Two_Third_CoxWL * dVth_dVb);
                    here.cbsb = -(here.cbgb + T3);
                    here.cbdb = 0.0;
                    here.qinv = -(qgate + qbulk);
                } else {
                    Alphaz = Vgst / Vdsat;
                    T1 = 2.0 * Vdsat - Vds;
                    T2 = Vds / (3.0 * T1);
                    T3 = T2 * Vds;
                    T9 = 0.25 * CoxWL;
                    T4 = T9 * Alphaz;
                    T7 = 2.0 * Vds - T1 - 3.0 * T3;
                    T8 = T3 - T1 - 2.0 * Vds;
                    qgate = CoxWL * (Vgs_eff - Vfb - pParam.phi - 0.5 * (Vds - T3));
                    T10 = T4 * T8;
                    qdrn = T4 * T7;
                    qbulk = -(qgate + qdrn + T10);
                    T5 = T3 / T1;
                    here.cggb = CoxWL * (1.0 - T5 * dVdsat_dVg) * dVgs_eff_dVg;
                    T11 = -CoxWL * T5 * dVdsat_dVb;
                    here.cgdb = CoxWL * (T2 - 0.5 + 0.5 * T5);
                    here.cgsb = -(here.cggb + T11 + here.cgdb);
                    T6 = 1.0 / Vdsat;
                    dAlphaz_dVg = T6 * (1.0 - Alphaz * dVdsat_dVg);
                    dAlphaz_dVb = -T6 * (dVth_dVb + Alphaz * dVdsat_dVb);
                    T7 = T9 * T7;
                    T8 = T9 * T8;
                    T9 = 2.0 * T4 * (1.0 - 3.0 * T5);
                    here.cdgb = (T7 * dAlphaz_dVg - T9 * dVdsat_dVg) * dVgs_eff_dVg;
                    T12 = T7 * dAlphaz_dVb - T9 * dVdsat_dVb;
                    here.cddb = T4 * (3.0 - 6.0 * T2 - 3.0 * T5);
                    here.cdsb = -(here.cdgb + T12 + here.cddb);
                    T9 = 2.0 * T4 * (1.0 + T5);
                    T10 = (T8 * dAlphaz_dVg - T9 * dVdsat_dVg) * dVgs_eff_dVg;
                    T11 = T8 * dAlphaz_dVb - T9 * dVdsat_dVb;
                    T12 = T4 * (2.0 * T2 + T5 - 1.0);
                    T0 = -(T10 + T11 + T12);
                    here.cbgb = -(here.cggb + here.cdgb + T10);
                    here.cbdb = -(here.cgdb + here.cddb + T12);
                    here.cbsb = -(here.cgsb + here.cdsb + T0);
                    here.qinv = -(qgate + qbulk);
                }
            } else if model.xpart < 0.5 {
                if Vds >= Vdsat {
                    T1 = Vdsat / 3.0;
                    qgate = CoxWL * (Vgs_eff - Vfb - pParam.phi - T1);
                    T2 = -Two_Third_CoxWL * Vgst;
                    qbulk = -(qgate + T2);
                    qdrn = 0.4 * T2;
                    here.cggb = One_Third_CoxWL * (3.0 - dVdsat_dVg) * dVgs_eff_dVg;
                    T2 = -One_Third_CoxWL * dVdsat_dVb;
                    here.cgsb = -(here.cggb + T2);
                    here.cgdb = 0.0;
                    T3 = 0.4 * Two_Third_CoxWL;
                    here.cdgb = -T3 * dVgs_eff_dVg;
                    here.cddb = 0.0;
                    T4 = T3 * dVth_dVb;
                    here.cdsb = -(T4 + here.cdgb);
                    here.cbgb = -(here.cggb - Two_Third_CoxWL * dVgs_eff_dVg);
                    T3 = -(T2 + Two_Third_CoxWL * dVth_dVb);
                    here.cbsb = -(here.cbgb + T3);
                    here.cbdb = 0.0;
                    here.qinv = -(qgate + qbulk);
                } else {
                    Alphaz = Vgst / Vdsat;
                    T1 = 2.0 * Vdsat - Vds;
                    T2 = Vds / (3.0 * T1);
                    T3 = T2 * Vds;
                    T9 = 0.25 * CoxWL;
                    T4 = T9 * Alphaz;
                    qgate = CoxWL * (Vgs_eff - Vfb - pParam.phi - 0.5 * (Vds - T3));
                    T5 = T3 / T1;
                    here.cggb = CoxWL * (1.0 - T5 * dVdsat_dVg) * dVgs_eff_dVg;
                    tmp = -CoxWL * T5 * dVdsat_dVb;
                    here.cgdb = CoxWL * (T2 - 0.5 + 0.5 * T5);
                    here.cgsb = -(here.cggb + here.cgdb + tmp);
                    T6 = 1.0 / Vdsat;
                    dAlphaz_dVg = T6 * (1.0 - Alphaz * dVdsat_dVg);
                    dAlphaz_dVb = -T6 * (dVth_dVb + Alphaz * dVdsat_dVb);
                    T6 = 8.0 * Vdsat * Vdsat - 6.0 * Vdsat * Vds + 1.2 * Vds * Vds;
                    T8 = T2 / T1;
                    T7 = Vds - T1 - T8 * T6;
                    qdrn = T4 * T7;
                    T7 *= T9;
                    tmp = T8 / T1;
                    tmp1 = T4 * (2.0 - 4.0 * tmp * T6 + T8 * (16.0 * Vdsat - 6.0 * Vds));
                    here.cdgb = (T7 * dAlphaz_dVg - tmp1 * dVdsat_dVg) * dVgs_eff_dVg;
                    T10 = T7 * dAlphaz_dVb - tmp1 * dVdsat_dVb;
                    here.cddb = T4
                        * (2.0 - (1.0 / (3.0 * T1 * T1) + 2.0 * tmp) * T6
                            + T8 * (6.0 * Vdsat - 2.4 * Vds));
                    here.cdsb = -(here.cdgb + T10 + here.cddb);
                    T7 = 2.0 * (T1 + T3);
                    qbulk = -(qgate - T4 * T7);
                    T7 *= T9;
                    T0 = 4.0 * T4 * (1.0 - T5);
                    T12 = (-T7 * dAlphaz_dVg - here.cdgb - T0 * dVdsat_dVg) * dVgs_eff_dVg;
                    T11 = -T7 * dAlphaz_dVb - T10 - T0 * dVdsat_dVb;
                    T10 = -4.0 * T4 * (T2 - 0.5 + 0.5 * T5) - here.cddb;
                    tmp = -(T10 + T11 + T12);
                    here.cbgb = -(here.cggb + here.cdgb + T12);
                    here.cbdb = -(here.cgdb + here.cddb + T10);
                    here.cbsb = -(here.cgsb + here.cdsb + tmp);
                    here.qinv = -(qgate + qbulk);
                }
            } else {
                if Vds >= Vdsat {
                    T1 = Vdsat / 3.0;
                    qgate = CoxWL * (Vgs_eff - Vfb - pParam.phi - T1);
                    T2 = -Two_Third_CoxWL * Vgst;
                    qbulk = -(qgate + T2);
                    qdrn = 0.5 * T2;
                    here.cggb = One_Third_CoxWL * (3.0 - dVdsat_dVg) * dVgs_eff_dVg;
                    T2 = -One_Third_CoxWL * dVdsat_dVb;
                    here.cgsb = -(here.cggb + T2);
                    here.cgdb = 0.0;
                    here.cdgb = -One_Third_CoxWL * dVgs_eff_dVg;
                    here.cddb = 0.0;
                    T4 = One_Third_CoxWL * dVth_dVb;
                    here.cdsb = -(T4 + here.cdgb);
                    here.cbgb = -(here.cggb - Two_Third_CoxWL * dVgs_eff_dVg);
                    T3 = -(T2 + Two_Third_CoxWL * dVth_dVb);
                    here.cbsb = -(here.cbgb + T3);
                    here.cbdb = 0.0;
                    here.qinv = -(qgate + qbulk);
                } else {
                    Alphaz = Vgst / Vdsat;
                    T1 = 2.0 * Vdsat - Vds;
                    T2 = Vds / (3.0 * T1);
                    T3 = T2 * Vds;
                    T9 = 0.25 * CoxWL;
                    T4 = T9 * Alphaz;
                    qgate = CoxWL * (Vgs_eff - Vfb - pParam.phi - 0.5 * (Vds - T3));
                    T5 = T3 / T1;
                    here.cggb = CoxWL * (1.0 - T5 * dVdsat_dVg) * dVgs_eff_dVg;
                    tmp = -CoxWL * T5 * dVdsat_dVb;
                    here.cgdb = CoxWL * (T2 - 0.5 + 0.5 * T5);
                    here.cgsb = -(here.cggb + here.cgdb + tmp);
                    T6 = 1.0 / Vdsat;
                    dAlphaz_dVg = T6 * (1.0 - Alphaz * dVdsat_dVg);
                    dAlphaz_dVb = -T6 * (dVth_dVb + Alphaz * dVdsat_dVb);
                    T7 = T1 + T3;
                    qdrn = -T4 * T7;
                    qbulk = -(qgate + qdrn + qdrn);
                    T7 *= T9;
                    T0 = T4 * (2.0 * T5 - 2.0);
                    here.cdgb = (T0 * dVdsat_dVg - T7 * dAlphaz_dVg) * dVgs_eff_dVg;
                    T12 = T0 * dVdsat_dVb - T7 * dAlphaz_dVb;
                    here.cddb = T4 * (1.0 - 2.0 * T2 - T5);
                    here.cdsb = -(here.cdgb + T12 + here.cddb);
                    here.cbgb = -(here.cggb + 2.0 * here.cdgb);
                    here.cbdb = -(here.cgdb + 2.0 * here.cddb);
                    here.cbsb = -(here.cgsb + 2.0 * here.cdsb);
                    here.qinv = -(qgate + qbulk);
                }
            }
        }
    } else {
        if Vbseff < 0.0 {
            VbseffCV = Vbseff;
            dVbseffCV_dVb = 1.0;
        } else {
            VbseffCV = pParam.phi - Phis;
            dVbseffCV_dVb = -dPhis_dVb;
        }
        CoxWL = model.cox * pParam.weffCV * pParam.leffCV;
        noff = n * pParam.noff;
        dnoff_dVd = pParam.noff * dn_dVd;
        dnoff_dVb = pParam.noff * dn_dVb;
        T0 = Vtm * noff;
        voffcv = pParam.voffcv;
        VgstNVt = (Vgst - voffcv) / T0;
        if VgstNVt > EXP_THRESHOLD {
            Vgsteff = Vgst - voffcv;
            dVgsteff_dVg = dVgs_eff_dVg;
            dVgsteff_dVd = -dVth_dVd;
            dVgsteff_dVb = -dVth_dVb;
        } else if VgstNVt < -EXP_THRESHOLD {
            Vgsteff = T0 * log(1.0 + MIN_EXP);
            dVgsteff_dVg = 0.0;
            dVgsteff_dVd = Vgsteff / noff;
            dVgsteff_dVb = dVgsteff_dVd * dnoff_dVb;
            dVgsteff_dVd *= dnoff_dVd;
        } else {
            ExpVgst = exp(VgstNVt);
            Vgsteff = T0 * log(1.0 + ExpVgst);
            dVgsteff_dVg = ExpVgst / (1.0 + ExpVgst);
            dVgsteff_dVd = -dVgsteff_dVg * (dVth_dVd + (Vgst - voffcv) / noff * dnoff_dVd)
                + Vgsteff / noff * dnoff_dVd;
            dVgsteff_dVb = -dVgsteff_dVg * (dVth_dVb + (Vgst - voffcv) / noff * dnoff_dVb)
                + Vgsteff / noff * dnoff_dVb;
            dVgsteff_dVg *= dVgs_eff_dVg;
        }
        if model.capMod == 1 {
            Vfb = here.vfbzb;
            Arg1 = Vgs_eff - VbseffCV - Vfb - Vgsteff;
            if Arg1 <= 0.0 {
                qgate = CoxWL * Arg1;
                Cgg = CoxWL * (dVgs_eff_dVg - dVgsteff_dVg);
                Cgd = -CoxWL * dVgsteff_dVd;
                Cgb = -CoxWL * (dVbseffCV_dVb + dVgsteff_dVb);
            } else {
                T0 = 0.5 * pParam.k1ox;
                T1 = sqrt(T0 * T0 + Arg1);
                T2 = CoxWL * T0 / T1;
                qgate = CoxWL * pParam.k1ox * (T1 - T0);
                Cgg = T2 * (dVgs_eff_dVg - dVgsteff_dVg);
                Cgd = -T2 * dVgsteff_dVd;
                Cgb = -T2 * (dVbseffCV_dVb + dVgsteff_dVb);
            }
            qbulk = -qgate;
            Cbg = -Cgg;
            Cbd = -Cgd;
            Cbb = -Cgb;
            One_Third_CoxWL = CoxWL / 3.0;
            Two_Third_CoxWL = 2.0 * One_Third_CoxWL;
            AbulkCV = Abulk0 * pParam.abulkCVfactor;
            dAbulkCV_dVb = pParam.abulkCVfactor * dAbulk0_dVb;
            VdsatCV = Vgsteff / AbulkCV;
            if VdsatCV < Vds {
                dVdsatCV_dVg = 1.0 / AbulkCV;
                dVdsatCV_dVb = -VdsatCV * dAbulkCV_dVb / AbulkCV;
                T0 = Vgsteff - VdsatCV / 3.0;
                dT0_dVg = 1.0 - dVdsatCV_dVg / 3.0;
                dT0_dVb = -dVdsatCV_dVb / 3.0;
                qgate += CoxWL * T0;
                Cgg1 = CoxWL * dT0_dVg;
                Cgb1 = CoxWL * dT0_dVb + Cgg1 * dVgsteff_dVb;
                Cgd1 = Cgg1 * dVgsteff_dVd;
                Cgg1 *= dVgsteff_dVg;
                Cgg += Cgg1;
                Cgb += Cgb1;
                Cgd += Cgd1;
                T0 = VdsatCV - Vgsteff;
                dT0_dVg = dVdsatCV_dVg - 1.0;
                dT0_dVb = dVdsatCV_dVb;
                qbulk += One_Third_CoxWL * T0;
                Cbg1 = One_Third_CoxWL * dT0_dVg;
                Cbb1 = One_Third_CoxWL * dT0_dVb + Cbg1 * dVgsteff_dVb;
                Cbd1 = Cbg1 * dVgsteff_dVd;
                Cbg1 *= dVgsteff_dVg;
                Cbg += Cbg1;
                Cbb += Cbb1;
                Cbd += Cbd1;
                if model.xpart > 0.5 {
                    T0 = -Two_Third_CoxWL;
                } else if model.xpart < 0.5 {
                    T0 = -0.4 * CoxWL;
                } else {
                    T0 = -One_Third_CoxWL;
                }
                qsrc = T0 * Vgsteff;
                Csg = T0 * dVgsteff_dVg;
                Csb = T0 * dVgsteff_dVb;
                Csd = T0 * dVgsteff_dVd;
                Cgb *= dVbseff_dVb;
                Cbb *= dVbseff_dVb;
                Csb *= dVbseff_dVb;
            } else {
                T0 = AbulkCV * Vds;
                T1 = 12.0 * (Vgsteff - 0.5 * T0 + 1.0e-20);
                T2 = Vds / T1;
                T3 = T0 * T2;
                dT3_dVg = -12.0 * T2 * T2 * AbulkCV;
                dT3_dVd = 6.0 * T0 * (4.0 * Vgsteff - T0) / T1 / T1 - 0.5;
                dT3_dVb = 12.0 * T2 * T2 * dAbulkCV_dVb * Vgsteff;
                qgate += CoxWL * (Vgsteff - 0.5 * Vds + T3);
                Cgg1 = CoxWL * (1.0 + dT3_dVg);
                Cgb1 = CoxWL * dT3_dVb + Cgg1 * dVgsteff_dVb;
                Cgd1 = CoxWL * dT3_dVd + Cgg1 * dVgsteff_dVd;
                Cgg1 *= dVgsteff_dVg;
                Cgg += Cgg1;
                Cgb += Cgb1;
                Cgd += Cgd1;
                qbulk += CoxWL * (1.0 - AbulkCV) * (0.5 * Vds - T3);
                Cbg1 = -CoxWL * ((1.0 - AbulkCV) * dT3_dVg);
                Cbb1 = -CoxWL * ((1.0 - AbulkCV) * dT3_dVb + (0.5 * Vds - T3) * dAbulkCV_dVb)
                    + Cbg1 * dVgsteff_dVb;
                Cbd1 = -CoxWL * (1.0 - AbulkCV) * dT3_dVd + Cbg1 * dVgsteff_dVd;
                Cbg1 *= dVgsteff_dVg;
                Cbg += Cbg1;
                Cbb += Cbb1;
                Cbd += Cbd1;
                if model.xpart > 0.5 {
                    T1 = T1 + T1;
                    qsrc = -CoxWL * (0.5 * Vgsteff + 0.25 * T0 - T0 * T0 / T1);
                    Csg = -CoxWL * (0.5 + 24.0 * T0 * Vds / T1 / T1 * AbulkCV);
                    Csb = -CoxWL
                        * (0.25 * Vds * dAbulkCV_dVb
                            - 12.0 * T0 * Vds / T1 / T1 * (4.0 * Vgsteff - T0) * dAbulkCV_dVb)
                        + Csg * dVgsteff_dVb;
                    Csd = -CoxWL
                        * (0.25 * AbulkCV - 12.0 * AbulkCV * T0 / T1 / T1 * (4.0 * Vgsteff - T0))
                        + Csg * dVgsteff_dVd;
                    Csg *= dVgsteff_dVg;
                } else if model.xpart < 0.5 {
                    T1 = T1 / 12.0;
                    T2 = 0.5 * CoxWL / (T1 * T1);
                    T3 = Vgsteff * (2.0 * T0 * T0 / 3.0 + Vgsteff * (Vgsteff - 4.0 * T0 / 3.0))
                        - 2.0 * T0 * T0 * T0 / 15.0;
                    qsrc = -T2 * T3;
                    T4 = 4.0 / 3.0 * Vgsteff * (Vgsteff - T0) + 0.4 * T0 * T0;
                    Csg = -2.0 * qsrc / T1
                        - T2 * (Vgsteff * (3.0 * Vgsteff - 8.0 * T0 / 3.0) + 2.0 * T0 * T0 / 3.0);
                    Csb = (qsrc / T1 * Vds + T2 * T4 * Vds) * dAbulkCV_dVb + Csg * dVgsteff_dVb;
                    Csd = (qsrc / T1 + T2 * T4) * AbulkCV + Csg * dVgsteff_dVd;
                    Csg *= dVgsteff_dVg;
                } else {
                    qsrc = -0.5 * (qgate + qbulk);
                    Csg = -0.5 * (Cgg1 + Cbg1);
                    Csb = -0.5 * (Cgb1 + Cbb1);
                    Csd = -0.5 * (Cgd1 + Cbd1);
                }
                Cgb *= dVbseff_dVb;
                Cbb *= dVbseff_dVb;
                Csb *= dVbseff_dVb;
            }
            qdrn = -(qgate + qbulk + qsrc);
            here.cggb = Cgg;
            here.cgsb = -(Cgg + Cgd + Cgb);
            here.cgdb = Cgd;
            here.cdgb = -(Cgg + Cbg + Csg);
            here.cdsb = (Cgg + Cgd + Cgb + Cbg + Cbd + Cbb + Csg + Csd + Csb);
            here.cddb = -(Cgd + Cbd + Csd);
            here.cbgb = Cbg;
            here.cbsb = -(Cbg + Cbd + Cbb);
            here.cbdb = Cbd;
            here.qinv = -(qgate + qbulk);
        } else if model.capMod == 2 {
            Vfb = here.vfbzb;
            V3 = Vfb - Vgs_eff + VbseffCV - DELTA_3;
            if Vfb <= 0.0 {
                T0 = sqrt(V3 * V3 - 4.0 * DELTA_3 * Vfb);
                T2 = -DELTA_3 / T0;
            } else {
                T0 = sqrt(V3 * V3 + 4.0 * DELTA_3 * Vfb);
                T2 = DELTA_3 / T0;
            }
            T1 = 0.5 * (1.0 + V3 / T0);
            Vfbeff = Vfb - 0.5 * (V3 + T0);
            dVfbeff_dVg = T1 * dVgs_eff_dVg;
            dVfbeff_dVb = -T1 * dVbseffCV_dVb;
            Qac0 = CoxWL * (Vfbeff - Vfb);
            dQac0_dVg = CoxWL * dVfbeff_dVg;
            dQac0_dVb = CoxWL * dVfbeff_dVb;
            T0 = 0.5 * pParam.k1ox;
            T3 = Vgs_eff - Vfbeff - VbseffCV - Vgsteff;
            if pParam.k1ox == 0.0 {
                T1 = 0.0;
                T2 = 0.0;
            } else if T3 < 0.0 {
                T1 = T0 + T3 / pParam.k1ox;
                T2 = CoxWL;
            } else {
                T1 = sqrt(T0 * T0 + T3);
                T2 = CoxWL * T0 / T1;
            }
            Qsub0 = CoxWL * pParam.k1ox * (T1 - T0);
            dQsub0_dVg = T2 * (dVgs_eff_dVg - dVfbeff_dVg - dVgsteff_dVg);
            dQsub0_dVd = -T2 * dVgsteff_dVd;
            dQsub0_dVb = -T2 * (dVfbeff_dVb + dVbseffCV_dVb + dVgsteff_dVb);
            AbulkCV = Abulk0 * pParam.abulkCVfactor;
            dAbulkCV_dVb = pParam.abulkCVfactor * dAbulk0_dVb;
            VdsatCV = Vgsteff / AbulkCV;
            V4 = VdsatCV - Vds - DELTA_4;
            T0 = sqrt(V4 * V4 + 4.0 * DELTA_4 * VdsatCV);
            VdseffCV = VdsatCV - 0.5 * (V4 + T0);
            T1 = 0.5 * (1.0 + V4 / T0);
            T2 = DELTA_4 / T0;
            T3 = (1.0 - T1 - T2) / AbulkCV;
            dVdseffCV_dVg = T3;
            dVdseffCV_dVd = T1;
            dVdseffCV_dVb = -T3 * VdsatCV * dAbulkCV_dVb;
            if Vds == 0.0 {
                VdseffCV = 0.0;
                dVdseffCV_dVg = 0.0;
                dVdseffCV_dVb = 0.0;
            }
            T0 = AbulkCV * VdseffCV;
            T1 = 12.0 * (Vgsteff - 0.5 * T0 + 1e-20);
            T2 = VdseffCV / T1;
            T3 = T0 * T2;
            T4 = (1.0 - 12.0 * T2 * T2 * AbulkCV);
            T5 = (6.0 * T0 * (4.0 * Vgsteff - T0) / (T1 * T1) - 0.5);
            T6 = 12.0 * T2 * T2 * Vgsteff;
            qinoi = -CoxWL * (Vgsteff - 0.5 * T0 + AbulkCV * T3);
            qgate = CoxWL * (Vgsteff - 0.5 * VdseffCV + T3);
            Cgg1 = CoxWL * (T4 + T5 * dVdseffCV_dVg);
            Cgd1 = CoxWL * T5 * dVdseffCV_dVd + Cgg1 * dVgsteff_dVd;
            Cgb1 = CoxWL * (T5 * dVdseffCV_dVb + T6 * dAbulkCV_dVb) + Cgg1 * dVgsteff_dVb;
            Cgg1 *= dVgsteff_dVg;
            T7 = 1.0 - AbulkCV;
            qbulk = CoxWL * T7 * (0.5 * VdseffCV - T3);
            T4 = -T7 * (T4 - 1.0);
            T5 = -T7 * T5;
            T6 = -(T7 * T6 + (0.5 * VdseffCV - T3));
            Cbg1 = CoxWL * (T4 + T5 * dVdseffCV_dVg);
            Cbd1 = CoxWL * T5 * dVdseffCV_dVd + Cbg1 * dVgsteff_dVd;
            Cbb1 = CoxWL * (T5 * dVdseffCV_dVb + T6 * dAbulkCV_dVb) + Cbg1 * dVgsteff_dVb;
            Cbg1 *= dVgsteff_dVg;
            if model.xpart > 0.5 {
                T1 = T1 + T1;
                qsrc = -CoxWL * (0.5 * Vgsteff + 0.25 * T0 - T0 * T0 / T1);
                T7 = (4.0 * Vgsteff - T0) / (T1 * T1);
                T4 = -(0.5 + 24.0 * T0 * T0 / (T1 * T1));
                T5 = -(0.25 * AbulkCV - 12.0 * AbulkCV * T0 * T7);
                T6 = -(0.25 * VdseffCV - 12.0 * T0 * VdseffCV * T7);
                Csg = CoxWL * (T4 + T5 * dVdseffCV_dVg);
                Csd = CoxWL * T5 * dVdseffCV_dVd + Csg * dVgsteff_dVd;
                Csb = CoxWL * (T5 * dVdseffCV_dVb + T6 * dAbulkCV_dVb) + Csg * dVgsteff_dVb;
                Csg *= dVgsteff_dVg;
            } else if model.xpart < 0.5 {
                T1 = T1 / 12.0;
                T2 = 0.5 * CoxWL / (T1 * T1);
                T3 = Vgsteff * (2.0 * T0 * T0 / 3.0 + Vgsteff * (Vgsteff - 4.0 * T0 / 3.0))
                    - 2.0 * T0 * T0 * T0 / 15.0;
                qsrc = -T2 * T3;
                T7 = 4.0 / 3.0 * Vgsteff * (Vgsteff - T0) + 0.4 * T0 * T0;
                T4 = -2.0 * qsrc / T1
                    - T2 * (Vgsteff * (3.0 * Vgsteff - 8.0 * T0 / 3.0) + 2.0 * T0 * T0 / 3.0);
                T5 = (qsrc / T1 + T2 * T7) * AbulkCV;
                T6 = (qsrc / T1 * VdseffCV + T2 * T7 * VdseffCV);
                Csg = (T4 + T5 * dVdseffCV_dVg);
                Csd = T5 * dVdseffCV_dVd + Csg * dVgsteff_dVd;
                Csb = (T5 * dVdseffCV_dVb + T6 * dAbulkCV_dVb) + Csg * dVgsteff_dVb;
                Csg *= dVgsteff_dVg;
            } else {
                qsrc = -0.5 * (qgate + qbulk);
                Csg = -0.5 * (Cgg1 + Cbg1);
                Csb = -0.5 * (Cgb1 + Cbb1);
                Csd = -0.5 * (Cgd1 + Cbd1);
            }
            qgate += Qac0 + Qsub0;
            qbulk -= (Qac0 + Qsub0);
            qdrn = -(qgate + qbulk + qsrc);
            Cgg = dQac0_dVg + dQsub0_dVg + Cgg1;
            Cgd = dQsub0_dVd + Cgd1;
            Cgb = dQac0_dVb + dQsub0_dVb + Cgb1;
            Cbg = Cbg1 - dQac0_dVg - dQsub0_dVg;
            Cbd = Cbd1 - dQsub0_dVd;
            Cbb = Cbb1 - dQac0_dVb - dQsub0_dVb;
            Cgb *= dVbseff_dVb;
            Cbb *= dVbseff_dVb;
            Csb *= dVbseff_dVb;
            here.cggb = Cgg;
            here.cgsb = -(Cgg + Cgd + Cgb);
            here.cgdb = Cgd;
            here.cdgb = -(Cgg + Cbg + Csg);
            here.cdsb = (Cgg + Cgd + Cgb + Cbg + Cbd + Cbb + Csg + Csd + Csb);
            here.cddb = -(Cgd + Cbd + Csd);
            here.cbgb = Cbg;
            here.cbsb = -(Cbg + Cbd + Cbb);
            here.cbdb = Cbd;
            here.qinv = qinoi;
        } else if model.capMod == 3 {
            V3 = here.vfbzb - Vgs_eff + VbseffCV - DELTA_3;
            if here.vfbzb <= 0.0 {
                T0 = sqrt(V3 * V3 - 4.0 * DELTA_3 * here.vfbzb);
                T2 = -DELTA_3 / T0;
            } else {
                T0 = sqrt(V3 * V3 + 4.0 * DELTA_3 * here.vfbzb);
                T2 = DELTA_3 / T0;
            }
            T1 = 0.5 * (1.0 + V3 / T0);
            Vfbeff = here.vfbzb - 0.5 * (V3 + T0);
            dVfbeff_dVg = T1 * dVgs_eff_dVg;
            dVfbeff_dVb = -T1 * dVbseffCV_dVb;
            Cox = model.cox;
            Tox = 1.0e8 * model.tox;
            T0 = (Vgs_eff - VbseffCV - here.vfbzb) / Tox;
            dT0_dVg = dVgs_eff_dVg / Tox;
            dT0_dVb = -dVbseffCV_dVb / Tox;
            tmp = T0 * pParam.acde;
            if (-EXP_THRESHOLD < tmp) && (tmp < EXP_THRESHOLD) {
                Tcen = pParam.ldeb * exp(tmp);
                dTcen_dVg = pParam.acde * Tcen;
                dTcen_dVb = dTcen_dVg * dT0_dVb;
                dTcen_dVg *= dT0_dVg;
            } else if tmp <= -EXP_THRESHOLD {
                Tcen = pParam.ldeb * MIN_EXP;
                dTcen_dVb = 0.0;
                dTcen_dVg = dTcen_dVb;
            } else {
                Tcen = pParam.ldeb * MAX_EXP;
                dTcen_dVb = 0.0;
                dTcen_dVg = dTcen_dVb;
            }
            LINK = 1.0e-3 * model.tox;
            V3 = pParam.ldeb - Tcen - LINK;
            V4 = sqrt(V3 * V3 + 4.0 * LINK * pParam.ldeb);
            Tcen = pParam.ldeb - 0.5 * (V3 + V4);
            T1 = 0.5 * (1.0 + V3 / V4);
            dTcen_dVg *= T1;
            dTcen_dVb *= T1;
            Ccen = EPSSI / Tcen;
            T2 = Cox / (Cox + Ccen);
            Coxeff = T2 * Ccen;
            T3 = -Ccen / Tcen;
            dCoxeff_dVg = T2 * T2 * T3;
            dCoxeff_dVb = dCoxeff_dVg * dTcen_dVb;
            dCoxeff_dVg *= dTcen_dVg;
            CoxWLcen = CoxWL * Coxeff / Cox;
            Qac0 = CoxWLcen * (Vfbeff - here.vfbzb);
            QovCox = Qac0 / Coxeff;
            dQac0_dVg = CoxWLcen * dVfbeff_dVg + QovCox * dCoxeff_dVg;
            dQac0_dVb = CoxWLcen * dVfbeff_dVb + QovCox * dCoxeff_dVb;
            T0 = 0.5 * pParam.k1ox;
            T3 = Vgs_eff - Vfbeff - VbseffCV - Vgsteff;
            if pParam.k1ox == 0.0 {
                T1 = 0.0;
                T2 = 0.0;
            } else if T3 < 0.0 {
                T1 = T0 + T3 / pParam.k1ox;
                T2 = CoxWLcen;
            } else {
                T1 = sqrt(T0 * T0 + T3);
                T2 = CoxWLcen * T0 / T1;
            }
            Qsub0 = CoxWLcen * pParam.k1ox * (T1 - T0);
            QovCox = Qsub0 / Coxeff;
            dQsub0_dVg = T2 * (dVgs_eff_dVg - dVfbeff_dVg - dVgsteff_dVg) + QovCox * dCoxeff_dVg;
            dQsub0_dVd = -T2 * dVgsteff_dVd;
            dQsub0_dVb = -T2 * (dVfbeff_dVb + dVbseffCV_dVb + dVgsteff_dVb) + QovCox * dCoxeff_dVb;
            if pParam.k1ox <= 0.0 {
                Denomi = 0.25 * pParam.moin * Vtm;
                T0 = 0.5 * pParam.sqrtPhi;
            } else {
                Denomi = pParam.moin * Vtm * pParam.k1ox * pParam.k1ox;
                T0 = pParam.k1ox * pParam.sqrtPhi;
            }
            T1 = 2.0 * T0 + Vgsteff;
            DeltaPhi = Vtm * log(1.0 + T1 * Vgsteff / Denomi);
            dDeltaPhi_dVg = 2.0 * Vtm * (T1 - T0) / (Denomi + T1 * Vgsteff);
            T0 = Vgsteff - DeltaPhi - 0.001;
            dT0_dVg = 1.0 - dDeltaPhi_dVg;
            T1 = sqrt(T0 * T0 + Vgsteff * 0.004);
            VgDP = 0.5 * (T0 + T1);
            dVgDP_dVg = 0.5 * (dT0_dVg + (T0 * dT0_dVg + 0.002) / T1);
            T3 = 4.0 * (Vth - here.vfbzb - pParam.phi);
            Tox += Tox;
            if T3 >= 0.0 {
                T0 = (Vgsteff + T3) / Tox;
                dT0_dVd = (dVgsteff_dVd + 4.0 * dVth_dVd) / Tox;
                dT0_dVb = (dVgsteff_dVb + 4.0 * dVth_dVb) / Tox;
            } else {
                T0 = (Vgsteff + 1.0e-20) / Tox;
                dT0_dVd = dVgsteff_dVd / Tox;
                dT0_dVb = dVgsteff_dVb / Tox;
            }
            tmp = exp(0.7 * log(T0));
            T1 = 1.0 + tmp;
            T2 = 0.7 * tmp / (T0 * Tox);
            Tcen = 1.9e-9 / T1;
            dTcen_dVg = -1.9e-9 * T2 / T1 / T1;
            dTcen_dVd = Tox * dTcen_dVg;
            dTcen_dVb = dTcen_dVd * dT0_dVb;
            dTcen_dVd *= dT0_dVd;
            dTcen_dVg *= dVgsteff_dVg;
            Ccen = EPSSI / Tcen;
            T0 = Cox / (Cox + Ccen);
            Coxeff = T0 * Ccen;
            T1 = -Ccen / Tcen;
            dCoxeff_dVg = T0 * T0 * T1;
            dCoxeff_dVd = dCoxeff_dVg * dTcen_dVd;
            dCoxeff_dVb = dCoxeff_dVg * dTcen_dVb;
            dCoxeff_dVg *= dTcen_dVg;
            CoxWLcen = CoxWL * Coxeff / Cox;
            AbulkCV = Abulk0 * pParam.abulkCVfactor;
            dAbulkCV_dVb = pParam.abulkCVfactor * dAbulk0_dVb;
            VdsatCV = VgDP / AbulkCV;
            T0 = VdsatCV - Vds - DELTA_4;
            dT0_dVg = dVgDP_dVg / AbulkCV;
            dT0_dVb = -VdsatCV * dAbulkCV_dVb / AbulkCV;
            T1 = sqrt(T0 * T0 + 4.0 * DELTA_4 * VdsatCV);
            dT1_dVg = (T0 + DELTA_4 + DELTA_4) / T1;
            dT1_dVd = -T0 / T1;
            dT1_dVb = dT1_dVg * dT0_dVb;
            dT1_dVg *= dT0_dVg;
            if T0 >= 0.0 {
                VdseffCV = VdsatCV - 0.5 * (T0 + T1);
                dVdseffCV_dVg = 0.5 * (dT0_dVg - dT1_dVg);
                dVdseffCV_dVd = 0.5 * (1.0 - dT1_dVd);
                dVdseffCV_dVb = 0.5 * (dT0_dVb - dT1_dVb);
            } else {
                T3 = (DELTA_4 + DELTA_4) / (T1 - T0);
                T4 = 1.0 - T3;
                T5 = VdsatCV * T3 / (T1 - T0);
                VdseffCV = VdsatCV * T4;
                dVdseffCV_dVg = dT0_dVg * T4 + T5 * (dT1_dVg - dT0_dVg);
                dVdseffCV_dVd = T5 * (dT1_dVd + 1.0);
                dVdseffCV_dVb = dT0_dVb * (1.0 - T5) + T5 * dT1_dVb;
            }
            if Vds == 0.0 {
                VdseffCV = 0.0;
                dVdseffCV_dVg = 0.0;
                dVdseffCV_dVb = 0.0;
            }
            T0 = AbulkCV * VdseffCV;
            T1 = VgDP;
            T2 = 12.0 * (T1 - 0.5 * T0 + 1.0e-20);
            T3 = T0 / T2;
            T4 = 1.0 - 12.0 * T3 * T3;
            T5 = AbulkCV * (6.0 * T0 * (4.0 * T1 - T0) / (T2 * T2) - 0.5);
            T6 = T5 * VdseffCV / AbulkCV;
            qinoi = CoxWLcen * (T1 - T0 * (0.5 - T3));
            qgate = qinoi;
            QovCox = qgate / Coxeff;
            Cgg1 = CoxWLcen * (T4 * dVgDP_dVg + T5 * dVdseffCV_dVg);
            Cgd1 = CoxWLcen * T5 * dVdseffCV_dVd + Cgg1 * dVgsteff_dVd + QovCox * dCoxeff_dVd;
            Cgb1 = CoxWLcen * (T5 * dVdseffCV_dVb + T6 * dAbulkCV_dVb)
                + Cgg1 * dVgsteff_dVb
                + QovCox * dCoxeff_dVb;
            Cgg1 = Cgg1 * dVgsteff_dVg + QovCox * dCoxeff_dVg;
            T7 = 1.0 - AbulkCV;
            T8 = T2 * T2;
            T9 = 12.0 * T7 * T0 * T0 / (T8 * AbulkCV);
            T10 = T9 * dVgDP_dVg;
            T11 = -T7 * T5 / AbulkCV;
            T12 = -(T9 * T1 / AbulkCV + VdseffCV * (0.5 - T0 / T2));
            qbulk = CoxWLcen * T7 * (0.5 * VdseffCV - T0 * VdseffCV / T2);
            QovCox = qbulk / Coxeff;
            Cbg1 = CoxWLcen * (T10 + T11 * dVdseffCV_dVg);
            Cbd1 = CoxWLcen * T11 * dVdseffCV_dVd + Cbg1 * dVgsteff_dVd + QovCox * dCoxeff_dVd;
            Cbb1 = CoxWLcen * (T11 * dVdseffCV_dVb + T12 * dAbulkCV_dVb)
                + Cbg1 * dVgsteff_dVb
                + QovCox * dCoxeff_dVb;
            Cbg1 = Cbg1 * dVgsteff_dVg + QovCox * dCoxeff_dVg;
            if model.xpart > 0.5 {
                qsrc = -CoxWLcen * (T1 / 2.0 + T0 / 4.0 - 0.5 * T0 * T0 / T2);
                QovCox = qsrc / Coxeff;
                T2 += T2;
                T3 = T2 * T2;
                T7 = -(0.25 - 12.0 * T0 * (4.0 * T1 - T0) / T3);
                T4 = -(0.5 + 24.0 * T0 * T0 / T3) * dVgDP_dVg;
                T5 = T7 * AbulkCV;
                T6 = T7 * VdseffCV;
                Csg = CoxWLcen * (T4 + T5 * dVdseffCV_dVg);
                Csd = CoxWLcen * T5 * dVdseffCV_dVd + Csg * dVgsteff_dVd + QovCox * dCoxeff_dVd;
                Csb = CoxWLcen * (T5 * dVdseffCV_dVb + T6 * dAbulkCV_dVb)
                    + Csg * dVgsteff_dVb
                    + QovCox * dCoxeff_dVb;
                Csg = Csg * dVgsteff_dVg + QovCox * dCoxeff_dVg;
            } else if model.xpart < 0.5 {
                T2 = T2 / 12.0;
                T3 = 0.5 * CoxWLcen / (T2 * T2);
                T4 = T1 * (2.0 * T0 * T0 / 3.0 + T1 * (T1 - 4.0 * T0 / 3.0))
                    - 2.0 * T0 * T0 * T0 / 15.0;
                qsrc = -T3 * T4;
                QovCox = qsrc / Coxeff;
                T8 = 4.0 / 3.0 * T1 * (T1 - T0) + 0.4 * T0 * T0;
                T5 = -2.0 * qsrc / T2
                    - T3 * (T1 * (3.0 * T1 - 8.0 * T0 / 3.0) + 2.0 * T0 * T0 / 3.0);
                T6 = AbulkCV * (qsrc / T2 + T3 * T8);
                T7 = T6 * VdseffCV / AbulkCV;
                Csg = T5 * dVgDP_dVg + T6 * dVdseffCV_dVg;
                Csd = Csg * dVgsteff_dVd + T6 * dVdseffCV_dVd + QovCox * dCoxeff_dVd;
                Csb = Csg * dVgsteff_dVb
                    + T6 * dVdseffCV_dVb
                    + T7 * dAbulkCV_dVb
                    + QovCox * dCoxeff_dVb;
                Csg = Csg * dVgsteff_dVg + QovCox * dCoxeff_dVg;
            } else {
                qsrc = -0.5 * qgate;
                Csg = -0.5 * Cgg1;
                Csd = -0.5 * Cgd1;
                Csb = -0.5 * Cgb1;
            }
            qgate += Qac0 + Qsub0 - qbulk;
            qbulk -= (Qac0 + Qsub0);
            qdrn = -(qgate + qbulk + qsrc);
            Cbg = Cbg1 - dQac0_dVg - dQsub0_dVg;
            Cbd = Cbd1 - dQsub0_dVd;
            Cbb = Cbb1 - dQac0_dVb - dQsub0_dVb;
            Cgg = Cgg1 - Cbg;
            Cgd = Cgd1 - Cbd;
            Cgb = Cgb1 - Cbb;
            Cgb *= dVbseff_dVb;
            Cbb *= dVbseff_dVb;
            Csb *= dVbseff_dVb;
            here.cggb = Cgg;
            here.cgsb = -(Cgg + Cgd + Cgb);
            here.cgdb = Cgd;
            here.cdgb = -(Cgg + Cbg + Csg);
            here.cdsb = (Cgg + Cgd + Cgb + Cbg + Cbd + Cbb + Csg + Csd + Csb);
            here.cddb = -(Cgd + Cbd + Csd);
            here.cbgb = Cbg;
            here.cbsb = -(Cbg + Cbd + Cbb);
            here.cbdb = Cbd;
            here.qinv = -qinoi;
        }
    }
    // C label `finished:`.
    here.qgate = qgate;
    here.qbulk = qbulk;
    here.qdrn = qdrn;
    here.cd = cdrain;
    if ChargeComputationNeeded != 0 {
        if model.acmMod == 0 {
            czbd = model.unitAreaTempJctCap * here.drainArea;
            czbs = model.unitAreaTempJctCap * here.sourceArea;
            if here.drainPerimeter < pParam.weff {
                czbdswg = model.unitLengthGateSidewallTempJctCap * here.drainPerimeter;
                czbdsw = 0.0;
            } else {
                czbdsw = model.unitLengthSidewallTempJctCap * (here.drainPerimeter - pParam.weff);
                czbdswg = model.unitLengthGateSidewallTempJctCap * pParam.weff;
            }
            if here.sourcePerimeter < pParam.weff {
                czbssw = 0.0;
                czbsswg = model.unitLengthGateSidewallTempJctCap * here.sourcePerimeter;
            } else {
                czbssw = model.unitLengthSidewallTempJctCap * (here.sourcePerimeter - pParam.weff);
                czbsswg = model.unitLengthGateSidewallTempJctCap * pParam.weff;
            }
        } else {
            return Err(SpiceError::not_yet_ported(
                format!("BSIM3 acmMod={} junction capacitances", model.acmMod),
                "src/spicelib/devices/bsim3/b3ld.c (ACM_junctionCapacitances)",
            ));
        }
        MJ = model.bulkJctBotGradingCoeff;
        MJSW = model.bulkJctSideGradingCoeff;
        MJSWG = model.bulkJctGateSideGradingCoeff;
        if vbs == 0.0 {
            ckt.set_state0(state::QBS, 0.0);
            here.capbs = czbs + czbssw + czbsswg;
        } else if vbs < 0.0 {
            if czbs > 0.0 {
                arg = 1.0 - vbs / model.PhiB;
                if MJ == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-MJ * log(arg));
                }
                ckt.set_state0(
                    state::QBS,
                    model.PhiB * czbs * (1.0 - arg * sarg) / (1.0 - MJ),
                );
                here.capbs = czbs * sarg;
            } else {
                ckt.set_state0(state::QBS, 0.0);
                here.capbs = 0.0;
            }
            if czbssw > 0.0 {
                arg = 1.0 - vbs / model.PhiBSW;
                if MJSW == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-MJSW * log(arg));
                }
                ckt.set_state0(
                    state::QBS,
                    ckt.state0(state::QBS)
                        + model.PhiBSW * czbssw * (1.0 - arg * sarg) / (1.0 - MJSW),
                );
                here.capbs += czbssw * sarg;
            }
            if czbsswg > 0.0 {
                arg = 1.0 - vbs / model.PhiBSWG;
                if MJSWG == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-MJSWG * log(arg));
                }
                ckt.set_state0(
                    state::QBS,
                    ckt.state0(state::QBS)
                        + model.PhiBSWG * czbsswg * (1.0 - arg * sarg) / (1.0 - MJSWG),
                );
                here.capbs += czbsswg * sarg;
            }
        } else {
            T0 = czbs + czbssw + czbsswg;
            T1 = vbs
                * (czbs * MJ / model.PhiB
                    + czbssw * MJSW / model.PhiBSW
                    + czbsswg * MJSWG / model.PhiBSWG);
            ckt.set_state0(state::QBS, vbs * (T0 + 0.5 * T1));
            here.capbs = T0 + T1;
        }
        if vbd == 0.0 {
            ckt.set_state0(state::QBD, 0.0);
            here.capbd = czbd + czbdsw + czbdswg;
        } else if vbd < 0.0 {
            if czbd > 0.0 {
                arg = 1.0 - vbd / model.PhiB;
                if MJ == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-MJ * log(arg));
                }
                ckt.set_state0(
                    state::QBD,
                    model.PhiB * czbd * (1.0 - arg * sarg) / (1.0 - MJ),
                );
                here.capbd = czbd * sarg;
            } else {
                ckt.set_state0(state::QBD, 0.0);
                here.capbd = 0.0;
            }
            if czbdsw > 0.0 {
                arg = 1.0 - vbd / model.PhiBSW;
                if MJSW == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-MJSW * log(arg));
                }
                ckt.set_state0(
                    state::QBD,
                    ckt.state0(state::QBD)
                        + model.PhiBSW * czbdsw * (1.0 - arg * sarg) / (1.0 - MJSW),
                );
                here.capbd += czbdsw * sarg;
            }
            if czbdswg > 0.0 {
                arg = 1.0 - vbd / model.PhiBSWG;
                if MJSWG == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-MJSWG * log(arg));
                }
                ckt.set_state0(
                    state::QBD,
                    ckt.state0(state::QBD)
                        + model.PhiBSWG * czbdswg * (1.0 - arg * sarg) / (1.0 - MJSWG),
                );
                here.capbd += czbdswg * sarg;
            }
        } else {
            T0 = czbd + czbdsw + czbdswg;
            T1 = vbd
                * (czbd * MJ / model.PhiB
                    + czbdsw * MJSW / model.PhiBSW
                    + czbdswg * MJSWG / model.PhiBSWG);
            ckt.set_state0(state::QBD, vbd * (T0 + 0.5 * T1));
            here.capbd = T0 + T1;
        }
    }
    if (here.off == 0) || (!((ckt.mode() & mode::MODEINITFIX) != 0)) {
        if Check == 1 {
            ckt.noncon();
        }
    }
    ckt.set_state0(state::VBS, vbs);
    ckt.set_state0(state::VBD, vbd);
    ckt.set_state0(state::VGS, vgs);
    ckt.set_state0(state::VDS, vds);
    ckt.set_state0(state::QDEF, qdef);
    // C's `goto line850`/`goto line1000` out of the charge block are
    // breaks with flags; leaving the block normally is C's `goto line860`.
    let mut goto_line850 = false;
    let mut goto_line1000 = false;
    'charges: {
        if !(ChargeComputationNeeded != 0) {
            goto_line850 = true;
            break 'charges;
        }
        if ((here.nqsMod) != 0) || ((here.acnqsMod) != 0) {
            qcheq = -(qbulk + qgate);
            here.cqgb = -(here.cggb + here.cbgb);
            here.cqdb = -(here.cgdb + here.cbdb);
            here.cqsb = -(here.cgsb + here.cbsb);
            here.cqbb = -(here.cqgb + here.cqdb + here.cqsb);
            gtau_drift = fabs(here.tconst * qcheq) * ScalingFactor;
            T0 = pParam.leffCV * pParam.leffCV;
            gtau_diff = 16.0 * here.u0temp * model.vtm / T0 * ScalingFactor;
            here.gtau = gtau_drift + gtau_diff;
            if here.acnqsMod != 0 {
                here.taunet = ScalingFactor / here.gtau;
            }
        }
        if model.capMod == 0 {
            cgdo = pParam.cgdo;
            qgdo = pParam.cgdo * vgd;
            cgso = pParam.cgso;
            qgso = pParam.cgso * vgs;
        } else if model.capMod == 1 {
            if vgd < 0.0 {
                T1 = sqrt(1.0 - 4.0 * vgd / pParam.ckappa);
                cgdo = pParam.cgdo + pParam.weffCV * pParam.cgdl / T1;
                qgdo = pParam.cgdo * vgd
                    - pParam.weffCV * 0.5 * pParam.cgdl * pParam.ckappa * (T1 - 1.0);
            } else {
                cgdo = pParam.cgdo + pParam.weffCV * pParam.cgdl;
                qgdo = (pParam.weffCV * pParam.cgdl + pParam.cgdo) * vgd;
            }
            if vgs < 0.0 {
                T1 = sqrt(1.0 - 4.0 * vgs / pParam.ckappa);
                cgso = pParam.cgso + pParam.weffCV * pParam.cgsl / T1;
                qgso = pParam.cgso * vgs
                    - pParam.weffCV * 0.5 * pParam.cgsl * pParam.ckappa * (T1 - 1.0);
            } else {
                cgso = pParam.cgso + pParam.weffCV * pParam.cgsl;
                qgso = (pParam.weffCV * pParam.cgsl + pParam.cgso) * vgs;
            }
        } else {
            T0 = vgd + DELTA_1;
            T1 = sqrt(T0 * T0 + 4.0 * DELTA_1);
            T2 = 0.5 * (T0 - T1);
            T3 = pParam.weffCV * pParam.cgdl;
            T4 = sqrt(1.0 - 4.0 * T2 / pParam.ckappa);
            cgdo = pParam.cgdo + T3 - T3 * (1.0 - 1.0 / T4) * (0.5 - 0.5 * T0 / T1);
            qgdo = (pParam.cgdo + T3) * vgd - T3 * (T2 + 0.5 * pParam.ckappa * (T4 - 1.0));
            T0 = vgs + DELTA_1;
            T1 = sqrt(T0 * T0 + 4.0 * DELTA_1);
            T2 = 0.5 * (T0 - T1);
            T3 = pParam.weffCV * pParam.cgsl;
            T4 = sqrt(1.0 - 4.0 * T2 / pParam.ckappa);
            cgso = pParam.cgso + T3 - T3 * (1.0 - 1.0 / T4) * (0.5 - 0.5 * T0 / T1);
            qgso = (pParam.cgso + T3) * vgs - T3 * (T2 + 0.5 * pParam.ckappa * (T4 - 1.0));
        }
        here.cgdo = cgdo;
        here.cgso = cgso;
        ag0 = ckt.ag(0);
        if here.mode > 0 {
            if here.nqsMod == 0 {
                gcggb = (here.cggb + cgdo + cgso + pParam.cgbo) * ag0;
                gcgdb = (here.cgdb - cgdo) * ag0;
                gcgsb = (here.cgsb - cgso) * ag0;
                gcdgb = (here.cdgb - cgdo) * ag0;
                gcddb = (here.cddb + here.capbd + cgdo) * ag0;
                gcdsb = here.cdsb * ag0;
                gcsgb = -(here.cggb + here.cbgb + here.cdgb + cgso) * ag0;
                gcsdb = -(here.cgdb + here.cbdb + here.cddb) * ag0;
                gcssb = (here.capbs + cgso - (here.cgsb + here.cbsb + here.cdsb)) * ag0;
                gcbgb = (here.cbgb - pParam.cgbo) * ag0;
                gcbdb = (here.cbdb - here.capbd) * ag0;
                gcbsb = (here.cbsb - here.capbs) * ag0;
                qgd = qgdo;
                qgs = qgso;
                qgb = pParam.cgbo * vgb;
                qgate += qgd + qgs + qgb;
                qbulk -= qgb;
                qdrn -= qgd;
                qsrc = -(qgate + qbulk + qdrn);
                ggts = 0.0;
                ggtb = ggts;
                ggtd = ggtb;
                ggtg = ggtd;
                sxpart = 0.6;
                dxpart = 0.4;
                ddxpart_dVs = 0.0;
                ddxpart_dVb = ddxpart_dVs;
                ddxpart_dVg = ddxpart_dVb;
                ddxpart_dVd = ddxpart_dVg;
                dsxpart_dVs = 0.0;
                dsxpart_dVb = dsxpart_dVs;
                dsxpart_dVg = dsxpart_dVb;
                dsxpart_dVd = dsxpart_dVg;
            } else {
                if qcheq > 0.0 {
                    T0 = here.tconst * qdef * ScalingFactor;
                } else {
                    T0 = -here.tconst * qdef * ScalingFactor;
                }
                here.gtg = T0 * here.cqgb;
                ggtg = here.gtg;
                here.gtd = T0 * here.cqdb;
                ggtd = here.gtd;
                here.gts = T0 * here.cqsb;
                ggts = here.gts;
                here.gtb = T0 * here.cqbb;
                ggtb = here.gtb;
                gqdef = ScalingFactor * ag0;
                gcqgb = here.cqgb * ag0;
                gcqdb = here.cqdb * ag0;
                gcqsb = here.cqsb * ag0;
                gcqbb = here.cqbb * ag0;
                gcggb = (cgdo + cgso + pParam.cgbo) * ag0;
                gcgdb = -cgdo * ag0;
                gcgsb = -cgso * ag0;
                gcdgb = -cgdo * ag0;
                gcddb = (here.capbd + cgdo) * ag0;
                gcdsb = 0.0;
                gcsgb = -cgso * ag0;
                gcsdb = 0.0;
                gcssb = (here.capbs + cgso) * ag0;
                gcbgb = -pParam.cgbo * ag0;
                gcbdb = -here.capbd * ag0;
                gcbsb = -here.capbs * ag0;
                CoxWL = model.cox * pParam.weffCV * pParam.leffCV;
                if fabs(qcheq) <= 1.0e-5 * CoxWL {
                    if model.xpart < 0.5 {
                        dxpart = 0.4;
                    } else if model.xpart > 0.5 {
                        dxpart = 0.0;
                    } else {
                        dxpart = 0.5;
                    }
                    ddxpart_dVs = 0.0;
                    ddxpart_dVb = ddxpart_dVs;
                    ddxpart_dVg = ddxpart_dVb;
                    ddxpart_dVd = ddxpart_dVg;
                } else {
                    dxpart = qdrn / qcheq;
                    Cdd = here.cddb;
                    Csd = -(here.cgdb + here.cddb + here.cbdb);
                    ddxpart_dVd = (Cdd - dxpart * (Cdd + Csd)) / qcheq;
                    Cdg = here.cdgb;
                    Csg = -(here.cggb + here.cdgb + here.cbgb);
                    ddxpart_dVg = (Cdg - dxpart * (Cdg + Csg)) / qcheq;
                    Cds = here.cdsb;
                    Css = -(here.cgsb + here.cdsb + here.cbsb);
                    ddxpart_dVs = (Cds - dxpart * (Cds + Css)) / qcheq;
                    ddxpart_dVb = -(ddxpart_dVd + ddxpart_dVg + ddxpart_dVs);
                }
                sxpart = 1.0 - dxpart;
                dsxpart_dVd = -ddxpart_dVd;
                dsxpart_dVg = -ddxpart_dVg;
                dsxpart_dVs = -ddxpart_dVs;
                dsxpart_dVb = -(dsxpart_dVd + dsxpart_dVg + dsxpart_dVs);
                qgd = qgdo;
                qgs = qgso;
                qgb = pParam.cgbo * vgb;
                qgate = qgd + qgs + qgb;
                qbulk = -qgb;
                qdrn = -qgd;
                qsrc = -(qgate + qbulk + qdrn);
            }
        } else {
            if here.nqsMod == 0 {
                gcggb = (here.cggb + cgdo + cgso + pParam.cgbo) * ag0;
                gcgdb = (here.cgsb - cgdo) * ag0;
                gcgsb = (here.cgdb - cgso) * ag0;
                gcdgb = -(here.cggb + here.cbgb + here.cdgb + cgdo) * ag0;
                gcddb = (here.capbd + cgdo - (here.cgsb + here.cbsb + here.cdsb)) * ag0;
                gcdsb = -(here.cgdb + here.cbdb + here.cddb) * ag0;
                gcsgb = (here.cdgb - cgso) * ag0;
                gcsdb = here.cdsb * ag0;
                gcssb = (here.cddb + here.capbs + cgso) * ag0;
                gcbgb = (here.cbgb - pParam.cgbo) * ag0;
                gcbdb = (here.cbsb - here.capbd) * ag0;
                gcbsb = (here.cbdb - here.capbs) * ag0;
                qgd = qgdo;
                qgs = qgso;
                qgb = pParam.cgbo * vgb;
                qgate += qgd + qgs + qgb;
                qbulk -= qgb;
                qsrc = qdrn - qgs;
                qdrn = -(qgate + qbulk + qsrc);
                ggts = 0.0;
                ggtb = ggts;
                ggtd = ggtb;
                ggtg = ggtd;
                sxpart = 0.4;
                dxpart = 0.6;
                ddxpart_dVs = 0.0;
                ddxpart_dVb = ddxpart_dVs;
                ddxpart_dVg = ddxpart_dVb;
                ddxpart_dVd = ddxpart_dVg;
                dsxpart_dVs = 0.0;
                dsxpart_dVb = dsxpart_dVs;
                dsxpart_dVg = dsxpart_dVb;
                dsxpart_dVd = dsxpart_dVg;
            } else {
                if qcheq > 0.0 {
                    T0 = here.tconst * qdef * ScalingFactor;
                } else {
                    T0 = -here.tconst * qdef * ScalingFactor;
                }
                here.gtg = T0 * here.cqgb;
                ggtg = here.gtg;
                here.gtd = T0 * here.cqdb;
                ggts = here.gtd;
                here.gts = T0 * here.cqsb;
                ggtd = here.gts;
                here.gtb = T0 * here.cqbb;
                ggtb = here.gtb;
                gqdef = ScalingFactor * ag0;
                gcqgb = here.cqgb * ag0;
                gcqdb = here.cqsb * ag0;
                gcqsb = here.cqdb * ag0;
                gcqbb = here.cqbb * ag0;
                gcggb = (cgdo + cgso + pParam.cgbo) * ag0;
                gcgdb = -cgdo * ag0;
                gcgsb = -cgso * ag0;
                gcdgb = -cgdo * ag0;
                gcddb = (here.capbd + cgdo) * ag0;
                gcdsb = 0.0;
                gcsgb = -cgso * ag0;
                gcsdb = 0.0;
                gcssb = (here.capbs + cgso) * ag0;
                gcbgb = -pParam.cgbo * ag0;
                gcbdb = -here.capbd * ag0;
                gcbsb = -here.capbs * ag0;
                CoxWL = model.cox * pParam.weffCV * pParam.leffCV;
                if fabs(qcheq) <= 1.0e-5 * CoxWL {
                    if model.xpart < 0.5 {
                        sxpart = 0.4;
                    } else if model.xpart > 0.5 {
                        sxpart = 0.0;
                    } else {
                        sxpart = 0.5;
                    }
                    dsxpart_dVs = 0.0;
                    dsxpart_dVb = dsxpart_dVs;
                    dsxpart_dVg = dsxpart_dVb;
                    dsxpart_dVd = dsxpart_dVg;
                } else {
                    sxpart = qdrn / qcheq;
                    Css = here.cddb;
                    Cds = -(here.cgdb + here.cddb + here.cbdb);
                    dsxpart_dVs = (Css - sxpart * (Css + Cds)) / qcheq;
                    Csg = here.cdgb;
                    Cdg = -(here.cggb + here.cdgb + here.cbgb);
                    dsxpart_dVg = (Csg - sxpart * (Csg + Cdg)) / qcheq;
                    Csd = here.cdsb;
                    Cdd = -(here.cgsb + here.cdsb + here.cbsb);
                    dsxpart_dVd = (Csd - sxpart * (Csd + Cdd)) / qcheq;
                    dsxpart_dVb = -(dsxpart_dVd + dsxpart_dVg + dsxpart_dVs);
                }
                dxpart = 1.0 - sxpart;
                ddxpart_dVd = -dsxpart_dVd;
                ddxpart_dVg = -dsxpart_dVg;
                ddxpart_dVs = -dsxpart_dVs;
                ddxpart_dVb = -(ddxpart_dVd + ddxpart_dVg + ddxpart_dVs);
                qgd = qgdo;
                qgs = qgso;
                qgb = pParam.cgbo * vgb;
                qgate = qgd + qgs + qgb;
                qbulk = -qgb;
                qsrc = -qgs;
                qdrn = -(qgate + qbulk + qsrc);
            }
        }
        cqcheq = 0.0;
        cqdef = cqcheq;
        // C's `if (ByPass) goto line860;`: ByPass is always 0 (no bypass).
        ckt.set_state0(state::QG, qgate);
        ckt.set_state0(state::QD, qdrn - ckt.state0(state::QBD));
        ckt.set_state0(
            state::QB,
            qbulk + ckt.state0(state::QBD) + ckt.state0(state::QBS),
        );
        if here.nqsMod != 0 {
            ckt.set_state0(state::QCDUMP, qdef * ScalingFactor);
            ckt.set_state0(state::QCHEQ, qcheq);
        }
        if ckt.mode() & mode::MODEINITSMSIG != 0 {
            goto_line1000 = true;
            break 'charges;
        }
        if !(ChargeComputationNeeded != 0) {
            goto_line850 = true;
            break 'charges;
        }
        if ckt.mode() & mode::MODEDCTRANCURVE != 0 {
            goto_line850 = true;
            break 'charges;
        }
        if ckt.mode() & mode::MODEINITTRAN != 0 {
            ckt.set_state1(state::QB, ckt.state0(state::QB));
            ckt.set_state1(state::QG, ckt.state0(state::QG));
            ckt.set_state1(state::QD, ckt.state0(state::QD));
            if here.nqsMod != 0 {
                ckt.set_state1(state::QCHEQ, ckt.state0(state::QCHEQ));
                ckt.set_state1(state::QCDUMP, ckt.state0(state::QCDUMP));
            }
        }
        (geq, ceq) = ckt.integrate(0.0, state::QB)?;
        (geq, ceq) = ckt.integrate(0.0, state::QG)?;
        (geq, ceq) = ckt.integrate(0.0, state::QD)?;
        if here.nqsMod != 0 {
            (geq, ceq) = ckt.integrate(0.0, state::QCDUMP)?;
            (geq, ceq) = ckt.integrate(0.0, state::QCHEQ)?;
        }
    }
    if !goto_line1000 {
        if goto_line850 {
            // C label `line850:`.
            ceqqd = 0.0;
            ceqqb = ceqqd;
            ceqqg = ceqqb;
            cqdef = 0.0;
            cqcheq = cqdef;
            gcdsb = 0.0;
            gcddb = gcdsb;
            gcdgb = gcddb;
            gcssb = 0.0;
            gcsdb = gcssb;
            gcsgb = gcsdb;
            gcgsb = 0.0;
            gcgdb = gcgsb;
            gcggb = gcgdb;
            gcbsb = 0.0;
            gcbdb = gcbsb;
            gcbgb = gcbdb;
            gcqbb = 0.0;
            gcqsb = gcqbb;
            gcqdb = gcqsb;
            gcqgb = gcqdb;
            gqdef = gcqgb;
            ggts = 0.0;
            ggtb = ggts;
            ggtd = ggtb;
            ggtg = ggtd;
            dxpart = if here.mode > 0 { 0.4 } else { 0.6 };
            sxpart = (1.0 - (dxpart));
            ddxpart_dVs = 0.0;
            ddxpart_dVb = ddxpart_dVs;
            ddxpart_dVg = ddxpart_dVb;
            ddxpart_dVd = ddxpart_dVg;
            dsxpart_dVs = 0.0;
            dsxpart_dVb = dsxpart_dVs;
            dsxpart_dVg = dsxpart_dVb;
            dsxpart_dVd = dsxpart_dVg;
            if here.nqsMod != 0 {
                here.gtau =
                    16.0 * here.u0temp * model.vtm / pParam.leffCV / pParam.leffCV * ScalingFactor;
            } else {
                here.gtau = 0.0;
            }
        } else {
            // C label `line860:`.
            cqgate = ckt.state0(state::CQG);
            cqbulk = ckt.state0(state::CQB);
            cqdrn = ckt.state0(state::CQD);
            ceqqg = cqgate - gcggb * vgb + gcgdb * vbd + gcgsb * vbs;
            ceqqb = cqbulk - gcbgb * vgb + gcbdb * vbd + gcbsb * vbs;
            ceqqd = cqdrn - gcdgb * vgb + gcddb * vbd + gcdsb * vbs;
            if here.nqsMod != 0 {
                T0 = ggtg * vgb - ggtd * vbd - ggts * vbs;
                ceqqg += T0;
                T1 = qdef * here.gtau;
                ceqqd -=
                    dxpart * T0 + T1 * (ddxpart_dVg * vgb - ddxpart_dVd * vbd - ddxpart_dVs * vbs);
                cqdef = ckt.state0(state::CQCDUMP) - gqdef * qdef;
                cqcheq = ckt.state0(state::CQCHEQ) - (gcqgb * vgb - gcqdb * vbd - gcqsb * vbs) + T0;
            }
            if ckt.mode() & mode::MODEINITTRAN != 0 {
                ckt.set_state1(state::CQB, ckt.state0(state::CQB));
                ckt.set_state1(state::CQG, ckt.state0(state::CQG));
                ckt.set_state1(state::CQD, ckt.state0(state::CQD));
                if here.nqsMod != 0 {
                    ckt.set_state1(state::CQCHEQ, ckt.state0(state::CQCHEQ));
                    ckt.set_state1(state::CQCDUMP, ckt.state0(state::CQCDUMP));
                }
            }
        }
        // C label `line900:`.
        if here.mode >= 0 {
            Gm = here.gm;
            Gmbs = here.gmbs;
            FwdSum = Gm + Gmbs;
            RevSum = 0.0;
            cdreq = f64::from(model.r#type) * (cdrain - here.gds * vds - Gm * vgs - Gmbs * vbs);
            ceqbd = f64::from(-model.r#type)
                * (here.csub - here.gbds * vds - here.gbgs * vgs - here.gbbs * vbs);
            ceqbs = 0.0;
            gbbdp = -here.gbds;
            gbbsp = (here.gbds + here.gbgs + here.gbbs);
            gbdpg = here.gbgs;
            gbdpdp = here.gbds;
            gbdpb = here.gbbs;
            gbdpsp = -(gbdpg + gbdpdp + gbdpb);
            gbspg = 0.0;
            gbspdp = 0.0;
            gbspb = 0.0;
            gbspsp = 0.0;
        } else {
            Gm = -here.gm;
            Gmbs = -here.gmbs;
            FwdSum = 0.0;
            RevSum = -(Gm + Gmbs);
            cdreq = f64::from(-model.r#type) * (cdrain + here.gds * vds + Gm * vgd + Gmbs * vbd);
            ceqbs = f64::from(-model.r#type)
                * (here.csub + here.gbds * vds - here.gbgs * vgd - here.gbbs * vbd);
            ceqbd = 0.0;
            gbbsp = -here.gbds;
            gbbdp = (here.gbds + here.gbgs + here.gbbs);
            gbdpg = 0.0;
            gbdpsp = 0.0;
            gbdpb = 0.0;
            gbdpdp = 0.0;
            gbspg = here.gbgs;
            gbspsp = here.gbds;
            gbspb = here.gbbs;
            gbspdp = -(gbspg + gbspsp + gbspb);
        }
        if model.r#type > 0 {
            ceqbs += (here.cbs - here.gbs * vbs);
            ceqbd += (here.cbd - here.gbd * vbd);
        } else {
            ceqbs -= (here.cbs - here.gbs * vbs);
            ceqbd -= (here.cbd - here.gbd * vbd);
            ceqqg = -ceqqg;
            ceqqb = -ceqqb;
            ceqqd = -ceqqd;
            cqdef = -cqdef;
            cqcheq = -cqcheq;
        }
        m = here.m;
        ckt.rhs(here.gNode, -(m * ceqqg));
        ckt.rhs(here.bNode, -(m * (ceqbs + ceqbd + ceqqb)));
        ckt.rhs(here.dNodePrime, m * (ceqbd - cdreq - ceqqd));
        ckt.rhs(here.sNodePrime, m * (cdreq + ceqbs + ceqqg + ceqqb + ceqqd));
        if here.nqsMod != 0 {
            ckt.rhs(here.qNode, m * (cqcheq - cqdef));
        }
        T1 = qdef * here.gtau;
        ckt.matrix(here.dNode, here.dNode, m * here.drainConductance);
        ckt.matrix(here.gNode, here.gNode, m * (gcggb - ggtg));
        ckt.matrix(here.sNode, here.sNode, m * here.sourceConductance);
        ckt.matrix(
            here.bNode,
            here.bNode,
            m * (here.gbd + here.gbs - gcbgb - gcbdb - gcbsb - here.gbbs),
        );
        ckt.matrix(
            here.dNodePrime,
            here.dNodePrime,
            m * (here.drainConductance
                + here.gds
                + here.gbd
                + RevSum
                + gcddb
                + dxpart * ggtd
                + T1 * ddxpart_dVd
                + gbdpdp),
        );
        ckt.matrix(
            here.sNodePrime,
            here.sNodePrime,
            m * (here.sourceConductance
                + here.gds
                + here.gbs
                + FwdSum
                + gcssb
                + sxpart * ggts
                + T1 * dsxpart_dVs
                + gbspsp),
        );
        ckt.matrix(here.dNode, here.dNodePrime, -(m * here.drainConductance));
        ckt.matrix(
            here.gNode,
            here.bNode,
            -(m * (gcggb + gcgdb + gcgsb + ggtb)),
        );
        ckt.matrix(here.gNode, here.dNodePrime, m * (gcgdb - ggtd));
        ckt.matrix(here.gNode, here.sNodePrime, m * (gcgsb - ggts));
        ckt.matrix(here.sNode, here.sNodePrime, -(m * here.sourceConductance));
        ckt.matrix(here.bNode, here.gNode, m * (gcbgb - here.gbgs));
        ckt.matrix(here.bNode, here.dNodePrime, m * (gcbdb - here.gbd + gbbdp));
        ckt.matrix(here.bNode, here.sNodePrime, m * (gcbsb - here.gbs + gbbsp));
        ckt.matrix(here.dNodePrime, here.dNode, -(m * here.drainConductance));
        ckt.matrix(
            here.dNodePrime,
            here.gNode,
            m * (Gm + gcdgb + dxpart * ggtg + T1 * ddxpart_dVg + gbdpg),
        );
        ckt.matrix(
            here.dNodePrime,
            here.bNode,
            -(m * (here.gbd - Gmbs + gcdgb + gcddb + gcdsb
                - dxpart * ggtb
                - T1 * ddxpart_dVb
                - gbdpb)),
        );
        ckt.matrix(
            here.dNodePrime,
            here.sNodePrime,
            -(m * (here.gds + FwdSum - gcdsb - dxpart * ggts - T1 * ddxpart_dVs - gbdpsp)),
        );
        ckt.matrix(
            here.sNodePrime,
            here.gNode,
            m * (gcsgb - Gm + sxpart * ggtg + T1 * dsxpart_dVg + gbspg),
        );
        ckt.matrix(here.sNodePrime, here.sNode, -(m * here.sourceConductance));
        ckt.matrix(
            here.sNodePrime,
            here.bNode,
            -(m * (here.gbs + Gmbs + gcsgb + gcsdb + gcssb
                - sxpart * ggtb
                - T1 * dsxpart_dVb
                - gbspb)),
        );
        ckt.matrix(
            here.sNodePrime,
            here.dNodePrime,
            -(m * (here.gds + RevSum - gcsdb - sxpart * ggtd - T1 * dsxpart_dVd - gbspdp)),
        );
        if here.nqsMod != 0 {
            ckt.matrix(here.qNode, here.qNode, m * (gqdef + here.gtau));
            ckt.matrix(here.dNodePrime, here.qNode, m * (dxpart * here.gtau));
            ckt.matrix(here.sNodePrime, here.qNode, m * (sxpart * here.gtau));
            ckt.matrix(here.gNode, here.qNode, -(m * here.gtau));
            ckt.matrix(here.qNode, here.gNode, m * (ggtg - gcqgb));
            ckt.matrix(here.qNode, here.dNodePrime, m * (ggtd - gcqdb));
            ckt.matrix(here.qNode, here.sNodePrime, m * (ggts - gcqsb));
            ckt.matrix(here.qNode, here.bNode, m * (ggtb - gcqbb));
        }
    }
    // C label `line1000:`.
    Ok(())
}
