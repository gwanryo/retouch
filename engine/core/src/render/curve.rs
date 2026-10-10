//! Tone curve, render stage 5a (master plan §3.1 `tone_curve`; crs `ToneCurvePV2012`,
//! `ToneCurvePV2012Red|Green|Blue`, points 0..255). Our definition (spec D13):
//!
//! - Monotonicity-preserving cubic Hermite through the points, tangents by Fritsch-Carlson
//!   (arithmetic-mean interior tangents, zero at local extrema and flat segments, then scaled so
//!   that alpha^2 + beta^2 <= 9). Monotone point data gives a monotone curve; no overshoot.
//!   The limit is evaluated without dividing by the secant (normalize the tangent pair by its
//!   larger magnitude, compare its length with 3|secant|), so subnormal secants cannot overflow
//!   to infinity and produce NaN (review Y1). f32 throughout.
//! - Outside `[x_first, x_last]` the curve continues along its end tangent. Nothing is clamped
//!   (master plan A3); `buffer::quantize` clips the final value.
//! - The master curve runs on all three channels first, then each channel's own curve.
//! - A channel equal to `[[0,0],[255,255]]` is the identity and is never evaluated, so an
//!   untouched channel stays bit-identical. Other point sets that happen to describe y = x are
//!   evaluated (within 1e-6 of the identity).
//!
//! Validation guarantees 2..=16 points with x increasing by at least 1 (`MIN_CURVE_X_STEP`), so
//! every secant is finite with |secant| < 256 (255 plus f32 rounding of x / 255 and y / 255);
//! the limiter keeps every |tangent| below 3 * 256. Subnormal secants and tangents are legal values
//! and are computed as IEEE says.
//! The WebGL2 preview ports this verbatim (A9).

use crate::recipe::{identity_curve, ToneCurve};

/// One prepared curve on the encoded 0..1 scale: knots and their tangents (dy/dx).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Curve {
    xs: Vec<f32>,
    ys: Vec<f32>,
    ms: Vec<f32>,
}

impl Curve {
    /// Prepares validated `points` (0..255 scale).
    pub(super) fn new(points: &[[f32; 2]]) -> Self {
        let xs: Vec<f32> = points.iter().map(|p| p[0] / 255.0).collect();
        let ys: Vec<f32> = points.iter().map(|p| p[1] / 255.0).collect();
        let n = xs.len();
        let secants: Vec<f32> = xs
            .windows(2)
            .zip(ys.windows(2))
            .map(|(x, y)| (y[1] - y[0]) / (x[1] - x[0]))
            .collect();
        let mut ms = vec![0.0f32; n];
        ms[0] = secants[0];
        ms[n - 1] = secants[n - 2];
        for (m, d) in ms[1..n - 1].iter_mut().zip(secants.windows(2)) {
            // Sign comparison, not `d[0] * d[1] > 0.0`: that product underflows to 0 for
            // subnormal secants.
            let same_sign = (d[0] > 0.0 && d[1] > 0.0) || (d[0] < 0.0 && d[1] < 0.0);
            *m = if same_sign { (d[0] + d[1]) * 0.5 } else { 0.0 };
        }
        for (k, &d) in secants.iter().enumerate() {
            if d == 0.0 {
                ms[k] = 0.0;
                ms[k + 1] = 0.0;
                continue;
            }
            // alpha^2 + beta^2 > 9 with alpha = m_k / d, beta = m_{k+1} / d, written without
            // dividing by d: scale the tangent pair by its larger magnitude first.
            let s = ms[k].abs().max(ms[k + 1].abs());
            if s == 0.0 {
                continue;
            }
            let a = ms[k] / s;
            let b = ms[k + 1] / s;
            let r = (a * a + b * b).sqrt();
            let limit = 3.0 * d.abs();
            if s * r > limit {
                ms[k] = limit * (a / r);
                ms[k + 1] = limit * (b / r);
            }
        }
        Self { xs, ys, ms }
    }

