use num::Integer;
use std::cmp::Ordering;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Add, BitAnd, Div, Mul, Neg, Not, Sub};

pub mod atomic;

/// Number represented like I + N / D
///
/// # Examples
///
/// ```
/// # use mpdelta_core::common::mixed_fraction::MixedFraction;
/// let fraction = MixedFraction::new(1, 2, 4);
/// let (integer, numerator, denominator) = fraction.deconstruct();
/// assert_eq!(integer, 1);
/// assert_eq!(numerator, 1);
/// assert_eq!(denominator, 2);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MixedFraction(i64);

#[inline]
fn validate_integer(integer: i32) -> Result<i32, ()> {
    if [0, 0xf800_0000u32 as i32].contains(&(integer & 0xf800_0000u32 as i32)) {
        Ok(integer)
    } else {
        Err(())
    }
}

#[inline]
fn validate_numerator(numerator: u32) -> Result<u32, ()> {
    if numerator & FRAC_VALUE_MASK == numerator {
        Ok(numerator)
    } else {
        Err(())
    }
}

#[inline]
fn validate_denominator(denominator: u32) -> Result<u32, ()> {
    if denominator != 0 && denominator & FRAC_VALUE_MASK == denominator {
        Ok(denominator)
    } else {
        Err(())
    }
}

#[inline]
fn round_into<T>(numerator: T, denominator: T, target_denominator: T) -> T
where
    T: Integer + Copy + Not<Output = T> + BitAnd<Output = T>,
{
    let numerator = numerator * target_denominator;
    let (quotient, remainder) = numerator.div_rem(&denominator);
    match (remainder + remainder).cmp(&denominator) {
        Ordering::Less => quotient,
        Ordering::Equal => (quotient + T::one()) & !T::one(),
        Ordering::Greater => quotient + T::one(),
    }
}

const FRAC_VALUE_MASK: u32 = (1 << 18) - 1;

impl MixedFraction {
    /// Zero value of MixedFraction 0 + 0 / 1
    pub const ZERO: MixedFraction = MixedFraction::new_inner(0, 0, 1);
    /// One value of MixedFraction 1 + 0 / 1
    pub const ONE: MixedFraction = MixedFraction::new_inner(1, 0, 1);
    /// Min value of MixedFraction -134,217,728 + 0 / 1
    pub const MIN: MixedFraction = MixedFraction::new_inner(-0x800_0000, 0, 1);
    /// Max value of MixedFraction 134,217,727 + 262,143 / 262,144
    pub const MAX: MixedFraction = MixedFraction::new_inner(0x7ff_ffff, FRAC_VALUE_MASK - 1, FRAC_VALUE_MASK);

    const fn new_inner(integer: i32, numerator: u32, denominator: u32) -> MixedFraction {
        MixedFraction((integer as i64) << 36 | (numerator as i64) << 18 | (denominator as i64))
    }

    /// Create new MixedFraction(I + N / D) from I, N, and D
    pub fn new(integer: i32, numerator: u32, denominator: u32) -> MixedFraction {
        let integer = validate_integer(integer).expect("MixedFraction Validate Error");
        let numerator = validate_numerator(numerator).expect("MixedFraction Validate Error");
        let denominator = validate_denominator(denominator).expect("MixedFraction Validate Error");
        assert!(numerator < denominator);
        let gcd = numerator.gcd(&denominator);
        let numerator = numerator / gcd;
        let denominator = denominator / gcd;
        MixedFraction::new_inner(integer, numerator, denominator)
    }

    /// Create new MixedFraction(I + N / D) from I, N, and D with overflow checking
    pub fn new_checked(integer: i32, numerator: u32, denominator: u32) -> Option<MixedFraction> {
        match (validate_integer(integer), validate_numerator(numerator), validate_denominator(denominator)) {
            (Ok(integer), Ok(numerator), Ok(denominator)) if numerator < denominator => Some(MixedFraction::new_inner(integer, numerator, denominator)),
            _ => None,
        }
    }

    pub fn from_integer(integer: i32) -> MixedFraction {
        MixedFraction::new(integer, 0, 1)
    }

