pub struct ThresholdSet {
    thresholds: Vec<Threshold>,
}

impl ThresholdSet {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn check(self, pool: &mut nv_flip::FlipPool) -> bool {
        // If there are no checks, we want to fail the test.
        let mut all_passed = !self.thresholds.is_empty();
        // We always iterate all of these, as the call to check prints
        for check in self.thresholds {
            all_passed &= check.check(pool);
        }
        all_passed
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Threshold {
    Mean(f32),
    Percentile { percentile: f32, threshold: f32 },
}

impl Threshold {
    #[cfg(not(target_arch = "wasm32"))]
    fn check(&self, pool: &mut nv_flip::FlipPool) -> bool {
        match *self {
            Self::Mean(v) => {
                let mean = pool.mean();
                let within = mean <= v;
                println!(
                    "    Expected Mean ({:.6}) to be under expected maximum ({}): {}",
                    mean,
                    v,
                    if within { "PASS" } else { "FAIL" }
                );
                within
            }
            Self::Percentile {
                percentile: p,
                threshold: v,
            } => {
                let percentile = pool.get_percentile(p, true);
                let within = percentile <= v;
                println!(
                    "    Expected {}% ({:.6}) to be under expected maximum ({}): {}",
                    p * 100.0,
                    percentile,
                    v,
                    if within { "PASS" } else { "FAIL" }
                );
                within
            }
        }
    }
}

impl From<Threshold> for ThresholdSet {
    fn from(threshold: Threshold) -> Self {
        Self {
            thresholds: vec![threshold],
        }
    }
}

impl From<&[Threshold]> for ThresholdSet {
    fn from(thresholds: &[Threshold]) -> Self {
        Self {
            thresholds: thresholds.into(),
        }
    }
}
