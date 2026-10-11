//! Spatial stages (6 onward), shared by both entry points. They need neighbours or position, so
//! they run on the whole unquantized RGB buffer after the per-pixel stages 1-5.

use super::clarity::LocalContrast;
use super::detail::{Grain, Sharpen, Vignette};
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
    /// then one pixel loop for vignette (8b) and grain (8c), in that order.
    pub(crate) fn apply_detail(&self, rgb: &mut [f32], width: usize, height: usize, seed: u32) {
        if let Some(sharpen) = &self.sharpen {
            sharpen.apply(rgb, width, height);
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
