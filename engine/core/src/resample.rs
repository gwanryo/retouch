//! Downscaling for the scoring path (master plan A4, AC-S1h). Every rule below is part of the
//! score contract; changing any of them requires a `SCORING_VERSION` bump.
//!
//! - kernel Lanczos3 (`a = 3`), evaluated at `(j - center) / scale`, `scale = src / dst`
//! - pixel centers at `i + 0.5`: `center = (i + 0.5) * scale - 0.5`
//! - taps `j` span `floor(center - 3*scale) ..= ceil(center + 3*scale)` in full; only the
//!   **sample index** is clamped to `[0, len - 1]` (clamp-to-edge), weights are never dropped
//! - weights normalized to sum 1, accumulated in ascending `j`, f32
//! - separable: horizontal pass, then vertical pass, both iterating rows in order
//! - an axis whose size does not change is copied unchanged (no filtering)

use crate::buffer::{dequantize, quantize, BufferError, ImageF32, Rgba8};
use crate::color::{srgb_decode, srgb_encode};
use thiserror::Error;

/// Errors from resampling.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResampleError {
    /// Requested target has a zero dimension.
    #[error("empty target size {width}x{height}")]
    EmptyTarget {
        /// Target width.
        width: u32,
        /// Target height.
        height: u32,
    },
    /// Target larger than source on some axis (this module only downscales).
    #[error("upscaling is not supported: {src_w}x{src_h} -> {dst_w}x{dst_h}")]
    Upscale {
        /// Source width.
        src_w: u32,
        /// Source height.
        src_h: u32,
        /// Target width.
        dst_w: u32,
        /// Target height.
        dst_h: u32,
    },
    /// Buffer construction failed.
    #[error(transparent)]
    Buffer(#[from] BufferError),
}

/// Target size whose longer edge equals `long_edge` (never upscales; short edge ≥ 1).
/// The short edge is `floor(short * long_edge / long + 0.5)`, computed in f64.
pub fn fit_long_edge(width: u32, height: u32, long_edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= long_edge {
        return (width, height);
    }
    let scale = f64::from(long_edge) / f64::from(long);
    let short = (f64::from(width.min(height)) * scale + 0.5)
        .floor()
        .max(1.0) as u32;
    if width >= height {
        (long_edge, short)
    } else {
        (short, long_edge)
    }
}

fn sinc(x: f32) -> f32 {
    if x == 0.0 {
        1.0
    } else {
        let px = std::f32::consts::PI * x;
        libm::sinf(px) / px
    }
}

fn lanczos3(x: f32) -> f32 {
    if x.abs() >= 3.0 {
        0.0
    } else {
        sinc(x) * sinc(x / 3.0)
    }
}

/// Per target index: `(clamped source index, normalized weight)` taps in ascending `j`.
fn axis_taps(src_len: u32, dst_len: u32) -> Vec<Vec<(usize, f32)>> {
    let scale = src_len as f32 / dst_len as f32;
    let radius = 3.0 * scale;
    let last = i64::from(src_len) - 1;
    (0..dst_len)
        .map(|i| {
            let center = (i as f32 + 0.5) * scale - 0.5;
            let lo = (center - radius).floor() as i64;
            let hi = (center + radius).ceil() as i64;
            let raw: Vec<(usize, f32)> = (lo..=hi)
                .map(|j| {
                    (
                        j.clamp(0, last) as usize,
                        lanczos3((j as f32 - center) / scale),
                    )
                })
                .collect();
            let sum: f32 = raw.iter().map(|&(_, w)| w).sum();
            raw.into_iter().map(|(idx, w)| (idx, w / sum)).collect()
        })
        .collect()
}

fn horizontal(src: &ImageF32, dst_w: u32) -> Result<ImageF32, ResampleError> {
    let (sw, sh) = (src.width(), src.height());
    let taps = axis_taps(sw, dst_w);
    let row_len = sw as usize * 3;
    let mut out = Vec::with_capacity(dst_w as usize * sh as usize * 3);
    for row in src.samples().chunks_exact(row_len) {
        for t in &taps {
            let mut acc = [0.0f32; 3];
            for &(idx, w) in t {
                let p = &row[idx * 3..idx * 3 + 3];
                acc[0] += p[0] * w;
                acc[1] += p[1] * w;
                acc[2] += p[2] * w;
            }
            out.extend_from_slice(&acc);
        }
    }
    Ok(ImageF32::from_samples(dst_w, sh, out)?)
}

