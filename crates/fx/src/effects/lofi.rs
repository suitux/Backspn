use super::EffectKind;
use crate::{Context, Effect};

#[derive(Debug)]
pub struct LoFi {
    bits: f32,
    factor: u32,
    mix: f32,
    hold: [f32; 2],
    counter: u32,
}

impl LoFi {
    pub fn new(_rate: f64) -> Self {
        Self {
            bits: 16.0,
            factor: 1,
            mix: 0.0,
            hold: [0.0; 2],
            counter: 0,
        }
    }
}

impl Effect for LoFi {
    fn kind(&self) -> EffectKind {
        EffectKind::LoFi
    }

    fn process(&mut self, buf: &mut [f32], _ctx: &Context) {
        let levels = 2f32.powf(self.bits - 1.0);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            if self.counter == 0 {
                self.hold = [frame[0], frame[1]].map(|x| (x * levels).round() / levels);
            }
            self.counter = (self.counter + 1) % self.factor.max(1);
            for ch in 0..2 {
                frame[ch] += (self.hold[ch] - frame[ch]) * self.mix;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        self.mix = if a > 0.0 { 1.0 } else { 0.0 };
        self.bits = 16.0 - 13.0 * a;
        self.factor = 1 + (20.0 * a * a) as u32;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let v = value.clamp(0.0, 1.0);
        match index {
            0 => self.bits = 16.0 - 14.0 * v,
            1 => self.factor = 1 + (40.0 * v * v) as u32,
            2 => self.mix = v,
            _ => {}
        }
    }

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn reset(&mut self) {
        self.counter = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn quantises_and_holds() {
        let mut l = LoFi::new(f64::from(RATE));
        l.set_param(0, 1.0);
        l.set_param(1, 0.5);
        l.set_param(2, 1.0);
        let out = run(&mut l, &sine(100.0, 4_800, 0.9), ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let mut distinct: Vec<i32> = left.iter().map(|v| (v * 100.0).round() as i32).collect();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(distinct.len() <= 5, "{distinct:?}");
        assert!(
            left[0..11]
                .iter()
                .all(|&v| (v - left[0]).abs() < f32::EPSILON)
        );
    }
}
