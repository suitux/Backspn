use std::f64::consts::TAU;

const STEPS: f32 = 255.0;
const LP_FULL_BELOW: i32 = 110;
const LP_LAST: i32 = 125;
const HP_FIRST: i32 = 131;
const HP_FULL_ABOVE: i32 = 146;
const FADE_STEPS: f32 = 16.0;
const SPAN: f32 = 126.0;
const LP_BOTTOM_HZ: f32 = 100.0;
const LP_SPAN_HZ: f32 = 25_000.0;
const HP_BOTTOM_HZ: f32 = 20.0;
const HP_SPAN_HZ: f32 = 8_200.0;
const LP_OPEN_HZ: f64 = 25_100.0;
const HP_OPEN_HZ: f64 = 20.0;
const HP_FLOOR_BELOW_HZ: f32 = 249.9;
const HP_FLOOR_SPAN_HZ: f32 = 230.0;
const RESONANCE_SLOPE: f32 = 15.0 / 4_096.0;
const STEP_MS: f32 = 1.0 / 3.0;
const MIX_RAMP_PER_S: f32 = 100.0;

/// Knob or resonance position (0–1) as one of the 256 steps the filter works in.
pub fn step_of(value: f32) -> i32 {
    let v = if value < 0.0 {
        0.0
    } else {
        value.min(1.0) * STEPS
    };
    f64::from(v).round_ties_even() as i32
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClubSetting {
    pub hp_hz: f64,
    pub lp_hz: f64,
    pub damping: f64,
    pub gain: f64,
}

fn damping_of(r: i32) -> f32 {
    if r <= 127 {
        let u = r as f32 / 128.0;
        let a = 1.0 - u;
        let a = (-u).mul_add(a, a);
        a.mul_add(3.5, 0.5)
    } else {
        ((r - 128) as f32).mul_add(-RESONANCE_SLOPE, 0.5)
    }
}

/// Cut-offs, damping and gain for knob step `m` and resonance step `r` (both 0–255).
pub fn club_setting(m: i32, r: i32) -> ClubSetting {
    let (hp_hz, lp_hz, floor) = if m <= LP_LAST {
        let x = m as f32 / SPAN;
        let lp = (x * x).mul_add(m as f32 * (LP_SPAN_HZ / SPAN), LP_BOTTOM_HZ);
        (HP_OPEN_HZ, f64::from(lp), 0.0)
    } else if m >= HP_FIRST {
        let x = (m - 130) as f32 / SPAN;
        let x3 = x * x * x;
        let hp = x3.mul_add(HP_SPAN_HZ, HP_BOTTOM_HZ);
        let floor = if hp < HP_FLOOR_BELOW_HZ {
            let depth = x3 * (HP_SPAN_HZ / HP_FLOOR_SPAN_HZ);
            let rest = 1.0 - depth;
            let rest = 4.0 * (-depth).mul_add(rest, rest);
            ((-depth).mul_add(rest, rest) + 0.03).min(2.0)
        } else {
            0.0
        };
        (f64::from(hp), LP_OPEN_HZ, floor)
    } else {
        (HP_OPEN_HZ, LP_OPEN_HZ, 0.0)
    };
    let damping = damping_of(r).max(floor);
    let distance = if m > 127 {
        (m - 192).abs()
    } else {
        (m - 64).abs()
    };
    let s = distance as f32 / 128.0;
    let k = if damping < 0.5 {
        damping + damping
    } else {
        1.0
    };
    let gain = (s + 0.5).mul_add(k, 0.5 - s);
    ClubSetting {
        hp_hz,
        lp_hz,
        damping: f64::from(damping),
        gain: f64::from(gain),
    }
}

/// Target weights `[dry, low-pass, high-pass]` for knob step `m`: the low-pass
/// fades in over the 16 steps left of the centre, the high-pass over the 16 right
/// of it.
pub fn mix_of(m: i32) -> [f32; 3] {
    if m < LP_FULL_BELOW {
        [0.0, 1.0, 0.0]
    } else if m <= LP_LAST {
        let dry = (m - LP_FULL_BELOW) as f32 / FADE_STEPS;
        [dry, 1.0 - dry, 0.0]
    } else if m > HP_FULL_ABOVE {
        [0.0, 0.0, 1.0]
    } else if m >= HP_FIRST {
        let hp = (m - 130) as f32 / FADE_STEPS;
        [1.0 - hp, 0.0, hp]
    } else {
        [1.0, 0.0, 0.0]
    }
}

/// Integer value that walks one step at a time towards its target.
#[derive(Debug, Clone, Copy)]
struct Stepper {
    value: i32,
    target: i32,
    dir: i32,
    period: u32,
    left: u32,
}

impl Stepper {
    fn new(value: i32, period: u32) -> Self {
        Self {
            value,
            target: value,
            dir: 0,
            period: period.max(1),
            left: 0,
        }
    }

    fn set(&mut self, to: i32) {
        if self.value == to {
            return;
        }
        if self.value == self.target {
            self.left = self.period;
        }
        self.target = to;
        self.dir = if self.value < to { 1 } else { -1 };
    }

    fn jump(&mut self) {
        self.value = self.target;
        self.left = 0;
    }

    /// Advances one sample; true when the value moved.
    #[inline]
    fn tick(&mut self) -> bool {
        if self.value == self.target {
            return false;
        }
        self.left = self.left.saturating_sub(1);
        if self.left > 0 {
            return false;
        }
        self.value += self.dir;
        self.left = if self.value == self.target {
            0
        } else {
            self.period
        };
        true
    }
}

/// Second-order section in transposed direct form; on a coefficient change its
/// state is rebuilt from the last two inputs and outputs, so sweeps behave like
/// direct form I.
#[derive(Debug, Clone, Copy, Default)]
struct Section {
    b: [f64; 3],
    a: [f64; 2],
    s: [[f64; 2]; 2],
    x: [[f64; 2]; 2],
    y: [[f64; 2]; 2],
}

impl Section {
    fn set(&mut self, high_pass: bool, hz: f64, damping: f64, gain: f64, period: f64) {
        let wide = 2.0 * TAU * hz * (damping * period);
        let omega = TAU * hz;
        let square = omega * omega * (period * period);
        let norm = 1.0 / (wide + 4.0 + square);
        self.b = if high_pass {
            let b0 = 4.0 * gain * norm;
            [b0, -2.0 * b0, b0]
        } else {
            let b0 = square * gain * norm;
            [b0, b0 + b0, b0]
        };
        self.a = [(square + square - 8.0) * norm, norm * (4.0 - wide + square)];
        let ([_, b1, b2], [a1, a2]) = (self.b, self.a);
        for ch in 0..2 {
            let ([x1, x2], [y1, y2]) = (self.x[ch], self.y[ch]);
            self.s[ch] = [x1 * b1 + x2 * b2 - (y1 * a1 + y2 * a2), x1 * b2 - y1 * a2];
        }
    }

    #[inline]
    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let ([b0, b1, b2], [a1, a2]) = (self.b, self.a);
        let s = &mut self.s[ch];
        let y = x * b0 + s[0];
        s[0] = x * b1 + s[1] - y * a1;
        s[1] = x * b2 - y * a2;
        self.x[ch] = [x, self.x[ch][0]];
        self.y[ch] = [y, self.y[ch][0]];
        y
    }
}