fn vertical(src: &ImageF32, dst_h: u32) -> Result<ImageF32, ResampleError> {
    let (sw, sh) = (src.width(), src.height());
    let taps = axis_taps(sh, dst_h);
    let row_len = sw as usize * 3;
    let samples = src.samples();
    let mut out = Vec::with_capacity(row_len * dst_h as usize);
    for t in &taps {
        let mut acc = vec![0.0f32; row_len];
        for &(idx, w) in t {
            let row = &samples[idx * row_len..(idx + 1) * row_len];
            for (a, s) in acc.iter_mut().zip(row) {
                *a += s * w;
            }
        }
        out.extend_from_slice(&acc);
    }
    Ok(ImageF32::from_samples(sw, dst_h, out)?)
}

/// Lanczos3 downscale of a **linear-light** image to exactly `dst_w`×`dst_h`.
pub fn downscale_lanczos3(
    src: &ImageF32,
    dst_w: u32,
    dst_h: u32,
) -> Result<ImageF32, ResampleError> {
    let (sw, sh) = (src.width(), src.height());
    if dst_w == 0 || dst_h == 0 {
        return Err(ResampleError::EmptyTarget {
            width: dst_w,
            height: dst_h,
        });
    }
    if dst_w > sw || dst_h > sh {
        return Err(ResampleError::Upscale {
            src_w: sw,
            src_h: sh,
            dst_w,
            dst_h,
        });
    }
    let tmp = if dst_w == sw {
        src.clone()
    } else {
        horizontal(src, dst_w)?
    };
    if dst_h == sh {
        Ok(tmp)
    } else {
        vertical(&tmp, dst_h)
    }
}

