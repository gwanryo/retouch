//! Spatial stages (6 onward), shared by both entry points. They need neighbours or position, so
//! they run on the whole unquantized RGB buffer after the per-pixel stages 1-5.

use super::clarity::LocalContrast;
use super::detail::Sharpen;
use crate::recipe::Recipe;

/// Active spatial features, prepared once per render.
pub(crate) struct SpatialOps {
    local: Option<LocalContrast>,
    sharpen: Option<Sharpen>,
}

impl SpatialOps {
    /// `None` when every spatial feature is inactive (the per-pixel byte path then runs).
    pub(crate) fn new(r: &Recipe) -> Option<Self> {
        let local = LocalContrast::new(&r.basic);
        let sharpen = Sharpen::new(&r.detail);
        (local.is_some() || sharpen.is_some()).then_some(Self { local, sharpen })
    }

    /// Stage 6: global spatial features (texture, clarity).
    pub(crate) fn apply_global(&self, rgb: &mut [f32], width: usize, height: usize) {
        if let Some(local) = &self.local {
            local.apply(rgb, width, height);
        }
    }

    /// Detail stages after the global ones (split 4 stage 7 goes between, R5): sharpening (8a).
    /// Grain (8c) joins in Task 6.
    pub(crate) fn apply_detail(&self, rgb: &mut [f32], width: usize, height: usize, seed: u32) {
        let _ = seed;
        if let Some(sharpen) = &self.sharpen {
            sharpen.apply(rgb, width, height);
        }
    }
}
