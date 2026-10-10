//! Color grading (render stage 5c) and split toning (stage 5d). Master plan §3.1
//! `color_grade` (wheels: hue 0..360, sat 0..100, lum -100..100; blending 0..100; balance
//! -100..100) and `split_toning`. crs key mapping is decided in split 6. Our definition (D13):
//!
//! - A wheel is a tint plus a lift. The tint is the luma-free chroma direction of its hue,
//!   `hexagon(hue / 60) - luma(hexagon)`, times `TINT * sat / 100`; it barely changes luma. The
//!   lift adds `LIFT * lum / 100` to all three channels. A wheel with sat 0 and lum 0 is
//!   inactive (hue alone has no effect).
//! - Grading zones weight by pixel luma, clamped to [0, 1] for the weights only: shadows fade
//!   out around `pivot - 0.25`, highlights fade in around `pivot + 0.25`, both with half width
//!   `0.05 + 0.2 * blending / 100`; midtones take the rest; global applies everywhere.
//!   `pivot = 0.5 - 0.25 * balance / 100`, so positive balance lets highlights reach darker
//!   tones. Zone weights come from the input luma; shadows, midtones, highlights, then global.
//! - Split toning runs after grading as its own pass: highlight weight
//!   `smoothstep(pivot - 0.25, pivot + 0.25, luma)`, shadow weight its complement, pivot from
//!   `split_toning.balance` by the same formula.
//! - A zero weight leaves the pixel bit-identical.
//!
//! Nothing is clamped (master plan A3). The WebGL2 preview ports this verbatim (A9).

use super::basic::{luma, smoothstep};
use super::hsl::{hexagon, wrap_sextant};
use crate::recipe::{ColorGrade, GradeZone, SplitToning};

/// Chroma offset of a sat-100 tint.
const TINT: f32 = 0.15;
/// Lift of a lum +-100 wheel.
const LIFT: f32 = 0.1;

/// One prepared wheel.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Tint {
    chroma: [f32; 3],
    lift: f32,
}

impl Tint {
    /// `None` when the wheel has no effect (sat 0 and lum 0).
    fn new(hue: f32, sat: f32, lum: f32) -> Option<Self> {
        if sat == 0.0 && lum == 0.0 {
            return None;
        }
        let hex = hexagon(wrap_sextant(hue / 60.0));
        let y = luma(hex);
        let k = TINT * (sat / 100.0);
        Some(Self {
            chroma: hex.map(|v| (v - y) * k),
            lift: LIFT * (lum / 100.0),
        })
    }

    fn zone(z: GradeZone) -> Option<Self> {
        Self::new(z.hue, z.sat, z.lum)
    }

    /// Adds weight `w` of this wheel.
    fn apply(self, px: [f32; 3], w: f32) -> [f32; 3] {
        if w == 0.0 {
            return px;
        }
        let lift = self.lift * w;
        [
            px[0] + (self.chroma[0] * w + lift),
            px[1] + (self.chroma[1] * w + lift),
            px[2] + (self.chroma[2] * w + lift),
        ]
    }
}

/// Shadow, midtone and highlight weights of luma `l`.
fn zone_weights(l: f32, pivot: f32, half_width: f32) -> [f32; 3] {
    let l = l.clamp(0.0, 1.0);
    let sh = 1.0 - smoothstep(pivot - 0.25 - half_width, pivot - 0.25 + half_width, l);
    let hi = smoothstep(pivot + 0.25 - half_width, pivot + 0.25 + half_width, l);
    [sh, 1.0 - sh - hi, hi]
}

/// Pivot luma for a balance in -100..100.
fn pivot(balance: f32) -> f32 {
    0.5 - 0.25 * (balance / 100.0)
}

/// Color grading, prepared once per render.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct GradePlan {
    zones: [Option<Tint>; 3],
    global: Option<Tint>,
    pivot: f32,
    half_width: f32,
}