/// Resonant DJ filter: a low-pass and a high-pass run side by side and are mixed
/// with the dry signal. The knob moves through 256 steps, one every third of a
/// millisecond, and the mix weights glide in 10 ms.
#[derive(Debug, Clone)]
pub struct ClubFilter {
    t: f64,
    knob: Stepper,
    resonance: Stepper,
    lp: Section,
    hp: Section,
    mix: [f32; 3],
    target: [f32; 3],
    ramp: f32,
}

impl ClubFilter {
    pub fn new(rate: f64) -> Self {
        let period = (STEP_MS * 0.001 * rate as f32) as u32;
        let centre = step_of(0.5);
        let mut f = Self {
            t: 1.0 / rate,
            knob: Stepper::new(centre, period),
            resonance: Stepper::new(centre, period),
            lp: Section::default(),
            hp: Section::default(),
            mix: mix_of(centre),
            target: mix_of(centre),
            ramp: MIX_RAMP_PER_S / rate as f32,
        };
        f.update();
        f
    }

    /// Knob from -1 (low-pass) through 0 (open) to 1 (high-pass).
    pub fn set_knob(&mut self, knob: f32) {
        self.knob.set(step_of(f32::midpoint(knob, 1.0)));
    }

    /// Resonance from 0 (no peak) to 1 (strong peak).
    pub fn set_resonance(&mut self, resonance: f32) {
        self.resonance.set(step_of(resonance));
    }

