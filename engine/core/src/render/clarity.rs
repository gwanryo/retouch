//! Texture and clarity (stage 6): local luma contrast.
//!
//! Both terms read the SAME luma plane `Y` of the stage-5 output, computed once before either
//! term, and add one detail value `d` to all three channels, so luminance contrast changes while
//! color differences are preserved.

use super::blur::{box_blur3, Plane};
use crate::recipe::Basic;

/// Box radius of the texture blur (fine detail).
pub(crate) const TEXTURE_RADIUS: usize = 4;
/// Texture gain at slider 100.
pub(crate) const TEXTURE_GAIN: f32 = 0.75;
/// Box radius of the clarity blur (local contrast).
pub(crate) const CLARITY_RADIUS: usize = 24;
/// Clarity gain at slider 100.
pub(crate) const CLARITY_GAIN: f32 = 0.8;

/// Active local-contrast terms, prepared once per render. Values are the sliders divided by 100;
/// the gain constants are multiplied in [`LocalContrast::apply`].
#[derive(Debug, PartialEq)]
pub(crate) struct LocalContrast {
    texture: Option<f32>,
    clarity: Option<f32>,
}

impl LocalContrast {
    /// `None` when both texture and clarity are 0.
    pub(crate) fn new(b: &Basic) -> Option<Self> {
        let lc = Self {
            texture: (b.texture != 0.0).then(|| b.texture / 100.0),
            clarity: (b.clarity != 0.0).then(|| b.clarity / 100.0),
        };
        (lc.texture.is_some() || lc.clarity.is_some()).then_some(lc)
    }

