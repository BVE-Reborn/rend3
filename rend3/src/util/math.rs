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
        self.partial_cmp(other).unwrap()
    }
}
