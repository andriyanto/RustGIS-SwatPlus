//! How well a simulated series fits an observed one, and the rating curve between a gauge's water
//! level (stage) and the discharge, written from their published definitions with the
//! conventions of swatplus-rust's calibration tools (`swatcal`), against which the values are
//! checked:
//!
//! - NSE (Nash and Sutcliffe, 1970): 1 is perfect, 0 is no better than the observed mean.
//! - KGE (Gupta et al., 2009): 1 less the distance of (r, alpha, beta) from (1, 1, 1): the
//!   correlation, the ratio of standard deviations and the ratio of means.
//! - PBIAS (%), positive when the simulation is too large (Moriasi et al., 2007, use the opposite
//!   sign: observed less simulated).
//! - RSR (Moriasi et al., 2007): the RMSE over the observed standard deviation.
//! - Spearman's rank correlation, ties sharing their mean rank: it doesn't depend on any
//!   monotonic rating curve, so it judges a model's discharge against stage without one.
//!
//! Each compares only the days where both values are finite. Standard deviations are of the
//! population (divided by n).
//!
//! A rating curve is `Q = a (H − h0)^b`: Q the discharge (m³/s), H the stage on the gauge (m), h0
//! the stage of zero flow, b the shape of the control (about 1.5 to 2.5 for natural sections), a
//! its size and roughness. [`fit_rating`] finds the curve that best turns a run's discharge into
//! the observed stage (least squares on stage).

/// The pairs where both are finite.
fn pairs(sim: &[f64], obs: &[f64]) -> Result<(Vec<f64>, Vec<f64>), String> {
    if sim.len() != obs.len() {
        return Err(format!("the simulated ({}) and observed ({}) series differ in length", sim.len(), obs.len()));
    }
    let (s, o): (Vec<f64>, Vec<f64>) = sim.iter().zip(obs).filter(|(s, o)| s.is_finite() && o.is_finite()).map(|(s, o)| (*s, *o)).unzip();
    if s.len() < 2 {
        return Err("fewer than 2 days with both values".into());
    }
    Ok((s, o))
}

fn mean(x: &[f64]) -> f64 {
    x.iter().sum::<f64>() / x.len().max(1) as f64
}

/// The population standard deviation.
fn std(x: &[f64]) -> f64 {
    let m = mean(x);
    (x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let (mx, my) = (mean(x), mean(y));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx).powi(2);
        syy += (b - my).powi(2);
    }
    sxy / (sxx * syy).sqrt()
}

pub fn nse(sim: &[f64], obs: &[f64]) -> Result<f64, String> {
    let (s, o) = pairs(sim, obs)?;
    let m = mean(&o);
    let num: f64 = s.iter().zip(&o).map(|(s, o)| (s - o).powi(2)).sum();
    let den: f64 = o.iter().map(|o| (o - m).powi(2)).sum();
    Ok(1.0 - num / den)
}

/// KGE and its parts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kge {
    pub kge: f64,
    pub r: f64,
    pub alpha: f64,
    pub beta: f64,
}

pub fn kge(sim: &[f64], obs: &[f64]) -> Result<Kge, String> {
    let (s, o) = pairs(sim, obs)?;
    let r = pearson(&s, &o);
    let alpha = std(&s) / std(&o);
    let beta = mean(&s) / mean(&o);
    Ok(Kge { kge: 1.0 - ((r - 1.0).powi(2) + (alpha - 1.0).powi(2) + (beta - 1.0).powi(2)).sqrt(), r, alpha, beta })
}

pub fn pbias(sim: &[f64], obs: &[f64]) -> Result<f64, String> {
    let (s, o) = pairs(sim, obs)?;
    Ok(100.0 * s.iter().zip(&o).map(|(s, o)| s - o).sum::<f64>() / o.iter().sum::<f64>())
}