    /// Adds the detail `Y - B` of each active term to all three channels of the interleaved RGB
    /// buffer. The luma plane is computed once and both terms read it. Multiplications associate
    /// left to right exactly as written: that order is part of the bit contract.
    pub(crate) fn apply(&self, rgb: &mut [f32], width: usize, height: usize) {
        let y = Plane::luma_of(rgb, width, height);
        let blur_t = self.texture.map(|_| box_blur3(&y, TEXTURE_RADIUS));
        let blur_c = self.clarity.map(|_| box_blur3(&y, CLARITY_RADIUS));
        for (i, px) in rgb.chunks_exact_mut(3).take(width * height).enumerate() {
            let yi = y.data[i];
            let mut d = 0.0f32;
            if let (Some(t), Some(bt)) = (self.texture, &blur_t) {
                d += t * TEXTURE_GAIN * (yi - bt.data[i]);
            }
            if let (Some(c), Some(bc)) = (self.clarity, &blur_c) {
                let yc = yi.clamp(0.0, 1.0);
                d += c * CLARITY_GAIN * 4.0 * yc * (1.0 - yc) * (yi - bc.data[i]);
            }
            for v in px {
                *v += d;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(width: usize, height: usize, v: f32) -> Vec<f32> {
        vec![v; width * height * 3]
    }

    #[test]
    fn local_contrast_leaves_a_flat_image_bit_identical() {
        let lc = LocalContrast {
            texture: Some(1.0),
            clarity: Some(-1.0),
        };
        let mut rgb = gray(30, 20, 0.42);
        let before = rgb.clone();
        lc.apply(&mut rgb, 30, 20);
        assert_eq!(rgb, before); // Y == B exactly, so d == 0
    }

    /// Vertical step at x = 32 (0.3 | 0.7), 64x8.
    fn step() -> Vec<f32> {
        (0..8)
            .flat_map(|_| (0..64).flat_map(|x| [if x < 32 { 0.3 } else { 0.7 }; 3]))
            .collect()
    }

    #[test]
    fn positive_clarity_increases_contrast_across_an_edge() {
        let mut rgb = step();
        LocalContrast {
            texture: None,
            clarity: Some(0.6),
        }
        .apply(&mut rgb, 64, 8);
        let at = |x: usize| rgb[(4 * 64 + x) * 3];
        assert!(at(28) < 0.3 && at(36) > 0.7, "{} {}", at(28), at(36));
    }

    #[test]
    fn negative_clarity_softens_an_edge() {
        let mut rgb = step();
        LocalContrast {
            texture: None,
            clarity: Some(-0.6),
        }
        .apply(&mut rgb, 64, 8);
        let at = |x: usize| rgb[(4 * 64 + x) * 3];
        assert!(at(31) > 0.3 && at(32) < 0.7);
    }

    #[test]
    fn positive_texture_amplifies_fine_detail() {
        // period-4 stripes 0.45/0.55, 32x4
        let mut rgb: Vec<f32> = (0..4)
            .flat_map(|_| (0..32).flat_map(|x| [if x % 4 < 2 { 0.45 } else { 0.55 }; 3]))
            .collect();
        LocalContrast {
            texture: Some(0.8),
            clarity: None,
        }
        .apply(&mut rgb, 32, 4);
        let row: Vec<f32> = (8..24).map(|x| rgb[(32 + x) * 3]).collect();
        let amp = row.iter().copied().fold(f32::MIN, f32::max)
            - row.iter().copied().fold(f32::MAX, f32::min);
        assert!(amp > 0.1 + 1e-3, "{amp}");
    }

    #[test]
    fn local_contrast_adds_the_same_delta_to_every_channel() {
        let mut rgb: Vec<f32> = (0..64 * 8)
            .flat_map(|i| {
                let v = if i % 64 < 32 { 0.3 } else { 0.7 };
                [v + 0.1, v, v - 0.1]
            })
            .collect();
        let before = rgb.clone();
        LocalContrast {
            texture: Some(0.5),
            clarity: Some(0.5),
        }
        .apply(&mut rgb, 64, 8);
        for (a, b) in rgb.chunks_exact(3).zip(before.chunks_exact(3)) {
            let (dr, dg, db) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
            assert!((dr - dg).abs() < 1e-6 && (dg - db).abs() < 1e-6);
        }
    }

    #[test]
    fn new_reads_each_field_from_its_own_slider() {
        let b = |t: f32, c: f32| Basic {
            texture: t,
            clarity: c,
            ..Basic::default()
        };
        assert_eq!(
            LocalContrast::new(&b(50.0, 0.0)),
            Some(LocalContrast {
                texture: Some(0.5),
                clarity: None
            })
        );
        assert_eq!(
            LocalContrast::new(&b(0.0, -30.0)),
            Some(LocalContrast {
                texture: None,
                clarity: Some(-0.3)
            })
        );
    }

    /// S2: texture and clarity both read the SAME stage-5 luma (computed once, before either term),
    /// checked against an independent computation; recomputing luma between them gives other bits.
    #[test]
    fn texture_and_clarity_share_the_stage5_luma() {
        let orig = step();
        let lc = LocalContrast::new(&Basic {
            texture: 50.0,
            clarity: 50.0,
            ..Basic::default()
        })
        .unwrap();
        let y = Plane::luma_of(&orig, 64, 8);
        let (bt, bc) = (box_blur3(&y, TEXTURE_RADIUS), box_blur3(&y, CLARITY_RADIUS));
        let mut expected = orig.clone();
        for i in 0..64 * 8 {
            let yi = y.data[i];
            let yc = yi.clamp(0.0, 1.0);
            let mut d = 0.0f32;
            d += 0.5 * TEXTURE_GAIN * (yi - bt.data[i]);
            d += 0.5 * CLARITY_GAIN * 4.0 * yc * (1.0 - yc) * (yi - bc.data[i]);
            for c in 0..3 {
                expected[i * 3 + c] += d;
            }
        }
        let mut got = orig.clone();
        lc.apply(&mut got, 64, 8);
        assert_eq!(
            got.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        let mut sequential = orig;
        LocalContrast {
            texture: Some(0.5),
            clarity: None,
        }
        .apply(&mut sequential, 64, 8);
        LocalContrast {
            texture: None,
            clarity: Some(0.5),
        }
        .apply(&mut sequential, 64, 8);
        assert_ne!(sequential, got);
    }

    #[test]
    fn new_is_none_when_both_are_zero() {
        assert!(LocalContrast::new(&Basic::default()).is_none());
    }
}
