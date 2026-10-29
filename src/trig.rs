/*
    Copyright (C) 2026 The AiQuokkaWannabe Authors <https://github.com/ai-quokka-wannabe/master-control>

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
//! platform's changed meaning. `exp` and `erfc` joined them for the odour field (odour.rs,
//! the flagship's CHEMORECEPTION.md): fdlibm's `e_exp.c` and `s_erf.c`, kept in f64 and
//! returned in f64, because the field is a sum of products whose range an f32 would clip and
//! whose result is rounded to f32 only where it leaves as a reading. Not for perception: the
//! flagship's ears and eyes may use whatever libm they like, because a picture is not state.

// The clippy configuration bans the platform's transcendentals for the whole crate; this module
// is where they are replaced, and its tests hold ours against the platform's within a tolerance.
// The one rounding the angles do, f64 to f32 at the end, is the point of them. The
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

// fdlibm's e_exp.c: ln 2 split into a head whose product with any reduction quotient is exact
// and a tail, its reciprocal, the over- and underflow thresholds, and the Remez coefficients of
// the rational kernel on the reduced range. fdlibm's `invln2` is 1/ln 2 correctly rounded, which
// is exactly the standard library's log2(e).
const LN2_HI: f64 = 6.931_471_803_691_238_164_90e-01;
const LN2_LO: f64 = 1.908_214_929_270_587_700_02e-10;
const INV_LN2: f64 = std::f64::consts::LOG2_E;
const EXP_OVERFLOW: f64 = 7.097_827_128_933_839_730_96e+02;
const EXP_UNDERFLOW: f64 = -7.451_332_191_019_411_084_20e+02;
const P1: f64 = 1.666_666_666_666_660_190_37e-01;
const P2: f64 = -2.777_777_777_701_559_338_42e-03;
const P3: f64 = 6.613_756_321_437_934_361_17e-05;
const P4: f64 = -1.653_390_220_546_525_153_90e-06;
const P5: f64 = 4.138_136_797_057_238_460_39e-08;
/// Two to the minus a thousand: the step fdlibm takes in two to land a deep underflow in the
/// subnormals with a single rounding.
const TWO_M1000: f64 = 9.332_636_185_032_188_789_90e-302;

/// The high 32 bits of an f64, which is how fdlibm reads a magnitude's range at a glance.
fn high_word(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}

/// Two to an integer power in the normal range, built from its bits: exact.
fn two_to(k: i32) -> f64 {
    debug_assert!((-1022..=1023).contains(&k));
    #[allow(clippy::cast_sign_loss)]
    let biased = (1023 + i64::from(k)) as u64;
    f64::from_bits(biased << 52)
}

/// e to the power `x`, the same bits on every machine: fdlibm's `__ieee754_exp`, in f64,
/// within an ulp of the true value. The argument is reduced by whole multiples of ln 2 with the
/// split constant, `x = k ln2 + r` with `|r| <= ln2 / 2`; e to the `r` is a rational function
/// of a Remez polynomial; the multiple is put back as two to the `k`, exactly, through the
/// exponent. fdlibm adds `k` to the exponent's bits; a multiplication by a power of two is the
/// same operation wherever the result is normal, and the subnormal tail takes fdlibm's own
/// detour through two to the minus a thousand. Infinities and NaN keep the IEEE answers.
#[must_use]
pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > EXP_OVERFLOW {
        return f64::INFINITY;
    }
    if x < EXP_UNDERFLOW {
        return 0.0;
    }
    let magnitude = high_word(x) & 0x7fff_ffff;
    let negative = x.is_sign_negative();
    let (reduced, hi, lo, k) = if magnitude > 0x3fd6_2e42 {
        // |x| > ln2 / 2: reduce. Under 1.5 ln2 the quotient is one, by the sign.
        let (hi, lo, k) = if magnitude < 0x3ff0_a2b2 {
            if negative {
                (x + LN2_HI, -LN2_LO, -1)
            } else {
                (x - LN2_HI, LN2_LO, 1)
            }
        } else {
            let half = if negative { -0.5 } else { 0.5 };
            // Truncation towards zero after the half: fdlibm's `(int)(invln2*x+halF[xsb])`.
            let k = (INV_LN2 * x + half) as i32;
            let t = f64::from(k);
            (x - t * LN2_HI, t * LN2_LO, k)
        };
        (hi - lo, hi, lo, k)
    } else if magnitude < 0x3e30_0000 {
        // |x| < 2^-28: e to the x is one and x, to the last bit.
        return 1.0 + x;
    } else {
        (x, 0.0, 0.0, 0)
    };
    let t = reduced * reduced;
    let c = reduced - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return 1.0 - ((reduced * c) / (c - 2.0) - reduced);
    }
    let y = 1.0 - ((lo - (reduced * c) / (2.0 - c)) - hi);
    if k > 1023 {
        // k reaches 1024 only just under the overflow threshold, where y is under one: the
        // last doubling is still exact.
        y * two_to(1023) * 2.0
    } else if k >= -1021 {
        y * two_to(k)
    } else {
        y * two_to(k + 1000) * TWO_M1000
    }
}

// fdlibm's s_erf.c: erx is erf(1) to single precision, the split point of the middle range;
// then the rational approximations' coefficients on [0, 0.84375] (pp, qq), [0.84375, 1.25]
// (pa, qa), [1.25, 1/0.35] (ra, sa) and [1/0.35, 28] (rb, sb).
const ERX: f64 = 8.450_629_115_104_675_292_97e-01;
const PP: [f64; 5] = [
    1.283_791_670_955_125_585_61e-01,
    -3.250_421_072_470_014_993_70e-01,
    -2.848_174_957_559_851_047_66e-02,
    -5.770_270_296_489_441_591_57e-03,
    -2.376_301_665_665_016_260_84e-05,
];
const QQ: [f64; 5] = [
    3.979_172_239_591_553_528_19e-01,
    6.502_224_998_876_729_444_85e-02,
    5.081_306_281_875_765_627_76e-03,
    1.324_947_380_043_216_445_26e-04,
    -3.960_228_278_775_368_123_20e-06,
];
const PA: [f64; 7] = [
    -2.362_118_560_752_659_440_77e-03,
    4.148_561_186_837_483_316_66e-01,
    -3.722_078_760_357_013_238_47e-01,
    3.183_466_199_011_617_536_74e-01,
    -1.108_946_942_823_966_774_76e-01,
    3.547_830_432_561_823_593_71e-02,
    -2.166_375_594_868_790_843_00e-03,
];
const QA: [f64; 6] = [
    1.064_208_804_008_442_282_86e-01,
    5.403_979_177_021_710_489_37e-01,
    7.182_865_441_419_626_628_68e-02,
    1.261_712_198_087_616_421_12e-01,
    1.363_708_391_202_905_073_62e-02,
    1.198_449_984_679_910_741_70e-02,
];
const RA: [f64; 8] = [
    -9.864_944_034_847_148_227_05e-03,
    -6.938_585_727_071_817_643_72e-01,
    -1.055_862_622_532_329_098_14e+01,
    -6.237_533_245_032_600_603_96e+01,
    -1.623_966_694_625_734_703_55e+02,
    -1.846_050_929_067_110_359_94e+02,
    -8.128_743_550_630_659_342_46e+01,
    -9.814_329_344_169_145_485_92e+00,
];
const SA: [f64; 8] = [
    1.965_127_166_743_925_712_92e+01,
    1.376_577_541_435_190_426_00e+02,
    4.345_658_774_752_292_288_21e+02,
    6.453_872_717_332_678_803_36e+02,
    4.290_081_400_275_678_333_86e+02,
    1.086_350_055_417_794_351_34e+02,
    6.570_249_770_319_281_701_35e+00,
    -6.042_441_521_485_809_874_38e-02,
];
const RB: [f64; 7] = [
    -9.864_942_924_700_099_285_97e-03,
    -7.992_832_376_805_230_065_74e-01,
    -1.775_795_491_775_475_198_89e+01,
    -1.606_363_848_558_219_160_62e+02,
    -6.375_664_433_683_896_277_22e+02,
    -1.025_095_131_611_077_249_54e+03,
    -4.835_191_916_086_513_970_19e+02,
];
const SB: [f64; 7] = [
    3.033_806_074_348_245_829_24e+01,
    3.257_925_129_965_739_188_26e+02,
    1.536_729_586_084_436_959_94e+03,
    3.199_858_219_508_595_539_08e+03,
    2.553_050_406_433_164_425_83e+03,
    4.745_285_412_069_553_672_15e+02,
    -2.244_095_244_658_581_833_62e+01,
];

/// A polynomial by Horner's rule from the highest coefficient down, in the order fdlibm writes
/// it out longhand: `c[0] + s (c[1] + s (c[2] + ...))`. The same operations in the same order.
fn horner(coefficients: &[f64], s: f64) -> f64 {
    coefficients
        .iter()
        .rev()
        .fold(0.0, |sum, coefficient| coefficient + s * sum)
}

/// `1 + s (c[0] + s (c[1] + ...))`: the denominators, whose constant term is one.
fn horner_one(coefficients: &[f64], s: f64) -> f64 {
    1.0 + s * horner(coefficients, s)
}

/// The complementary error function, `1 - erf(x)`, the same bits on every machine: fdlibm's
/// `erfc`, in f64, within an ulp or so. Near zero it is one less a rational approximation to
/// erf; in the middle range a rational function about erf(1); in the tail, where erfc is all
/// that is left, `exp(-x^2 - 0.5625 + R/S) / x` with `x^2` split so the large part of the
/// exponent is exact - both exponentials through [`exp`] above. Negative arguments are two less
/// the positive tail. erfc(+inf) is 0, erfc(-inf) is 2, NaN stays NaN.
#[must_use]
pub fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let high = high_word(x);
    let magnitude = high & 0x7fff_ffff;
    let negative = x.is_sign_negative();
    if magnitude < 0x3feb_0000 {
        // |x| < 0.84375.
        if magnitude < 0x3c70_0000 {
            return 1.0 - x;
        }
        let z = x * x;
        let y = horner(&PP, z) / horner_one(&QQ, z);
        // fdlibm compares the signed high word: every negative is "under a quarter".
        if negative || high < 0x3fd0_0000 {
            return 1.0 - (x + x * y);
        }
        return 0.5 - (x * y + (x - 0.5));
    }
    if magnitude < 0x3ff4_0000 {
        // 0.84375 <= |x| < 1.25.
        let s = x.abs() - 1.0;
        let ratio = horner(&PA, s) / horner_one(&QA, s);
        return if negative {
            1.0 + (ERX + ratio)
        } else {
            (1.0 - ERX) - ratio
        };
    }
    if magnitude >= 0x403c_0000 {
        // |x| >= 28: nothing left to the right, everything to the left.
        return if negative { 2.0 } else { 0.0 };
    }
    let a = x.abs();
    let s = 1.0 / (a * a);
    let correction = if magnitude < 0x4006_db6d {
        // |x| < 1/0.35.
        horner(&RA, s) / horner_one(&SA, s)
    } else {
        if negative && magnitude >= 0x4018_0000 {
            // x < -6: two, less a remainder below the last bit.
            return 2.0;
        }
        horner(&RB, s) / horner_one(&SB, s)
    };
    // |x| with the low word cleared, so z*z is exact and the exponent's large part has no error.
    let z = f64::from_bits(a.to_bits() & 0xffff_ffff_0000_0000);
    let tail = exp(-z * z - 0.5625) * exp((z - a) * (z + a) + correction);
    if negative { 2.0 - tail / a } else { tail / a }
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

    /// How many representable f64 values apart two finite non-negative values are.
    fn ulps64(a: f64, b: f64) -> u64 {
        a.to_bits().abs_diff(b.to_bits())
    }

    #[test]
    fn our_exp_stands_within_one_ulp_of_the_platforms_across_its_range() {
        // Every binade the world can reach, both signs, the reduction's branch points and the
        // subnormal tail included.
        for i in 0..200_000u32 {
            let x = -745.0 + f64::from(i) * (745.0 + 709.7) / 200_000.0;
            let (ours, theirs) = (exp(x), x.exp());
            if theirs >= f64::MIN_POSITIVE {
                assert!(
                    ulps64(ours, theirs) <= 1,
                    "exp({x}) = {ours:e} vs {theirs:e}"
                );
            } else {
                // Subnormal: both round into the same few bits; an ulp is all there is.
                assert!(
                    (ours - theirs).abs() <= 2.0 * 4.9e-324,
                    "exp({x}) = {ours:e} vs {theirs:e}"
                );
            }
        }
        for x in [
            0.0, -0.0, 1e-30, -1e-30, 0.34657, 1.03972, -1.03972, 1.0, -1.0, 100.0,
        ] {
            assert!(ulps64(exp(x), x.exp()) <= 1, "exp({x})");
        }
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(exp(1000.0), f64::INFINITY);
        assert_eq!(exp(-1000.0), 0.0);
        assert_eq!(exp(f64::INFINITY), f64::INFINITY);
        assert_eq!(exp(f64::NEG_INFINITY), 0.0);
        assert!(exp(f64::NAN).is_nan());
    }

    /// erfc by quadrature, the yardstick the standard library does not yet offer: Simpson's
    /// rule over `2/sqrt(pi) exp(-t^2)` from `x` to where the integrand is gone, with the
    /// platform's own `exp`. Independent of fdlibm's rational approximations entirely.
    fn erfc_by_quadrature(x: f64) -> f64 {
        // The tail falls off on a scale of 1/(2x): past one, the span shrinks with it so the
        // nodes stay dense where the integrand still lives.
        let upper = if x > 1.0 {
            x + 24.0 / x
        } else {
            x.max(0.0) + 9.0
        };
        let intervals = 40_000u32;
        let h = (upper - x) / f64::from(intervals);
        let f = |t: f64| (-t * t).exp();
        let mut sum = f(x) + f(upper);
        for i in 1..intervals {
            let weight = if i % 2 == 1 { 4.0 } else { 2.0 };
            sum += weight * f(x + f64::from(i) * h);
        }
        sum * h / 3.0 * 2.0 / std::f64::consts::PI.sqrt()
    }

    #[test]
    fn our_erfc_agrees_with_quadrature_and_the_tables_across_its_range() {
        // Every branch: the tails, both sides of 0.84375, 1.25, 1/0.35 and 6.
        for i in 0..=320u32 {
            let x = -7.0 + f64::from(i) * 0.1 + 0.003 * f64::from(i % 3);
            let (ours, theirs) = (erfc(x), erfc_by_quadrature(x));
            assert!(
                ((ours - theirs) / theirs).abs() < 1e-12,
                "erfc({x}) = {ours:e} vs quadrature {theirs:e}"
            );
        }
        // Published values (Abramowitz and Stegun's table 7.1, carried further by any CAS).
        for (x, value) in [
            (0.5, 0.479_500_122_186_953_5),
            (1.0, 0.157_299_207_050_285_13),
            (2.0, 0.004_677_734_981_047_266),
            (3.0, 2.209_049_699_858_544e-5),
            (-1.0, 1.842_700_792_949_714_9),
        ] {
            assert!(
                ((erfc(x) - value) / value).abs() < 1e-15,
                "erfc({x}) = {:e}",
                erfc(x)
            );
        }
        assert_eq!(erfc(0.0), 1.0);
        assert_eq!(erfc(30.0), 0.0);
        assert_eq!(erfc(-30.0), 2.0);
        assert_eq!(erfc(f64::INFINITY), 0.0);
        assert_eq!(erfc(f64::NEG_INFINITY), 2.0);
        assert!(erfc(f64::NAN).is_nan());
        // Monotone falling, which the field's growth from nothing leans on.
        let mut previous = erfc(-7.0);
        for i in 1..14_000u32 {
            let next = erfc(-7.0 + f64::from(i) * 0.002);
            assert!(
                next <= previous,
                "erfc rises at {}",
                -7.0 + f64::from(i) * 0.002
            );
            previous = next;
        }
    }
}
