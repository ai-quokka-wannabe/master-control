/*
    Copyright (C) 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/master-control>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

//! The world owns its transcendentals. The owner's ruling (2026-08-27), after the chain golden
//! life found the edge: `f32::sin` and its kin are the platform's libm, and glibc rounds some
//! arguments a last ulp differently from MSVC's UCRT, so a life recorded on one machine
//! diverged on another at the first hash after a chain rezzed. Everything else that reaches
//! state is IEEE basic arithmetic - add, subtract, multiply, divide, square root, remainder and
//! fused multiply-add - which every machine rounds identically; Rust never fuses or reorders
//! on its own. These functions are built from nothing else, so with them the world replays bit
//! for bit on any machine that runs the same build: per build, any machine, is the promise
//! TOPOLOGY makes from here on.
//!
//! The arithmetic is fdlibm's (Sun's freely distributable libm, whose kernels every serious
//! libm descends from): the argument reduced to an eighth of a turn with a split constant so
//! the subtraction is exact for the small quotients a yaw or a wave phase produces, then the
//! classic minimax polynomials, all evaluated in f64 and rounded once to f32 - accurate to a
//! unit in the last place or so, which is what a libm gives, and the same bits everywhere,
//! which is what a libm does not. The sine is exactly odd and the cosine exactly even, sign of
//! zero included, because the kernels are polynomials in odd and even powers and the reduction
//! is symmetric; `atan2` keeps the IEEE conventions at the axes so nothing that used the
//! platform's changed meaning. Not for perception: the flagship's ears and eyes may use whatever
//! libm they like, because a picture is not state.

// The clippy configuration bans the platform's transcendentals for the whole crate; this module
// is where they are replaced, and its tests hold ours against the platform's within a tolerance.
// The one rounding these functions do, f64 to f32 at the end, is the point of them. The
// constants are quoted as fdlibm publishes them, to more digits than an f64 holds: the parser
// rounds them to the same f64 either way, and a quoted number can be checked against its source.
#![allow(clippy::cast_possible_truncation, clippy::excessive_precision)]

/// Two over pi: the quotient of an angle by a quarter turn. fdlibm's `invpio2` is 2/pi
/// correctly rounded, which is exactly the standard library's constant.
const INV_PIO2: f64 = std::f64::consts::FRAC_2_PI;
/// A quarter turn in three split parts, each with its tail: the first 33 bits, the next 33, the
/// rest - so a product with a quotient under 2^20 is exact at every stage. fdlibm's `pio2_1`,
/// `pio2_1t`, `pio2_2`, `pio2_2t`, `pio2_3`, `pio2_3t`.
const PIO2_1: f64 = 1.570_796_326_734_125_614_17;
const PIO2_1T: f64 = 6.077_100_506_506_192_249_32e-11;
const PIO2_2: f64 = 6.077_100_506_303_965_976_60e-11;
const PIO2_2T: f64 = 2.022_266_248_795_950_631_54e-21;
const PIO2_3: f64 = 2.022_266_248_711_166_455_80e-21;
const PIO2_3T: f64 = 8.478_427_660_368_899_569_97e-32;
/// Below this magnitude (2^20 quarter turns, about 1.65e6 radians - a quarter of a million
/// turns) the split reduction is exact in its products and accurate to the last bit of an f64.
/// Beyond it the remainder path is taken: still the same bits everywhere, no longer accurate,
/// and nothing in the world gets there - a body would have to spin at its top rate for days.
const MEDIUM: f64 = 1_647_099.0;
/// A whole turn in f64, for the far path's remainder (`%` is exact).
const TAU: f64 = std::f64::consts::TAU;

// fdlibm's __kernel_sin coefficients: sin(r) = r + r^3 (S1 + r^2 (S2 + ... )), |r| <= pi/4.
const S1: f64 = -1.666_666_666_666_663_243_48e-01;
const S2: f64 = 8.333_333_333_322_489_461_24e-03;
const S3: f64 = -1.984_126_982_985_794_931_34e-04;
const S4: f64 = 2.755_731_370_707_006_767_89e-06;
const S5: f64 = -2.505_076_025_340_686_341_95e-08;
const S6: f64 = 1.589_690_995_211_550_102_21e-10;
// fdlibm's __kernel_cos coefficients: cos(r) = 1 - r^2/2 + r^4 (C1 + r^2 (C2 + ... )).
const C1: f64 = 4.166_666_666_666_660_190_37e-02;
const C2: f64 = -1.388_888_888_887_410_957_49e-03;
const C3: f64 = 2.480_158_728_947_672_941_78e-05;
const C4: f64 = -2.755_731_435_139_066_330_35e-07;
const C5: f64 = 2.087_572_321_298_174_827_90e-09;
const C6: f64 = -1.135_964_755_778_819_482_65e-11;
// fdlibm's atan coefficients and the split arctangents of 0.5, 1, 1.5 and infinity.
const AT: [f64; 11] = [
    3.333_333_333_333_293_180_27e-01,
    -1.999_999_999_987_648_324_76e-01,
    1.428_571_427_250_346_637_11e-01,
    -1.111_111_040_546_235_578_80e-01,
    9.090_887_133_436_506_561_96e-02,
    -7.691_876_205_044_829_994_95e-02,
    6.661_073_137_387_531_206_69e-02,
    -5.833_570_133_790_573_486_45e-02,
    4.976_877_994_615_932_360_17e-02,
    -3.653_157_274_421_691_552_70e-02,
    1.628_582_011_536_578_236_23e-02,
];
// fdlibm's `atanhi[1]` and `atanhi[3]` are pi/4 and pi/2 correctly rounded, which are exactly
// the standard library's constants.
const ATAN_HI: [f64; 4] = [
    4.636_476_090_008_060_935_15e-01,
    std::f64::consts::FRAC_PI_4,
    9.827_937_232_473_290_540_82e-01,
    std::f64::consts::FRAC_PI_2,
];
const ATAN_LO: [f64; 4] = [
    2.269_877_745_296_168_709_24e-17,
    3.061_616_997_868_383_017_93e-17,
    1.390_331_103_123_099_845_16e-17,
    6.123_233_995_736_766_035_87e-17,
];

/// The biased exponent of an f64, read from its bits: how the reduction judges cancellation.
fn exponent(value: f64) -> i64 {
    ((value.to_bits() >> 52) & 0x7ff) as i64
}

/// A quotient's quadrant, 0 to 3, the sign handled: `rem_euclid` keeps it non-negative.
fn quadrant(quotient: f64) -> u8 {
    (quotient as i64).rem_euclid(4) as u8
}

/// The argument reduced to `[-pi/4, pi/4]` (a hair more with rounding) and the quadrant it
/// came from. fdlibm's medium path: the magnitude's quotient by a quarter turn, then the split
/// constant taken away part by part, each next part only when the previous one cancelled more
/// than sixteen bits, so the remainder keeps a full f64's accuracy. Reduced on the magnitude
/// and mirrored, so an angle and its negative reduce to remainders that are exact negatives in
/// mirrored quadrants - which is what makes the sine exactly odd and the cosine exactly even,
/// the sign of zero included.
fn reduce(x: f64) -> (f64, u8) {
    let magnitude = x.abs();
    let (remainder, n) = if magnitude >= MEDIUM {
        let turn = magnitude % TAU;
        let n = (turn * INV_PIO2).round();
        ((turn - n * PIO2_1) - n * PIO2_1T, n)
    } else {
        let n = (magnitude * INV_PIO2 + 0.5).floor();
        let mut r = magnitude - n * PIO2_1;
        let mut w = n * PIO2_1T;
        let mut y = r - w;
        let j = exponent(magnitude);
        if j - exponent(y) > 16 {
            let t = r;
            w = n * PIO2_2;
            r = t - w;
            w = n * PIO2_2T - ((t - r) - w);
            y = r - w;
            if j - exponent(y) > 49 {
                let t = r;
                w = n * PIO2_3;
                r = t - w;
                w = n * PIO2_3T - ((t - r) - w);
                y = r - w;
            }
        }
        (y, n)
    };
    if x.is_sign_negative() {
        (-remainder, quadrant(-n))
    } else {
        (remainder, quadrant(n))
    }
}

fn kernel_sin(r: f64) -> f64 {
    let z = r * r;
    let v = z * r;
    r + v * (S1 + z * (S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)))))
}

fn kernel_cos(r: f64) -> f64 {
    let z = r * r;
    let poly = C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6))));
    // fdlibm's arrangement: the half-square and the polynomial's correction are combined first
    // and taken from one last, so the large terms cancel before the small one is added.
    1.0 - (0.5 * z - z * z * poly)
}

/// The sine and cosine of an angle in radians, the same bits on every machine.
#[must_use]
pub fn sin_cos(x: f32) -> (f32, f32) {
    if !x.is_finite() {
        return (f32::NAN, f32::NAN);
    }
    if x == 0.0 {
        // The kernel's polynomial would add a positive zero to a negative one and lose the
        // sign; a zero angle's sine is that zero, as fdlibm answers for any tiny argument.
        return (x, 1.0);
    }
    let (r, quadrant) = reduce(f64::from(x));
    let s = kernel_sin(r);
    let c = kernel_cos(r);
    let (sine, cosine) = match quadrant {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    };
    (sine as f32, cosine as f32)
}

/// The sine of an angle in radians, the same bits on every machine; exactly odd, `-0.0` kept.
#[must_use]
pub fn sin(x: f32) -> f32 {
    sin_cos(x).0
}

/// The cosine of an angle in radians, the same bits on every machine; exactly even.
#[must_use]
pub fn cos(x: f32) -> f32 {
    sin_cos(x).1
}

/// fdlibm's atan on a non-negative argument, in f64.
fn atan_positive(t: f64) -> f64 {
    if t.is_infinite() {
        return ATAN_HI[3] + ATAN_LO[3];
    }
    // The range the argument falls in, and the argument transformed into it.
    let (id, t) = if t < 0.4375 {
        (None, t)
    } else if t < 0.6875 {
        (Some(0), (2.0 * t - 1.0) / (2.0 + t))
    } else if t < 1.1875 {
        (Some(1), (t - 1.0) / (t + 1.0))
    } else if t < 2.4375 {
        (Some(2), (t - 1.5) / (1.0 + 1.5 * t))
    } else {
        (Some(3), -1.0 / t)
    };
    let z = t * t;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    match id {
        None => t - t * (s1 + s2),
        Some(id) => ATAN_HI[id] - ((t * (s1 + s2) - ATAN_LO[id]) - t),
    }
}

/// The angle of the point `(x, y)` from the +x axis, in radians, `[-pi, pi]`, the same bits on
/// every machine. The IEEE conventions at the axes are the platform's: `atan2(±0, +0)` is `±0`,
/// `atan2(±0, -0)` and `atan2(±0, negative)` are `±pi`, `atan2(y, ±0)` is `±pi/2` by `y`'s sign.
#[must_use]
pub fn atan2(y: f32, x: f32) -> f32 {
    if x.is_nan() || y.is_nan() {
        return f32::NAN;
    }
    let (yd, xd) = (f64::from(y), f64::from(x));
    let half_pi = ATAN_HI[3] + ATAN_LO[3];
    let pi = 2.0 * half_pi;
    let magnitude = if yd == 0.0 {
        // On the x axis: zero towards +x, a half turn towards -x; the sign of zero says which
        // side, below.
        if xd.is_sign_negative() { pi } else { 0.0 }
    } else if xd == 0.0 {
        half_pi
    } else {
        let (ay, ax) = (yd.abs(), xd.abs());
        // The ratio never overflows: the larger magnitude is always the divisor.
        let z = if ay > ax {
            half_pi - atan_positive(ax / ay)
        } else {
            atan_positive(ay / ax)
        };
        if xd.is_sign_negative() { pi - z } else { z }
    };
    let angle = if yd.is_sign_negative() {
        -magnitude
    } else {
        magnitude
    };
    angle as f32
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // The platform's own, as the yardstick.
mod tests {
    use super::*;

    /// How many representable f32 values apart two finite values are, across zero included.
    fn ulps(a: f32, b: f32) -> u32 {
        // Sign-magnitude bits to a monotone integer: a negative float's bits, read as i32, are
        // i32::MIN plus its magnitude, so i32::MIN minus them counts downwards from -0.0.
        let monotone = |value: f32| {
            let bits = i64::from(value.to_bits() as i32);
            if bits < 0 {
                i64::from(i32::MIN) - bits
            } else {
                bits
            }
        };
        u32::try_from((monotone(a) - monotone(b)).abs()).expect("small")
    }

    fn sweep() -> impl Iterator<Item = f32> {
        (0..400_000).map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f32;
            // Irregular spacing across a wide range, both signs, through zero.
            (t * 0.037_1 - 7_400.0) * (1.0 + 0.001 * (t % 7.0))
        })
    }

    #[test]
    fn ours_stand_within_two_ulps_of_the_platforms_across_the_sweep() {
        for x in sweep() {
            let (s, c) = sin_cos(x);
            assert!(ulps(s, x.sin()) <= 2, "sin({x}) = {s} vs {}", x.sin());
            assert!(ulps(c, x.cos()) <= 2, "cos({x}) = {c} vs {}", x.cos());
            assert_eq!(s, sin(x));
            assert_eq!(c, cos(x));
        }
        for y in [-3.7f32, -1.0, -1e-3, 0.5, 1.0, 2.5, 9.0, 1e4] {
            for x in [-9.0f32, -2.5, -1.0, -1e-3, 1e-3, 1.0, 3.0, 1e4] {
                let ours = atan2(y, x);
                assert!(
                    ulps(ours, y.atan2(x)) <= 2,
                    "atan2({y}, {x}) = {ours} vs {}",
                    y.atan2(x)
                );
            }
        }
    }

    #[test]
    fn the_sine_is_exactly_odd_the_cosine_exactly_even_and_zero_keeps_its_sign() {
        for x in sweep().step_by(7) {
            assert_eq!(sin(-x).to_bits(), (-sin(x)).to_bits(), "sin is odd at {x}");
            assert_eq!(cos(-x).to_bits(), cos(x).to_bits(), "cos is even at {x}");
        }
        assert_eq!(sin(0.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(sin(-0.0).to_bits(), (-0.0f32).to_bits());
        assert_eq!(cos(0.0), 1.0);
        assert_eq!(cos(-0.0), 1.0);
    }

    #[test]
    fn the_axes_keep_the_ieee_conventions() {
        let pi = std::f32::consts::PI;
        let half = std::f32::consts::FRAC_PI_2;
        assert_eq!(atan2(0.0, 1.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(atan2(-0.0, 1.0).to_bits(), (-0.0f32).to_bits());
        assert_eq!(atan2(0.0, 0.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(atan2(-0.0, 0.0).to_bits(), (-0.0f32).to_bits());
        assert_eq!(atan2(0.0, -0.0), pi);
        assert_eq!(atan2(-0.0, -0.0), -pi);
        assert_eq!(atan2(0.0, -1.0), pi);
        assert_eq!(atan2(-0.0, -1.0), -pi);
        assert_eq!(atan2(1.0, 0.0), half);
        assert_eq!(atan2(-1.0, 0.0), -half);
        assert_eq!(atan2(1.0, -0.0), half);
        assert!(ulps(atan2(1.0, 1.0), std::f32::consts::FRAC_PI_4) <= 1);
        assert!(ulps(atan2(-1.0, -1.0), -3.0 * std::f32::consts::FRAC_PI_4) <= 1);
        assert!(atan2(f32::NAN, 1.0).is_nan());
        assert!(sin(f32::INFINITY).is_nan());
        // Far out, still finite and still a sine: the reduction does not fall over.
        for x in [1e6f32, -1e6, 3.0e8, f32::MAX] {
            let (s, c) = sin_cos(x);
            assert!(
                (-1.0..=1.0).contains(&s) && (-1.0..=1.0).contains(&c),
                "{x}: {s} {c}"
            );
        }
    }
}
