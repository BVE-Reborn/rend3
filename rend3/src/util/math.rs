//! Math utilites.

use num_traits::PrimInt;

pub trait IntegerExt: PrimInt {
    /// Rounds T away from zero to the nearest multiple of b.
    ///
    /// Panics if b is zero or negative.
    fn round_up(self, b: Self) -> Self {
        round_up(self, b)
    }

    /// Performs integer division between a and b rounding away from zero, instead of towards it.
    ///
    /// Panics if b is zero or negative.
    fn div_round_up(self, b: Self) -> Self {
        div_round_up(self, b)
    }
}

impl<T: PrimInt> IntegerExt for T {}

/// Rounds T away from zero to the nearest multiple of b.
///
/// Panics if b is zero or negative.
pub fn round_up<T: PrimInt>(a: T, b: T) -> T {
    assert!(b > T::zero(), "divisor must be non-zero and positive");
    // All the negative infrastructure will compile away if T is unsigned as this is unconditionally false
    let negative = a < T::zero();

    let pos_a = if negative { T::zero() - a } else { a };

    let rem = pos_a % b;
    if rem == T::zero() {
        return a;
    }

    let pos_res = pos_a + (b - rem);

    if negative {
        T::zero() - pos_res
    } else {
        pos_res
    }
}

/// Performs integer division between a and b rounding away from zero, instead of towards it.
///
/// Panics if b is zero or negative.
pub fn div_round_up<T: PrimInt>(a: T, b: T) -> T {
    assert!(b > T::zero(), "divisor must be non-zero and positive");
    // All the negative infrastructure will compile away if T is unsigned as this is unconditionally false
    let negative = a < T::zero();

    let pos_a = if negative { T::zero() - a } else { a };

    let pos_res = (pos_a + (b - T::one())) / b;

    if negative {
        T::zero() - pos_res
    } else {
        pos_res
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_up() {
        assert_eq!(super::round_up(2, 12), 12);
        assert_eq!(super::round_up(12, 12), 12);
        assert_eq!(super::round_up(0, 12), 0);

        // Negatives
        assert_eq!(super::round_up(-14, 12), -24);
        assert_eq!(super::round_up(-8, 12), -12);

        // Identity
        assert_eq!(super::round_up(2, 1), 2);
    }

    #[test]
    fn round_up_div() {
        assert_eq!(super::div_round_up(2, 12), 1);
        assert_eq!(super::div_round_up(12, 12), 1);
        assert_eq!(super::div_round_up(18, 12), 2);
        assert_eq!(super::div_round_up(0, 12), 0);

        // Negatives
        assert_eq!(super::div_round_up(-14, 12), -2);
        assert_eq!(super::div_round_up(-8, 12), -1);

        // Identity
        assert_eq!(super::div_round_up(2, 1), 2);
    }
}
