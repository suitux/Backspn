use crate::transparent_limiter::{LimiterSettings, TransparentLimiter};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorSettings {
    pub enabled: bool,
    pub input_gain_db: f32,
    pub output_gain_db: f32,
    pub threshold_db: f32,
    pub dry_wet: f32,
    pub attack_s: f32,
    pub release_s: f32,
    pub ratio: f32,
    pub hp_cutoff: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemLimiterSettings {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ceiling_db: f32,
    pub release_s: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemMasterSettings {
    pub compressor: CompressorSettings,
    pub limiter: StemLimiterSettings,
}

impl Default for StemMasterSettings {
    fn default() -> Self {
        Self {
            compressor: CompressorSettings {
                enabled: false,
                input_gain_db: 0.0,
                output_gain_db: 0.0,
                threshold_db: 0.0,
                dry_wet: 50.0,
                attack_s: 0.003,
                release_s: 0.3,
                ratio: 3.0,
                hp_cutoff: 300.0,
            },
            limiter: StemLimiterSettings {
                enabled: false,
                threshold_db: 0.0,
                ceiling_db: 0.0,
                release_s: 0.001,
            },
        }
    }
}

const RATIOS: [(f32, f32); 6] = [
    (1.5, -12.0),
    (2.0, -9.0),
    (3.0, -5.0),
    (4.0, -4.0),
    (5.0, -3.3),
    (10.0, -0.2),
];
const WET_MAKEUP_EXP10: f32 = 0.117_27;
const DETECTOR_SCALE: f32 = 1.0 / 128.0;
const LEVEL_FLOOR: f32 = -0.75;
const LN_0_8: f32 = -0.223_143_55;
const RELEASE_SCALE: f32 = 1.0 / 16.0;
const LIMITER_LOOKAHEAD: usize = 32;

fn level(x: f32) -> f32 {
    if x < 2f32.powi(-24) {
        LEVEL_FLOOR
    } else {
        x.log2() / 32.0
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct HighPass {
    b0: f64,
    b1: f64,
    a1: f64,
    a2: f64,
    s1: f64,
    s2: f64,
}

impl HighPass {
    fn new(freq: f32, rate: f32) -> Self {
        let k = 1.0 / (std::f64::consts::PI * f64::from(freq) / f64::from(rate)).tan();
        let k2 = k * k;
        let norm = 1.0 / (k2 + std::f64::consts::SQRT_2 * k + 1.0);
        Self {
            b0: k2 * norm,
            b1: -2.0 * k2 * norm,
            a1: 2.0 * (1.0 - k2) * norm,
            a2: (k2 - std::f64::consts::SQRT_2 * k + 1.0) * norm,
            s1: 0.0,
            s2: 0.0,
        }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b0 * x - self.a2 * y;
        y
    }
}

#[derive(Debug, Clone)]
struct Compressor {
    input_gain: f32,
    high_pass: Option<[HighPass; 2]>,
    threshold: f32,
    slope: f32,
    attack: f32,
    release: f32,
    wet: f32,
    dry: f32,
    state: f32,
    feedback: f32,
}

impl Compressor {
    fn new(s: &CompressorSettings, rate: f32) -> Self {
        let db_to_gain = |db: f32| 10f32.powf(db / 20.0);
        let index = RATIOS
            .iter()
            .position(|&(r, _)| s.ratio <= r)
            .unwrap_or(RATIOS.len() - 1);
        let (ratio, offset) = RATIOS[index];
        let threshold_db = s.threshold_db.clamp(-40.0, 0.0) + offset;
        let input_gain = db_to_gain(s.input_gain_db.clamp(-12.0, 12.0));
        let output_gain = db_to_gain(s.output_gain_db.clamp(-12.0, 12.0));
        let wet = s.dry_wet.clamp(0.0, 100.0) / 100.0;
        let attack_s = s.attack_s.clamp(1e-4, 0.03);
        let release_s = s.release_s.clamp(0.1, 1.6) * RELEASE_SCALE;
        let time = |t: f32| 1.0 - (LN_0_8 / (rate * t)).exp();
        let cutoff = s.hp_cutoff.min(500.0);
        Self {
            input_gain,
            high_pass: (s.hp_cutoff >= 20.0).then(|| [HighPass::new(cutoff, rate); 2]),
            threshold: level(db_to_gain(threshold_db) * DETECTOR_SCALE),
            slope: ratio - 1.0,
            attack: time(attack_s),
            release: time(release_s),
            wet: input_gain * 10f32.powf(WET_MAKEUP_EXP10) * wet * output_gain,
            dry: (1.0 - wet) * output_gain,
            state: 0.0,
            feedback: 0.0,
        }
    }

    fn gain(&mut self, frame: [f32; 2]) -> f32 {
        let x = frame.map(|s| s * self.input_gain);
        let detected = match &mut self.high_pass {
            Some(filters) => {
                let l = filters[0].process(f64::from(x[0])) as f32;
                let r = filters[1].process(f64::from(x[1])) as f32;
                l.abs().max(r.abs())
            }
            None => x[0].abs().max(x[1].abs()),
        };
        let over = (level(self.feedback.min(1.0)) - self.threshold) * self.slope;
        let target = if over < 0.0 { 0.0 } else { over.min(1.0) };
        let coef = if target > self.state {
            self.attack
        } else {
            self.release
        };
        self.state += (target - self.state) * coef;
        let gain = (self.state * -32.0).clamp(-32.0, 0.0).exp2();
        self.feedback = detected * DETECTOR_SCALE * gain;
        self.wet * gain + self.dry
    }
}

#[derive(Debug, Clone)]
struct Delay {
    frames: Box<[[f32; 2]]>,
    pos: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self {
            frames: vec![[0.0; 2]; len.max(1)].into_boxed_slice(),
            pos: 0,
        }
    }

    fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        if self.frames.len() == 1 {
            return frame;
        }
        let out = self.frames[self.pos];
        self.frames[self.pos] = frame;
        self.pos = (self.pos + 1) % self.frames.len();
        out
    }
}

#[derive(Debug, Clone)]
pub struct StemMaster {
    compressor: Option<Compressor>,
    limiter: TransparentLimiter,
    limiter_on: bool,
    delay: Delay,
}

impl StemMaster {
    pub fn new(settings: &StemMasterSettings, rate: f64) -> Self {
        let l = &settings.limiter;
        let limiter = TransparentLimiter::with(
            rate,
            LimiterSettings {
                lookahead: LIMITER_LOOKAHEAD,
                drive: 10f32.powf(-l.threshold_db.clamp(-40.0, 0.0) / 20.0),
                ceiling: 10f32.powf(l.ceiling_db.clamp(-40.0, -0.001) / 20.0),
                release_s: l.release_s.clamp(0.001, 1.0),
            },
        );
        let delay = Delay::new(limiter.latency());
        Self {
            compressor: settings
                .compressor
                .enabled
                .then(|| Compressor::new(&settings.compressor, rate as f32)),
            limiter,
            limiter_on: l.enabled,
            delay,
        }
    }

    pub fn latency(&self) -> usize {
        self.limiter.latency()
    }

    #[inline]
    pub fn tick(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let gain = self.compressor.as_mut().map_or(1.0, |c| c.gain(frame));
        let x = frame.map(|s| s * gain);
        if self.limiter_on {
            self.limiter.tick(x)
        } else {
            self.delay.process(x)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 44_100.0;

    fn sine_peak(master: &mut StemMaster, amplitude: f32, seconds: f32) -> f32 {
        let n = (seconds * RATE as f32) as usize;
        let mut peak = 0.0f32;
        for i in 0..n {
            let x = amplitude * (std::f32::consts::TAU * 1_000.0 * i as f32 / RATE as f32).sin();
            let [l, _] = master.tick([x, x]);
            if i > n / 2 {
                peak = peak.max(l.abs());
            }
        }
        peak
    }

    fn compressor_only(threshold_db: f32, ratio: f32, dry_wet: f32) -> StemMasterSettings {
        let mut s = StemMasterSettings::default();
        s.compressor = CompressorSettings {
            enabled: true,
            threshold_db,
            ratio,
            dry_wet,
            ..s.compressor
        };
        s
    }

    #[test]
    fn with_both_parts_off_the_signal_passes_late_by_the_look_ahead() {
        let mut m = StemMaster::new(&StemMasterSettings::default(), RATE);
        assert_eq!(m.latency(), 31);
        let out: Vec<f32> = (0..40)
            .map(|i| m.tick([if i == 0 { 0.5 } else { 0.0 }; 2])[0])
            .collect();
        assert_eq!(out.iter().position(|&v| v != 0.0), Some(31));
        assert!((out[31] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn above_the_threshold_the_ratio_holds_the_level_down() {
        let mut s = compressor_only(-20.0, 3.0, 100.0);
        s.compressor.hp_cutoff = 0.0;
        let mut m = StemMaster::new(&s, RATE);
        let amplitude = 10f32.powf(-5.0 / 20.0);
        let mut last = 0.0;
        for i in 0..88_200 {
            let x = if (i / 22) % 2 == 0 {
                amplitude
            } else {
                -amplitude
            };
            last = m.tick([x, x])[0].abs();
        }
        let got = 20.0 * last.log10();
        let want = -25.0 + 20.0 / 3.0 + 20.0 * WET_MAKEUP_EXP10;
        assert!((got - want).abs() < 0.05, "{got} dB, want {want}");
    }

    #[test]
    fn below_the_threshold_only_the_fixed_gains_apply() {
        let mut m = StemMaster::new(&compressor_only(0.0, 3.0, 50.0), RATE);
        let peak = sine_peak(&mut m, 0.01, 1.0);
        let want = 0.01 * (0.5 * 10f32.powf(WET_MAKEUP_EXP10) + 0.5);
        assert!((peak - want).abs() < 1e-4, "{peak} vs {want}");
    }

    #[test]
    fn the_limiter_keeps_peaks_under_the_ceiling() {
        let mut s = StemMasterSettings::default();
        s.limiter.enabled = true;
        s.limiter.ceiling_db = -1.0;
        let mut m = StemMaster::new(&s, RATE);
        let peak = sine_peak(&mut m, 2.0, 1.0);
        assert!(peak <= 10f32.powf(-1.0 / 20.0) + 1e-3, "{peak}");
        assert!(peak > 0.8, "{peak}");
    }

    #[test]
    fn ratios_pick_the_next_step_up() {
        let pick = |r: f32| {
            let c = Compressor::new(
                &CompressorSettings {
                    ratio: r,
                    ..StemMasterSettings::default().compressor
                },
                RATE as f32,
            );
            c.slope + 1.0
        };
        assert!((pick(1.0) - 1.5).abs() < 1e-6);
        assert!((pick(2.5) - 3.0).abs() < 1e-6);
        assert!((pick(50.0) - 10.0).abs() < 1e-6);
    }
}