pub fn rsr(sim: &[f64], obs: &[f64]) -> Result<f64, String> {
    let (s, o) = pairs(sim, obs)?;
    let rmse = (s.iter().zip(&o).map(|(s, o)| (s - o).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
    Ok(rmse / std(&o))
}

/// Ranks from 1, ties sharing their mean rank.
fn ranks(x: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..x.len()).collect();
    order.sort_by(|&a, &b| x.get(a).partial_cmp(&x.get(b)).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = vec![0.0; x.len()];
    let at = |k: usize| order.get(k).and_then(|&i| x.get(i)).copied();
    let mut i = 0;
    while i < order.len() {
        let mut j = i;
        while j + 1 < order.len() && at(j + 1) == at(i) {
            j += 1;
        }
        for k in i..=j {
            if let Some(slot) = order.get(k).and_then(|&o| out.get_mut(o)) {
                *slot = (i + j) as f64 / 2.0 + 1.0;
            }
        }
        i = j + 1;
    }
    out
}

pub fn spearman(sim: &[f64], obs: &[f64]) -> Result<f64, String> {
    let (s, o) = pairs(sim, obs)?;
    Ok(pearson(&ranks(&s), &ranks(&o)))
}

/// `Q = a (H − h0)^b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rating {
    pub a: f64,
    pub b: f64,
    pub h0: f64,
}

impl Rating {
    /// The stage (m) of discharge `q` (m³/s): a flow of zero or less is at h0; NaN stays NaN.
    pub fn stage(&self, q: f64) -> f64 {
        if q.is_nan() {
            return q;
        }
        self.h0 + (q.max(0.0) / self.a).powf(1.0 / self.b)
    }

    /// The discharge (m³/s) at stage `h` (m): none at or below h0.
    pub fn flow(&self, h: f64) -> f64 {
        if h.is_nan() {
            return h;
        }
        self.a * (h - self.h0).max(0.0).powf(self.b)
    }
}

/// A fitted rating curve and the RMSE (m) of its stage against the observed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RatingFit {
    pub rating: Rating,
    pub rmse: f64,
}

/// The best (stage RMSE, ln a, b) for one h0: a regression in logs for a start, then
/// Gauss-Newton steps on the stage residuals.
fn fit_at_h0(lq: &[f64], h: &[f64], h0: f64, (b_lo, b_hi): (f64, f64)) -> (f64, f64, f64) {
    let ld: Vec<f64> = h.iter().map(|h| (h - h0).ln()).collect();
    // ln(H − h0) = (ln Q − ln a) / b: a straight line in ln Q.
    let (mx, my) = (mean(lq), mean(&ld));
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (x, y) in lq.iter().zip(&ld) {
        sxy += (x - mx) * (y - my);
        sxx += (x - mx).powi(2);
    }
    let slope = sxy / sxx;
    let mut b = (if slope > 0.0 { 1.0 / slope } else { b_hi }).clamp(b_lo, b_hi);
    let mut la = mean(&lq.iter().zip(&ld).map(|(q, d)| q - b * d).collect::<Vec<_>>());
    for _ in 0..8 {
        // Least squares for the step: the normal equations of the n × 2 Jacobian.
        let (mut j11, mut j12, mut j22, mut g1, mut g2) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (q, hh) in lq.iter().zip(h) {
            let s = ((q - la) / b).exp();
            let r = h0 + s - hh;
            let (ja, jb) = (-s / b, -s * (q - la) / (b * b));
            j11 += ja * ja;
            j12 += ja * jb;
            j22 += jb * jb;
            g1 -= ja * r;
            g2 -= jb * r;
        }
        let det = j11 * j22 - j12 * j12;
        if !det.is_finite() || det.abs() < 1e-300 {
            break;
        }
        let (da, db) = ((g1 * j22 - g2 * j12) / det, (j11 * g2 - j12 * g1) / det);
        la += da;
        b = (b + db).clamp(b_lo, b_hi);
        if da.abs().max(db.abs()) < 1e-9 {
            break;
        }
    }
    let rmse = (lq.iter().zip(h).map(|(q, hh)| (h0 + ((q - la) / b).exp() - hh).powi(2)).sum::<f64>() / lq.len() as f64).sqrt();
    (rmse, la, b)
}