    /// Lands on the last knob and resonance with a clean state, for when the
    /// filter starts sounding again.
    pub fn settle(&mut self) {
        let moving =
            self.knob.value != self.knob.target || self.resonance.value != self.resonance.target;
        if moving {
            self.knob.jump();
            self.resonance.jump();
            self.lp = Section::default();
            self.hp = Section::default();
            self.update();
        }
        self.mix = self.target;
    }

    fn update(&mut self) {
        let s = club_setting(self.knob.value, self.resonance.value);
        self.hp.set(true, s.hp_hz, s.damping, s.gain, self.t);
        self.lp.set(false, s.lp_hz, s.damping, s.gain, self.t);
        self.target = mix_of(self.knob.value);
    }

    #[inline]
    pub fn tick(&mut self, frame: &mut [f32; 2]) {
        for (m, t) in self.mix.iter_mut().zip(self.target) {
            if *m > t {
                *m = (*m - self.ramp).max(t);
            } else if *m < t {
                *m = (*m + self.ramp).min(t);
            }
        }
        let [dry, lp_w, hp_w] = self.mix;
        for (ch, x) in frame.iter_mut().enumerate() {
            let input = f64::from(*x);
            let lp = self.lp.tick(ch, input) * f64::from(lp_w);
            let hp = self.hp.tick(ch, input) * f64::from(hp_w);
            *x = *x * dry + (hp + lp) as f32;
        }
        let moved = self.knob.tick();
        if self.resonance.tick() || moved {
            self.update();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn settled(knob: f32, resonance: f32) -> ClubFilter {
        let mut f = ClubFilter::new(RATE);
        f.set_knob(knob);
        f.set_resonance(resonance);
        f.settle();
        f
    }

    fn response(knob: f32, resonance: f32, hz: f64) -> f64 {
        let mut f = settled(knob, resonance);
        let n = 48_000;
        let mut peak = 0.0f64;
        for i in 0..n {
            let x = (TAU * hz * f64::from(i) / RATE).sin() as f32;
            let mut frame = [x, x];
            f.tick(&mut frame);
            if i > n / 2 {
                peak = peak.max(f64::from(frame[0]).abs());
            }
        }
        peak
    }

    #[test]
    fn knob_maps_to_steps_with_an_open_centre() {
        assert_eq!(step_of(0.0), 0);
        assert_eq!(step_of(0.5), 128);
        assert_eq!(step_of(1.0), 255);
        assert_eq!(step_of(-0.2), 0);
        assert_eq!(mix_of(128), [1.0, 0.0, 0.0]);
        assert_eq!(mix_of(126), [1.0, 0.0, 0.0]);
        assert_eq!(mix_of(130), [1.0, 0.0, 0.0]);
        assert_eq!(mix_of(118), [0.5, 0.5, 0.0]);
        assert_eq!(mix_of(138), [0.5, 0.0, 0.5]);
        assert_eq!(mix_of(109), [0.0, 1.0, 0.0]);
        assert_eq!(mix_of(147), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn cut_offs_follow_a_cubic_law() {
        let lp = club_setting(0, 128);
        assert!((lp.lp_hz - 100.0).abs() < 1e-9);
        let lp = club_setting(63, 128);
        assert!(
            (lp.lp_hz - (100.0 + 25_000.0 / 8.0)).abs() < 0.01,
            "{}",
            lp.lp_hz
        );
        let hp = club_setting(255, 128);
        let x: f64 = 125.0 / 126.0;
        assert!((hp.hp_hz - (20.0 + 8_200.0 * x.powi(3))).abs() < 0.01);
        assert!(club_setting(20, 128).lp_hz < club_setting(60, 128).lp_hz);
        assert!(club_setting(160, 128).hp_hz < club_setting(240, 128).hp_hz);
    }

    #[test]
    fn resonance_spans_damping_four_to_a_strong_peak() {
        assert!((damping_of(0) - 4.0).abs() < 1e-6);
        assert!((damping_of(128) - 0.5).abs() < 1e-6);
        assert!((damping_of(255) - (0.5 - 127.0 * RESONANCE_SLOPE)).abs() < 1e-6);
    }

    #[test]
    fn low_high_pass_keeps_a_damping_floor() {
        let low = club_setting(140, 255);
        assert!(low.hp_hz < f64::from(HP_FLOOR_BELOW_HZ));
        assert!(low.damping > 1.0, "{}", low.damping);
        let high = club_setting(240, 255);
        assert!(high.damping < 0.1, "{}", high.damping);
    }

    #[test]
    fn centre_is_dry() {
        for hz in [60.0, 1_000.0, 12_000.0] {
            assert!((response(0.0, 0.5, hz) - 1.0).abs() < 1e-3, "{hz}");
        }
    }

    #[test]
    fn low_pass_passes_bass_and_cuts_treble() {
        assert!(response(-0.6, 0.5, 100.0) > 0.9);
        assert!(response(-0.6, 0.5, 10_000.0) < 0.1);
    }

    #[test]
    fn high_pass_cuts_bass_and_passes_treble() {
        assert!(response(0.6, 0.5, 8_000.0) > 0.8);
        assert!(response(0.6, 0.5, 60.0) < 0.05);
    }

    #[test]
    fn resonance_peaks_at_the_cut_off() {
        let f = settled(-0.5, 1.0);
        let hz = club_setting(f.knob.value, f.resonance.value).lp_hz;
        let at = response(-0.5, 1.0, hz);
        let below = response(-0.5, 1.0, hz / 8.0);
        assert!(at > 2.0 * below, "{at} vs {below}");
    }

    #[test]
    fn knob_walks_one_step_per_third_of_a_millisecond() {
        let mut f = ClubFilter::new(RATE);
        f.set_knob(-1.0);
        let mut frame = [0.0; 2];
        for _ in 0..15 {
            f.tick(&mut frame);
        }
        assert_eq!(f.knob.value, 128);
        f.tick(&mut frame);
        assert_eq!(f.knob.value, 127);
        for _ in 0..16 * 127 {
            f.tick(&mut frame);
        }
        assert_eq!(f.knob.value, 0);
        assert_eq!(f.target, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn mix_glides_in_ten_milliseconds() {
        let mut f = settled(-1.0, 0.5);
        f.set_knob(0.0);
        let mut frame = [0.0; 2];
        for _ in 0..16 * 128 {
            f.tick(&mut frame);
        }
        assert_eq!(f.knob.value, 128);
        assert!(f.mix[0] > 0.0 && f.mix[0] < 1.0);
        for _ in 0..480 {
            f.tick(&mut frame);
        }
        assert_eq!(f.mix, [1.0, 0.0, 0.0]);
    }
}
