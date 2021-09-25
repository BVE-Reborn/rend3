// TODO: make generic
pub fn round_up_pot(src: usize, factor: usize) -> usize {
    debug_assert!(factor.is_power_of_two());
    let minus1 = factor - 1;
    (src + minus1) & !minus1
}
