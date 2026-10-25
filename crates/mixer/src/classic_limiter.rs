const THRESHOLD: f32 = 1.0;
const RELEASE_S: f32 = 0.2;
const WINDOW_S: f32 = 0.0007;
const EPS: f32 = 1e-18;

#[derive(Debug, Clone)]
pub struct ClassicLimiter {
    delay: Box<[[f32; 2]]>,
    peaks: Box<[f32]>,
    held: Box<[f32]>,
    weights: Box<[f32]>,
    pos: usize,
    prev: usize,
    max: Option<(f32, usize)>,
    release: f32,
    pub min_gain: f32,
}

impl ClassicLimiter {
    pub fn new(rate: f64) -> Self {
        let fs = rate as f32;
        let n = ((WINDOW_S * fs) as usize).max(1);
        let weights = (0..n)
            .map(|i| (1.0 - (std::f32::consts::TAU * i as f32 / n as f32).cos()) / n as f32)
            .collect();
        Self {
            delay: vec![[0.0; 2]; n].into_boxed_slice(),
            peaks: vec![0.0; n].into_boxed_slice(),
            held: vec![0.0; n].into_boxed_slice(),
            weights,
            pos: 1 % n,
            prev: 0,
            max: None,
            release: (-1.0 / (RELEASE_S * fs.trunc())).exp(),
            min_gain: 1.0,
        }
    }

    fn window_max(&mut self, peak: f32) -> f32 {
        let (max, at) = match self.max {
            Some((max, _)) if peak >= max => (peak, self.pos),
            Some(known) => known,
            None => {
                self.peaks.iter().enumerate().fold(
                    (0.0, 0),
                    |(m, a), (i, &p)| if m < p { (p, i) } else { (m, a) },
                )
            }
        };
        let next = (self.pos + 1) % self.peaks.len();
        self.max = (at != next).then_some((max, at));
        max
    }

    #[inline]
    pub fn tick(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let n = self.peaks.len();
        let next = (self.pos + 1) % n;
        self.delay[self.pos] = frame;
        let peak = frame[0].abs().max(frame[1].abs()) + EPS;
        self.peaks[self.pos] = peak;
        let level = self.window_max(peak).max(THRESHOLD);
        let before = self.held[self.prev];
        self.held[self.pos] = if level >= before {
            level
        } else {
            THRESHOLD + self.release * (before - THRESHOLD)
        };
        let mut smooth = EPS;
        for (i, w) in self.weights.iter().enumerate() {
            smooth += w * self.held[(self.pos + n - i) % n];
        }
        let gain = THRESHOLD / smooth;
        self.min_gain = self.min_gain.min(gain);
        let out = self.delay[next];
        self.prev = self.pos;
        self.pos = next;
        out.map(|s| s * gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_and_release() {
        let lim = ClassicLimiter::new(48_000.0);
        assert_eq!(lim.peaks.len(), 33);
        assert!((lim.release - (-1.0f32 / 9600.0).exp()).abs() < 1e-7);
    }

    #[test]
    fn loud_audio_is_held_near_full_scale_and_quiet_passes() {
        let rate = 48_000.0f32;
        let mut lim = ClassicLimiter::new(f64::from(rate));
        let (mut loud, mut quiet) = (0.0f32, 0.0f32);
        for n in 0..144_000 {
            let amp = if n < 48_000 { 4.0 } else { 0.25 };
            let x = amp * (std::f32::consts::TAU * 100.0 * n as f32 / rate).sin();
            let [l, _] = lim.tick([x, x]);
            if (10_000..48_000).contains(&n) {
                loud = loud.max(l.abs());
            }
            if n > 120_000 {
                quiet = quiet.max(l.abs());
            }
        }
        assert!(loud > 0.95 && loud < 1.05, "loud {loud}");
        assert!((quiet - 0.25).abs() < 0.01, "quiet {quiet}");
    }

    #[test]
    fn the_output_lags_by_the_window_less_one() {
        let mut lim = ClassicLimiter::new(48_000.0);
        for _ in 0..100 {
            lim.tick([0.0; 2]);
        }
        let mut first = None;
        for n in 0..100 {
            let x = if n == 0 { 0.5 } else { 0.0 };
            if lim.tick([x, x])[0] > 0.1 {
                first.get_or_insert(n);
            }
        }
        assert_eq!(first, Some(32));
    }
}
