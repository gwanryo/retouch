//! Detail stages after the global spatial ones: sharpening (8a).
//!
//! Unsharp mask on luma only: `D = Y - B` is added to all three channels, damped against halos
//! and optionally weighted by blurred edge strength. Operation order is part of the bit contract.

use super::basic::smoothstep;
use super::blur::{gaussian_blur, gaussian_kernel, Plane};
use crate::recipe::Detail;

/// Sharpening gain at slider 100.
pub(crate) const SHARPEN_GAIN: f32 = 1.0;
/// Halo damping strength at `sharpen_detail == 0`.
pub(crate) const HALO_DAMP: f32 = 20.0;
/// Edge-strength threshold at `sharpen_masking == 100`.
pub(crate) const MASK_EDGE: f32 = 0.2;

/// Active sharpening, prepared once per render.
#[derive(Debug, PartialEq)]
pub(crate) struct Sharpen {
    /// `SHARPEN_GAIN * amount / 100`.
    amount: f32,
    /// Halo damping `k = HALO_DAMP * (1 - detail / 100)`.
    halo: f32,
    /// Masking threshold `t`; `None` when masking is off (`t` is not positive).
    edge: Option<f32>,
    /// Gaussian kernel for the blur `B`.
    kernel: Vec<f32>,
}

impl Sharpen {
    /// `None` when the amount is 0, whatever the other fields say.
    pub(crate) fn new(d: &Detail) -> Option<Self> {
        if d.sharpen_amount == 0.0 {
            return None;
        }
        // `t` first: a subnormal masking underflows it to 0, and `smoothstep(0, 0, g)` is 0/0.
        let t = MASK_EDGE * d.sharpen_masking / 100.0;
        Some(Self {
            amount: SHARPEN_GAIN * d.sharpen_amount / 100.0,
            halo: HALO_DAMP * (1.0 - d.sharpen_detail / 100.0),
            edge: (t > 0.0).then_some(t),
            kernel: gaussian_kernel(d.sharpen_radius),
        })
    }

    /// Sharpens the interleaved RGB buffer in place.
    pub(crate) fn apply(&self, rgb: &mut [f32], width: usize, height: usize) {
        let y = Plane::luma_of(rgb, width, height);
        let b = gaussian_blur(&y, &self.kernel);
        let at = |x: usize, row: usize| b.data[row * width + x];
        for (i, px) in rgb.chunks_exact_mut(3).take(width * height).enumerate() {
            let (x, row) = (i % width, i / width);
            let d = y.data[i] - b.data[i];
            let damped = d / (1.0 + self.halo * d.abs());
            let m = match self.edge {
                Some(t) => {
                    let g = (at((x + 1).min(width - 1), row) - at(x.saturating_sub(1), row)).abs()
                        + (at(x, (row + 1).min(height - 1)) - at(x, row.saturating_sub(1))).abs();
                    smoothstep(0.5 * t, t, g)
                }
                None => 1.0,
            };
            let delta = self.amount * m * damped;
            for v in px {
                *v += delta;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn detail(f: impl FnOnce(&mut Detail)) -> Detail {
        let mut d = Detail::default();
        f(&mut d);
        d
    }

    fn step() -> Vec<f32> {
        (0..8)
            .flat_map(|_| (0..64).flat_map(|x| [if x < 32 { 0.3 } else { 0.7 }; 3]))
            .collect()
    }

    fn at(rgb: &[f32], x: usize) -> f32 {
        rgb[(4 * 64 + x) * 3]
    }

    #[test]
    fn sharpen_overshoots_both_sides_of_an_edge() {
        let mut rgb = step();
        Sharpen::new(&detail(|d| {
            d.sharpen_amount = 100.0;
            d.sharpen_detail = 100.0;
        }))
        .unwrap()
        .apply(&mut rgb, 64, 8);
        assert!(at(&rgb, 31) < 0.3 && at(&rgb, 32) > 0.7);
    }

    #[test]
    fn low_detail_suppresses_the_halo() {
        let run = |det: f32| {
            let mut rgb = step();
            Sharpen::new(&detail(|d| {
                d.sharpen_amount = 100.0;
                d.sharpen_detail = det;
            }))
            .unwrap()
            .apply(&mut rgb, 64, 8);
            at(&rgb, 32) - 0.7
        };
        assert!(run(0.0) < run(100.0) * 0.5);
    }

    #[test]
    fn masking_skips_low_contrast_texture_but_keeps_edges() {
        // left half: +-0.01 checker around 0.5 (flat), right: the step
        let flat = |rgb: &[f32]| rgb[(4 * 64 + 8) * 3];
        let mut src: Vec<f32> = step();
        for y in 0..8 {
            for x in 0..16 {
                let v = if (x + y) % 2 == 0 { 0.51 } else { 0.49 };
                for c in 0..3 {
                    src[(y * 64 + x) * 3 + c] = v;
                }
            }
        }
        let run = |m: f32| {
            let mut rgb = src.clone();
            Sharpen::new(&detail(|d| {
                d.sharpen_amount = 150.0;
                d.sharpen_detail = 100.0;
                d.sharpen_masking = m;
            }))
            .unwrap()
            .apply(&mut rgb, 64, 8);
            rgb
        };
        let (off, on) = (run(0.0), run(100.0));
        assert!((flat(&on) - 0.51).abs() < (flat(&off) - 0.51).abs() * 0.1);
        assert!(at(&on, 32) > 0.7 + 0.01);
    }

    #[test]
    fn sharpen_barely_changes_a_flat_image() {
        let mut rgb = vec![0.6; 20 * 10 * 3];
        Sharpen::new(&detail(|d| {
            d.sharpen_amount = 150.0;
            d.sharpen_radius = 3.0;
        }))
        .unwrap()
        .apply(&mut rgb, 20, 10);
        assert!(rgb.iter().all(|&v| (v - 0.6).abs() < 1e-6));
    }

    #[test]
    fn subnormal_masking_is_treated_as_off() {
        let s = Sharpen::new(&detail(|d| {
            d.sharpen_amount = 100.0;
            d.sharpen_masking = 1e-44;
        }))
        .unwrap();
        assert!(s.edge.is_none());
        let mut rgb = vec![0.0; 16 * 8 * 3]; // black: g == 0 everywhere
        s.apply(&mut rgb, 16, 8);
        assert!(rgb.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn radius_widens_the_sharpening_halo() {
        let reach = |radius: f32| {
            let s = Sharpen::new(&detail(|d| {
                d.sharpen_amount = 100.0;
                d.sharpen_detail = 100.0;
                d.sharpen_radius = radius;
            }))
            .unwrap();
            let mut rgb = step();
            s.apply(&mut rgb, 64, 8);
            (0..32)
                .filter(|&x| (at(&rgb, x) - 0.3).abs() > 1e-4)
                .count()
        };
        let kernel_len = |radius: f32| {
            Sharpen::new(&detail(|d| {
                d.sharpen_amount = 1.0;
                d.sharpen_radius = radius;
            }))
            .unwrap()
            .kernel
            .len()
        };
        assert_eq!(kernel_len(0.5), 5);
        assert_eq!(kernel_len(3.0), 19);
        assert!(
            reach(0.5) <= 2 && reach(3.0) >= 6,
            "{} {}",
            reach(0.5),
            reach(3.0)
        );
    }

    #[test]
    fn sharpen_new_is_none_at_zero_amount_whatever_the_other_fields() {
        assert!(Sharpen::new(&detail(|d| {
            d.sharpen_radius = 3.0;
            d.sharpen_masking = 80.0;
        }))
        .is_none());
    }
}
