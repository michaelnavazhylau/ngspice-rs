//! BSIM4 layout-dependent junction geometry and source/drain diffusion
//! resistance, a port of `bsim4/b4geo.c`.
//!
//! C prints its warnings to stdout; the port appends the same text to a
//! caller-supplied list instead, which the setup stages turn into
//! diagnostics.

/// `BSIM4NumFingerDiff`: the numbers of interior and end source/drain
/// diffusions `(nuIntD, nuEndD, nuIntS, nuEndS)` for `nf` fingers.
fn num_finger_diff(nf: f64, min_sd: i32) -> (f64, f64, f64, f64) {
    // C truncates with `(int)nf`; `as` saturates where C is undefined.
    let nf_int = nf as i32;
    if (nf_int % 2) != 0 {
        let int = 2.0 * f64::max((nf - 1.0) / 2.0, 0.0);
        (int, 1.0, int, 1.0)
    } else if min_sd == 1 {
        // Minimise the number of sources.
        (2.0 * f64::max(nf / 2.0 - 1.0, 0.0), 2.0, nf, 0.0)
    } else {
        (nf, 0.0, 2.0 * f64::max(nf / 2.0 - 1.0, 0.0), 2.0)
    }
}

/// `BSIM4PAeffGeo`: effective source/drain perimeters and areas
/// `(Ps, Pd, As, Ad)`. An unmatched `geo` leaves them at `initial`, as C
/// leaves its output arguments untouched.
#[allow(clippy::too_many_arguments)] // C's own argument list.
pub(crate) fn pa_eff_geo(
    nf: f64,
    geo: i32,
    min_sd: i32,
    weffcj: f64,
    dmcg: f64,
    dmci: f64,
    dmdg: f64,
    initial: (f64, f64, f64, f64),
    warnings: &mut Vec<String>,
) -> (f64, f64, f64, f64) {
    // For geo = 9 and 10 the numbers of diffusions are already known.
    let (nu_int_d, nu_end_d, nu_int_s, nu_end_s) = if geo < 9 {
        num_finger_diff(nf, min_sd)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };
    let t0 = dmcg + dmci;
    let t1 = dmcg + dmcg;
    let t2 = dmdg + dmdg;

    let ps_iso = t0 + t0 + weffcj;
    let pd_iso = ps_iso;
    let (ps_sha, pd_sha) = (t1, t1);
    let (ps_mer, pd_mer) = (t2, t2);

    let as_iso = t0 * weffcj;
    let ad_iso = as_iso;
    let as_sha = dmcg * weffcj;
    let ad_sha = as_sha;
    let as_mer = dmdg * weffcj;
    let ad_mer = as_mer;

    match geo {
        0 => (
            nu_end_s * ps_iso + nu_int_s * ps_sha,
            nu_end_d * pd_iso + nu_int_d * pd_sha,
            nu_end_s * as_iso + nu_int_s * as_sha,
            nu_end_d * ad_iso + nu_int_d * ad_sha,
        ),
        1 => (
            nu_end_s * ps_iso + nu_int_s * ps_sha,
            (nu_end_d + nu_int_d) * pd_sha,
            nu_end_s * as_iso + nu_int_s * as_sha,
            (nu_end_d + nu_int_d) * ad_sha,
        ),
        2 => (
            (nu_end_s + nu_int_s) * ps_sha,
            nu_end_d * pd_iso + nu_int_d * pd_sha,
            (nu_end_s + nu_int_s) * as_sha,
            nu_end_d * ad_iso + nu_int_d * ad_sha,
        ),
        3 => (
            (nu_end_s + nu_int_s) * ps_sha,
            (nu_end_d + nu_int_d) * pd_sha,
            (nu_end_s + nu_int_s) * as_sha,
            (nu_end_d + nu_int_d) * ad_sha,
        ),
        4 => (
            nu_end_s * ps_iso + nu_int_s * ps_sha,
            nu_end_d * pd_mer + nu_int_d * pd_sha,
            nu_end_s * as_iso + nu_int_s * as_sha,
            nu_end_d * ad_mer + nu_int_d * ad_sha,
        ),
        5 => (
            (nu_end_s + nu_int_s) * ps_sha,
            nu_end_d * pd_mer + nu_int_d * pd_sha,
            (nu_end_s + nu_int_s) * as_sha,
            nu_end_d * ad_mer + nu_int_d * ad_sha,
        ),
        6 => (
            nu_end_s * ps_mer + nu_int_s * ps_sha,
            nu_end_d * pd_iso + nu_int_d * pd_sha,
            nu_end_s * as_mer + nu_int_s * as_sha,
            nu_end_d * ad_iso + nu_int_d * ad_sha,
        ),
        7 => (
            nu_end_s * ps_mer + nu_int_s * ps_sha,
            (nu_end_d + nu_int_d) * pd_sha,
            nu_end_s * as_mer + nu_int_s * as_sha,
            (nu_end_d + nu_int_d) * ad_sha,
        ),
        8 => (
            nu_end_s * ps_mer + nu_int_s * ps_sha,
            nu_end_d * pd_mer + nu_int_d * pd_sha,
            nu_end_s * as_mer + nu_int_s * as_sha,
            nu_end_d * ad_mer + nu_int_d * ad_sha,
        ),
        // geo = 9 and 10 happen only when nf is even.
        9 => (
            ps_iso + (nf - 1.0) * ps_sha,
            nf * pd_sha,
            as_iso + (nf - 1.0) * as_sha,
            nf * ad_sha,
        ),
        10 => (
            nf * ps_sha,
            pd_iso + (nf - 1.0) * pd_sha,
            nf * as_sha,
            ad_iso + (nf - 1.0) * ad_sha,
        ),
        _ => {
            warnings.push(format!("Warning: Specified GEO = {geo} not matched"));
            initial
        }
    }
}

