use crate::hilbert::Hilbert;

const LOOKAHEAD: usize = 128;
const BASE_RATE: f32 = 44_100.0;
const WARP: f32 = -0.89;
const ENV_RISE_S: f32 = 0.014;
const ENV_FALL_S: f32 = 0.003;
const RELEASE_S: f32 = 0.035;
const SMOOTH_S: f32 = 0.0006;
const EPS: f32 = 1e-12;
const FLOOR_DB: f32 = -100.0;

fn coef(t: f32, rate: f32) -> f32 {
    1.0 - 1.0 / (t * rate)
}

fn db(x: f32) -> f32 {
    20.0 * x.log10()
}

#[derive(Debug, Clone)]
struct SlidingMax {
    levels: Box<[f32]>,
    max: f32,
    left: usize,
}

impl SlidingMax {
    fn new(len: usize) -> Self {
        Self {
            levels: vec![0.0; len].into_boxed_slice(),
            max: FLOOR_DB,
            left: 0,
        }
    }

    fn push(&mut self, pos: usize, level: f32) -> f32 {
        let len = self.levels.len();
        self.levels[pos] = level;
        if level >= self.max {
            self.max = level;
            self.left = len - 1;
        } else if self.left > 0 {
            self.left -= 1;
        } else {
            self.max = FLOOR_DB;
            let mut at = 0;
            for back in 0..len {
                let v = self.levels[(pos + len - back) % len];
                if v > self.max {
                    self.max = v;
                    at = back;
                }
            }
            self.left = len - 1 - at;
        }
        self.max
    }
}

#[derive(Debug, Clone)]
struct Hann {
    n: usize,
    inv_n: f64,
    cos: f64,
    history: Box<[f64]>,
    pos: usize,
    banks: [[f64; 3]; 2],
    count: usize,
    restart: usize,
}

impl Hann {
    fn new(n: usize, restart: usize) -> Self {
        Self {
            n,
            inv_n: 1.0 / n as f64,
            cos: (std::f64::consts::TAU / n as f64).cos(),
            history: vec![0.0; n + 1].into_boxed_slice(),
            pos: 0,
            banks: [[0.0; 3]; 2],
            count: 0,
            restart: restart.max(n),
        }
    }

    fn step(bank: &mut [f64; 3], two_cos: f64, x: f64) -> f64 {
        let [sum, y1, y2] = *bank;
        let y = two_cos * y1 + x - y2;
        *bank = [sum + x, y, y1];
        y1
    }