    pub fn from_fraction(numerator: i64, denominator: u32) -> MixedFraction {
        let (integer, numerator) = numerator.div_rem(&(denominator as i64));
        let (integer, numerator) = if numerator < 0 { (integer - 1, (numerator + denominator as i64) as u32) } else { (integer, numerator as u32) };
        MixedFraction::new(i32::try_from(integer).unwrap(), numerator, denominator)
    }

    pub fn from_f64(value: f64) -> MixedFraction {
        let integer = value.trunc();
        let fract = value.fract();
        let (integer, fract) = if fract < 0. { (integer - 1., fract + 1.) } else { (integer, fract) };
        MixedFraction::new(integer as i32, ((fract * (FRAC_VALUE_MASK as f64)) as u32).min(FRAC_VALUE_MASK - 1), FRAC_VALUE_MASK)
    }

    /// Deconstruct MixedFraction(I + N / D) into (I, N, D)
    pub fn deconstruct(self) -> (i32, u32, u32) {
        let MixedFraction(x) = self;
        let integer = (x >> 36) as i32;
        let numerator = (x >> 18) as u32 & FRAC_VALUE_MASK;
        let denominator = x as u32 & FRAC_VALUE_MASK;
        (integer, numerator, denominator)
    }

    pub fn signum(self) -> i8 {
        let (i, _, _) = self.deconstruct();
        i.signum() as i8
    }

    pub fn checked_add(self, rhs: MixedFraction) -> Option<MixedFraction> {
        let (i1, n1, d1) = self.deconstruct();
        let (i2, n2, d2) = rhs.deconstruct();
        let i = i1 + i2;
        let d = (d1 as u64) * (d2 as u64);
        let n = (n1 as u64) * (d2 as u64) + (n2 as u64) * (d1 as u64);
        let (i, n) = if n >= d { (i + 1, n - d) } else { (i, n) };
        assert!(n < d);
        let Ok(i) = validate_integer(i) else {
            return None;
        };
        let gcd = n.gcd(&d);
        let n = n / gcd;
        let d = d / gcd;
        if let Ok(d) = u32::try_from(d).map_err(|_| ()).and_then(validate_denominator) {
            return Some(MixedFraction::new_inner(i, n as u32, d));
        }
        let n = round_into(n, d, FRAC_VALUE_MASK as u64);
        let d = FRAC_VALUE_MASK;
        Some(MixedFraction::new_inner(i, n as u32, d))
    }

    pub fn saturating_add(self, rhs: MixedFraction) -> MixedFraction {
        self.checked_add(rhs).unwrap_or_else(|| if self.signum() >= 0 { MixedFraction::MAX } else { MixedFraction::MIN })
    }

    pub fn checked_sub(self, rhs: MixedFraction) -> Option<MixedFraction> {
        let (i1, n1, d1) = self.deconstruct();
        let (i2, n2, d2) = rhs.deconstruct();
        let i = i1 - i2 - 1;
        let d = (d1 as u64) * (d2 as u64);
        let n = d + (n1 as u64) * (d2 as u64) - (n2 as u64) * (d1 as u64);
        let (i, n) = if n >= d { (i + 1, n - d) } else { (i, n) };
        assert!(n < d);
        let Ok(i) = validate_integer(i) else {
            return None;
        };
        let gcd = n.gcd(&d);
        let n = n / gcd;
        let d = d / gcd;
        if let Ok(d) = u32::try_from(d).map_err(|_| ()).and_then(validate_denominator) {
            return Some(MixedFraction::new_inner(i, n as u32, d));
        }
        let n = round_into(n, d, FRAC_VALUE_MASK as u64);
        let d = FRAC_VALUE_MASK;
        Some(MixedFraction::new_inner(i, n as u32, d))
    }

    pub fn saturating_sub(self, rhs: MixedFraction) -> MixedFraction {
        self.checked_sub(rhs).unwrap_or_else(|| if self.signum() >= 0 { MixedFraction::MAX } else { MixedFraction::MIN })
    }

