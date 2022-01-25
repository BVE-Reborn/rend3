//! Math utilites.

use num_traits::PrimInt;

/// Rounds up `src` to the power of two `factor`.
pub fn round_up_pot<T: PrimInt>(src: T, factor: T) -> T {
    debug_assert_eq!(factor.count_ones(), 1); // .is_power_of_two()
    let minus1 = factor - T::one();
    (src + minus1) & !minus1
}

/// Performs integer division between a and b rounding up, instead of down
pub fn round_up_div<T: PrimInt>(a: T, b: T) -> T {
    (a + (b - T::one())) / b
}
