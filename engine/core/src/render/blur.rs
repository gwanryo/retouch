//! Deterministic blur primitives for the detail stages (texture, clarity, sharpening).
//!
//! Summation order is part of the contract: the same math summed in a different order gives
//! different bits. Box sums run in f64 (IEEE add/sub/div are deterministic) and are stored
//! back as f32; the Gaussian accumulates in f32 in ascending tap order.

use super::basic;

/// Single-channel row-major f32 image.
#[derive(Clone, Debug, PartialEq)]
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

/// `box_pass` without per-sample clamping in the interior: the same samples summed in the same
/// order, so every output is bit-identical.
fn box_pass_fast(line: &[f32], out: &mut [f32], radius: usize) {
    let n = line.len();
    if 2 * radius + 2 > n {
        return box_pass(line, out, radius);
    }
    let r = radius;
    let div = (2 * r + 1) as f64;
    let first = f64::from(line[0]);
    let last = f64::from(line[n - 1]);
    let mut s = 0.0f64;
    for _ in 0..r {
        s += first; // k = -r..-1 clamps to 0
    }
    for v in &line[..=r] {
        s += f64::from(*v);
    }
    for x in 0..r {
        out[x] = (s / div) as f32;
        s += f64::from(line[x + r + 1]);
        s -= first;
    }
    for x in r..n - r - 1 {
        out[x] = (s / div) as f32;
        s += f64::from(line[x + r + 1]);
        s -= f64::from(line[x - r]);
    }
    for x in n - r - 1..n {
        out[x] = (s / div) as f32;
        s += last;
        s -= f64::from(line[x - r]);
    }
}

/// Runs a 1-D pass over every row of `data` in place, through one scratch line of width `w`.
fn rows_in_place(data: &mut [f32], line: &mut [f32], pass: impl Fn(&[f32], &mut [f32])) {
    for row in data.chunks_exact_mut(line.len()) {
        line.copy_from_slice(row);
        pass(line, row);
    }
}

/// The most recent `slots` input rows of a plane whose rows are overwritten in place from top to
/// bottom. Each row is copied in before its output replaces it, so column passes read inputs only.
struct RowRing {
    width: usize,
    slots: usize,
    rows: Vec<f32>,
    /// Rows `[0, loaded)` have been copied in.
    loaded: usize,
}

impl RowRing {
    fn new(width: usize, slots: usize) -> Self {
        Self {
            width,
            slots,
            rows: vec![0.0; width * slots],
            loaded: 0,
        }
    }

    /// Copies input rows up to and including `y` (`y` below the plane height).
    fn load_through(&mut self, data: &[f32], y: usize) {
        let w = self.width;
        while self.loaded <= y {
            let o = (self.loaded % self.slots) * w;
            let src = self.loaded * w;
            self.rows[o..o + w].copy_from_slice(&data[src..src + w]);
            self.loaded += 1;
        }
    }

    /// Input row `y`; valid while fewer than `slots` rows have been loaded after it.
    fn row(&self, y: usize) -> &[f32] {
        let o = (y % self.slots) * self.width;
        &self.rows[o..o + self.width]
    }
}

/// In-place vertical box pass over all columns at once, row by row. Per column the operations
/// are those of `box_pass`: initial sum in ascending `k`, then output, add, subtract, in f64.
fn box_columns_in_place(data: &mut [f32], w: usize, h: usize, radius: usize, sums: &mut [f64]) {
    let hi = h - 1;
    let r = radius as isize;
    let div = (2 * radius + 1) as f64;
    let clamp_row = |k: isize| k.clamp(0, hi as isize) as usize;
    // Rows y - r ..= y + r + 1 are live at step y.
    let mut ring = RowRing::new(w, (2 * radius + 2).min(h));
    ring.load_through(data, radius.min(hi));
    sums.fill(0.0);
    for k in -r..=r {
        for (s, v) in sums.iter_mut().zip(ring.row(clamp_row(k))) {
            *s += f64::from(*v);
        }
    }
    for y in 0..h {
        let add_y = clamp_row(y as isize + r + 1);
        let sub_y = clamp_row(y as isize - r);
        ring.load_through(data, add_y);
        let out = &mut data[y * w..(y + 1) * w];
        for (((o, s), p), m) in out
            .iter_mut()
            .zip(sums.iter_mut())
            .zip(ring.row(add_y))
            .zip(ring.row(sub_y))
        {
            *o = (*s / div) as f32;
            *s += f64::from(*p);
            *s -= f64::from(*m);
        }
    }
}

/// Three rounds of (horizontal, vertical) box blur with the given radius in pixels.
///
/// Bit-identical to `box_pass` on every row and then every column: rows run in place, columns
/// all at once row by row from a ring of input rows, so no second plane is allocated.
pub(crate) fn box_blur3(src: &Plane, radius: usize) -> Plane {
    let (w, h) = (src.width, src.height);
    let mut data = src.data.clone();
    let mut line = vec![0.0f32; w];
    let mut sums = vec![0.0f64; w];
    for _ in 0..3 {
        rows_in_place(&mut data, &mut line, |l, o| box_pass_fast(l, o, radius));
        box_columns_in_place(&mut data, w, h, radius, &mut sums);
    }
    Plane {
        width: w,
        height: h,
        data,
    }
}

