// Take a value and round it upwards to x * multiple, like memory address alignment
pub(crate) fn round_to_multiple(value: u32, multiple: u32) -> u32 {
    if multiple.is_power_of_two() {
        let mask = multiple - 1;
        (value + mask) & !mask
    } else {
        let rem = value % multiple;
        if rem == 0 {
            value
        } else {
            value + multiple - rem
        }
    }
}
