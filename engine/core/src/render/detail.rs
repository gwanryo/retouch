//! Detail stages after the global spatial ones: sharpening (8a), vignette (8b), grain (8c).
//!
//! Unsharp mask on luma only: `D = Y - B` is added to all three channels, damped against halos
//! and optionally weighted by blurred edge strength. Operation order is part of the bit contract.

use super::basic::{luma, smoothstep};
use super::blur::{gaussian_blur, gaussian_kernel, Plane};
use crate::recipe::Detail;

/// Sharpening gain at slider 100.
pub(crate) const SHARPEN_GAIN: f32 = 1.0;
/// Halo damping strength at `sharpen_detail == 0`.
pub(crate) const HALO_DAMP: f32 = 20.0;
/// Edge-strength threshold at `sharpen_masking == 100`.
pub(crate) const MASK_EDGE: f32 = 0.2;
/// Vignette gain at `|amount| == 100` and full weight.
pub(crate) const VIGNETTE_GAIN: f32 = 0.8;
/// Grain amplitude at slider 100, before the tone weighting.
pub(crate) const GRAIN_GAIN: f32 = 0.25;
/// Seed offset of the second (finer) grain octave.
const GRAIN_OCTAVE_SEED: u32 = 0x68E3_1DA4;

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
/// Active vignette, prepared once per render.
#[derive(Debug, PartialEq)]
pub(crate) struct Vignette {
    /// `VIGNETTE_GAIN * amount / 100` (negative darkens).
    gain: f32,
    /// Radius where the transition is halfway.
    center: f32,
    /// Half the transition width.
    half_width: f32,
}

impl Vignette {
    /// `None` when the amount is 0.
    pub(crate) fn new(d: &Detail) -> Option<Self> {
        if d.vignette_amount == 0.0 {
            return None;
        }
        Some(Self {
            gain: VIGNETTE_GAIN * (d.vignette_amount / 100.0),
            center: 0.3 + 0.6 * d.vignette_midpoint / 100.0,
            half_width: 0.05 + 0.45 * d.vignette_feather / 100.0,
        })
    }

    /// Radial weight in [0, 1] of pixel `(x, y)`: the radius is normalized per axis, so the
    /// falloff is an ellipse following the frame aspect.
    pub(crate) fn weight(&self, x: usize, y: usize, width: usize, height: usize) -> f32 {
        let u = ((x as f32 + 0.5) / width as f32) * 2.0 - 1.0;
        let v = ((y as f32 + 0.5) / height as f32) * 2.0 - 1.0;
        let r = ((u * u + v * v) * 0.5).sqrt();
        smoothstep(
            self.center - self.half_width,
            self.center + self.half_width,
            r,
        )
    }

    /// Multiplies all three channels by `1 + gain * weight`.
    pub(crate) fn apply_px(&self, px: [f32; 3], weight: f32) -> [f32; 3] {
        let g = 1.0 + self.gain * weight;
        px.map(|v| v * g)
    }
}

