const SNAP: f32 = 1e-15;

#[derive(Debug, Clone, Copy)]
pub struct BlockRamp<const N: usize> {
    current: [f32; N],
    target: [f32; N],
    step: [f32; N],
    left: u32,
    phase: u32,
    block: u32,
    len: u32,
    pending: bool,
}

impl<const N: usize> BlockRamp<N> {
    pub fn new(rate: f64, block: u32, time_s: f64, initial: [f32; N]) -> Self {
        let blocks = ((rate * time_s / f64::from(block) + 0.5) as u32).max(1);
        Self {
            current: initial,
            target: initial,
            step: [0.0; N],
            left: 0,
            phase: 0,
            block,
            len: blocks * block,
            pending: false,
        }
    }

    pub fn set(&mut self, target: [f32; N]) {
        self.target = target;
        self.pending = true;
    }

    #[inline]
    pub fn tick(&mut self) -> [f32; N] {
        if self.phase == 0 {
            self.phase = self.block;
            if self.pending {
                self.pending = false;
                self.left = self.len;
                for i in 0..N {
                    self.step[i] = (self.target[i] - self.current[i]) / self.len as f32;
                    if self.step[i] * self.step[i] < SNAP {
                        self.step[i] = 0.0;
                        self.current[i] = self.target[i];
                    }
                }
            }
        }
        self.phase -= 1;
        if self.left > 0 {
            self.left -= 1;
            if self.left == 0 {
                self.current = self.target;
            } else {
                for i in 0..N {
                    self.current[i] += self.step[i];
                }
            }
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_length_is_whole_blocks() {
        assert_eq!(BlockRamp::<1>::new(44_100.0, 64, 0.02, [0.0]).len, 896);
        assert_eq!(
            BlockRamp::<1>::new(44_100.0, 128, 0.14, [0.0]).len,
            48 * 128
        );
    }

    #[test]
    fn changes_start_on_a_block_boundary_and_end_on_target() {
        let mut ramp = BlockRamp::new(44_100.0, 64, 0.02, [1.0, 1.0]);
        for _ in 0..64 {
            ramp.tick();
        }
        ramp.tick();
        ramp.set([0.0, 2.0]);
        for _ in 0..63 {
            assert_eq!(ramp.tick(), [1.0, 1.0]);
        }
        let first = ramp.tick();
        assert!(first[0] < 1.0 && first[1] > 1.0);
        for _ in 0..894 {
            ramp.tick();
        }
        assert_eq!(ramp.tick(), [0.0, 2.0]);
    }
}