    pub fn checked_mul(self, rhs: MixedFraction) -> Option<MixedFraction> {
        let (i1, n1, d1) = self.deconstruct();
        let (i2, n2, d2) = rhs.deconstruct();
        let (i1, n1, d1) = (i1 as i128, n1 as i128, d1 as u64);
        let (i2, n2, d2) = (i2 as i128, n2 as i128, d2 as u64);
        let i = i1 * i2;
        let n = n1 * n2 + n1 * i2 * d2 as i128 + n2 * i1 * d1 as i128;
        let d = d1 * d2;
        let (div, n) = n.div_rem(&(d as i128));
        let i = i + div;
        let (i, n) = if n < 0 { (i - 1, (n + d as i128) as u64) } else { (i, n as u64) };
        let Ok(i) = i32::try_from(i).map_err(|_| ()).and_then(validate_integer) else {
            return None;
        };
        let gcd = n.gcd(&d);
        let n = n / gcd;
        let d = d / gcd;
        if let Ok(d) = u32::try_from(d).map_err(|_| ()).and_then(validate_denominator) {
            return Some(MixedFraction::new_inner(i, n as u32, d));
        }
        let n = round_into(n, d, FRAC_VALUE_MASK as u64);
        let d = FRAC_VALUE_MASK;
        Some(MixedFraction::new_inner(i, n as u32, d))
    }

    pub fn saturating_mul(self, rhs: MixedFraction) -> MixedFraction {
        self.checked_mul(rhs).unwrap_or_else(|| if self.signum() * rhs.signum() >= 0 { MixedFraction::MAX } else { MixedFraction::MIN })
    }

    pub fn checked_div(self, rhs: MixedFraction) -> Option<MixedFraction> {
        let (i1, n1, d1) = self.deconstruct();
        let (i2, n2, d2) = rhs.deconstruct();
        let (i1, n1, d1) = (i1 as i128, n1 as i128, d1 as i128);
        let (i2, n2, d2) = (i2 as i128, n2 as i128, d2 as i128);
        let n = (d1 * i1 + n1) * d2;
        let d = (i2 * d2 + n2) * d1;
        let (n, d) = match d.cmp(&0) {
            Ordering::Less => (-n, -d),
            Ordering::Equal => return None,
            Ordering::Greater => (n, d),
        };
        let gcd = n.gcd(&d);
        let n = n / gcd;
        let d = d / gcd;
        let (div, rem) = n.div_rem(&d);
        let (i, n) = if rem < 0 { (div - 1, (rem + d) as u128) } else { (div, rem as u128) };
        let Ok(i) = i32::try_from(i).map_err(|_| ()).and_then(validate_integer) else {
            return None;
        };
        if let Ok(d) = u32::try_from(d).map_err(|_| ()).and_then(validate_denominator) {
            return Some(MixedFraction::new_inner(i, n as u32, d));
        }
        let n = round_into(n, d as u128, FRAC_VALUE_MASK as u128);
        let d = FRAC_VALUE_MASK;
        Some(MixedFraction::new_inner(i, n as u32, d))
    }

    pub fn saturating_div(self, rhs: MixedFraction) -> MixedFraction {
        self.checked_div(rhs).unwrap_or_else(|| if self.signum() * rhs.signum() >= 0 { MixedFraction::MAX } else { MixedFraction::MIN })
    }

    pub fn checked_neg(self) -> Option<MixedFraction> {
        MixedFraction::ZERO.checked_sub(self)
    }

    pub fn saturating_neg(self) -> MixedFraction {
        MixedFraction::ZERO.saturating_sub(self)
    }

    pub fn into_f64(self) -> f64 {
        let (i, n, d) = self.deconstruct();
        (i as f64) + (n as f64) / (d as f64)
    }

    /// Deconstruct MixedFraction(I + N / D) into (I, N') s.t. N / D ~ N' / D'
    pub fn deconstruct_with_round(self, denominator: u32) -> (i32, u32) {
        let (i, n, d) = self.deconstruct();
        if d == denominator {
            (i, n)
        } else {
            (i, round_into(n, d, denominator))
        }
    }
}

impl Add for MixedFraction {
    type Output = MixedFraction;

