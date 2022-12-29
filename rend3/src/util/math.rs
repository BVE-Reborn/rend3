//! Math utilites.

use num_traits::PrimInt;

/// Rounds up `src` to the power of two `factor`.
pub fn round_up<T: PrimInt>(src: T, factor: T) -> T {
    let minus1 = factor - T::one();
    ((src + minus1) / factor) * factor
}

/// Performs integer division between a and b rounding up, instead of down
pub fn round_up_div<T: PrimInt>(a: T, b: T) -> T {
    (a + (b - T::one())) / b
}