    /// Curve value at encoded `t` (any real).
    pub(super) fn eval(&self, t: f32) -> f32 {
        let last = self.xs.len() - 1;
        if t <= self.xs[0] {
            return self.ys[0] + self.ms[0] * (t - self.xs[0]);
        }
        if t >= self.xs[last] {
            return self.ys[last] + self.ms[last] * (t - self.xs[last]);
        }
        // xs[0] < t < xs[last]: segment k with xs[k] <= t < xs[k + 1].
        let k = self.xs[1..last]
            .iter()
            .position(|&x| t < x)
            .unwrap_or(last - 1);
        let h = self.xs[k + 1] - self.xs[k];
        let s = (t - self.xs[k]) / h;
        let s2 = s * s;
        let s3 = s2 * s;
        let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let h10 = s3 - 2.0 * s2 + s;
        let h01 = -2.0 * s3 + 3.0 * s2;
        let h11 = s3 - s2;
        h00 * self.ys[k] + h10 * h * self.ms[k] + h01 * self.ys[k + 1] + h11 * h * self.ms[k + 1]
    }
}

/// A channel's curve, or `None` for the identity `[[0,0],[255,255]]` (not evaluated).
fn prepared(points: &[[f32; 2]]) -> Option<Curve> {
    (points != identity_curve().as_slice()).then(|| Curve::new(points))
}

/// The recipe's four curves.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CurveSet {
    master: Option<Curve>,
    rgb: [Option<Curve>; 3],
}

impl CurveSet {
    /// `None` when every channel is the identity (stage inactive).
    pub(super) fn new(t: &ToneCurve) -> Option<Self> {
        let set = Self {
            master: prepared(&t.master),
            rgb: [prepared(&t.red), prepared(&t.green), prepared(&t.blue)],
        };
        (set.master.is_some() || set.rgb.iter().any(Option::is_some)).then_some(set)
    }