    #[inline]
    #[track_caller]
    fn add(self, rhs: Self) -> Self::Output {
        self.checked_add(rhs).expect("attempt to add with overflow")
    }
}

impl Sub for MixedFraction {
    type Output = MixedFraction;

    #[inline]
    #[track_caller]
    fn sub(self, rhs: Self) -> Self::Output {
        self.checked_sub(rhs).expect("attempt to subtract with overflow")
    }
}

impl Mul for MixedFraction {
    type Output = MixedFraction;

    #[inline]
    #[track_caller]
    fn mul(self, rhs: Self) -> Self::Output {
        self.checked_mul(rhs).expect("attempt to multiply with overflow")
    }
}

impl Div for MixedFraction {
    type Output = MixedFraction;

    #[inline]
    #[track_caller]
    fn div(self, rhs: Self) -> Self::Output {
        self.checked_div(rhs).expect("divide by zero or attempt to divide with overflow")
    }
}

impl Neg for MixedFraction {
    type Output = MixedFraction;

    #[inline]
    #[track_caller]
    fn neg(self) -> Self::Output {
        MixedFraction::ZERO - self
    }
}

impl PartialOrd for MixedFraction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MixedFraction {
    fn cmp(&self, other: &Self) -> Ordering {
        let (i1, n1, d1) = self.deconstruct();
        let (i2, n2, d2) = other.deconstruct();
        i1.cmp(&i2).then_with(|| {
            let n1 = (n1 as u64) * (d2 as u64);
            let n2 = (n2 as u64) * (d1 as u64);
            n1.cmp(&n2)
        })
    }
}

impl Debug for MixedFraction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (i, n, d) = self.deconstruct();
        write!(f, "{}+{}/{}", i, n, d)
    }
}

impl Display for MixedFraction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (i, n, d) = self.deconstruct();
        write!(f, "{}+{}/{}", i, n, d)
    }
}

impl Default for MixedFraction {
    fn default() -> Self {
        MixedFraction::ZERO
    }
}

#[macro_export]
macro_rules! mfrac {
    ($i:expr) => {
        $crate::common::mixed_fraction::MixedFraction::from_integer($i)
    };
    ($n:expr, $d:expr) => {
        $crate::common::mixed_fraction::MixedFraction::from_fraction($n, $d)
    };
    ($i:expr, $n:expr, $d:expr) => {
        $crate::common::mixed_fraction::MixedFraction::new($i, $n, $d)
    };
}

