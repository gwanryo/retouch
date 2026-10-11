//! HSL 8 bands, render stage 5b (master plan §3.1 `hsl`; crs `HueAdjustment*`,
//! `SaturationAdjustment*`, `LuminanceAdjustment*` for Red, Orange, Yellow, Green, Aqua, Blue,
//! Purple, Magenta, each -100..100). Our definition (spec D13):
//!
//! - Hue is the RGB hexagon hue in sextants (degrees / 60, [0, 6)). No trigonometry: a pixel is
//!   `min + chroma * hexagon(hue)`, so rotating the hue keeps max, min and chroma.
//! - Band centers 0, 30, 60, 120, 180, 240, 270, 300 degrees. A pixel belongs to its two
//!   neighboring bands with triangular weights that sum to 1.
//! - The band mix is scaled by `smoothstep(0, FULL_CHROMA, chroma)`: achromatic pixels
//!   (max == min) are returned untouched, dull pixels get proportionally less.
//! - Hue +-100 rotates by +-30 degrees (positive = toward the next band, red -> orange).
//!   Saturation +-100 scales chroma around Rec.709 luma by (1 +- 1). Luminance +-100 scales the
//!   pixel by (1 +- 0.4). Each step is skipped when its amount is zero, so pixels outside the
//!   adjusted bands stay bit-identical.
//!
//! Nothing is clamped (master plan A3). The WebGL2 preview ports this verbatim (A9).

use super::basic::{luma, smoothstep};
use crate::recipe::Hsl;

/// Band centers in sextants, R O Y G A B P M, then R again at 6.
const CENTERS: [f32; 9] = [0.0, 0.5, 1.0, 2.0, 3.0, 4.0, 4.5, 5.0, 6.0];
/// Chroma (max - min, encoded) from which a pixel gets the full band adjustment.
const FULL_CHROMA: f32 = 0.2;
/// Hue +-100 rotates by this many sextants (30 degrees).
const HUE_SHIFT: f32 = 0.5;
/// Luminance +-100 scales the pixel by 1 +- this.
const LUM_GAIN: f32 = 0.4;

/// Wraps a hue in sextants into [0, 6). Inputs are within one turn of that range.
pub(super) fn wrap_sextant(mut h: f32) -> f32 {
    if h < 0.0 {
        h += 6.0;
    }
    if h >= 6.0 {
        h -= 6.0;
    }
    h
}

/// Hexagon hue in [0, 6), minimum and chroma of an encoded pixel; `None` when the pixel is
/// achromatic (chroma <= 0) and has no hue.
pub(super) fn hue_sextant(px: [f32; 3]) -> Option<(f32, f32, f32)> {
    let [r, g, b] = px;
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let c = mx - mn;
    if c <= 0.0 {
        return None;
    }
    let h = if mx == r {
        (g - b) / c
    } else if mx == g {
        2.0 + (b - r) / c
    } else {
        4.0 + (r - g) / c
    };
    Some((wrap_sextant(h), mn, c))
}

/// Unit-chroma, zero-minimum color of hue `h` in [0, 6): the inverse of [`hue_sextant`]
/// (`min + chroma * hexagon(hue)`).
pub(super) fn hexagon(h: f32) -> [f32; 3] {
    let i = h as u32; // h >= 0, so truncation is floor
    let f = h - i as f32;
    match i {
        0 => [1.0, f, 0.0],
        1 => [1.0 - f, 1.0, 0.0],
        2 => [0.0, 1.0, f],
        3 => [0.0, 1.0 - f, 1.0],
        4 => [f, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0 - f],
    }
}

/// The two bands around hue `h` in [0, 6) and their triangular weights (sum 1).
fn band_weights(h: f32) -> [(usize, f32); 2] {
    let i = CENTERS[1..].iter().position(|&c| h < c).unwrap_or(7);
    let t = (h - CENTERS[i]) / (CENTERS[i + 1] - CENTERS[i]);
    [(i, 1.0 - t), ((i + 1) % 8, t)]
}

/// HSL amounts as fractions (-1..1), prepared once per render.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct HslPlan {
    hue: [f32; 8],
    sat: [f32; 8],
    lum: [f32; 8],
}

impl HslPlan {
    /// `None` when all 24 values are zero (stage inactive).
    pub(super) fn new(h: &Hsl) -> Option<Self> {
        let active = h
            .hue
            .iter()
            .chain(&h.saturation)
            .chain(&h.luminance)
            .any(|&v| v != 0.0);
        if !active {
            return None;
        }
        Some(Self {
            hue: h.hue.map(|v| v / 100.0),
            sat: h.saturation.map(|v| v / 100.0),
            lum: h.luminance.map(|v| v / 100.0),
        })
    }

