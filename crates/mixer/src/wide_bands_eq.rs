use crate::block_ramp::BlockRamp;
use crate::rbj::{Band, Kind, Params};

const LOW_HZ: f64 = 70.0;
const MID_HZ: f64 = 1000.0;
const BLOCK: u32 = 128;
const RAMP_S: f64 = 0.03;

fn step(k: f64, centre: f64, width: f64) -> f64 {
    1.0 / (1.0 + ((k - centre) / width).exp())
}

fn gain_db(k: f64, depth: f64, centre: f64, width: f64) -> f64 {
    if k < 0.5 {
        let s = |x| step(x, centre, width);
        -depth * (s(k) - s(0.5)) / (s(0.0) - s(0.5))
    } else {
        let v = 2.0 * k - 1.0;
        7.5 * v - 1.5 * v * v
    }
}

fn amplitude(db: f64) -> f32 {
    10f64.powf(db / 40.0) as f32
}

fn targets([low, mid, high]: [f64; 3]) -> [f32; 7] {
    let width = |k: f64, c: [f64; 4]| (c[0] + k * (c[1] + k * (c[2] + k * c[3]))) as f32;
    let slope = 0.5 + 0.1 * (1.0 - high).powi(10);
    let corner = 1255.0 + 2960.0 * step(high, 0.166, 0.027) + 734.0 * (high - 0.6).max(0.0);
    [
        amplitude(gain_db(low, 26.6, 0.143, 0.084)),
        width(low, [2.96, 19.0, -26.8, 11.0]),
        amplitude(gain_db(mid, 26.6, 0.143, 0.084)),
        width(mid, [2.24, 17.4, -24.3, 10.1]),
        amplitude(gain_db(high, 27.0, 0.172, 0.06)),
        (1.0 / slope) as f32,
        corner as f32,
    ]
}

#[derive(Debug, Clone)]
pub struct WideBandsEq {
    rate: f64,
    ramp: BlockRamp<7>,
    values: [f32; 7],
    filters: [Band; 3],
    delayed: [f64; 2],
    started: bool,
}

impl WideBandsEq {
    pub fn new(rate: f64) -> Self {
        let values = targets([0.5; 3]);
        let mut eq = Self {
            rate,
            ramp: BlockRamp::new(rate, BLOCK, RAMP_S, values),
            values,
            filters: [Kind::WidePeak, Kind::WidePeak, Kind::HighShelf]
                .map(|kind| Band::new(kind, Params::new(1000.0, 1.0, 0.0, rate), rate)),
            delayed: [0.0; 2],
            started: false,
        };
        eq.update();
        eq
    }

    fn update(&mut self) {
        let v = self.values.map(f64::from);
        let db = |a: f64| 40.0 * a.log10();
        let params = [
            Params::new(LOW_HZ, v[1], db(v[0]), self.rate),
            Params::new(MID_HZ, v[3], db(v[2]), self.rate),
            Params::new(v[6], v[5], db(v[4]), self.rate),
        ];
        for (filter, p) in self.filters.iter_mut().zip(params) {
            filter.set(p, true);
        }
    }

    pub fn set(&mut self, knobs: [f32; 3]) {
        let target = targets(knobs.map(|k| f64::from(k.clamp(0.0, 1.0))));
        if self.started {
            self.ramp.set(target);
        } else {
            self.ramp = BlockRamp::new(self.rate, BLOCK, RAMP_S, target);
            self.values = target;
            self.update();
        }
        self.started = true;
    }

    #[inline]
    pub fn tick(&mut self, frame: &mut [f32; 2]) {
        let values = self.ramp.tick();
        if values.map(f32::to_bits) != self.values.map(f32::to_bits) {
            self.values = values;
            self.update();
        }
        let (first, rest) = self.filters.split_at_mut(2);
        for (ch, sample) in frame.iter_mut().enumerate() {
            let x = first
                .iter_mut()
                .fold(f64::from(*sample), |x, f| f.process(ch, x));
            let x = std::mem::replace(&mut self.delayed[ch], x);
            *sample = rest[0].process(ch, x) as f32;
        }
    }

    pub fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
        self.delayed = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_law() {
        assert!(gain_db(0.5, 26.6, 0.143, 0.084).abs() < 1e-9);
        assert!((gain_db(0.0, 26.6, 0.143, 0.084) + 26.6).abs() < 1e-9);
        assert!((gain_db(1.0, 26.6, 0.143, 0.084) - 6.0).abs() < 1e-9);
        assert!(gain_db(0.1, 26.6, 0.143, 0.084) < -18.0);
        assert!(gain_db(0.4, 26.6, 0.143, 0.084) > -2.0);
    }

    fn level(knobs: [f32; 3], hz: f32) -> f32 {
        let rate = 48_000.0;
        let mut eq = WideBandsEq::new(f64::from(rate));
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
        20.0 * ((sum / 24_000.0).sqrt() * 2f32.sqrt()).log10()
    }

    #[test]
    fn flat_at_the_centre_and_bands_cut() {
        for hz in [50.0, 1000.0, 8000.0] {
            assert!(level([0.5; 3], hz).abs() < 0.05, "{hz} Hz flat");
        }
        assert!((level([0.0, 0.5, 0.5], 70.0) + 26.6).abs() < 0.5, "low cut");
        assert!(
            (level([0.5, 0.0, 0.5], 1000.0) + 26.6).abs() < 0.5,
            "mid cut"
        );
        assert!(level([0.5, 0.5, 0.0], 15_000.0) < -20.0, "high cut");
    }
}