/// Integer hash of a lattice point (the grain's only source of randomness).
pub(crate) fn grain_hash(seed: u32, i: u32, j: u32) -> u32 {
    let mut x = seed ^ i.wrapping_mul(0x8DA6_B343) ^ j.wrapping_mul(0xD816_3841);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// Lattice value: an exact 24-bit fraction in [-0.5, 0.5).
fn lattice(seed: u32, i: u32, j: u32) -> f32 {
    (grain_hash(seed, i, j) >> 8) as f32 / 16_777_216.0 - 0.5
}

/// Smooth value noise at `(px, py)`, both positive.
fn value_noise(seed: u32, px: f32, py: f32) -> f32 {
    let (fx, fy) = (px.floor(), py.floor());
    let (i, j) = (fx as u32, fy as u32);
    let (tx, ty) = (px - fx, py - fy);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (a, b) = (lattice(seed, i, j), lattice(seed, i + 1, j));
    let (c, d) = (lattice(seed, i, j + 1), lattice(seed, i + 1, j + 1));
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

/// Active grain, prepared once per render.
#[derive(Debug, PartialEq)]
pub(crate) struct Grain {
    /// `GRAIN_GAIN * amount / 100`.
    amount: f32,
    /// Noise cell size in pixels, `1 + 3 * size / 100`.
    cell: f32,
    /// Weight `q = frequency / 100` of the finer octave (it enters as `0.5 * q`).
    rough: f32,
}

impl Grain {
    /// `None` when the amount is 0.
    pub(crate) fn new(d: &Detail) -> Option<Self> {
        if d.grain_amount == 0.0 {
            return None;
        }
        Some(Self {
            amount: GRAIN_GAIN * (d.grain_amount / 100.0),
            cell: 1.0 + 3.0 * d.grain_size / 100.0,
            rough: d.grain_frequency / 100.0,
        })
    }

    /// Two-octave value noise of pixel `(x, y)`, roughly in [-0.5, 0.5].
    pub(crate) fn noise(&self, seed: u32, x: usize, y: usize) -> f32 {
        let p = ((x as f32 + 0.5) / self.cell, (y as f32 + 0.5) / self.cell);
        let n1 = value_noise(seed, p.0, p.1);
        let n2 = value_noise(seed ^ GRAIN_OCTAVE_SEED, 2.0 * p.0, 2.0 * p.1);
        n1 * (1.0 - 0.5 * self.rough) + n2 * (0.5 * self.rough)
    }

    /// Adds monochrome grain, strongest in the midtones (weight 1 at luma 0.5, 0.25 at the ends).
    pub(crate) fn apply_px(&self, px: [f32; 3], noise: f32) -> [f32; 3] {
        let y = luma(px).clamp(0.0, 1.0);
        let delta = self.amount * noise * (0.25 + 0.75 * 4.0 * y * (1.0 - y));
        px.map(|v| v + delta)
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

    const GRAIN_HASH_1_0_0: u32 = 0x6889_90C0;
    const GRAIN_HASH_7_1023_511: u32 = 0xDCB6_1702;

    #[test]
    fn grain_hash_is_pinned() {
        // Golden values of the integer hash: any change here is an ENGINE_VERSION change.
        // (0, 0, 0) hashes to 0 (a fixed point), so it is not pinned.
        assert_eq!(grain_hash(1, 0, 0), GRAIN_HASH_1_0_0);
        assert_eq!(grain_hash(7, 1023, 511), GRAIN_HASH_7_1023_511);
        assert_ne!(grain_hash(1, 0, 0), grain_hash(0, 0, 0));
    }

    #[test]
    fn lattice_values_cover_minus_half_to_half() {
        let v: Vec<f32> = (0..4096).map(|i| lattice(3, i, i / 7)).collect();
        assert!(v.iter().all(|&x| (-0.5..0.5).contains(&x)));
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        assert!(mean.abs() < 0.02, "{mean}");
    }

    #[test]
    fn value_noise_hits_lattice_values_at_integer_points() {
        assert_eq!(value_noise(5, 3.0, 9.0), lattice(5, 3, 9));
    }

    #[test]
    fn grain_is_reproducible_and_seed_dependent() {
        let g = Grain::new(&detail(|d| {
            d.grain_amount = 60.0;
        }))
        .unwrap();
        let field = |seed| {
            (0..200)
                .map(|i| g.noise(seed, i % 20, i / 20))
                .collect::<Vec<_>>()
        };
        assert_eq!(field(11), field(11));
        assert_ne!(field(11), field(12));
    }

    #[test]
    fn grain_has_near_zero_mean_and_scales_with_amount() {
        let mean_abs = |amt: f32| {
            let g = Grain::new(&detail(|d| d.grain_amount = amt)).unwrap();
            let (mut s, mut a) = (0.0f64, 0.0f64);
            for y in 0..64 {
                for x in 0..64 {
                    let d = f64::from(g.apply_px([0.5; 3], g.noise(1, x, y))[0] - 0.5);
                    s += d;
                    a += d.abs();
                }
            }
            (s / 4096.0, a / 4096.0)
        };
        let (m50, a50) = mean_abs(50.0);
        let (_, a100) = mean_abs(100.0);
        assert!(m50.abs() < 0.2 * a50, "{m50} {a50}");
        assert!((a100 / a50 - 2.0).abs() < 1e-3);
    }

    #[test]
    fn bigger_grain_size_is_smoother() {
        let rough = |size: f32| {
            let g = Grain::new(&detail(|d| {
                d.grain_amount = 100.0;
                d.grain_size = size;
            }))
            .unwrap();
            (0..63)
                .map(|x| (g.noise(2, x + 1, 5) - g.noise(2, x, 5)).abs())
                .sum::<f32>()
        };
        assert!(rough(100.0) < rough(0.0) * 0.6);
    }

    #[test]
    fn frequency_mixes_the_two_octaves_as_specified() {
        for q in [0.0f32, 1.0] {
            let g = Grain::new(&detail(|d| {
                d.grain_amount = 50.0;
                d.grain_frequency = q * 100.0;
            }))
            .unwrap();
            let (x, y) = (13usize, 7usize);
            let p = ((x as f32 + 0.5) / g.cell, (y as f32 + 0.5) / g.cell);
            let n1 = value_noise(4, p.0, p.1);
            let n2 = value_noise(4 ^ 0x68E3_1DA4, 2.0 * p.0, 2.0 * p.1);
            assert_eq!(
                g.noise(4, x, y).to_bits(),
                (n1 * (1.0 - 0.5 * q) + n2 * (0.5 * q)).to_bits()
            );
        }
    }

    #[test]
    fn feather_sets_the_transition_width() {
        // (74, 74) on 100x100: u = v = 0.49, r = 0.49. midpoint 50 -> center 0.6.
        let w = |f: f32| {
            Vignette::new(&detail(|d| {
                d.vignette_amount = -50.0;
                d.vignette_feather = f;
            }))
            .unwrap()
            .weight(74, 74, 100, 100)
        };
        assert_eq!(w(0.0), 0.0); // transition [0.55, 0.65]
        assert!(w(100.0) > 0.2); // transition [0.1, 1.1]
    }

    #[test]
    fn vignette_darkens_corners_and_keeps_the_center() {
        let v = Vignette::new(&detail(|d| d.vignette_amount = -100.0)).unwrap();
        assert_eq!(v.weight(50, 50, 101, 101), 0.0);
        assert_eq!(v.apply_px([0.5; 3], v.weight(50, 50, 101, 101)), [0.5; 3]);
        let corner = v.apply_px([0.5; 3], v.weight(0, 0, 101, 101));
        assert!(corner[0] < 0.2, "{corner:?}"); // w~1 at r~0.99: 0.5*(1-0.8*w)
    }

    #[test]
    fn positive_vignette_brightens_corners() {
        let v = Vignette::new(&detail(|d| d.vignette_amount = 60.0)).unwrap();
        assert!(v.apply_px([0.5; 3], v.weight(0, 0, 64, 48))[0] > 0.5);
    }

    #[test]
    fn higher_midpoint_shrinks_the_vignette() {
        let w = |m: f32| {
            Vignette::new(&detail(|d| {
                d.vignette_amount = -50.0;
                d.vignette_midpoint = m;
            }))
            .unwrap()
            .weight(16, 12, 64, 48)
        };
        assert!(w(90.0) < w(10.0));
    }

    #[test]
    fn vignette_follows_the_frame_aspect() {
        // On a 2:1 frame the left edge middle (u~-0.995, v=0.01) and the top edge middle
        // (u=0.005, v~-0.99) are at the same normalized radius (~0.70) although their pixel
        // distances from the center differ 2x, so their weights match; the center is 0.
        let v = Vignette::new(&detail(|d| d.vignette_amount = -50.0)).unwrap();
        let (side, top) = (v.weight(0, 50, 200, 100), v.weight(100, 0, 200, 100));
        assert!(side > 0.5 && top > 0.5, "{side} {top}");
        assert!((side - top).abs() < 0.05, "{side} {top}");
        assert_eq!(v.weight(100, 50, 200, 100), 0.0);
    }
}
