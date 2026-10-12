//! Spatial stages (6 onward), shared by both entry points. They need neighbours or position, so
//! they run on the whole unquantized RGB buffer after the per-pixel stages 1-5.

use super::clarity::LocalContrast;
use super::detail::{Grain, GrainRows, Sharpen, Vignette};
use crate::recipe::Recipe;

/// Active spatial features, prepared once per render.
pub(crate) struct SpatialOps {
    local: Option<LocalContrast>,
    sharpen: Option<Sharpen>,
    vignette: Option<Vignette>,
    grain: Option<Grain>,
}

impl SpatialOps {
    /// `None` when every spatial feature is inactive (the per-pixel byte path then runs).
    pub(crate) fn new(r: &Recipe) -> Option<Self> {
        let local = LocalContrast::new(&r.basic);
        let sharpen = Sharpen::new(&r.detail);
        let vignette = Vignette::new(&r.detail);
        let grain = Grain::new(&r.detail);
        (local.is_some() || sharpen.is_some() || vignette.is_some() || grain.is_some()).then_some(
            Self {
                local,
                sharpen,
                vignette,
                grain,
            },
        )
    }

    /// Stage 6: global spatial features (texture, clarity).
    pub(crate) fn apply_global(&self, rgb: &mut [f32], width: usize, height: usize) {
        if let Some(local) = &self.local {
            local.apply(rgb, width, height);
        }
    }

    /// Detail stages after the global ones (split 4 stage 7 goes between, R5): sharpening (8a),
    /// then one row-order loop for vignette (8b) and grain (8c), in that order. Per pixel the
    /// operations are those of [`Vignette::weight`] and [`Grain::noise`]; only the per-column and
    /// per-row parts are computed once from tables.
    pub(crate) fn apply_detail(&self, rgb: &mut [f32], width: usize, height: usize, seed: u32) {
        if let Some(sharpen) = &self.sharpen {
            sharpen.apply(rgb, width, height);
        }
        if self.vignette.is_none() && self.grain.is_none() {
            return;
        }
        let vignette = self
            .vignette
            .as_ref()
            .map(|v| (v, Vignette::column_u2(width)));
        let mut grain = self
            .grain
            .as_ref()
            .map(|g| (g, GrainRows::new(g, seed, width), vec![0.0f32; width]));
        for (y, line) in rgb.chunks_exact_mut(3 * width).take(height).enumerate() {
            let v2 = Vignette::row_v2(y, height);
            if let Some((_, rows, noise)) = &mut grain {
                rows.noise_row(y, noise);
            }
            for (x, px) in line.chunks_exact_mut(3).enumerate() {
                let mut p = [px[0], px[1], px[2]];
                if let Some((v, u2)) = &vignette {
                    p = v.apply_px(p, v.weight_sq(u2[x], v2));
                }
                if let Some((g, _, noise)) = &grain {
                    p = g.apply_px(p, noise[x]);
                }
                px.copy_from_slice(&p);
            }
        }
    }

    /// The original pixel-order loops: the oracle for [`SpatialOps::apply_detail`].
    #[cfg(test)]
    fn apply_detail_ref(&self, rgb: &mut [f32], width: usize, height: usize, seed: u32) {
        if let Some(sharpen) = &self.sharpen {
            sharpen.apply_ref(rgb, width, height);
        }
        if self.vignette.is_none() && self.grain.is_none() {
            return;
        }
        for (i, px) in rgb.chunks_exact_mut(3).take(width * height).enumerate() {
            let (x, y) = (i % width, i / width);
            let mut p = [px[0], px[1], px[2]];
            if let Some(v) = &self.vignette {
                p = v.apply_px(p, v.weight(x, y, width, height));
            }
            if let Some(g) = &self.grain {
                p = g.apply_px(p, g.noise(seed, x, y));
            }
            px.copy_from_slice(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::Detail;

    /// (sharpen amount, radius, masking, grain amount, size, frequency, vignette amount)
    type Setting = (f32, f32, f32, f32, f32, f32, f32);

    const SETTINGS: [Setting; 6] = [
        (0.0, 1.0, 0.0, 80.0, 0.0, 0.0, -80.0),
        (0.0, 1.0, 0.0, 25.0, 100.0, 100.0, 50.0),
        (0.0, 1.0, 0.0, 100.0, 37.0, 60.0, 0.0),
        (0.0, 1.0, 0.0, 0.0, 25.0, 50.0, -30.0),
        (150.0, 3.0, 50.0, 80.0, 0.0, 100.0, -80.0),
        (60.0, 0.5, 0.0, 0.0, 25.0, 50.0, 0.0),
    ];

    fn ops(s: Setting) -> SpatialOps {
        let (sharpen_amount, sharpen_radius, sharpen_masking) = (s.0, s.1, s.2);
        let d = Detail {
            sharpen_amount,
            sharpen_radius,
            sharpen_masking,
            grain_amount: s.3,
            grain_size: s.4,
            grain_frequency: s.5,
            vignette_amount: s.6,
            vignette_midpoint: 20.0,
            vignette_feather: 80.0,
            ..Detail::default()
        };
        SpatialOps {
            local: None,
            sharpen: Sharpen::new(&d),
            vignette: Vignette::new(&d),
            grain: Grain::new(&d),
        }
    }

    #[test]
    fn row_order_detail_is_bit_identical_to_the_pixel_loop() {
        let shapes = [(1usize, 1usize), (5, 1), (1, 5), (5, 5), (97, 61), (300, 7)];
        for (w, h) in shapes {
            let src: Vec<f32> = (0..w * h * 3)
                .map(|i| ((i * 7919 % 1000) as f32 / 700.0) - 0.2)
                .collect();
            for s in SETTINGS {
                let sp = ops(s);
                for seed in [0u32, 7, u32::MAX] {
                    let (mut a, mut b) = (src.clone(), src.clone());
                    sp.apply_detail(&mut a, w, h, seed);
                    sp.apply_detail_ref(&mut b, w, h, seed);
                    assert!(
                        a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()),
                        "{w}x{h} {s:?} seed {seed}"
                    );
                }
            }
        }
    }
}