impl GradePlan {
    /// `None` when no wheel has sat or lum (stage inactive).
    pub(super) fn new(g: &ColorGrade) -> Option<Self> {
        let zones = [
            Tint::zone(g.shadows),
            Tint::zone(g.midtones),
            Tint::zone(g.highlights),
        ];
        let global = Tint::zone(g.global);
        if zones.iter().all(Option::is_none) && global.is_none() {
            return None;
        }
        Some(Self {
            zones,
            global,
            pivot: pivot(g.balance),
            half_width: 0.05 + 0.2 * (g.blending / 100.0),
        })
    }

    /// Shadows, midtones, highlights (weights from the input luma), then global.
    pub(super) fn apply(&self, px: [f32; 3]) -> [f32; 3] {
        let w = zone_weights(luma(px), self.pivot, self.half_width);
        let mut out = px;
        for (t, &wz) in self.zones.iter().zip(&w) {
            if let Some(t) = t {
                out = t.apply(out, wz);
            }
        }
        if let Some(t) = self.global {
            out = t.apply(out, 1.0);
        }
        out
    }
}

/// Split toning, prepared once per render.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SplitPlan {
    shadow: Option<Tint>,
    highlight: Option<Tint>,
    pivot: f32,
}

impl SplitPlan {
    /// `None` when both saturations are zero (stage inactive).
    pub(super) fn new(s: &SplitToning) -> Option<Self> {
        let shadow = Tint::new(s.shadow_hue, s.shadow_sat, 0.0);
        let highlight = Tint::new(s.highlight_hue, s.highlight_sat, 0.0);
        if shadow.is_none() && highlight.is_none() {
            return None;
        }
        Some(Self {
            shadow,
            highlight,
            pivot: pivot(s.balance),
        })
    }

    /// Shadow tint, then highlight tint, weights from the input luma.
    pub(super) fn apply(&self, px: [f32; 3]) -> [f32; 3] {
        let l = luma(px).clamp(0.0, 1.0);
        let hi = smoothstep(self.pivot - 0.25, self.pivot + 0.25, l);
        let mut out = px;
        if let Some(t) = self.shadow {
            out = t.apply(out, 1.0 - hi);
        }
        if let Some(t) = self.highlight {
            out = t.apply(out, hi);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(p: [f32; 3]) -> [u32; 3] {
        p.map(f32::to_bits)
    }

    fn zone(hue: f32, sat: f32, lum: f32) -> GradeZone {
        GradeZone { hue, sat, lum }
    }

    #[test]
    fn zone_weights_partition_unity_and_are_ordered() {
        for (pivot, hw) in [(0.5, 0.15), (0.25, 0.05), (0.75, 0.25)] {
            let mut prev = [1.0f32, 0.0, 0.0];
            for i in 0..=100 {
                let w = zone_weights(i as f32 / 100.0, pivot, hw);
                assert!(
                    w.iter().all(|&x| (-1e-6..=1.0 + 1e-6).contains(&x)),
                    "{w:?}"
                );
                assert!((w.iter().sum::<f32>() - 1.0).abs() <= 1e-6, "{w:?}");
                assert!(w[0] <= prev[0] && w[2] >= prev[2], "{prev:?} -> {w:?}");
                prev = w;
            }
        }
    }

    #[test]
    fn hue_blending_or_balance_alone_is_inactive() {
        assert!(GradePlan::new(&ColorGrade::default()).is_none());
        assert!(SplitPlan::new(&SplitToning::default()).is_none());
        let g = ColorGrade {
            shadows: zone(200.0, 0.0, 0.0),
            blending: 90.0,
            balance: -40.0,
            ..ColorGrade::default()
        };
        assert!(GradePlan::new(&g).is_none());
        let s = SplitToning {
            shadow_hue: 30.0,
            highlight_hue: 200.0,
            balance: 50.0,
            ..SplitToning::default()
        };
        assert!(SplitPlan::new(&s).is_none());
    }

    #[test]
    fn blue_shadows_tint_dark_pixels_and_spare_bright_ones() {
        let g = GradePlan::new(&ColorGrade {
            shadows: zone(240.0, 100.0, 0.0),
            ..ColorGrade::default()
        })
        .unwrap();
        let dark = g.apply([0.1; 3]);
        assert!(dark[2] > dark[0], "{dark:?}");
        assert_eq!(bits(g.apply([0.95; 3])), bits([0.95; 3]));
    }

    fn gray(v: f32) -> [f32; 3] {
        [v; 3]
    }

    fn assert_near(out: [f32; 3], expected: [f32; 3], tol: f32) {
        for (o, e) in out.iter().zip(expected) {
            assert!((o - e).abs() <= tol, "{out:?} vs {expected:?}");
        }
    }

    fn chroma(p: [f32; 3]) -> f32 {
        p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])
    }