/// Normalized Gaussian kernel of half-width `ceil(3 * sigma)` (length `2R + 1`).
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

/// `gaussian_pass` without clamping in the interior (same taps, same ascending order).
fn gaussian_pass_fast(line: &[f32], out: &mut [f32], kernel: &[f32]) {
    let n = line.len();
    let r = kernel.len() / 2;
    if 2 * r + 1 > n {
        return gaussian_pass(line, out, kernel);
    }
    let edge = |x: usize| {
        let mut acc = 0.0f32;
        for (j, kj) in kernel.iter().enumerate() {
            acc += *kj * line[(x + j).saturating_sub(r).min(n - 1)];
        }
        acc
    };
    for x in (0..r).chain(n - r..n) {
        out[x] = edge(x);
    }
    for x in r..n - r {
        let mut acc = 0.0f32;
        for (kj, v) in kernel.iter().zip(&line[x - r..=x + r]) {
            acc += *kj * *v;
        }
        out[x] = acc;
    }
}

/// Separable Gaussian blur: horizontal then vertical.
///
/// Bit-identical to `gaussian_pass` on every row and then every column: per pixel the column
/// taps accumulate from `0.0` in ascending order. Columns run all at once row by row from a ring
/// of input rows, so no second plane is allocated.
pub(crate) fn gaussian_blur(src: &Plane, kernel: &[f32]) -> Plane {
    let (w, h) = (src.width, src.height);
    let mut data = src.data.clone();
    let mut line = vec![0.0f32; w];
    rows_in_place(&mut data, &mut line, |l, o| {
        gaussian_pass_fast(l, o, kernel);
    });
    let r = kernel.len() / 2;
    let hi = h - 1;
    // Rows y - r ..= y + r are live at step y.
    let mut ring = RowRing::new(w, (2 * r + 1).min(h));
    let acc = &mut line;
    for y in 0..h {
        ring.load_through(&data, (y + r).min(hi));
        acc.fill(0.0);
        for (j, kj) in kernel.iter().enumerate() {
            for (a, v) in acc
                .iter_mut()
                .zip(ring.row((y + j).saturating_sub(r).min(hi)))
            {
                *a += *kj * *v;
            }
        }
        data[y * w..(y + 1) * w].copy_from_slice(acc);
    }
    Plane {
        width: w,
        height: h,
        data,
    }
}

/// Applies `pass` to every row, then every column, through scratch lines (never in place).
#[cfg(test)]
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

/// The original column-gather box blur: the oracle for [`box_blur3`].
#[cfg(test)]
fn box_blur3_ref(src: &Plane, radius: usize) -> Plane {
    let mut p = src.clone();
    for _ in 0..3 {
        separable(&mut p, &|line, out| box_pass(line, out, radius));
    }
    p
}

/// The original column-gather Gaussian blur: the oracle for [`gaussian_blur`].
#[cfg(test)]
fn gaussian_blur_ref(src: &Plane, kernel: &[f32]) -> Plane {
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

    /// Shapes covering 1x1, single rows/columns, non-square planes and radii above the size.
    const SHAPES: [(usize, usize); 9] = [
        (1, 1),
        (5, 1),
        (1, 5),
        (2, 3),
        (5, 5),
        (17, 9),
        (9, 50),
        (64, 40),
        (130, 70),
    ];

    fn pattern(w: usize, h: usize) -> Plane {
        let data = (0..w * h)
            .map(|i| ((i * 7919 % 1000) as f32 / 333.0) - 0.7)
            .collect();
        plane(w, h, data)
    }

    fn same_bits(a: &Plane, b: &Plane) -> bool {
        a.width == b.width
            && a.height == b.height
            && a.data.len() == b.data.len()
            && a.data
                .iter()
                .zip(&b.data)
                .all(|(x, y)| x.to_bits() == y.to_bits())
    }

    #[test]
    fn box_blur3_is_bit_identical_to_the_column_reference() {
        for (w, h) in SHAPES {
            let p = pattern(w, h);
            for r in [1usize, 2, 4, 24, 80] {
                assert!(
                    same_bits(&box_blur3(&p, r), &box_blur3_ref(&p, r)),
                    "{w}x{h} r{r}"
                );
            }
        }
    }

    #[test]
    fn gaussian_blur_is_bit_identical_to_the_column_reference() {
        for (w, h) in SHAPES {
            let p = pattern(w, h);
            for sigma in [0.5f32, 1.0, 1.2, 3.0, 20.0] {
                let k = gaussian_kernel(sigma);
                assert!(
                    same_bits(&gaussian_blur(&p, &k), &gaussian_blur_ref(&p, &k)),
                    "{w}x{h} sigma {sigma}"
                );
            }
        }
    }

    #[test]
    fn luma_of_uses_rec709_weights() {
        let p = Plane::luma_of(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 2, 1);
        assert_eq!(p.data, vec![0.2126, 0.7152]);
    }
}
