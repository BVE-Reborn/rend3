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

pub struct IndexedDistance {
    pub distance: f32,
    pub index: usize,
}

impl PartialEq for IndexedDistance {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for IndexedDistance {}

impl PartialOrd for IndexedDistance {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.distance.partial_cmp(&other.distance)
    }
}

impl Ord for IndexedDistance {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(&other).unwrap()
    }
}