    /// Expected values below are hand-computed from the module docs (D3 constants), not from
    /// the implementation (review Y2).
    #[test]
    fn global_tint_adds_chroma_and_keeps_luma() {
        for hue in [0.0, 40.0, 120.0, 240.0, 300.0] {
            let g = GradePlan::new(&ColorGrade {
                global: zone(hue, 100.0, 0.0),
                ..ColorGrade::default()
            })
            .unwrap();
            let out = g.apply(gray(0.5));
            assert!((chroma(out) - 0.15).abs() <= 1e-6, "{hue} {out:?}");
            assert!((luma(out) - 0.5).abs() <= 1e-6, "{hue} {out:?}");
        }
        // hue 0: hexagon [1, 0, 0], luma 0.2126 -> offsets 0.15 * [0.7874, -0.2126, -0.2126].
        let red = GradePlan::new(&ColorGrade {
            global: zone(0.0, 100.0, 0.0),
            ..ColorGrade::default()
        })
        .unwrap();
        assert_near(red.apply(gray(0.5)), [0.618_11, 0.468_11, 0.468_11], 1e-5);
    }

    #[test]
    fn shadow_lum_lifts_dark_pixels() {
        let g = GradePlan::new(&ColorGrade {
            shadows: zone(0.0, 0.0, 100.0),
            ..ColorGrade::default()
        })
        .unwrap();
        assert_near(g.apply(gray(0.1)), gray(0.2), 1e-6);
    }

    #[test]
    fn midtone_and_global_lum_lift_by_their_own_weights() {
        let mid = GradePlan::new(&ColorGrade {
            midtones: zone(0.0, 0.0, 100.0),
            ..ColorGrade::default()
        })
        .unwrap();
        // Defaults: pivot 0.5, half width 0.15. Luma 0.5: shadow and highlight weights 0.
        assert_near(mid.apply(gray(0.5)), gray(0.6), 1e-6);
        // Luma 0.35: shadow = 1 - smoothstep(0.1, 0.4, 0.35) = 1 - 0.925926, midtone 0.925926.
        assert_near(mid.apply(gray(0.35)), gray(0.442_592_6), 1e-5);
        let global = GradePlan::new(&ColorGrade {
            global: zone(0.0, 0.0, -100.0),
            ..ColorGrade::default()
        })
        .unwrap();
        assert_near(global.apply(gray(0.95)), gray(0.85), 1e-6);
    }

    #[test]
    fn positive_balance_extends_highlights_into_darker_tones() {
        let at_0_55 = |balance| {
            GradePlan::new(&ColorGrade {
                highlights: zone(40.0, 0.0, 100.0),
                balance,
                ..ColorGrade::default()
            })
            .unwrap()
            .apply(gray(0.55))
        };
        // balance +100: pivot 0.25, highlight ramp 0.35..0.65, weight at 0.55 = 0.740741.
        assert_near(at_0_55(100.0), gray(0.624_074_1), 1e-5);
        // balance 0: ramp 0.6..0.9; balance -100: ramp 0.85..1.15. Weight 0 at 0.55 (kept off
        // the ramp edges so f32 rounding of luma cannot matter).
        assert_eq!(bits(at_0_55(0.0)), bits(gray(0.55)));
        assert_eq!(bits(at_0_55(-100.0)), bits(gray(0.55)));
    }