/// `BSIM4RdseffGeo`: the total source (`ty == 1`) or drain diffusion
/// resistance. An unmatched `geo` leaves `Rend` at zero, as C does.
#[allow(clippy::too_many_arguments)] // C's own argument list.
pub(crate) fn rdseff_geo(
    nf: f64,
    geo: i32,
    rgeo: i32,
    min_sd: i32,
    weffcj: f64,
    rsh: f64,
    dmcg: f64,
    dmci: f64,
    dmdg: f64,
    ty: i32,
    warnings: &mut Vec<String>,
) -> f64 {
    let mut rint = 0.0;
    let mut rend = 0.0;
    // geo = 9 and 10 only happen when nf is even.
    let (nu_int_d, nu_end_d, nu_int_s, nu_end_s) = if geo < 9 {
        num_finger_diff(nf, min_sd)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };
    if geo < 9 {
        // Internal S/D resistance: assume shared S or D and all wide contacts.
        if ty == 1 {
            rint = if nu_int_s == 0.0 {
                0.0
            } else {
                rsh * dmcg / (weffcj * nu_int_s)
            };
        } else {
            rint = if nu_int_d == 0.0 {
                0.0
            } else {
                rsh * dmcg / (weffcj * nu_int_d)
            };
        }
    }
    let iso = |nu_end: f64, ty: i32, rend: &mut f64, w: &mut Vec<String>| {
        rds_end_iso(weffcj, rsh, dmcg, dmci, nu_end, rgeo, ty, rend, w);
    };
    let sha = |nu_end: f64, ty: i32, rend: &mut f64, w: &mut Vec<String>| {
        rds_end_sha(weffcj, rsh, dmcg, nu_end, rgeo, ty, rend, w);
    };
    // End S/D resistance, geo dependent.
    match geo {
        0 => {
            if ty == 1 {
                iso(nu_end_s, 1, &mut rend, warnings);
            } else {
                iso(nu_end_d, 0, &mut rend, warnings);
            }
        }
        1 => {
            if ty == 1 {
                iso(nu_end_s, 1, &mut rend, warnings);
            } else {
                sha(nu_end_d, 0, &mut rend, warnings);
            }
        }
        2 => {
            if ty == 1 {
                sha(nu_end_s, 1, &mut rend, warnings);
            } else {
                iso(nu_end_d, 0, &mut rend, warnings);
            }
        }
        3 => {
            if ty == 1 {
                sha(nu_end_s, 1, &mut rend, warnings);
            } else {
                sha(nu_end_d, 0, &mut rend, warnings);
            }
        }
        4 => {
            if ty == 1 {
                iso(nu_end_s, 1, &mut rend, warnings);
            } else {
                rend = rsh * dmdg / weffcj;
            }
        }
        5 => {
            if ty == 1 {
                sha(nu_end_s, 1, &mut rend, warnings);
            } else {
                rend = rsh * dmdg / (weffcj * nu_end_d);
            }
        }
        6 => {
            if ty == 1 {
                rend = rsh * dmdg / weffcj;
            } else {
                iso(nu_end_d, 0, &mut rend, warnings);
            }
        }
        7 => {
            if ty == 1 {
                rend = rsh * dmdg / (weffcj * nu_end_s);
            } else {
                sha(nu_end_d, 0, &mut rend, warnings);
            }
        }
        8 => rend = rsh * dmdg / weffcj,
        // All wide contacts assumed for geo = 9 and 10.
        9 => {
            if ty == 1 {
                rend = 0.5 * rsh * dmcg / weffcj;
                rint = if nf == 2.0 {
                    0.0
                } else {
                    rsh * dmcg / (weffcj * (nf - 2.0))
                };
            } else {
                rend = 0.0;
                rint = rsh * dmcg / (weffcj * nf);
            }
        }
        10 => {
            if ty == 1 {
                rend = 0.0;
                rint = rsh * dmcg / (weffcj * nf);
            } else {
                rend = 0.5 * rsh * dmcg / weffcj;
                rint = if nf == 2.0 {
                    0.0
                } else {
                    rsh * dmcg / (weffcj * (nf - 2.0))
                };
            }
        }
        _ => warnings.push(format!("Warning: Specified GEO = {geo} not matched")),
    }
    let rtot = if rint <= 0.0 {
        rend
    } else if rend <= 0.0 {
        rint
    } else {
        rint * rend / (rint + rend)
    };
    if rtot == 0.0 {
        warnings.push("Warning: Zero resistance returned from RdseffGeo".into());
    }
    rtot
}

