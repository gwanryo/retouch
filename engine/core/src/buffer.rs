//! Pixel buffers and THE single 8-bit quantization rule (master plan A3, AC-E1e).

use thiserror::Error;

/// Largest accepted image, in pixels (≈ 8660×5773). Bounds every allocation and keeps
/// `width * height * 12` far below `usize::MAX` on wasm32.
pub const MAX_PIXELS: u64 = 50_000_000;

/// Errors from buffer construction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BufferError {
    /// Width or height is zero.
    #[error("empty image: {width}x{height}")]
    Empty {
        /// Declared width.
        width: u32,
        /// Declared height.
        height: u32,
    },
    /// More than [`MAX_PIXELS`] pixels.
    #[error("image too large: {width}x{height} exceeds {MAX_PIXELS} pixels")]
    TooLarge {
        /// Declared width.
        width: u32,
        /// Declared height.
        height: u32,
    },
    /// Byte length does not match `width * height * 4`.
    #[error("rgba buffer has {actual} bytes, expected {expected} for {width}x{height}")]
    LengthMismatch {
        /// Declared width.
        width: u32,
        /// Declared height.
        height: u32,
        /// Bytes expected.
        expected: usize,
        /// Bytes received.
        actual: usize,
    },
}

/// Validates dimensions and returns `width * height * channels` without overflow.
pub fn checked_len(width: u32, height: u32, channels: usize) -> Result<usize, BufferError> {
    if width == 0 || height == 0 {
        return Err(BufferError::Empty { width, height });
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_PIXELS {
        return Err(BufferError::TooLarge { width, height });
    }
    usize::try_from(pixels)
        .ok()
        .and_then(|p| p.checked_mul(channels))
        .ok_or(BufferError::TooLarge { width, height })
}

/// Encoded [0,1] → 8-bit: clamp, scale, round half up. The ONLY quantization in the engine.
/// NaN maps to 0; render never produces NaN (it returns `RenderError::NonFinite` instead).
pub fn quantize(v: f32) -> u8 {
    if v.is_nan() || v <= 0.0 {
        0
    } else if v >= 1.0 {
        255
    } else {
        // `as u8` truncates toward zero; +0.5 first gives round-half-up for non-negative input.
        (v * 255.0 + 0.5) as u8
    }
}

/// 8-bit → encoded [0,1].
pub fn dequantize(v: u8) -> f32 {
    f32::from(v) / 255.0
}

/// Validated RGBA8 pixels (alpha carried but ignored by render/score).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Rgba8 {
    /// Wraps `data` after checking dimensions and length.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, BufferError> {
        let expected = checked_len(width, height, 4)?;
        if data.len() != expected {
            return Err(BufferError::LengthMismatch {
                width,
                height,
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Interleaved RGBA bytes, row-major.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consumes the buffer and returns the bytes.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

/// RGB image, interleaved `f32`, row-major. Whether samples are encoded sRGB or linear is a
/// convention of the call site. Values are NOT clamped (master plan A3).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageF32 {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

impl ImageF32 {
    /// Image filled with one pixel value.
    pub fn new_filled(width: u32, height: u32, px: [f32; 3]) -> Result<Self, BufferError> {
        let n = checked_len(width, height, 3)?;
        let data = px.iter().copied().cycle().take(n).collect();
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Wraps interleaved RGB samples after checking dimensions and length.
    pub fn from_samples(width: u32, height: u32, data: Vec<f32>) -> Result<Self, BufferError> {
        let expected = checked_len(width, height, 3)?;
        if data.len() != expected {
            return Err(BufferError::LengthMismatch {
                width,
                height,
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// From RGBA8 (alpha dropped), samples dequantized to encoded sRGB.
    pub fn from_rgba8(src: &Rgba8) -> Self {
        let data = src
            .data()
            .chunks_exact(4)
            .flat_map(|px| [dequantize(px[0]), dequantize(px[1]), dequantize(px[2])])
            .collect();
        Self {
            width: src.width(),
            height: src.height(),
            data,
        }
    }

    /// To RGBA8 with alpha 255, via [`quantize`].
    pub fn to_rgba8(&self) -> Rgba8 {
        let data = self
            .data
            .chunks_exact(3)
            .flat_map(|px| [quantize(px[0]), quantize(px[1]), quantize(px[2]), 255])
            .collect();
        Rgba8 {
            width: self.width,
            height: self.height,
            data,
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Interleaved RGB samples.
    pub fn samples(&self) -> &[f32] {
        &self.data
    }

    /// Pixel at (x, y). Panics if out of bounds (callers iterate within `width`/`height`).
    pub fn pixel(&self, x: u32, y: u32) -> [f32; 3] {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// Sets the pixel at (x, y). Panics if out of bounds.
    pub fn set_pixel(&mut self, x: u32, y: u32, px: [f32; 3]) {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        self.data[i..i + 3].copy_from_slice(&px);
    }

    /// New image with `f` applied to every pixel, in row-major order.
    #[must_use]
    pub fn map_pixels(&self, f: impl Fn([f32; 3]) -> [f32; 3]) -> Self {
        let data = self
            .data
            .chunks_exact(3)
            .flat_map(|px| f([px[0], px[1], px[2]]))
            .collect();
        Self {
            width: self.width,
            height: self.height,
            data,
        }
    }

    /// Pixels in row-major order.
    pub fn pixels(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.data.chunks_exact(3).map(|px| [px[0], px[1], px[2]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod quantize {
        use super::*;

        #[test]
        fn rounds_half_up() {
            assert_eq!(quantize(0.5 / 255.0), 1);
        }

        #[test]
        fn maps_one_to_255() {
            assert_eq!(quantize(1.0), 255);
        }

        #[test]
        fn clamps_above_one() {
            assert_eq!(quantize(1.5), 255);
        }

        #[test]
        fn clamps_below_zero() {
            assert_eq!(quantize(-0.2), 0);
        }

        #[test]
        fn maps_nan_to_zero() {
            assert_eq!(quantize(f32::NAN), 0);
        }

        #[test]
        fn round_trips_every_byte() {
            for v in 0..=255u8 {
                assert_eq!(quantize(dequantize(v)), v, "byte {v}");
            }
        }
    }

    mod rgba8_new {
        use super::*;

        #[test]
        fn rejects_length_mismatch() {
            let err = Rgba8::new(2, 1, vec![0; 7]).unwrap_err();
            let want = BufferError::LengthMismatch {
                width: 2,
                height: 1,
                expected: 8,
                actual: 7,
            };
            assert_eq!(err, want);
        }

        #[test]
        fn rejects_zero_width() {
            let err = Rgba8::new(0, 5, Vec::new()).unwrap_err();
            assert_eq!(
                err,
                BufferError::Empty {
                    width: 0,
                    height: 5
                }
            );
        }

        #[test]
        fn rejects_more_than_max_pixels_before_checking_length() {
            let err = Rgba8::new(10_000, 10_000, Vec::new()).unwrap_err();
            let want = BufferError::TooLarge {
                width: 10_000,
                height: 10_000,
            };
            assert_eq!(err, want);
        }

        #[test]
        fn rejects_u32_max_square_without_overflow() {
            let err = Rgba8::new(u32::MAX, u32::MAX, Vec::new()).unwrap_err();
            assert!(matches!(err, BufferError::TooLarge { .. }), "{err:?}");
        }
    }

    mod image_f32 {
        use super::*;

        #[test]
        fn from_rgba8_drops_alpha_and_dequantizes() {
            let src = Rgba8::new(1, 1, vec![255, 0, 128, 7]).unwrap();
            assert_eq!(
                ImageF32::from_rgba8(&src).pixel(0, 0),
                [1.0, 0.0, 128.0 / 255.0]
            );
        }

        #[test]
        fn round_trips_rgba8_exactly() {
            let bytes: Vec<u8> = (0..64u8)
                .flat_map(|i| [i, 255 - i, i.wrapping_mul(3), 255])
                .collect();
            let src = Rgba8::new(8, 8, bytes).unwrap();
            assert_eq!(ImageF32::from_rgba8(&src).to_rgba8(), src);
        }

        #[test]
        fn new_filled_rejects_empty() {
            let err = ImageF32::new_filled(3, 0, [0.0; 3]).unwrap_err();
            assert_eq!(
                err,
                BufferError::Empty {
                    width: 3,
                    height: 0
                }
            );
        }

        #[test]
        fn new_filled_sets_every_pixel() {
            let img = ImageF32::new_filled(3, 2, [0.1, 0.2, 0.3]).unwrap();
            assert!(img.pixels().all(|p| p == [0.1, 0.2, 0.3]));
        }
    }
}