    #[test]
    fn blending_widens_the_shadow_transition() {
        let at_0_32 = |blending| {
            GradePlan::new(&ColorGrade {
                shadows: zone(0.0, 0.0, 100.0),
                blending,
                ..ColorGrade::default()
            })
            .unwrap()
            .apply(gray(0.32))
        };
        // blending 0: half width 0.05, shadow ramp 0.2..0.3, weight 0 at 0.32.
        assert_eq!(bits(at_0_32(0.0)), bits(gray(0.32)));
        // blending 100: half width 0.25, ramp 0..0.5, shadow weight 1 - 0.704512 = 0.295488.
        assert_near(at_0_32(100.0), gray(0.349_548_8), 1e-5);
    }

    /// Hand-computed (review Z1): hue 240 = hexagon [0, 0, 1], luma 0.0722, offsets
    /// 0.15 * [-0.0722, -0.0722, 0.9278]; hue 0 offsets 0.15 * [0.7874, -0.2126, -0.2126].
    /// Gray 0.5 with pivot 0.75 / 0.5 / 0.25 gives shadow weight 1 / 0.5 / 0.
    #[test]
    fn split_shadow_tint_and_balance_outputs() {
        let split = |hue, balance| {
            SplitPlan::new(&SplitToning {
                shadow_hue: hue,
                shadow_sat: 100.0,
                balance,
                ..SplitToning::default()
            })
            .unwrap()
        };
        assert_near(
            split(240.0, 0.0).apply(gray(0.1)),
            [0.089_17, 0.089_17, 0.239_17],
            1e-6,
        );
        assert_near(
            split(0.0, -100.0).apply(gray(0.5)),
            [0.618_11, 0.468_11, 0.468_11],
            1e-6,
        );
        assert_near(
            split(0.0, 0.0).apply(gray(0.5)),
            [0.559_055, 0.484_055, 0.484_055],
            1e-6,
        );
        assert_near(split(0.0, 100.0).apply(gray(0.5)), gray(0.5), 1e-6);
    }

    #[test]
    fn split_highlight_warms_bright_pixels_and_spares_dark_ones() {
        let s = SplitPlan::new(&SplitToning {
            highlight_hue: 40.0,
            highlight_sat: 100.0,
            ..SplitToning::default()
        })
        .unwrap();
        let bright = s.apply([0.9; 3]);
        assert!(bright[0] > bright[2], "{bright:?}");
        assert_eq!(bits(s.apply([0.05; 3])), bits([0.05; 3]));
    }

    #[test]
    fn hue_360_is_hue_0() {
        assert_eq!(Tint::new(360.0, 50.0, 0.0), Tint::new(0.0, 50.0, 0.0));
    }

    #[test]
    fn extremes_stay_finite() {
        let pixels = [
            [0.0; 3],
            [1.0; 3],
            [4.7, 0.3, -0.2],
            [-0.1; 3],
            [0.5, 0.2, 0.9],
        ];
        for lum in [-100.0, 100.0] {
            for balance in [-100.0, 100.0] {
                for blending in [0.0, 100.0] {
                    let z = zone(300.0, 100.0, lum);
                    let g = GradePlan::new(&ColorGrade {
                        shadows: z,
                        midtones: z,
                        highlights: z,
                        global: z,
                        blending,
                        balance,
                    })
                    .unwrap();
                    let s = SplitPlan::new(&SplitToning {
                        shadow_hue: 200.0,
                        shadow_sat: 100.0,
                        highlight_hue: 30.0,
                        highlight_sat: 100.0,
                        balance,
                    })
                    .unwrap();
                    for px in pixels {
                        let out = s.apply(g.apply(px));
                        assert!(out.iter().all(|v| v.is_finite()), "{px:?} {out:?}");
                    }
                }
            }
        }
    }
}