/// The rating curve that best turns one run's discharge `q` into the observed stage `h` (least
/// squares on stage), with b in `b_range` (1.2 to 3.0 by default) and h0 from 2 m below the lowest
/// observed stage up to just below it: for each h0 the best a and b as [`fit_at_h0`] finds them,
/// h0 by a scan of 25 and then a golden-section search, so the fit moves smoothly with the flows.
pub fn fit_rating(q: &[f64], h: &[f64], b_range: Option<(f64, f64)>) -> Result<RatingFit, String> {
    let (q, h): (Vec<f64>, Vec<f64>) =
        q.iter().zip(h).filter(|(q, h)| q.is_finite() && h.is_finite() && **q > 0.0).map(|(q, h)| (*q, *h)).unzip();
    if q.len() < 3 {
        return Err("fewer than 3 days with both a flow and a stage".into());
    }
    let b_range = b_range.unwrap_or((1.2, 3.0));
    let hmin = h.iter().copied().fold(f64::INFINITY, f64::min);
    let (lo, hi) = (hmin - 2.0, hmin - 1e-3);
    let lq: Vec<f64> = q.iter().map(|q| q.ln()).collect();
    let err = |h0: f64| fit_at_h0(&lq, &h, h0, b_range).0;
    const SCAN: usize = 25;
    let grid: Vec<f64> = (0..SCAN).map(|i| lo + (hi - lo) * i as f64 / (SCAN - 1) as f64).collect();
    let errs: Vec<f64> = grid.iter().map(|&x| err(x)).collect();
    let k = errs.iter().enumerate().min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal)).map_or(0, |(i, _)| i);
    let mut a = grid.get(k.saturating_sub(1)).copied().unwrap_or(lo);
    let mut c = grid.get((k + 1).min(SCAN - 1)).copied().unwrap_or(hi);
    let g = (5f64.sqrt() - 1.0) / 2.0;
    let (mut x1, mut x2) = (c - g * (c - a), a + g * (c - a));
    let (mut f1, mut f2) = (err(x1), err(x2));
    while c - a > 1e-5 {
        if f1 < f2 {
            (c, x2, f2) = (x2, x1, f1);
            x1 = c - g * (c - a);
            f1 = err(x1);
        } else {
            (a, x1, f1) = (x1, x2, f2);
            x2 = a + g * (c - a);
            f2 = err(x2);
        }
    }
    let h0 = (a + c) / 2.0;
    let (rmse, la, b) = fit_at_h0(&lq, &h, h0, b_range);
    Ok(RatingFit { rating: Rating { a: la.exp(), b, h0 }, rmse })
}

