//! Color science. All transcendental math goes through `libm` so native and wasm agree
//! bit-for-bit (master plan §5.1).

/// sRGB EOTF, encoded → linear. **Not clamped** (master plan A3). Extended sign-symmetrically
/// as in W3C CSS Color 4: `decode(-v) = -decode(v)`; values above 1 continue the power curve.
pub fn srgb_decode(v: f32) -> f32 {
    let a = v.abs();
    let lin = if a <= 0.040_45 {
        a / 12.92
    } else {
        libm::powf((a + 0.055) / 1.055, 2.4)
    };
    if v < 0.0 {
        -lin
    } else {
        lin
    }
}

/// sRGB OETF, linear → encoded. **Not clamped**, sign-symmetric like [`srgb_decode`].
pub fn srgb_encode(v: f32) -> f32 {
    let a = v.abs();
    let enc = if a <= 0.003_130_8 {
        a * 12.92
    } else {
        1.055 * libm::powf(a, 1.0 / 2.4) - 0.055
    };
    if v < 0.0 {
        -enc
    } else {
        enc
    }
}

/// CIE L*a*b* under the D50 illuminant (ICC PCS). Used for CIEDE2000.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    /// Lightness, 0..100.
    pub l: f64,
    /// Green-red axis.
    pub a: f64,
    /// Blue-yellow axis.
    pub b: f64,
}

// Linear sRGB → XYZ (D65), IEC 61966-2-1.
const M_RGB_TO_XYZ_D65: [[f64; 3]; 3] = [
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192_0, 0.950_304_1],
];

// Bradford chromatic adaptation D65 → D50 (Lindbloom).
const M_BRADFORD_D65_TO_D50: [[f64; 3]; 3] = [
    [1.047_811_2, 0.022_886_6, -0.050_127_0],
    [0.029_542_4, 0.990_484_4, -0.017_049_1],
    [-0.009_234_5, 0.015_043_6, 0.752_131_6],
];

const D50_WHITE: [f64; 3] = [0.964_22, 1.0, 0.825_21];