pub use mfrac;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn mixed_fraction() -> impl Strategy<Value = MixedFraction> {
        (-0x800_0000i32..0x800_0000, 0u32..0x4_0000, 0u32..0x4_0000).prop_filter_map("two values are equal", |(i, n, d)| MixedFraction::new_checked(i, n, d))
    }

    #[test]
    fn test_mixed_fraction_validate() {
        assert!(validate_integer(-0x800_0000 - 1).is_err());
        assert!(validate_integer(-0x800_0000).is_ok());
        assert!(validate_integer(0x7ff_ffff).is_ok());
        assert!(validate_integer(0x800_0000).is_err());

        assert!(validate_numerator(0x3_ffff).is_ok());
        assert!(validate_numerator(0x4_0000).is_err());

        assert!(validate_denominator(0x3_ffff).is_ok());
        assert!(validate_denominator(0x4_0000).is_err());
    }

    #[test]
    fn test_round_into() {
        assert_eq!(round_into(1, 4, 2), 0);
        assert_eq!(round_into(3, 4, 2), 2);
    }

    #[test]
    fn test_mixed_fraction_add() {
        assert_eq!(MixedFraction::ZERO + MixedFraction::ZERO, MixedFraction::ZERO);
        assert_eq!(MixedFraction::ZERO + MixedFraction::new(0, 1, 2), MixedFraction::new(0, 1, 2));
        assert_eq!(MixedFraction::new(0, 1, 3) + MixedFraction::new(0, 1, 2), MixedFraction::new(0, 5, 6));
    }

    #[test]
    fn test_mixed_fraction_sub() {
        assert_eq!(MixedFraction::ZERO - MixedFraction::ZERO, MixedFraction::ZERO);
        assert_eq!(MixedFraction::ZERO - MixedFraction::new(0, 1, 2), MixedFraction::new(-1, 1, 2));
        assert_eq!(MixedFraction::new(0, 1, 2) - MixedFraction::new(0, 1, 3), MixedFraction::new(0, 1, 6));
    }

    #[test]
    fn test_mixed_fraction_mul() {
        assert_eq!(MixedFraction::ONE * MixedFraction::ONE, MixedFraction::ONE);
        assert_eq!(MixedFraction::ONE * MixedFraction::new(0, 1, 2), MixedFraction::new(0, 1, 2));
        assert_eq!(MixedFraction::new(0, 1, 2) * MixedFraction::new(0, 1, 3), MixedFraction::new(0, 1, 6));
    }

    #[test]
    fn test_mixed_fraction_div() {
        assert_eq!(MixedFraction::ONE / MixedFraction::ONE, MixedFraction::ONE);
        assert_eq!(MixedFraction::ONE / MixedFraction::new(0, 1, 2), MixedFraction::new(2, 0, 1));
        assert_eq!(MixedFraction::new(0, 1, 2) / MixedFraction::new(0, 1, 3), MixedFraction::new(1, 1, 2));
    }

    #[test]
    fn test_mixed_fraction_ordering() {
        assert!(MixedFraction::ZERO < MixedFraction::ONE);
        assert!(MixedFraction::ZERO < MixedFraction::new(0, 1, 2));
        assert!(MixedFraction::new(0, 1, 3) < MixedFraction::new(0, 1, 2));
    }

    proptest! {
        #[test]
        fn test_construct_deconstruct_prop(
            integer in -0x800_0000i32..0x800_0000,
            (numerator, denominator) in [0u32..0x4_0000, 0..0x4_0000].prop_filter_map("two values are equal", |[a, b]| (a != b).then(|| (a.min(b), a.max(b)))),
        ) {
            let fraction = MixedFraction::new(integer, numerator, denominator);
            let (i, n, d) = fraction.deconstruct();
            assert_eq!(i, integer);
            assert_eq!(n as u64 * denominator as u64, d as u64 * numerator as u64);
            let fraction = MixedFraction::new_checked(integer, numerator, denominator).unwrap();
            let (i, n, d) = fraction.deconstruct();
            assert_eq!(i, integer);
            assert_eq!(n as u64 * denominator as u64, d as u64 * numerator as u64);
        }

        #[test]
        fn test_round_into_prop(
            (numerator, denominator) in any::<[u32; 2]>().prop_filter_map("two values are equal", |[a, b]| (a != b).then(|| (a.min(b), a.max(b)))),
            target_denominator in 1u32..,
        ) {
            let result = round_into(numerator as u64, denominator as u64, target_denominator as u64);
            let numerator = numerator as u64;
            let denominator = denominator as u64;
            let target_denominator = target_denominator as u64;
            let result_is_even = result % 2 == 0;
            let result_diff = (numerator * target_denominator).abs_diff(result * denominator);
            if let Some(left) = result.checked_sub(1) {
                let left_diff = (numerator * target_denominator).abs_diff(left * denominator);
                assert!(left_diff > result_diff || left_diff == result_diff && result_is_even);
            }
            let right = result + 1;
            let right_diff = (numerator * target_denominator).abs_diff(right * denominator);
            assert!(right_diff > result_diff || right_diff == result_diff && !result_is_even);
        }

        #[test]
        fn test_mixed_fraction_add_prop(
            a in mixed_fraction(),
            b in mixed_fraction(),
        ) {
            a.checked_add(b);
        }

        #[test]
        fn test_mixed_fraction_sub_prop(
            a in mixed_fraction(),
            b in mixed_fraction(),
        ) {
            a.checked_sub(b);
        }

        #[test]
        fn test_mixed_fraction_mul_prop(
            a in mixed_fraction(),
            b in mixed_fraction(),
        ) {
            a.checked_mul(b);
        }

        #[test]
        fn test_mixed_fraction_div_prop(
            a in mixed_fraction(),
            b in mixed_fraction(),
        ) {
            a.checked_div(b);
        }
    }
}
