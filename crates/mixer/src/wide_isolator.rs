use backspn_dsp::Isolator;

use crate::block_ramp::BlockRamp;

pub const LOW_HZ: f64 = 892.212_34 / std::f64::consts::PI;
pub const HIGH_HZ: f64 = 14_608.406 / std::f64::consts::PI;
const BLOCK: u32 = 64;
const RAMP_S: f64 = 0.02;
#[derive(Debug, Clone, Copy)]
pub struct WideIsolator {
    split: Isolator,
    gains: BlockRamp<3>,
}

impl WideIsolator {
    pub fn new(rate: f64) -> Self {
        Self {
            split: Isolator::new(LOW_HZ, HIGH_HZ, rate),
            gains: BlockRamp::new(rate, BLOCK, RAMP_S, [1.0; 3]),
        }
    }

    pub fn set(&mut self, knobs: [f32; 3]) {
        self.gains.set(knobs.map(|k| 2.0 * k.clamp(0.0, 1.0)));
    }

    #[inline]
    pub fn tick(&mut self, frame: &mut [f32; 2]) {
        let [lo, mid, hi] = self.gains.tick();
        for (ch, x) in frame.iter_mut().enumerate() {
            let [l, m, h] = self.split.split(ch, *x);
            *x = l * lo + m * mid + h * hi;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossovers_and_gain_law() {
        assert!((LOW_HZ - 284.0).abs() < 0.1 && (HIGH_HZ - 4650.0).abs() < 0.5);
        let mut eq = WideIsolator::new(44_100.0);
        eq.set([0.0, 0.5, 1.0]);
        for _ in 0..2000 {
            eq.gains.tick();
        }
        assert_eq!(eq.gains.tick(), [0.0, 1.0, 2.0]);
    }
}