    /// Master curve on every channel, then each channel's own curve.
    pub(super) fn apply(&self, px: [f32; 3]) -> [f32; 3] {
        let mut out = self.master.as_ref().map_or(px, |c| px.map(|v| c.eval(v)));
        for (v, c) in out.iter_mut().zip(&self.rgb) {
            if let Some(c) = c {
                *v = c.eval(*v);
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> impl Iterator<Item = f32> {
        (0..=1000).map(|i| i as f32 / 1000.0)
    }

    #[test]
    fn passes_exactly_through_every_control_point() {
        let pts = [
            [0.0, 10.0],
            [64.0, 40.0],
            [128.0, 140.0],
            [192.0, 200.0],
            [255.0, 250.0],
        ];
        let c = Curve::new(&pts);
        for p in pts {
            assert_eq!(
                c.eval(p[0] / 255.0).to_bits(),
                (p[1] / 255.0).to_bits(),
                "{p:?}"
            );
        }
    }

    #[test]
    fn monotone_points_give_a_non_decreasing_curve() {
        let c = Curve::new(&[[0.0, 0.0], [30.0, 200.0], [60.0, 210.0], [255.0, 255.0]]);
        let ys: Vec<f32> = samples().map(|t| c.eval(t)).collect();
        for w in ys.windows(2) {
            assert!(w[1] >= w[0] - 1e-6, "{w:?}");
        }
    }

    #[test]
    fn flat_segment_does_not_overshoot() {
        let c = Curve::new(&[[0.0, 0.0], [100.0, 100.0], [150.0, 100.0], [255.0, 255.0]]);
        let flat = 100.0 / 255.0;
        for t in samples().filter(|&t| t > 100.0 / 255.0 && t < 150.0 / 255.0) {
            assert!((c.eval(t) - flat).abs() <= 1e-6, "{t}");
        }
    }

    #[test]
    fn local_extremum_is_not_exceeded() {
        let c = Curve::new(&[[0.0, 0.0], [128.0, 255.0], [255.0, 0.0]]);
        assert!(samples().all(|t| c.eval(t) <= 1.0 + 1e-6));
    }

    #[test]
    fn continues_along_end_tangents_without_clamping() {
        let c = Curve::new(&[[0.0, 0.0], [128.0, 160.0], [255.0, 255.0]]);
        assert!(c.eval(1.2) > c.eval(1.0));
        assert!(c.eval(-0.1) < c.eval(0.0));
        assert!(c.eval(30.0).is_finite() && c.eval(-30.0).is_finite());
    }

    #[test]
    fn steepest_valid_curve_stays_finite() {
        let c = Curve::new(&[[0.0, 0.0], [1.0, 255.0], [255.0, 255.0]]);
        for t in [-5.0, -0.01, 0.0, 0.002, 0.5, 1.0, 5.0] {
            assert!(c.eval(t).is_finite(), "{t}");
        }
    }

    /// Review Y1: a subnormal secant next to a normal tangent made the textbook limiter divide
    /// by ~1e-41 (beta = inf, tau = 0, NaN). Increasing and decreasing, narrow and wide.
    const TINY_SECANT_CURVES: [[[f32; 2]; 3]; 4] = [
        [[0.0, 0.0], [1.0, 1e-38], [2.0, 255.0]],
        [[0.0, 0.0], [128.0, 1e-38], [255.0, 255.0]],
        [[0.0, 255.0], [1.0, 1e-38], [2.0, 0.0]],
        [[0.0, 255.0], [128.0, 1e-38], [255.0, 0.0]],
    ];

    #[test]
    fn tiny_secants_keep_tangents_and_values_finite() {
        let probes: Vec<f32> = (-500..=1500)
            .map(|i| i as f32 / 1000.0)
            .chain([-30.0, -5.0, 5.0, 30.0])
            .collect();
        for pts in TINY_SECANT_CURVES {
            let c = Curve::new(&pts);
            assert!(c.ms.iter().all(|m| m.is_finite()), "{pts:?}: {:?}", c.ms);
            assert!(probes.iter().all(|&t| c.eval(t).is_finite()), "{pts:?}");
            for p in pts {
                assert_eq!(
                    c.eval(p[0] / 255.0).to_bits(),
                    (p[1] / 255.0).to_bits(),
                    "{pts:?}"
                );
            }
            let increasing = pts[2][1] > pts[0][1];
            let ys: Vec<f32> = samples().map(|t| c.eval(t)).collect();
            for w in ys.windows(2) {
                let ok = if increasing {
                    w[1] >= w[0] - 1e-6
                } else {
                    w[1] <= w[0] + 1e-6
                };
                assert!(ok, "{pts:?}: {w:?}");
            }
        }
    }

    #[test]
    fn default_curves_are_inactive() {
        assert!(CurveSet::new(&ToneCurve::default()).is_none());
    }

    #[test]
    fn identity_channels_are_left_bit_identical() {
        let t = ToneCurve {
            red: vec![[0.0, 0.0], [128.0, 150.0], [255.0, 255.0]],
            ..ToneCurve::default()
        };
        let set = CurveSet::new(&t).unwrap();
        let px = [0.3, 0.123_456_7, 1.7];
        let out = set.apply(px);
        assert_ne!(out[0].to_bits(), px[0].to_bits());
        assert_eq!(out[1].to_bits(), px[1].to_bits());
        assert_eq!(out[2].to_bits(), px[2].to_bits());
    }

    #[test]
    fn master_runs_before_the_channel_curve() {
        let t = ToneCurve {
            master: vec![[0.0, 30.0], [255.0, 255.0]],
            red: vec![[0.0, 0.0], [128.0, 170.0], [255.0, 255.0]],
            ..ToneCurve::default()
        };
        let set = CurveSet::new(&t).unwrap();
        let (m, r) = (Curve::new(&t.master), Curve::new(&t.red));
        let out = set.apply([0.4; 3]);
        assert_eq!(out[0].to_bits(), r.eval(m.eval(0.4)).to_bits());
        assert_eq!(out[1].to_bits(), m.eval(0.4).to_bits());
    }

    #[test]
    fn explicit_identity_points_are_active_but_near_identity() {
        let t = ToneCurve {
            master: vec![[0.0, 0.0], [128.0, 128.0], [255.0, 255.0]],
            ..ToneCurve::default()
        };
        let set = CurveSet::new(&t).unwrap();
        for v in samples() {
            assert!((set.apply([v; 3])[0] - v).abs() <= 1e-6, "{v}");
        }
    }
}
