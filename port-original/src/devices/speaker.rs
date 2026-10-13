#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub cycle: u64,
    pub high: bool,
}

#[derive(Default)]
pub struct Speaker {
    high: bool,
    pub edges: Vec<Edge>,
}

impl Speaker {
    pub fn access(&mut self, cycle: u64) {
        assert!(self.edges.last().is_none_or(|edge| cycle >= edge.cycle));
        self.high = !self.high;
        self.edges.push(Edge {
            cycle,
            high: self.high,
        });
    }

    // Sampling averages the speaker drive across each output sample. Multiple
    // toggles in one sample therefore contribute rather than aliasing away.
    pub fn samples(&self, start: u64, end: u64, sample_rate: u32, cpu_hz: u64) -> Vec<f32> {
        assert!(end >= start && sample_rate > 0 && cpu_hz > 0);
        let count = ((end - start) as u128 * u128::from(sample_rate) / u128::from(cpu_hz)) as usize;
        let mut index = self.edges.partition_point(|edge| edge.cycle <= start);
        let mut high = index != 0 && self.edges[index - 1].high;
        let mut samples = Vec::with_capacity(count);
        for sample in 0..count {
            let left = start as f64 + sample as f64 * cpu_hz as f64 / f64::from(sample_rate);
            let right = start as f64 + (sample + 1) as f64 * cpu_hz as f64 / f64::from(sample_rate);
            let mut position = left;
            let mut integral = 0.0;
            while index < self.edges.len() && (self.edges[index].cycle as f64) < right {
                let edge = self.edges[index];
                integral += (edge.cycle as f64 - position) * if high { 1.0 } else { -1.0 };
                position = edge.cycle as f64;
                high = edge.high;
                index += 1;
            }
            integral += (right - position) * if high { 1.0 } else { -1.0 };
            samples.push((integral / (right - left)) as f32);
        }
        samples
    }
}