    /// Hue rotation, then saturation, then luminance, each weighted by band and chroma.
    pub(super) fn apply(&self, px: [f32; 3]) -> [f32; 3] {
        let Some((h, mn, c)) = hue_sextant(px) else {
            return px;
        };
        let w = smoothstep(0.0, FULL_CHROMA, c);
        let [(i, wi), (j, wj)] = band_weights(h);
        let mix = |band: &[f32; 8]| (band[i] * wi + band[j] * wj) * w;
        let (dh, ds, dl) = (mix(&self.hue), mix(&self.sat), mix(&self.lum));
        let mut out = px;
        if dh != 0.0 {
            out = hexagon(wrap_sextant(h + HUE_SHIFT * dh)).map(|v| mn + c * v);
        }
        if ds != 0.0 {
            let y = luma(out);
            let k = (1.0 + ds).max(0.0);
            out = out.map(|x| y + (x - y) * k);
        }
        if dl != 0.0 {
            let g = 1.0 + LUM_GAIN * dl;
            out = out.map(|x| x * g);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIVID_RED: [f32; 3] = [0.8, 0.2, 0.15];
    const BLUE: [f32; 3] = [0.1, 0.2, 0.9];

    fn plan(f: impl FnOnce(&mut Hsl)) -> HslPlan {
        let mut h = Hsl::default();
        f(&mut h);
        HslPlan::new(&h).unwrap()
    }

    fn chroma(p: [f32; 3]) -> f32 {
        p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])
    }

    fn bits(p: [f32; 3]) -> [u32; 3] {
        p.map(f32::to_bits)
    }

