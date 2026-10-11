//! Deterministic blur primitives for the detail stages (texture, clarity, sharpening).
//!
//! Summation order is part of the contract: the same math summed in a different order gives
//! different bits. Box sums run in f64 (IEEE add/sub/div are deterministic) and are stored
//! back as f32; the Gaussian accumulates in f32 in ascending tap order.

use super::basic;

/// Single-channel row-major f32 image.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(test), expect(dead_code, reason = "consumed by clarity (Task 4)"))]
pub(crate) struct Plane {
    /// Width in pixels.
    pub(crate) width: usize,
    /// Height in pixels.
    pub(crate) height: usize,
    /// `width * height` samples, row-major.
    pub(crate) data: Vec<f32>,
}

impl Plane {
    /// Rec.709 luma plane of an interleaved RGB f32 buffer.
    #[cfg_attr(not(test), expect(dead_code, reason = "consumed by clarity (Task 4)"))]
    pub(crate) fn luma_of(rgb: &[f32], width: usize, height: usize) -> Plane {
        let data = rgb
            .chunks_exact(3)
            .take(width * height)
            .map(|p| basic::luma([p[0], p[1], p[2]]))
            .collect();
        Plane {
            width,
            height,
            data,
        }
    }
}

/// One 1-D moving-sum box pass with clamp-to-edge sampling (`line.len() >= 1`).
fn box_pass(line: &[f32], out: &mut [f32], radius: usize) {
    let n = line.len() as isize;
    let r = radius as isize;
    let at = |k: isize| f64::from(line[k.clamp(0, n - 1) as usize]);
    let div = (2 * r + 1) as f64;
    let mut s = 0.0f64;
    for k in -r..=r {
        s += at(k);
    }
    for x in 0..n {
        out[x as usize] = (s / div) as f32;
        s += at(x + r + 1);
        s -= at(x - r);
    }
}

/// Applies `pass` to every row, then every column, through scratch lines (never in place).
fn separable(src: &mut Plane, pass: &dyn Fn(&[f32], &mut [f32])) {
    let (w, h) = (src.width, src.height);
    let mut line = vec![0.0f32; w.max(h)];
    let mut out = vec![0.0f32; w.max(h)];
    for y in 0..h {
        let row = &mut src.data[y * w..(y + 1) * w];
        line[..w].copy_from_slice(row);
        pass(&line[..w], &mut out[..w]);
        row.copy_from_slice(&out[..w]);
    }
    for x in 0..w {
        for (y, v) in line.iter_mut().take(h).enumerate() {
            *v = src.data[y * w + x];
        }
        pass(&line[..h], &mut out[..h]);
        for (y, v) in out.iter().take(h).enumerate() {
            src.data[y * w + x] = *v;
        }
    }
}

/// Three rounds of (horizontal, vertical) box blur with the given radius in pixels.
#[cfg_attr(not(test), expect(dead_code, reason = "consumed by clarity (Task 4)"))]
pub(crate) fn box_blur3(src: &Plane, radius: usize) -> Plane {
    let mut p = src.clone();
    for _ in 0..3 {
        separable(&mut p, &|line, out| box_pass(line, out, radius));
    }
    p
}

/// Normalized Gaussian kernel of half-width `ceil(3 * sigma)` (length `2R + 1`).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by sharpening (Task 5)")
)]
pub(crate) fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let r = (3.0 * sigma).ceil() as isize;
    let w: Vec<f32> = (-r..=r)
        .map(|k| libm::expf(-((k * k) as f32) / (2.0 * sigma * sigma)))
        .collect();
    let mut sum = 0.0f32;
    for v in &w {
        sum += *v;
    }
    w.into_iter().map(|v| v / sum).collect()
}

/// 1-D convolution with clamp-to-edge, taps summed in ascending order.
fn gaussian_pass(line: &[f32], out: &mut [f32], kernel: &[f32]) {
    let n = line.len() as isize;
    let r = (kernel.len() / 2) as isize;
    for x in 0..n {
        let mut acc = 0.0f32;
        for (j, kj) in kernel.iter().enumerate() {
            acc += *kj * line[(x + j as isize - r).clamp(0, n - 1) as usize];
        }
        out[x as usize] = acc;
    }
}

