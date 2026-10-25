const STAGE_W: [f32; 2] = [3_298.575, 6_597.15];
const RAMP: u32 = 128;
const EPS: f32 = 1e-12;

fn band_db(k: f32) -> f32 {
    if k < 0.5 {
        24.0 * k - 12.0
    } else {
        12.0 * k - 6.0
    }
}

fn gain(k: f32) -> f32 {
    10f32.powf(band_db(k) / 20.0)
}

#[derive(Debug, Clone, Copy, Default)]
struct Stage {
    band: f32,
    low: f32,
}

impl Stage {
    #[inline]
    fn tick(&mut self, x: f32, c: f32, [low, mid, high]: [f32; 3]) -> f32 {
        let hp = x - (2.0 * self.band + self.low);
        let y = high * hp + mid * self.band + low * self.low;
        self.band += c * hp;
        self.low += c * self.band + EPS;
        y
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClassicEq {
    c: [f32; 2],
    gains: [f32; 3],
    step: [f32; 3],
    left: u32,
    stages: [[Stage; 2]; 2],
}

impl ClassicEq {
    pub fn new(rate: f64) -> Self {
        Self {
            c: STAGE_W.map(|w| w / rate as f32),
            gains: [1.0, 2.0, 1.0],
            step: [0.0; 3],
            left: 0,
            stages: [[Stage::default(); 2]; 2],
        }
    }

    pub fn set(&mut self, [low, mid, high]: [f32; 3]) {
        let target = [gain(low), 2.0 * gain(mid), gain(high)];
        for ((step, t), g) in self.step.iter_mut().zip(target).zip(self.gains) {
            *step = (t - g) / RAMP as f32;
        }
        self.left = RAMP;
    }

    #[inline]
    pub fn tick(&mut self, frame: &mut [f32; 2]) {
        if self.left > 0 {
            for (g, s) in self.gains.iter_mut().zip(self.step) {
                *g += s;
            }
            self.left -= 1;
        }
        for (x, stages) in frame.iter_mut().zip(&mut self.stages) {
            let y = stages[0].tick(*x, self.c[0], self.gains);
            *x = stages[1].tick(y, self.c[1], self.gains);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knob_law() {
        assert_eq!(band_db(0.0), -12.0);
        assert_eq!(band_db(0.25), -6.0);
        assert_eq!(band_db(0.5), 0.0);
        assert_eq!(band_db(1.0), 6.0);
    }

    fn level(knobs: [f32; 3], hz: f32) -> f32 {
        let rate = 48_000.0;
        let mut eq = ClassicEq::new(f64::from(rate));
        eq.set(knobs);
        let mut sum = 0.0;
        for n in 0..48_000 {
            let x = (std::f32::consts::TAU * hz * n as f32 / rate).sin();
            let mut frame = [x, x];
            eq.tick(&mut frame);
            if n >= 24_000 {
                sum += frame[0] * frame[0];
            }
        }
        (sum / 24_000.0).sqrt() * 2f32.sqrt()
    }

    #[test]
    fn flat_at_the_centre_and_bands_cut() {
        for hz in [50.0, 700.0, 8_000.0] {
            assert!((level([0.5; 3], hz) - 1.0).abs() < 1e-3, "{hz} Hz flat");
        }
        let bottom = 10f32.powf(-24.0 / 20.0);
        assert!((level([0.0, 0.5, 0.5], 20.0) - bottom).abs() < 0.01, "low");
        assert!(level([0.5, 0.5, 0.0], 18_000.0) < 0.1, "high");
        let top = 10f32.powf(12.0 / 20.0);
        assert!(
            (level([1.0, 0.5, 0.5], 20.0) - top).abs() < 0.1,
            "low boost"
        );
    }
}