/// THE scoring-image function (master plan A4): RGBA8 → dequantize → sRGB decode (linear) →
/// Lanczos3 to `long_edge` → sRGB encode → quantize → RGBA8 (alpha 255). CLI, golden and wasm
/// all call this; a saved answer PNG reproduces the scoring input exactly.
pub fn scoring_image(src: &Rgba8, long_edge: u32) -> Result<Rgba8, ResampleError> {
    if long_edge == 0 {
        return Err(ResampleError::EmptyTarget {
            width: 0,
            height: 0,
        });
    }
    let (dw, dh) = fit_long_edge(src.width(), src.height(), long_edge);
    if (dw, dh) == (src.width(), src.height()) {
        let opaque = src
            .data()
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        return Ok(Rgba8::new(dw, dh, opaque)?);
    }
    let lut: Vec<f32> = (0..=255u8).map(|v| srgb_decode(dequantize(v))).collect();
    let linear: Vec<f32> = src
        .data()
        .chunks_exact(4)
        .flat_map(|p| {
            [
                lut[usize::from(p[0])],
                lut[usize::from(p[1])],
                lut[usize::from(p[2])],
            ]
        })
        .collect();
    let linear = ImageF32::from_samples(src.width(), src.height(), linear)?;
    let small = downscale_lanczos3(&linear, dw, dh)?;
    let bytes = small
        .pixels()
        .flat_map(|p| {
            [
                quantize(srgb_encode(p[0])),
                quantize(srgb_encode(p[1])),
                quantize(srgb_encode(p[2])),
                255,
            ]
        })
        .collect();
    Ok(Rgba8::new(dw, dh, bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[f32]) -> ImageF32 {
        let data = values.iter().flat_map(|&v| [v, v, v]).collect();
        ImageF32::from_samples(values.len() as u32, 1, data).unwrap()
    }

    fn assert_row_close(img: &ImageF32, want: &[f32]) {
        let got: Vec<f32> = img.pixels().map(|p| p[0]).collect();
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-6, "got {got:?} want {want:?}");
        }
    }

    mod fit_long_edge {
        use super::*;

        #[test]
        fn scales_landscape_2048x1365_to_1024x683() {
            assert_eq!(fit_long_edge(2048, 1365, 1024), (1024, 683));
        }

        #[test]
        fn scales_portrait_1530x2040_to_768x1024() {
            assert_eq!(fit_long_edge(1530, 2040, 1024), (768, 1024));
        }

        #[test]
        fn never_upscales() {
            assert_eq!(fit_long_edge(800, 600, 1024), (800, 600));
        }

        #[test]
        fn keeps_a_one_pixel_short_edge() {
            assert_eq!(fit_long_edge(3000, 1, 1024), (1024, 1));
        }
    }

    // Expected values: independent float64 reference implementation of the rules in the module
    // doc (plan Task 3). Tolerance 1e-6 absorbs f32 accumulation.
    mod downscale_lanczos3 {
        use super::*;

        #[test]
        fn edge_impulse_matches_reference() {
            let mut v = [0.0f32; 12];
            v[0] = 1.0;
            let out = downscale_lanczos3(&row(&v), 4, 1).unwrap();
            assert_row_close(&out, &[0.332_843_9, -0.065_327_91, 0.014_622_78, 0.0]);
        }

        #[test]
        fn center_impulse_matches_reference() {
            let mut v = [0.0f32; 12];
            v[6] = 1.0;
            let out = downscale_lanczos3(&row(&v), 4, 1).unwrap();
            assert_row_close(
                &out,
                &[-0.031_200_27, 0.127_278_3, 0.270_893_5, -0.048_750_42],
            );
        }

        #[test]
        fn ramp_10_to_4_matches_reference() {
            let v: Vec<f32> = (0..10).map(|i| i as f32 / 9.0).collect();
            let out = downscale_lanczos3(&row(&v), 4, 1).unwrap();
            assert_row_close(&out, &[0.079_193_74, 0.362_135_4, 0.637_864_6, 0.920_806_3]);
        }

        #[test]
        fn keeps_a_constant_image_constant() {
            let src = ImageF32::new_filled(64, 48, [0.25, 0.5, 0.75]).unwrap();
            let out = downscale_lanczos3(&src, 30, 17).unwrap();
            for p in out.pixels() {
                let ok = (p[0] - 0.25).abs() < 1e-5
                    && (p[1] - 0.5).abs() < 1e-5
                    && (p[2] - 0.75).abs() < 1e-5;
                assert!(ok, "{p:?}");
            }
        }

        #[test]
        fn three_by_one_to_one_by_one_uses_clamped_taps() {
            let out = downscale_lanczos3(&row(&[0.4, 0.4, 0.4]), 1, 1).unwrap();
            assert_row_close(&out, &[0.4]);
        }

        #[test]
        fn unchanged_axes_are_copied_bit_exact() {
            let src = row(&[0.1, 0.7, 0.3, 0.9, 0.2]);
            assert_eq!(downscale_lanczos3(&src, 5, 1).unwrap(), src);
        }

        #[test]
        fn returns_requested_dimensions() {
            let src = ImageF32::new_filled(100, 70, [0.0; 3]).unwrap();
            let out = downscale_lanczos3(&src, 50, 35).unwrap();
            assert_eq!((out.width(), out.height()), (50, 35));
        }

        #[test]
        fn rejects_upscale() {
            let src = ImageF32::new_filled(10, 10, [0.0; 3]).unwrap();
            let err = downscale_lanczos3(&src, 20, 10).unwrap_err();
            let want = ResampleError::Upscale {
                src_w: 10,
                src_h: 10,
                dst_w: 20,
                dst_h: 10,
            };
            assert_eq!(err, want);
        }

        #[test]
        fn rejects_empty_target() {
            let src = ImageF32::new_filled(10, 10, [0.0; 3]).unwrap();
            let err = downscale_lanczos3(&src, 0, 5).unwrap_err();
            assert_eq!(
                err,
                ResampleError::EmptyTarget {
                    width: 0,
                    height: 5
                }
            );
        }
    }

    mod scoring_image {
        use super::*;

        fn gradient_rgba(w: u32, h: u32) -> Rgba8 {
            let mut data = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    data.extend_from_slice(&[
                        (x % 256) as u8,
                        (y % 256) as u8,
                        ((x + y) % 256) as u8,
                        255,
                    ]);
                }
            }
            Rgba8::new(w, h, data).unwrap()
        }

        #[test]
        fn produces_1024x683_from_2048x1365() {
            let out = scoring_image(&gradient_rgba(2048, 1365), 1024).unwrap();
            assert_eq!((out.width(), out.height()), (1024, 683));
        }

        #[test]
        fn returns_the_same_pixels_when_already_small() {
            let src = gradient_rgba(40, 30);
            assert_eq!(scoring_image(&src, 1024).unwrap(), src);
        }

        #[test]
        fn forces_alpha_to_255() {
            let src = Rgba8::new(1, 1, vec![10, 20, 30, 0]).unwrap();
            assert_eq!(
                scoring_image(&src, 1024).unwrap().data(),
                &[10, 20, 30, 255]
            );
        }

        #[test]
        fn keeps_a_flat_gray_flat() {
            let src = Rgba8::new(300, 200, [77u8, 77, 77, 255].repeat(300 * 200)).unwrap();
            let out = scoring_image(&src, 100).unwrap();
            assert!(out.data().chunks_exact(4).all(|p| p == [77, 77, 77, 255]));
        }

        #[test]
        fn rejects_zero_long_edge() {
            let err = scoring_image(&gradient_rgba(4, 4), 0).unwrap_err();
            assert_eq!(
                err,
                ResampleError::EmptyTarget {
                    width: 0,
                    height: 0
                }
            );
        }
    }
}
