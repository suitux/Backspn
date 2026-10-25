use std::f64::consts::TAU;

const SCALE: f64 = 255.0;
const BYPASS_FROM: f64 = 126.0;
const BYPASS_TO: f64 = 130.0;
const LP_BOTTOM_HZ: f64 = 100.0;
const LP_SPAN_HZ: f64 = 25_000.0;
const HP_BOTTOM_HZ: f64 = 20.0;
const HP_SPAN_HZ: f64 = 8_200.0;
const LP_OPEN_HZ: f64 = 25_100.0;
const HP_OPEN_HZ: f64 = 20.0;
const HP_FLOOR_BELOW_HZ: f64 = 249.9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClubSetting {
    pub bypass: bool,
    pub hp_hz: f64,
    pub lp_hz: f64,
    pub damping: f64,
    pub gain: f64,
}

fn damping_of(resonance: f32) -> f64 {
    let r = f64::from(resonance.clamp(0.0, 1.0)) * SCALE;
    if r <= 127.0 {
        let u = r / 128.0;
        0.5 + 3.5 * (1.0 - u) * (1.0 - u)
    } else {
        0.5 - (r - 128.0) * 0.003_662_109_375
    }
}

pub fn club_setting(knob: f32, resonance: f32) -> ClubSetting {
    let m = f64::midpoint(f64::from(knob.clamp(-1.0, 1.0)), 1.0) * SCALE;
    let mut damping = damping_of(resonance);
    let (hp_hz, lp_hz, floor) = if m < BYPASS_FROM {
        let x = m / 126.0;
        (HP_OPEN_HZ, LP_BOTTOM_HZ + LP_SPAN_HZ * x * x * x, 0.0)
    } else if m > BYPASS_TO {
        let x3 = ((m - 130.0) / 126.0).powi(3);
        let hp = HP_BOTTOM_HZ + HP_SPAN_HZ * x3;
        let floor = if hp < HP_FLOOR_BELOW_HZ {
            let t = x3 * 35.65;
            (4.0 * (1.0 - t).powi(3) + 0.03).min(2.0)
        } else {
            0.0
        };
        (hp, LP_OPEN_HZ, floor)
    } else {
        return ClubSetting {
            bypass: true,
            hp_hz: HP_OPEN_HZ,
            lp_hz: LP_OPEN_HZ,
            damping,
            gain: 1.0,
        };
    };
    damping = damping.max(floor);
    let distance = if m > 127.0 {
        (m - 192.0).abs()
    } else {
        (m - 64.0).abs()
    };
    let s = distance / 128.0;
    let gain = if damping < 0.5 {
        (s + 0.5) * 2.0 * damping + (0.5 - s)
    } else {
        1.0
    };
    ClubSetting {
        bypass: false,
        hp_hz,
        lp_hz,
        damping,
        gain,
    }
}

#[derive(Debug, Clone, Default)]
struct Section {
    b: [f64; 3],
    a: [f64; 2],
    z: [[f64; 2]; 2],
}

impl Section {
    fn set(&mut self, high_pass: bool, hz: f64, damping: f64, gain: f64, t: f64) {
        let w = TAU * hz * t;
        let a = 2.0 * damping * w;
        let b = w * w;
        let norm = 1.0 / (4.0 + a + b);
        self.b = if high_pass {
            let b0 = 4.0 * gain * norm;
            [b0, -2.0 * b0, b0]
        } else {
            let b0 = b * gain * norm;
            [b0, 2.0 * b0, b0]
        };
        self.a = [(2.0 * b - 8.0) * norm, (4.0 - a + b) * norm];
    }

    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let z = &mut self.z[ch];
        let y = self.b[0] * x + z[0];
        z[0] = self.b[1] * x - self.a[0] * y + z[1];
        z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClubFilter {
    hp: Section,
    lp: Section,
}

impl ClubFilter {
    pub fn set(&mut self, setting: &ClubSetting, rate: f64) {
        let t = 1.0 / rate;
        let lp_hz = setting.lp_hz.min(0.49 * rate);
        self.hp
            .set(true, setting.hp_hz, setting.damping, setting.gain, t);
        self.lp.set(false, lp_hz, setting.damping, setting.gain, t);
    }

    pub fn reset(&mut self) {
        self.hp.z = [[0.0; 2]; 2];
        self.lp.z = [[0.0; 2]; 2];
    }

    pub fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let y = self.hp.tick(ch, f64::from(x));
        self.lp.tick(ch, y) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn response(setting: &ClubSetting, hz: f64) -> f64 {
        let mut f = ClubFilter::default();
        f.set(setting, RATE);
        let n = 48_000;
        let mut peak = 0.0f64;
        for i in 0..n {
            let x = (TAU * hz * f64::from(i) / RATE).sin() as f32;
            let y = f64::from(f.tick(0, x));
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn the_centre_bypasses_and_the_ends_reach_the_documented_cutoffs() {
        assert!(club_setting(0.0, 0.5).bypass);
        let lp = club_setting(-1.0, 0.5);
        assert!(!lp.bypass && (lp.lp_hz - 100.0).abs() < 1e-9);
        let hp = club_setting(1.0, 0.5);
        let x: f64 = 125.0 / 126.0;
        assert!((hp.hp_hz - (20.0 + 8_200.0 * x.powi(3))).abs() < 1e-6);
        assert!(club_setting(-0.9, 0.5).lp_hz < club_setting(-0.5, 0.5).lp_hz);
        assert!(club_setting(0.5, 0.5).hp_hz < club_setting(0.9, 0.5).hp_hz);
    }

    #[test]
    fn resonance_knob_spans_damping_four_to_a_strong_peak() {
        assert!((damping_of(0.0) - 4.0).abs() < 1e-9);
        assert!((damping_of(128.0 / 255.0) - 0.5).abs() < 1e-6);
        assert!((damping_of(1.0) - (0.5 - 127.0 * 0.003_662_109_375)).abs() < 1e-9);
    }

    #[test]
    fn low_pass_passes_bass_and_cuts_treble() {
        let s = club_setting(-0.6, 0.5);
        assert!(s.lp_hz > 1_000.0 && s.lp_hz < 3_000.0, "{}", s.lp_hz);
        assert!(response(&s, 100.0) > 0.9);
        assert!(response(&s, 10_000.0) < 0.1);
    }

    #[test]
    fn high_pass_cuts_bass_and_keeps_a_floor_on_low_cutoffs() {
        let s = club_setting(0.8, 0.5);
        assert!(response(&s, 8_000.0) > 0.8);
        assert!(response(&s, 60.0) < 0.05);
        let low = club_setting(0.1, 1.0);
        assert!(low.hp_hz < HP_FLOOR_BELOW_HZ);
        assert!(low.damping > 1.0, "{}", low.damping);
    }

    #[test]
    fn resonance_peaks_at_the_cutoff_with_lowered_gain() {
        let s = club_setting(-0.5, 1.0);
        assert!(s.damping < 0.1 && s.gain < 1.0);
        let at_cutoff = response(&s, s.lp_hz);
        let below = response(&s, s.lp_hz / 8.0);
        assert!(at_cutoff > 2.0 * below, "{at_cutoff} vs {below}");
    }
}