// `cfg(test)` on its own, as clippy's allow-unwrap-in-tests looks for it.
#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * b.abs().max(1.0)
    }

    /// The values `swatcal.metrics` gives for the same series (numpy 2.5).
    #[test]
    fn metrics_match_swatcal() {
        let obs = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(nse(&obs, &obs).unwrap(), 1.0);
        assert!(near(nse(&[3.0; 5], &obs).unwrap(), 0.0, 1e-12));
        let k = kge(&obs.map(|x| 2.0 * x), &obs).unwrap();
        assert!(near(k.beta, 2.0, 1e-12) && near(k.alpha, 2.0, 1e-12) && near(k.r, 1.0, 1e-12));
        assert!(near(pbias(&obs.map(|x| 1.1 * x), &obs).unwrap(), 10.0, 1e-12));
        assert!(near(spearman(&obs.map(f64::exp), &obs).unwrap(), 1.0, 1e-12));
        assert!(near(spearman(&[1.0, 1.0, 2.0, 3.0], &[1.0, 2.0, 3.0, 4.0]).unwrap(), 0.948_683_3, 1e-6));
        assert_eq!(nse(&[1.0, f64::NAN, 3.0], &[1.0, 2.0, 3.0]).unwrap(), 1.0);
        // A series of swatcal's own checks (sim, obs), with its answers.
        let sim = [2.1, 3.9, 6.2, 7.8, 10.5, 12.1, 9.0, 5.5];
        let obs = [2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 10.0, 6.0];
        assert!(near(nse(&sim, &obs).unwrap(), REF_NSE, 1e-12), "{}", nse(&sim, &obs).unwrap());
        let k = kge(&sim, &obs).unwrap();
        assert!(near(k.kge, REF_KGE, 1e-12) && near(k.r, REF_R, 1e-12) && near(k.alpha, REF_ALPHA, 1e-12), "{k:?}");
        assert!(near(pbias(&sim, &obs).unwrap(), REF_PBIAS, 1e-12));
        assert!(near(rsr(&sim, &obs).unwrap(), REF_RSR, 1e-12));
        assert!(near(spearman(&sim, &obs).unwrap(), REF_SPEARMAN, 1e-12));
        assert!(nse(&[1.0], &[1.0]).is_err());
        assert!(nse(&[1.0, 2.0], &[1.0]).is_err());
    }

    // swatcal.metrics on these two series.
    const REF_NSE: f64 = 0.979748427672956;
    const REF_KGE: f64 = 0.9817412050575115;
    const REF_R: f64 = 0.9904977304257266;
    const REF_ALPHA: f64 = 0.9984815515596827;
    const REF_PBIAS: f64 = -1.551724137931035;
    const REF_RSR: f64 = 0.1423080191944362;
    const REF_SPEARMAN: f64 = 0.9880235200593538;

    #[test]
    fn a_rating_curve_goes_both_ways_and_is_found_back() {
        let r = Rating { a: 12.0, b: 1.8, h0: 0.5 };
        assert!(near(r.flow(r.stage(37.0)), 37.0, 1e-12));
        assert_eq!(r.stage(0.0), 0.5);
        assert_eq!(r.flow(0.2), 0.0);
        assert!(r.stage(f64::NAN).is_nan());
        let q: Vec<f64> = (1..200).map(|i| 0.5 + (i as f64 * 0.37).sin().abs() * 80.0 + i as f64 * 0.1).collect();
        let h: Vec<f64> = q.iter().map(|&q| r.stage(q)).collect();
        let fit = fit_rating(&q, &h, None).unwrap();
        assert!(fit.rmse < 1e-4, "{fit:?}");
        assert!(near(fit.rating.b, 1.8, 1e-3) && near(fit.rating.h0, 0.5, 1e-3) && near(fit.rating.a, 12.0, 1e-2), "{fit:?}");
        // Stage can't see a volume error: half as much again flow is fitted as well, through a
        // larger a.
        let more: Vec<f64> = q.iter().map(|q| 1.5 * q).collect();
        let fit = fit_rating(&more, &h, None).unwrap();
        assert!(fit.rmse < 1e-4 && near(fit.rating.a, 18.0, 1e-2), "{fit:?}");
        assert!(fit_rating(&[1.0, 2.0], &[1.0, 2.0], None).is_err());
        // A stage read with errors: the curve swatcal.stage.fit_rating finds for the same record.
        let noisy: Vec<f64> = h.iter().enumerate().map(|(i, h)| h + 0.01 * (i as f64 * 1.7).sin()).collect();
        let fit = fit_rating(&q, &noisy, None).unwrap();
        assert!(near(fit.rating.a, 12.076935747690992, 1e-4), "{fit:?}");
        assert!(near(fit.rating.b, 1.7964112517515718, 1e-4), "{fit:?}");
        assert!(near(fit.rating.h0, 0.5041808536589545, 1e-4), "{fit:?}");
        assert!(near(fit.rmse, 0.0070533232930199575, 1e-4), "{fit:?}");
    }
}