/// Separable Gaussian blur: horizontal then vertical.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "consumed by sharpening (Task 5)")
)]
pub(crate) fn gaussian_blur(src: &Plane, kernel: &[f32]) -> Plane {
    let mut p = src.clone();
    separable(&mut p, &|line, out| gaussian_pass(line, out, kernel));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(width: usize, height: usize, data: Vec<f32>) -> Plane {
        Plane {
            width,
            height,
            data,
        }
    }

    fn naive_line(v: &[f64], r: isize) -> Vec<f64> {
        let n = v.len() as isize;
        (0..n)
            .map(|x| {
                let s: f64 = (-r..=r).map(|k| v[(x + k).clamp(0, n - 1) as usize]).sum();
                s / (2 * r + 1) as f64
            })
            .collect()
    }

    fn naive_box3(src: &Plane, radius: usize) -> Vec<f64> {
        let (w, h) = (src.width, src.height);
        let r = radius as isize;
        let mut d: Vec<f64> = src.data.iter().map(|v| f64::from(*v)).collect();
        for _ in 0..3 {
            for y in 0..h {
                let row = naive_line(&d[y * w..(y + 1) * w], r);
                d[y * w..(y + 1) * w].copy_from_slice(&row);
            }
            for x in 0..w {
                let col: Vec<f64> = (0..h).map(|y| d[y * w + x]).collect();
                for (y, v) in naive_line(&col, r).into_iter().enumerate() {
                    d[y * w + x] = v;
                }
            }
        }
        d
    }

    #[test]
    fn box_pass_spreads_an_impulse_evenly() {
        let mut out = [0.0; 5];
        box_pass(&[0.0, 0.0, 3.0, 0.0, 0.0], &mut out, 1);
        assert_eq!(out, [0.0, 1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn box_pass_clamps_indices_to_the_edge() {
        let mut out = [0.0; 3];
        box_pass(&[3.0, 0.0, 0.0], &mut out, 1);
        assert_eq!(out, [2.0, 1.0, 0.0]);
    }

    #[test]
    fn box_pass_with_radius_larger_than_the_line_still_averages_clamped_samples() {
        let mut out = [0.0; 2];
        box_pass(&[1.0, 4.0], &mut out, 3); // x=0: 1*4 + 4*3 = 16 / 7
        assert_eq!(out[0], (16.0f64 / 7.0) as f32);
    }

    #[test]
    fn box_blur3_keeps_a_constant_plane_bit_identical() {
        let src = plane(7, 5, vec![0.3; 35]);
        assert_eq!(box_blur3(&src, 24), src);
    }

    #[test]
    fn box_blur3_matches_a_naive_f64_reference_closely() {
        let data: Vec<f32> = (0..54).map(|i| ((i * 37) % 11) as f32 / 10.0).collect();
        let src = plane(9, 6, data);
        let got = box_blur3(&src, 2);
        let want = naive_box3(&src, 2);
        for (g, w) in got.data.iter().zip(&want) {
            assert!((f64::from(*g) - w).abs() < 1e-6, "{g} vs {w}");
        }
    }

    #[test]
    fn gaussian_kernel_is_normalized_symmetric_and_peaked() {
        for sigma in [0.5f32, 1.0, 1.2, 3.0] {
            let k = gaussian_kernel(sigma);
            let r = (3.0 * sigma).ceil() as usize;
            assert_eq!(k.len(), 2 * r + 1);
            for j in 0..r {
                assert_eq!(k[j].to_bits(), k[2 * r - j].to_bits());
                assert!(k[j] < k[j + 1]);
            }
            assert!((k.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn gaussian_blur_of_an_impulse_is_the_separable_kernel() {
        let k = gaussian_kernel(1.0); // len 7
        let mut data = vec![0.0; 81];
        data[4 * 9 + 4] = 1.0;
        let out = gaussian_blur(&plane(9, 9, data), &k);
        for y in 0..7 {
            for x in 0..7 {
                assert_eq!(out.data[(y + 1) * 9 + x + 1], k[y] * k[x]);
            }
        }
    }

    #[test]
    fn luma_of_uses_rec709_weights() {
        let p = Plane::luma_of(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 2, 1);
        assert_eq!(p.data, vec![0.2126, 0.7152]);
    }
}
