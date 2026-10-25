use backspn_decode::Track;
use ebur128::{EbuR128, Mode};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    pub integrated_lufs: f64,
    pub peak_db: f64,
    pub true_peak_db: f64,
}

pub fn measure(track: &Track) -> Option<Loudness> {
    let mut meter = EbuR128::new(
        2,
        track.sample_rate(),
        Mode::I | Mode::SAMPLE_PEAK | Mode::TRUE_PEAK,
    )
    .ok()?;
    meter.add_frames_f32(track.samples()).ok()?;
    let integrated_lufs = meter.loudness_global().ok()?;
    let peak = meter.sample_peak(0).ok()?.max(meter.sample_peak(1).ok()?);
    let true_peak = meter.true_peak(0).ok()?.max(meter.true_peak(1).ok()?);
    Some(Loudness {
        integrated_lufs,
        peak_db: 20.0 * peak.log10(),
        true_peak_db: 20.0 * true_peak.log10(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halving_amplitude_drops_six_db() {
        let rate = 48_000;
        let make = |amp: f64| {
            let s: Vec<f32> = (0..rate * 5)
                .flat_map(|i| {
                    let v = (amp
                        * (2.0 * std::f64::consts::PI * 997.0 * f64::from(i) / f64::from(rate))
                            .sin()) as f32;
                    [v, v]
                })
                .collect();
            measure(&Track::from_stereo(rate, s)).unwrap()
        };
        let loud = make(0.5);
        let quiet = make(0.25);
        assert!((loud.integrated_lufs - quiet.integrated_lufs - 6.02).abs() < 0.05);
        assert!((loud.peak_db + 6.02).abs() < 0.05);
    }

    #[test]
    fn silence_is_minus_infinity() {
        let l = measure(&Track::from_stereo(44_100, vec![0.0; 44_100 * 2 * 3])).unwrap();
        assert!(l.integrated_lufs.is_infinite() && l.integrated_lufs < 0.0);
    }
}
