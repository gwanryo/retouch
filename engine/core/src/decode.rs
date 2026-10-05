//! Reference-path decode/encode (master plan A2). JPEG goes through `zune-jpeg` built without
//! its `x86`/`neon` features, so every target runs the same scalar IDCT, upsampler and color
//! converter. PNG goes through `png`. Browser `<img>` decoding never feeds the reference path.
//!
//! Accepted input: 8-bit baseline/progressive JPEG in YCbCr, RGB or grayscale; 8-bit PNG
//! (gray/RGB/palette, alpha only if every pixel is opaque). Everything else is rejected
//! instead of being silently converted (16-bit PNG, translucent PNG, CMYK/YCCK JPEG).
//! EXIF orientation and ICC profiles are NOT applied here; content originals are normalized
//! to sRGB with orientation baked in at build time (AC-E1c, Plan 3).

use crate::buffer::{checked_len, BufferError, Rgba8};
use std::io::Cursor;
use thiserror::Error;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// Decode/encode errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    /// Bytes are neither JPEG nor PNG.
    #[error("unrecognized image format (expected JPEG or PNG)")]
    UnknownFormat,
    /// The JPEG decoder rejected the bytes.
    #[error("jpeg: {0}")]
    Jpeg(String),
    /// The PNG decoder/encoder rejected the bytes.
    #[error("png: {0}")]
    Png(String),
    /// A well-formed image the reference path does not accept.
    #[error("unsupported image: {0}")]
    Unsupported(String),
    /// Dimensions out of bounds.
    #[error(transparent)]
    Buffer(#[from] BufferError),
}

const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF];
const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Decode JPEG or PNG bytes to RGBA8 (alpha 255).
pub fn decode_rgba8(bytes: &[u8]) -> Result<Rgba8, DecodeError> {
    if bytes.starts_with(JPEG_MAGIC) {
        decode_jpeg(bytes)
    } else if bytes.starts_with(PNG_MAGIC) {
        decode_png(bytes)
    } else {
        Err(DecodeError::UnknownFormat)
    }
}

fn to_u32(v: usize) -> Result<u32, DecodeError> {
    u32::try_from(v).map_err(|_| DecodeError::Unsupported(format!("dimension {v} too large")))
}

fn decode_jpeg(bytes: &[u8]) -> Result<Rgba8, DecodeError> {
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_use_unsafe(false)
        .set_strict_mode(true);
    let mut dec = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    dec.decode_headers()
        .map_err(|e| DecodeError::Jpeg(e.to_string()))?;
    match dec.input_colorspace() {
        Some(ColorSpace::YCbCr | ColorSpace::RGB | ColorSpace::Luma) => {}
        other => {
            return Err(DecodeError::Unsupported(format!(
                "jpeg colorspace {other:?}"
            )))
        }
    }
    let (w, h) = dec
        .dimensions()
        .ok_or_else(|| DecodeError::Jpeg("missing dimensions".into()))?;
    let (w, h) = (to_u32(w)?, to_u32(h)?);
    checked_len(w, h, 4)?;
    let rgb = dec.decode().map_err(|e| DecodeError::Jpeg(e.to_string()))?;
    let rgba = rgb
        .chunks_exact(3)
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    Ok(Rgba8::new(w, h, rgba)?)
}

fn decode_png(bytes: &[u8]) -> Result<Rgba8, DecodeError> {
    let png_err = |e: png::DecodingError| DecodeError::Png(e.to_string());
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().map_err(png_err)?;
    let (w, h) = reader.info().size();
    checked_len(w, h, 4)?;
    let (color, depth) = reader.output_color_type();
    if depth != png::BitDepth::Eight {
        return Err(DecodeError::Unsupported(format!("png bit depth {depth:?}")));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| DecodeError::Png("output buffer size overflow".into()))?;
    let mut buf = vec![0u8; size];
    let frame = reader.next_frame(&mut buf).map_err(png_err)?;
    let px = &buf[..frame.buffer_size()];
    let opaque = |a: u8| {
        if a == 255 {
            Ok(())
        } else {
            Err(DecodeError::Unsupported("png with transparency".into()))
        }
    };
    let rgba: Vec<u8> = match color {
        png::ColorType::Rgb => px
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Rgba => {
            px.chunks_exact(4).try_for_each(|p| opaque(p[3]))?;
            px.to_vec()
        }
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            px.chunks_exact(2).try_for_each(|p| opaque(p[1]))?;
            px.chunks_exact(2)
                .flat_map(|p| [p[0], p[0], p[0], 255])
                .collect()
        }
        png::ColorType::Indexed => {
            return Err(DecodeError::Unsupported("png palette not expanded".into()))
        }
    };
    Ok(Rgba8::new(w, h, rgba)?)
}