/// `BSIM4RdsEndIso`: end resistance of an isolated diffusion. An unmatched
/// `rgeo` leaves `rend` unchanged.
#[allow(clippy::too_many_arguments)] // C's own argument list.
fn rds_end_iso(
    weffcj: f64,
    rsh: f64,
    dmcg: f64,
    dmci: f64,
    nu_end: f64,
    rgeo: i32,
    ty: i32,
    rend: &mut f64,
    warnings: &mut Vec<String>,
) {
    let (narrow, wide): (&[i32], &[i32]) = if ty == 1 {
        (&[1, 2, 5], &[3, 4, 6])
    } else {
        (&[1, 3, 7], &[2, 4, 8])
    };
    if narrow.contains(&rgeo) {
        *rend = if nu_end == 0.0 {
            0.0
        } else {
            rsh * dmcg / (weffcj * nu_end)
        };
    } else if wide.contains(&rgeo) {
        if (dmcg + dmci) == 0.0 {
            warnings.push("(DMCG + DMCI) can not be equal to zero".into());
        }
        *rend = if nu_end == 0.0 || (dmcg + dmci) == 0.0 {
            0.0
        } else {
            rsh * weffcj / (3.0 * nu_end * (dmcg + dmci))
        };
    } else {
        warnings.push(format!("Warning: Specified RGEO = {rgeo} not matched"));
    }
}

/// `BSIM4RdsEndSha`: end resistance of a shared diffusion. An unmatched
/// `rgeo` leaves `rend` unchanged.
#[allow(clippy::too_many_arguments)] // C's own argument list.
fn rds_end_sha(
    weffcj: f64,
    rsh: f64,
    dmcg: f64,
    nu_end: f64,
    rgeo: i32,
    ty: i32,
    rend: &mut f64,
    warnings: &mut Vec<String>,
) {
    let (narrow, wide): (&[i32], &[i32]) = if ty == 1 {
        (&[1, 2, 5], &[3, 4, 6])
    } else {
        (&[1, 3, 7], &[2, 4, 8])
    };
    if narrow.contains(&rgeo) {
        *rend = if nu_end == 0.0 {
            0.0
        } else {
            rsh * dmcg / (weffcj * nu_end)
        };
    } else if wide.contains(&rgeo) {
        if dmcg == 0.0 {
            warnings.push("DMCG can not be equal to zero".into());
        }
        *rend = if nu_end == 0.0 {
            0.0
        } else {
            rsh * weffcj / (6.0 * nu_end * dmcg)
        };
    } else {
        warnings.push(format!("Warning: Specified RGEO = {rgeo} not matched"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finger_split_follows_c_parity_rules() {
        assert_eq!(num_finger_diff(3.0, 0), (2.0, 1.0, 2.0, 1.0));
        assert_eq!(num_finger_diff(4.0, 1), (2.0, 2.0, 4.0, 0.0));
        assert_eq!(num_finger_diff(4.0, 0), (4.0, 0.0, 2.0, 2.0));
        assert_eq!(num_finger_diff(1.0, 0), (0.0, 1.0, 0.0, 1.0));
    }

    #[test]
    fn unmatched_geo_keeps_outputs_and_warns() {
        let mut w = Vec::new();
        let out = pa_eff_geo(1.0, 11, 0, 1e-6, 0., 0., 0., (1., 2., 3., 4.), &mut w);
        assert_eq!(out, (1., 2., 3., 4.));
        assert_eq!(w, ["Warning: Specified GEO = 11 not matched"]);
    }

    #[test]
    fn single_finger_isolated_resistance() {
        let mut w = Vec::new();
        // geo 0, rgeo 1, one finger: no interior diffusion, end Rsh*DMCG/W.
        let r = rdseff_geo(1.0, 0, 1, 0, 2e-6, 10.0, 1e-7, 0., 0., 1, &mut w);
        assert_eq!(r, 10.0 * 1e-7 / (2e-6 * 1.0));
        assert!(w.is_empty());
    }
}
