use crate::rbj::{Band, Kind, Params};

const HZ: [f64; 3] = [64.0, 1900.0, 12_750.0];

const CHAIN: [(Kind, usize); 5] = [
    (Kind::Peak, 0),
    (Kind::LowShelf, 0),
    (Kind::Peak, 1),
    (Kind::Peak, 2),
    (Kind::HighShelf, 2),
];

fn params(band: usize, k: f64, rate: f64) -> Params {
    let k2 = k * k;
    let db = 20.0 * (0.06 + 3.76 * k2).log10();
    let db = if band == 1 { db } else { db * 2.0 / 3.0 };
    Params::new(HZ[band], 2.4 * (1.0 + k2), db, rate)
}

#[derive(Debug, Clone)]
pub struct ThreeCentreEq {
    rate: f64,
    filters: [Band; 5],
    delayed: [f64; 2],
    started: bool,
}

impl ThreeCentreEq {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            filters: CHAIN.map(|(kind, band)| Band::new(kind, params(band, 0.5, rate), rate)),
            delayed: [0.0; 2],
            started: false,
        }
    }

    pub fn set(&mut self, knobs: [f32; 3]) {
        for (filter, (_, band)) in self.filters.iter_mut().zip(CHAIN) {
            let k = f64::from(knobs[band].clamp(0.0, 1.0));
            filter.set(params(band, k, self.rate), !self.started);
        }
        self.started = true;
    }

    #[inline]
    pub fn tick(&mut self, frame: &mut [f32; 2]) {
        for filter in &mut self.filters {
            filter.advance();
        }
        let (first, rest) = self.filters.split_at_mut(2);
        for (ch, sample) in frame.iter_mut().enumerate() {
            let low = first
                .iter_mut()
                .fold(f64::from(*sample), |x, f| f.process(ch, x));
            let x = std::mem::replace(&mut self.delayed[ch], low);
            *sample = rest.iter_mut().fold(x, |x, f| f.process(ch, x)) as f32;
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
    use crate::rbj::{coefficients, response_db};

    const RATE: f64 = 48_000.0;

    fn chain_db(knobs: [f64; 3], hz: f64) -> f64 {
        CHAIN
            .iter()
            .map(|&(kind, band)| {
                response_db(
                    coefficients(kind, params(band, knobs[band], RATE)),
                    hz,
                    RATE,
                )
            })
            .sum()
    }

    #[test]
    fn centred_knobs_are_flat() {
        for hz in [30.0, 64.0, 500.0, 1900.0, 12_750.0, 18_000.0] {
            assert!(chain_db([0.5; 3], hz).abs() < 0.01, "{hz} Hz");
        }
    }

    #[test]
    fn knobs_cut_and_boost_their_band() {
        for band in 0..3 {
            let mut knobs = [0.5; 3];
            knobs[band] = 0.0;
            assert!((chain_db(knobs, HZ[band]) + 24.4).abs() < 0.1, "cut {band}");
            knobs[band] = 1.0;
            assert!(
                (chain_db(knobs, HZ[band]) - 11.6).abs() < 0.1,
                "boost {band}"
            );
        }
    }

    #[test]
    fn the_signal_lags_one_sample() {
        let mut eq = ThreeCentreEq::new(RATE);
        eq.set([0.5; 3]);
        let mut first = [1.0f32, 1.0];
        eq.tick(&mut first);
        let mut second = [0.0f32; 2];
        eq.tick(&mut second);
        assert!(first[0].abs() < 1e-6 && (second[0] - 1.0).abs() < 1e-3);
    }
}