/// Encode RGBA8 as PNG (lossless; answer references, AC-S1d). Byte output may change with the
/// `png` crate version; only decoded pixels are ever hashed.
pub fn encode_png(img: &Rgba8) -> Result<Vec<u8>, DecodeError> {
    let enc_err = |e: png::EncodingError| DecodeError::Png(e.to_string());
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width(), img.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(enc_err)?;
    writer.write_image_data(img.data()).map_err(enc_err)?;
    writer.finish().map_err(enc_err)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

    fn checker_rgb(w: u16, h: u16) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let on = (x / 4 + y / 4) % 2 == 0;
                v.extend_from_slice(if on { &[200, 60, 30] } else { &[20, 90, 180] });
            }
        }
        v
    }

    fn jpeg(data: &[u8], w: u16, h: u16, color: ColorType, progressive: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = Encoder::new(&mut out, 90);
        enc.set_sampling_factor(SamplingFactor::R_4_2_0);
        enc.set_progressive(progressive);
        enc.encode(data, w, h, color).unwrap();
        out
    }

    fn png_bytes(
        w: u32,
        h: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        data: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(depth);
        let mut wr = enc.write_header().unwrap();
        wr.write_image_data(data).unwrap();
        wr.finish().unwrap();
        out
    }

    #[test]
    fn png_round_trips_pixels_exactly() {
        let data = (0..12u8)
            .flat_map(|i| [i * 20, 255 - i, i * 7, 255])
            .collect();
        let img = Rgba8::new(4, 3, data).unwrap();
        assert_eq!(decode_rgba8(&encode_png(&img).unwrap()).unwrap(), img);
    }

    #[test]
    fn decodes_rgb_png_with_opaque_alpha() {
        let bytes = png_bytes(
            2,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[1, 2, 3, 4, 5, 6],
        );
        assert_eq!(
            decode_rgba8(&bytes).unwrap().data(),
            &[1, 2, 3, 255, 4, 5, 6, 255]
        );
    }

    #[test]
    fn decodes_grayscale_png_to_gray_rgb() {
        let bytes = png_bytes(1, 1, png::ColorType::Grayscale, png::BitDepth::Eight, &[77]);
        assert_eq!(decode_rgba8(&bytes).unwrap().data(), &[77, 77, 77, 255]);
    }

    #[test]
    fn rejects_16_bit_png() {
        let bytes = png_bytes(1, 1, png::ColorType::Rgb, png::BitDepth::Sixteen, &[0; 6]);
        assert!(matches!(
            decode_rgba8(&bytes),
            Err(DecodeError::Unsupported(_))
        ));
    }

    #[test]
    fn rejects_translucent_png() {
        let bytes = png_bytes(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[9, 9, 9, 128],
        );
        assert!(matches!(
            decode_rgba8(&bytes),
            Err(DecodeError::Unsupported(_))
        ));
    }

    #[test]
    fn decodes_baseline_420_jpeg_close_to_source() {
        let src: Vec<u8> = (0..48u8)
            .flat_map(|y| (0..64u8).flat_map(move |x| [x * 4, y * 5, 128]))
            .collect();
        let out = decode_rgba8(&jpeg(&src, 64, 48, ColorType::Rgb, false)).unwrap();
        assert_eq!((out.width(), out.height()), (64, 48));
        let mean_err: f64 = out
            .data()
            .chunks_exact(4)
            .zip(src.chunks_exact(3))
            .map(|(d, s)| (0..3).map(|i| f64::from(d[i].abs_diff(s[i]))).sum::<f64>())
            .sum::<f64>()
            / (64.0 * 48.0 * 3.0);
        assert!(mean_err < 3.0, "mean abs error {mean_err}");
    }

    #[test]
    fn progressive_and_baseline_jpeg_decode_to_the_same_size() {
        let src = checker_rgb(33, 17);
        let a = decode_rgba8(&jpeg(&src, 33, 17, ColorType::Rgb, false)).unwrap();
        let b = decode_rgba8(&jpeg(&src, 33, 17, ColorType::Rgb, true)).unwrap();
        assert_eq!(
            (a.width(), a.height(), b.width(), b.height()),
            (33, 17, 33, 17)
        );
    }

    #[test]
    fn decodes_grayscale_jpeg_to_gray_rgb() {
        let out = decode_rgba8(&jpeg(&[128; 64], 8, 8, ColorType::Luma, false)).unwrap();
        assert!(out
            .data()
            .chunks_exact(4)
            .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255));
    }

    #[test]
    fn rejects_cmyk_jpeg() {
        let bytes = jpeg(&[0, 0, 0, 0].repeat(64), 8, 8, ColorType::Cmyk, false);
        assert!(matches!(
            decode_rgba8(&bytes),
            Err(DecodeError::Unsupported(_))
        ));
    }

    #[test]
    fn decodes_a_1x1_jpeg() {
        let out = decode_rgba8(&jpeg(&[10, 200, 30], 1, 1, ColorType::Rgb, false)).unwrap();
        assert_eq!((out.width(), out.height()), (1, 1));
    }

    #[test]
    fn decodes_a_3000x1_jpeg() {
        let out = decode_rgba8(&jpeg(&[90; 9000], 3000, 1, ColorType::Rgb, false)).unwrap();
        assert_eq!((out.width(), out.height()), (3000, 1));
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert_eq!(
            decode_rgba8(b"not an image"),
            Err(DecodeError::UnknownFormat)
        );
    }

    #[test]
    fn rejects_truncated_jpeg() {
        let bytes = jpeg(&checker_rgb(64, 48), 64, 48, ColorType::Rgb, false);
        assert!(decode_rgba8(&bytes[..bytes.len() / 2]).is_err());
    }
}