    #[test]
    fn hexagon_inverts_hue_sextant() {
        let colors = [
            VIVID_RED,
            BLUE,
            [0.2, 0.7, 0.4],
            [0.9, 0.1, 0.6],
            [0.5, 0.45, 0.4],
            [1.4, 0.2, -0.1],
            [0.3, 0.9, 0.9],
        ];
        for px in colors {
            let (h, mn, c) = hue_sextant(px).unwrap();
            assert!((0.0..6.0).contains(&h), "{px:?} h={h}");
            let back = hexagon(h).map(|v| mn + c * v);
            for (b, p) in back.iter().zip(px) {
                assert!((b - p).abs() <= 1e-6, "{px:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn achromatic_pixels_have_no_hue() {
        for v in [0.0, 0.4, 1.0, 1.3, -0.2] {
            assert_eq!(hue_sextant([v; 3]), None);
        }
    }

    #[test]
    fn band_weights_partition_unity() {
        for i in 0..600 {
            let h = i as f32 / 100.0;
            let [(_, a), (_, b)] = band_weights(h);
            assert!((a + b - 1.0).abs() <= 1e-6 && a >= 0.0 && b >= 0.0, "{h}");
        }
    }

    /// Expected values written from the spec, independent of `CENTERS` (review Y3).
    #[test]
    fn band_centers_and_midpoints_match_the_spec() {
        const R: usize = 0;
        const O: usize = 1;
        const Y: usize = 2;
        const G: usize = 3;
        const A: usize = 4;
        const B: usize = 5;
        const P: usize = 6;
        const M: usize = 7;
        let weight = |deg: f32, band: usize| {
            band_weights(deg / 60.0)
                .iter()
                .filter(|(i, _)| *i == band)
                .map(|(_, w)| w)
                .sum::<f32>()
        };
        for (deg, band) in [
            (0.0, R),
            (30.0, O),
            (60.0, Y),
            (120.0, G),
            (180.0, A),
            (240.0, B),
            (270.0, P),
            (300.0, M),
        ] {
            assert!((weight(deg, band) - 1.0).abs() <= 1e-6, "{deg} deg");
        }
        for (deg, lo, hi) in [
            (15.0, R, O),
            (45.0, O, Y),
            (90.0, Y, G),
            (150.0, G, A),
            (210.0, A, B),
            (255.0, B, P),
            (285.0, P, M),
            (330.0, M, R),
        ] {
            assert!((weight(deg, lo) - 0.5).abs() <= 1e-6, "{deg} deg band {lo}");
            assert!((weight(deg, hi) - 0.5).abs() <= 1e-6, "{deg} deg band {hi}");
        }
    }

    #[test]
    fn wrap_sextant_edges() {
        assert_eq!(wrap_sextant(0.0), 0.0);
        assert_eq!(wrap_sextant(6.0), 0.0);
        assert_eq!(wrap_sextant(5.999_999), 5.999_999);
        assert!((wrap_sextant(6.000_001) - 0.000_001).abs() <= 1e-6);
        assert_eq!(wrap_sextant(-0.25), 5.75);
        // Precondition about f32 arithmetic, not an engine check: -1e-8 + 6.0 rounds to
        // exactly 6.0, so the second check in `wrap_sextant` must bring it to 0.
        assert_eq!(-1e-8f32 + 6.0, 6.0);
        assert_eq!(wrap_sextant(-1e-8), 0.0);
        for i in -600..1200 {
            let h = wrap_sextant(i as f32 / 100.0);
            assert!((0.0..6.0).contains(&h), "{i}: {h}");
        }
    }

    #[test]
    fn default_hsl_is_inactive() {
        assert!(HslPlan::new(&Hsl::default()).is_none());
    }

    #[test]
    fn achromatic_pixels_are_untouched_bitwise() {
        let p = plan(|h| {
            h.hue = [100.0; 8];
            h.saturation = [-100.0; 8];
            h.luminance = [100.0; 8];
        });
        for v in [0.0, 0.5, 1.0, 1.3] {
            assert_eq!(bits(p.apply([v; 3])), bits([v; 3]));
        }
    }

    #[test]
    fn untargeted_band_is_untouched_bitwise() {
        let p = plan(|h| h.saturation[0] = 50.0);
        assert_eq!(bits(p.apply(BLUE)), bits(BLUE));
        assert_ne!(bits(p.apply(VIVID_RED)), bits(VIVID_RED));
    }

    #[test]
    fn red_saturation_up_increases_red_chroma() {
        assert!(chroma(plan(|h| h.saturation[0] = 100.0).apply(VIVID_RED)) > chroma(VIVID_RED));
    }

    #[test]
    fn red_hue_up_moves_red_toward_orange() {
        let (h0, ..) = hue_sextant(VIVID_RED).unwrap();
        let (h1, ..) = hue_sextant(plan(|h| h.hue[0] = 100.0).apply(VIVID_RED)).unwrap();
        assert!(h1 > h0, "{h0} -> {h1}");
    }

    #[test]
    fn red_hue_down_wraps_toward_magenta() {
        let (h1, ..) = hue_sextant(plan(|h| h.hue[0] = -100.0).apply(VIVID_RED)).unwrap();
        assert!(h1 > 5.0, "{h1}");
    }

    #[test]
    fn blue_luminance_down_darkens_blue_and_spares_red() {
        let p = plan(|h| h.luminance[5] = -100.0);
        assert!(luma(p.apply(BLUE)) < luma(BLUE));
        assert_eq!(bits(p.apply(VIVID_RED)), bits(VIVID_RED));
    }

    #[test]
    fn dull_pixels_change_less_than_vivid_ones() {
        let p = plan(|h| h.saturation[0] = 100.0);
        let dull = [0.5, 0.47, 0.465];
        let gain = |px: [f32; 3]| chroma(p.apply(px)) / chroma(px);
        assert!(
            gain(dull) < gain(VIVID_RED),
            "{} {}",
            gain(dull),
            gain(VIVID_RED)
        );
    }

    #[test]
    fn minus_100_saturation_on_a_full_weight_pixel_is_gray() {
        let out = plan(|h| h.saturation = [-100.0; 8]).apply(VIVID_RED);
        assert!(chroma(out) <= 1e-6, "{out:?}");
    }

    /// D3 effect sizes, hand-computed (review Z2): hue +-100 = +-30 degrees = half a sextant,
    /// luminance +-100 = x(1 +- 0.4). The pixel has hue 0 and chroma 0.6, so the red band and
    /// the chroma ramp both weigh 1.
    #[test]
    fn red_band_effect_sizes_match_d3() {
        let px = [0.8, 0.2, 0.2];
        let near = |out: [f32; 3], expected: [f32; 3]| {
            for (o, e) in out.iter().zip(expected) {
                assert!((o - e).abs() <= 1e-6, "{out:?} vs {expected:?}");
            }
        };
        near(plan(|h| h.hue[0] = 100.0).apply(px), [0.8, 0.5, 0.2]);
        near(plan(|h| h.hue[0] = -100.0).apply(px), [0.8, 0.2, 0.5]);
        near(
            plan(|h| h.luminance[0] = 100.0).apply(px),
            [1.12, 0.28, 0.28],
        );
        near(
            plan(|h| h.luminance[0] = -100.0).apply(px),
            [0.48, 0.12, 0.12],
        );
    }

    #[test]
    fn extremes_stay_finite() {
        let pixels = [
            VIVID_RED,
            BLUE,
            [4.7, 0.3, -0.2],
            [1e-3, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        for v in [-100.0, 100.0] {
            let p = plan(|h| {
                h.hue = [v; 8];
                h.saturation = [v; 8];
                h.luminance = [v; 8];
            });
            for px in pixels {
                let out = p.apply(px);
                assert!(out.iter().all(|x| x.is_finite()), "{v} {px:?} {out:?}");
            }
        }
    }
}