    fn process(&mut self, x: f64) -> f64 {
        let len = self.history.len();
        self.history[self.pos] = x;
        let old = self.history[(self.pos + len - self.n) % len];
        self.pos = (self.pos + 1) % len;
        let two_cos = 2.0 * self.cos;
        let prev = Self::step(&mut self.banks[0], two_cos, x - old);
        let [sum, y, _] = self.banks[0];
        let out = self.inv_n * (sum - (y - self.cos * prev));
        if self.count >= self.restart - self.n {
            Self::step(&mut self.banks[1], two_cos, x);
        }
        self.count += 1;
        if self.count >= self.restart {
            self.banks = [self.banks[1], [0.0; 3]];
            self.count = 0;
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimiterSettings {
    pub lookahead: usize,
    pub drive: f32,
    pub ceiling: f32,
    pub release_s: f32,
}

impl LimiterSettings {
    pub const MASTER: Self = Self {
        lookahead: LOOKAHEAD,
        drive: 1.0,
        ceiling: 1.0,
        release_s: RELEASE_S,
    };
}

#[derive(Debug, Clone)]
pub struct TransparentLimiter {
    delay: Box<[[f32; 2]]>,
    windows: [SlidingMax; 2],
    pos: usize,
    lookahead: usize,
    hilbert: Hilbert,
    env: [f32; 2],
    env_rise: f32,
    env_fall: f32,
    hold_db: f32,
    release: f32,
    smooth_db: f32,
    smooth: f32,
    hann: Hann,
    drive: f32,
    ceiling: f32,
    pub min_gain: f32,
}

impl TransparentLimiter {
    pub fn new(rate: f64) -> Self {
        Self::with(rate, LimiterSettings::MASTER)
    }

    pub fn with(rate: f64, settings: LimiterSettings) -> Self {
        let fs = rate as f32;
        let multiplier = ((fs / BASE_RATE) as usize).max(1);
        let lookahead = settings.lookahead.max(1) * multiplier;
        Self {
            delay: vec![[0.0; 2]; 2 * lookahead].into_boxed_slice(),
            windows: [SlidingMax::new(lookahead), SlidingMax::new(lookahead)],
            pos: 0,
            lookahead,
            hilbert: Hilbert::new(WARP, multiplier),
            env: [0.0; 2],
            env_rise: coef(ENV_RISE_S, fs),
            env_fall: coef(ENV_FALL_S, fs),
            hold_db: 0.0,
            release: coef(settings.release_s.max(0.001), fs),
            smooth_db: 0.0,
            smooth: coef(SMOOTH_S, fs),
            hann: Hann::new(lookahead, fs as usize),
            drive: settings.drive,
            ceiling: settings.ceiling,
            min_gain: 1.0,
        }
    }

    pub fn latency(&self) -> usize {
        self.lookahead - 1
    }

    #[inline]
    pub fn tick(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let x = frame.map(|s| s * self.drive + EPS);
        let len = self.delay.len();
        self.delay[self.pos] = x;
        let window_pos = self.pos % self.lookahead;
        let window_max = [0, 1].map(|ch| self.windows[ch].push(window_pos, db(x[ch].abs())));

        let [lr, li, rr, ri] = self.hilbert.process(x[0], x[1]);
        for (env, mag) in self.env.iter_mut().zip([lr.hypot(li), rr.hypot(ri)]) {
            let c = if *env < mag {
                self.env_rise
            } else {
                self.env_fall
            };
            *env = mag + c * (*env - mag);
        }
        let peak = db(self.env[0].max(self.env[1]).max(x[0].abs()).max(x[1].abs()));
        self.hold_db = if self.hold_db < peak {
            peak
        } else {
            peak + self.release * (self.hold_db - peak)
        };
        self.smooth_db = self.hold_db + self.smooth * (self.smooth_db - self.hold_db);
        let reduction = (-self.smooth_db)
            .min(0.0)
            .min(-window_max[0])
            .min(-window_max[1]);
        let gain = 10f32.powf(reduction / 20.0);
        self.min_gain = self.min_gain.min(gain);
        let gain = self.hann.process(f64::from(gain)) as f32;

        let out = self.delay[(self.pos + len - (self.lookahead - 1)) % len];
        self.pos = (self.pos + 1) % len;
        out.map(|s| s * gain * self.ceiling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_is_the_weighted_average() {
        let n = 8;
        let mut h = Hann::new(n, 20);
        let input: Vec<f64> = (0..60).map(|i| f64::from((i * 7 % 11) as u8)).collect();
        for (i, &x) in input.iter().enumerate() {
            let got = h.process(x);
            let want: f64 = (0..n.min(i + 1))
                .map(|k| {
                    (1.0 - (std::f64::consts::TAU * k as f64 / n as f64).cos()) / n as f64
                        * input[i - k]
                })
                .sum();
            assert!((got - want).abs() < 1e-9, "{i}: {got} vs {want}");
        }
    }

    #[test]
    fn sliding_max_follows_the_window() {
        let mut w = SlidingMax::new(4);
        let levels = [1.0, 5.0, 2.0, 3.0, 0.0, 0.0, 4.0, 1.0, 1.0, 1.0, 1.0];
        for (i, &l) in levels.iter().enumerate() {
            let want = levels[i.saturating_sub(3)..=i]
                .iter()
                .fold(FLOOR_DB, |a, &b| a.max(b));
            assert_eq!(w.push(i % 4, l), want, "{i}");
        }
    }

    #[test]
    fn peaks_stay_under_full_scale_and_quiet_audio_passes() {
        let rate = 48_000.0;
        let mut lim = TransparentLimiter::new(rate);
        let mut loud_peak = 0.0f32;
        let mut quiet_peak = 0.0f32;
        for n in 0..96_000 {
            let t = n as f32 / rate as f32;
            let amp = if n < 48_000 { 3.0 } else { 0.25 };
            let x = amp * (std::f32::consts::TAU * 100.0 * t).sin();
            let [l, r] = lim.tick([x, -x]);
            assert!(l.abs() <= 1.0 + 1e-4 && r.abs() <= 1.0 + 1e-4, "{n}: {l}");
            if (10_000..48_000).contains(&n) {
                loud_peak = loud_peak.max(l.abs());
            }
            if n > 60_000 {
                quiet_peak = quiet_peak.max(l.abs());
            }
        }
        assert!(loud_peak > 0.9, "loud {loud_peak}");
        assert!((quiet_peak - 0.25).abs() < 0.01, "quiet {quiet_peak}");
    }

    #[test]
    fn the_output_lags_by_the_look_ahead() {
        let mut lim = TransparentLimiter::new(48_000.0);
        for _ in 0..1_000 {
            lim.tick([0.0; 2]);
        }
        let mut first = None;
        for n in 0..400 {
            let x = if n == 0 { 0.5 } else { 0.0 };
            if lim.tick([x, x])[0] > 0.1 {
                first.get_or_insert(n);
            }
        }
        assert_eq!(first, Some(LOOKAHEAD - 1));
    }
}