fn mul3(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn lab_f(t: f64) -> f64 {
    const DELTA: f64 = 6.0 / 29.0;
    if t > DELTA * DELTA * DELTA {
        libm::cbrt(t)
    } else {
        t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

/// Linear sRGB (D65) → XYZ D65 → Bradford → XYZ D50 → Lab D50 (AC-S1f).
pub fn linear_srgb_to_lab_d50(r: f32, g: f32, b: f32) -> Lab {
    let xyz65 = mul3(
        &M_RGB_TO_XYZ_D65,
        [f64::from(r), f64::from(g), f64::from(b)],
    );
    let xyz50 = mul3(&M_BRADFORD_D65_TO_D50, xyz65);
    let fx = lab_f(xyz50[0] / D50_WHITE[0]);
    let fy = lab_f(xyz50[1] / D50_WHITE[1]);
    let fz = lab_f(xyz50[2] / D50_WHITE[2]);
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// CIEDE2000 color difference with `kL = kC = kH = 1` (AC-S1f). f64 + `libm` throughout.
pub fn delta_e00(c1: Lab, c2: Lab) -> f64 {
    use libm::{atan2, cos, exp, hypot, pow, sin, sqrt};
    use std::f64::consts::PI;
    const TWO_PI: f64 = 2.0 * PI;
    const POW25_7: f64 = 6_103_515_625.0; // 25^7

    let hue = |a: f64, b: f64| -> f64 {
        if a == 0.0 && b == 0.0 {
            0.0
        } else {
            let h = atan2(b, a);
            if h < 0.0 {
                h + TWO_PI
            } else {
                h
            }
        }
    };

    let cbar = (hypot(c1.a, c1.b) + hypot(c2.a, c2.b)) / 2.0;
    let cbar7 = pow(cbar, 7.0);
    let g = 0.5 * (1.0 - sqrt(cbar7 / (cbar7 + POW25_7)));

    let a1p = (1.0 + g) * c1.a;
    let a2p = (1.0 + g) * c2.a;
    let c1p = hypot(a1p, c1.b);
    let c2p = hypot(a2p, c2.b);
    let h1p = hue(a1p, c1.b);
    let h2p = hue(a2p, c2.b);

    let dlp = c2.l - c1.l;
    let dcp = c2p - c1p;
    let dhp = if c1p * c2p == 0.0 {
        0.0
    } else {
        let d = h2p - h1p;
        if d.abs() <= PI {
            d
        } else if d > PI {
            d - TWO_PI
        } else {
            d + TWO_PI
        }
    };
    let dhp_big = 2.0 * sqrt(c1p * c2p) * sin(dhp / 2.0);

    let lbp = (c1.l + c2.l) / 2.0;
    let cbp = (c1p + c2p) / 2.0;
    let hbp = if c1p * c2p == 0.0 {
        h1p + h2p
    } else {
        let s = h1p + h2p;
        if (h1p - h2p).abs() <= PI {
            s / 2.0
        } else if s < TWO_PI {
            (s + TWO_PI) / 2.0
        } else {
            (s - TWO_PI) / 2.0
        }
    };

    let t = 1.0 - 0.17 * cos(hbp - PI / 6.0)
        + 0.24 * cos(2.0 * hbp)
        + 0.32 * cos(3.0 * hbp + PI / 30.0)
        - 0.20 * cos(4.0 * hbp - 63.0 * PI / 180.0);
    let hbp_deg = hbp * 180.0 / PI;
    let dtheta = (30.0 * PI / 180.0) * exp(-pow((hbp_deg - 275.0) / 25.0, 2.0));
    let cbp7 = pow(cbp, 7.0);
    let rc = 2.0 * sqrt(cbp7 / (cbp7 + POW25_7));
    let lbp50 = pow(lbp - 50.0, 2.0);
    let sl = 1.0 + 0.015 * lbp50 / sqrt(20.0 + lbp50);
    let sc = 1.0 + 0.045 * cbp;
    let sh = 1.0 + 0.015 * cbp * t;
    let rt = -sin(2.0 * dtheta) * rc;

    let tl = dlp / sl;
    let tc = dcp / sc;
    let th = dhp_big / sh;
    sqrt(tl * tl + tc * tc + th * th + rt * tc * th)
}

/// Sharma 2005 vectors, shared by the native unit tests and the wasm test suite.
#[doc(hidden)]
pub const SHARMA_2005_CSV: &str = include_str!("../testdata/sharma2005.csv");

/// Parses [`SHARMA_2005_CSV`] into `(c1, c2, expected_de00)` rows (test support).
#[doc(hidden)]
pub fn sharma_2005_rows() -> Vec<(Lab, Lab, f64)> {
    SHARMA_2005_CSV
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let v: Vec<f64> = l.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 7).then(|| {
                let c1 = Lab {
                    l: v[0],
                    a: v[1],
                    b: v[2],
                };
                let c2 = Lab {
                    l: v[3],
                    a: v[4],
                    b: v[5],
                };
                (c1, c2, v[6])
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab8(r: u8, g: u8, b: u8) -> Lab {
        let d = |v: u8| srgb_decode(f32::from(v) / 255.0);
        linear_srgb_to_lab_d50(d(r), d(g), d(b))
    }

    fn assert_lab_close(got: Lab, l: f64, a: f64, b: f64) {
        let ok = (got.l - l).abs() < 0.05 && (got.a - a).abs() < 0.05 && (got.b - b).abs() < 0.05;
        assert!(ok, "got {got:?}, expected ({l}, {a}, {b})");
    }

    mod srgb_decode {
        use super::*;

        #[test]
        fn maps_zero_to_zero() {
            assert_eq!(srgb_decode(0.0), 0.0);
        }

        #[test]
        fn maps_mid_gray_128_to_known_linear_value() {
            let lin = srgb_decode(128.0 / 255.0);
            assert!((lin - 0.215_86).abs() < 1e-4, "got {lin}");
        }

        #[test]
        fn is_continuous_at_the_knee() {
            let below = srgb_decode(0.040_45);
            let above = srgb_decode(0.040_450_1);
            assert!((above - below).abs() < 1e-6, "{below} {above}");
        }

        #[test]
        fn extends_above_one_without_clamping() {
            assert!(srgb_decode(1.2) > 1.0);
        }

        #[test]
        fn is_sign_symmetric_below_zero() {
            for v in [0.01f32, 0.04, 0.2, 0.5, 1.3] {
                assert_eq!(srgb_decode(-v), -srgb_decode(v), "v={v}");
            }
        }
    }

    mod srgb_encode {
        use super::*;

        #[test]
        fn is_inverse_of_decode_on_every_8bit_level() {
            for i in 0..=255u8 {
                let v = f32::from(i) / 255.0;
                let back = srgb_encode(srgb_decode(v));
                assert!((back - v).abs() < 2e-6, "i={i} v={v} back={back}");
            }
        }

        #[test]
        fn is_continuous_at_the_knee() {
            let below = srgb_encode(0.003_130_8);
            let above = srgb_encode(0.003_130_81);
            assert!((above - below).abs() < 1e-6, "{below} {above}");
        }

        #[test]
        fn extends_above_one_without_clamping() {
            assert!(srgb_encode(2.0) > 1.0);
        }

        #[test]
        fn is_sign_symmetric_below_zero() {
            for v in [0.001f32, 0.003, 0.05, 0.5, 2.0] {
                assert_eq!(srgb_encode(-v), -srgb_encode(v), "v={v}");
            }
        }
    }

    // Expected values come from an independent float64 derivation (sRGB primaries + D65 xy →
    // RGB→XYZ; Bradford cone matrix → D65→D50), not from the constants above. Tolerance 0.05
    // covers the white-point rounding difference between the two.
    mod linear_srgb_to_lab_d50 {
        use super::*;

        #[test]
        fn white_is_l100_neutral() {
            assert_lab_close(lab8(255, 255, 255), 100.0, 0.0, 0.0);
        }

        #[test]
        fn black_is_l0() {
            assert_lab_close(lab8(0, 0, 0), 0.0, 0.0, 0.0);
        }

        #[test]
        fn mid_gray_128() {
            assert_lab_close(lab8(128, 128, 128), 53.585, 0.0, 0.0);
        }

        #[test]
        fn pure_red() {
            assert_lab_close(lab8(255, 0, 0), 54.289, 80.811, 69.888);
        }

        #[test]
        fn pure_green() {
            assert_lab_close(lab8(0, 255, 0), 87.819, -79.280, 80.996);
        }

        #[test]
        fn pure_blue() {
            assert_lab_close(lab8(0, 0, 255), 29.569, 68.297, -112.028);
        }

        #[test]
        fn warm_tone_200_150_100() {
            assert_lab_close(lab8(200, 150, 100), 66.126, 14.996, 33.950);
        }

        #[test]
        fn dark_blue_gray_10_20_30() {
            assert_lab_close(lab8(10, 20, 30), 5.851, -1.496, -8.255);
        }
    }

    mod delta_e00 {
        use super::*;

        #[test]
        fn data_file_has_34_rows() {
            assert_eq!(sharma_2005_rows().len(), 34);
        }

        #[test]
        fn matches_all_sharma_vectors_within_1e_4() {
            for (i, (c1, c2, want)) in sharma_2005_rows().into_iter().enumerate() {
                let got = delta_e00(c1, c2);
                assert!(
                    (got - want).abs() <= 1e-4,
                    "pair {}: want {want} got {got}",
                    i + 1
                );
            }
        }

        #[test]
        fn matches_all_sharma_vectors_with_arguments_swapped() {
            for (i, (c1, c2, want)) in sharma_2005_rows().into_iter().enumerate() {
                let got = delta_e00(c2, c1);
                assert!(
                    (got - want).abs() <= 1e-4,
                    "pair {} swapped: want {want} got {got}",
                    i + 1
                );
            }
        }

        #[test]
        fn identical_colors_have_zero_difference() {
            let c = Lab {
                l: 40.0,
                a: -12.0,
                b: 30.0,
            };
            assert_eq!(delta_e00(c, c), 0.0);
        }

        #[test]
        fn achromatic_pair_reduces_to_the_lightness_term() {
            // Both chroma 0: hue terms vanish, dE = |dL| / SL with L-bar = 55.
            let c1 = Lab {
                l: 50.0,
                a: 0.0,
                b: 0.0,
            };
            let c2 = Lab {
                l: 60.0,
                a: 0.0,
                b: 0.0,
            };
            let sl = 1.0 + 0.015 * 25.0 / libm::sqrt(45.0);
            assert!((delta_e00(c1, c2) - 10.0 / sl).abs() < 1e-12);
        }
    }
}
