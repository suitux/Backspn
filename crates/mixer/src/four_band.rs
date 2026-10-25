use crate::rbj::{Band, Kind, Params};

const BANDS: [Kind; 4] = [Kind::LowShelf, Kind::Peak, Kind::Peak, Kind::HighShelf];

fn params(index: usize, k: f64, rate: f64) -> Params {
    let (hz, shape, gain) = match index {
        0 => (50.0 + 150.0 * k, 1.0, 1.996 * k + 0.002),
        1 => (350.0, 2.4 - 1.6 * k, 1.928_644_5 * k + 0.035_676),
        2 => (1850.0, 2.4 - 1.6 * k, 1.928_644_5 * k + 0.035_676),
        _ => (10_000.0 - 8000.0 * k, 1.0, 1.992_006 * k + 0.003_996),
    };
    Params::new(hz, shape, 20.0 * gain.log10(), rate)
}

#[derive(Debug, Clone)]
pub struct FourBandEq {
    rate: f64,
    bands: [Band; 4],
    started: bool,
}

impl FourBandEq {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            bands: std::array::from_fn(|i| Band::new(BANDS[i], params(i, 0.5, rate), rate)),
            started: false,
        }
    }

    pub fn set(&mut self, knobs: [f32; 4]) {
        for (i, band) in self.bands.iter_mut().enumerate() {
            let target = params(i, f64::from(knobs[i].clamp(0.0, 1.0)), self.rate);
            band.set(target, !self.started);
        }
        self.started = true;
    }

    #[inline]
    pub fn tick(&mut self, frame: &mut [f32; 2]) {
        for band in &mut self.bands {
            band.advance();
            for (ch, sample) in frame.iter_mut().enumerate() {
                *sample = band.process(ch, f64::from(*sample)) as f32;
            }
        }
    }

    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbj::{coefficients, response_db};

    const RATE: f64 = 48_000.0;

    fn band_db(index: usize, k: f64, hz: f64) -> f64 {
        response_db(coefficients(BANDS[index], params(index, k, RATE)), hz, RATE)
    }

    #[test]
    fn centred_knobs_are_flat() {
        for index in 0..4 {
            for hz in [40.0, 350.0, 1850.0, 9000.0] {
                assert!(
                    band_db(index, 0.5, hz).abs() < 0.1,
                    "band {index} at {hz} Hz"
                );
            }
        }
    }

    #[test]
    fn knobs_cut_and_boost_their_band() {
        assert!(band_db(0, 0.0, 5.0) < -45.0, "low kill");
        assert!(band_db(0, 0.0, 20.0) < -25.0, "low kill at 20 Hz");
        assert!((band_db(0, 1.0, 30.0) - 6.0).abs() < 0.3, "low boost");
        assert!(band_db(1, 0.0, 350.0) < -25.0, "mid-low cut");
        assert!(
            (band_db(2, 1.0, 1850.0) - 5.9).abs() < 0.3,
            "mid-high boost"
        );
        assert!(band_db(3, 0.0, 18_000.0) < -40.0, "high kill");
        assert!(band_db(3, 0.0, 100.0).abs() < 0.5, "high leaves the bass");
    }

    #[test]
    fn changes_ramp_over_a_tenth_of_a_second() {
        let mut eq = FourBandEq::new(RATE);
        eq.set([0.5; 4]);
        eq.set([0.0, 0.5, 0.5, 0.5]);
        let mut frame = [0.0f32; 2];
        for _ in 0..(RATE as usize / 20) {
            eq.tick(&mut frame);
        }
        let half = eq.bands[0].current.db;
        for _ in 0..(RATE as usize / 10) {
            eq.tick(&mut frame);
        }
        let end = eq.bands[0].current.db;
        assert!(half > end + 10.0 && (end - params(0, 0.0, RATE).db).abs() < 1e-9);
    }
}
