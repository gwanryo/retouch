//! Basic panel math. These are OUR definitions, direction-compatible with Lightroom but not
//! identical (spec D8, AC-E3b). Two stages: linear-light ops, then encoded-domain ops.
//! Nothing here clamps (master plan A3): values above 1 or below 0 flow to the next stage and
//! only `buffer::quantize` clamps. The WebGL2 preview must port these formulas verbatim (A9).

use crate::recipe::Basic;

/// Rec.709 luma of an encoded pixel (a tonal weight, not a colorimetric quantity).
pub fn luma(px: [f32; 3]) -> f32 {
    0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear stage: exposure, gain `2^EV`.
pub fn apply_exposure_linear(px: [f32; 3], ev: f32) -> [f32; 3] {
    let g = libm::exp2f(ev);
    px.map(|x| x * g)
}

/// Linear stage: relative white balance. +temperature = warmer (more R, less B),
/// +tint = more magenta (less G). ±100 maps to ±20% channel gain.
pub fn apply_white_balance_linear(px: [f32; 3], temperature: f32, tint: f32) -> [f32; 3] {
    let t = temperature / 100.0;
    let n = tint / 100.0;
    [
        px[0] * (1.0 + 0.2 * t),
        px[1] * (1.0 - 0.2 * n),
        px[2] * (1.0 - 0.2 * t),
    ]
}

/// Encoded stage: contrast around 0.5, then luma-weighted highlights / shadows / whites /
/// blacks. Highlights scale proportionally (`x * (1 + 0.25 * hi * w)`), so values pushed above 1
/// by exposure stay distinguishable and `highlights < 0` can bring them back below 1.
pub fn apply_tone_encoded(px: [f32; 3], b: &Basic) -> [f32; 3] {
    let c = b.contrast / 100.0;
    let contrasted = px.map(|x| 0.5 + (x - 0.5) * (1.0 + 0.6 * c));

    let l = luma(contrasted);
    let w_hi = smoothstep(0.5, 1.0, l);
    let w_sh = 1.0 - smoothstep(0.0, 0.5, l);
    let hi = b.highlights / 100.0;
    let sh = b.shadows / 100.0;
    let wh = b.whites / 100.0;
    let bl = b.blacks / 100.0;
    let one_minus_l = 1.0 - l;

    contrasted.map(|x| {
        x + hi * 0.25 * w_hi * x
            + sh * 0.25 * w_sh * (1.0 - x)
            + wh * 0.15 * l * l
            + bl * 0.15 * one_minus_l * one_minus_l
    })
}

/// Encoded stage: saturation scales chroma around luma; vibrance does the same weighted toward
/// less saturated pixels. The vibrance weight `1 - spread` is clamped to [0, 1]: inputs are not
/// clamped (A3), so an over-exposed pixel can have `spread > 1`, and an unclamped weight would
/// make the chroma factor negative and invert the hue. With `s, v ∈ [-1, 1]` the factor
/// `k = (1 + s) * (1 + v * w)` is therefore always ≥ 0.
pub fn apply_color_encoded(px: [f32; 3], vibrance: f32, saturation: f32) -> [f32; 3] {
    let y = luma(px);
    let s = saturation / 100.0;
    let v = vibrance / 100.0;
    let mx = px[0].max(px[1]).max(px[2]);
    let mn = px[0].min(px[1]).min(px[2]);
    let w = (1.0 - (mx - mn)).clamp(0.0, 1.0);
    let k = (1.0 + s) * (1.0 + v * w);
    px.map(|x| y + (x - y) * k)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAY: [f32; 3] = [0.4, 0.4, 0.4];
    const WARM: [f32; 3] = [0.7, 0.5, 0.3];

    fn chroma(p: [f32; 3]) -> f32 {
        p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])
    }

    fn basic(f: impl FnOnce(&mut Basic)) -> Basic {
        let mut b = Basic::default();
        f(&mut b);
        b
    }

    #[test]
    fn plus_one_ev_doubles_linear_values() {
        assert_eq!(
            apply_exposure_linear([0.25, 0.1, 0.4], 1.0),
            [0.5, 0.2, 0.8]
        );
    }

    #[test]
    fn zero_ev_is_identity() {
        assert_eq!(apply_exposure_linear(WARM, 0.0), WARM);
    }

    #[test]
    fn positive_temperature_raises_red_over_blue() {
        let out = apply_white_balance_linear(GRAY, 50.0, 0.0);
        assert!(out[0] > GRAY[0] && out[2] < GRAY[2], "{out:?}");
    }

    #[test]
    fn positive_tint_lowers_green() {
        assert!(apply_white_balance_linear(GRAY, 0.0, 50.0)[1] < GRAY[1]);
    }

    #[test]
    fn tone_defaults_are_identity() {
        assert_eq!(apply_tone_encoded(WARM, &Basic::default()), WARM);
    }

    #[test]
    fn tone_defaults_keep_values_above_one() {
        assert_eq!(
            apply_tone_encoded([1.3, 1.2, 1.1], &Basic::default()),
            [1.3, 1.2, 1.1]
        );
    }

    #[test]
    fn positive_contrast_pushes_values_away_from_mid_gray() {
        assert!(apply_tone_encoded([0.7; 3], &basic(|b| b.contrast = 50.0))[0] > 0.7);
    }

    #[test]
    fn negative_highlights_darken_bright_pixels() {
        assert!(apply_tone_encoded([0.9; 3], &basic(|b| b.highlights = -100.0))[0] < 0.9);
    }

    #[test]
    fn negative_highlights_bring_1_2_below_1() {
        let out = apply_tone_encoded([1.2; 3], &basic(|b| b.highlights = -100.0));
        assert!(out[0] < 1.0, "{out:?}");
    }

    #[test]
    fn positive_shadows_lift_dark_pixels() {
        assert!(apply_tone_encoded([0.1; 3], &basic(|b| b.shadows = 100.0))[0] > 0.1);
    }

    #[test]
    fn negative_saturation_reduces_chroma() {
        assert!(chroma(apply_color_encoded(WARM, 0.0, -50.0)) < chroma(WARM));
    }

    #[test]
    fn minus_100_saturation_yields_neutral_gray() {
        let out = apply_color_encoded(WARM, 0.0, -100.0);
        assert!(
            (out[0] - out[1]).abs() < 1e-6 && (out[1] - out[2]).abs() < 1e-6,
            "{out:?}"
        );
    }

    #[test]
    fn negative_vibrance_never_inverts_an_over_range_pixel() {
        // spread 2.9 > 1: an unclamped weight would give k = 1 + (-1)(1 - 2.9) = 2.9 (fine),
        // while +100 would give k = 1 + (1 - 2.9) = -0.9 (inverted). Both must keep hue order.
        let hot = [3.0, 0.2, 0.1];
        for v in [-100.0, 100.0] {
            let out = apply_color_encoded(hot, v, 0.0);
            assert!(out[0] > out[1] && out[1] >= out[2], "vibrance {v}: {out:?}");
        }
    }

    #[test]
    fn vibrance_boosts_dull_pixels_more_than_vivid_ones() {
        let dull = [0.5, 0.45, 0.4];
        let vivid = [0.9, 0.3, 0.1];
        let gain_dull = chroma(apply_color_encoded(dull, 100.0, 0.0)) / chroma(dull);
        let gain_vivid = chroma(apply_color_encoded(vivid, 100.0, 0.0)) / chroma(vivid);
        assert!(
            gain_dull > gain_vivid,
            "dull {gain_dull} vivid {gain_vivid}"
        );
    }
}
