use backspn_dsp::{DelayLine, OnePole, Smoothed};

const COMB_MS: [(f64, f64); 5] = [
    (22.094, 38.667),
    (39.917, 23.229),
    (23.854, 40.865),
    (41.5, 24.802),
    (21.5, 23.885),
];
const ALLPASS_MS: [(f64, f64); 2] = [(6.979, 8.240), (22.740, 21.479)];
const ALLPASS_GAIN: f32 = 0.7;
const OUT_GAIN: f32 = 0.707_9;
const INPUT_LP_HZ: f64 = 14_000.0;
const INPUT_HP_HZ: f64 = 80.0;
const SEND_S: f64 = 0.003_3;
const COMB_S: f64 = 0.003_333;
const SIZE_STEP_S: f64 = 0.003_333;
const MAX_SCALE: f64 = 2.1;

pub(crate) fn zone(k: i32) -> (f64, f64, f32) {
    let k64 = f64::from(k);
    if k <= 115 {
        (38.0, 4000.0 - 30.172_4 * k64, 1.0)
    } else if k <= 139 {
        (10.0, 500.0, 0.0)
    } else {
        (100.0, 39.568_3 * k64 - 5000.0, 1.0)
    }
}

pub(crate) fn input_gain(s: f64) -> f64 {
    let base = 0.2 - 6.611_6e-5 * (s - 55.0).powi(2);
    if s > 90.0 {
        base + 0.016_529 * (0.1 * s - 9.0).powi(4)
    } else {
        base
    }
}

pub(crate) fn delay_scale(s: f64) -> f64 {
    0.2 + 0.171 * (0.023_06 * s + 1.01).powi(2)
}

fn damping_hz(rate: f64) -> f64 {
    match rate.round() as u32 {
        44_100 => 18_000.0,
        48_000 => 20_500.0,
        96_000 => 30_000.0,
        _ => 20_000.0,
    }
}

#[derive(Debug)]
struct Comb {
    line: DelayLine,
    damp: OnePole,
    delay: [Smoothed; 2],
    gain: [Smoothed; 2],
}

#[derive(Debug)]
pub(crate) struct ColorReverb {
    rate: f64,
    size: f64,
    size_target: f64,
    reverb_ms: f64,
    size_counter: f64,
    send: Smoothed,
    input: OnePole,
    input_hp: OnePole,
    combs: Vec<Comb>,
    allpasses: [DelayLine; 2],
}

impl ColorReverb {
    pub fn new(rate: f64) -> Self {
        let max = (0.05 * MAX_SCALE * rate) as usize + 8;
        let mut input = OnePole::default();
        input.set(INPUT_LP_HZ, rate);
        let mut input_hp = OnePole::default();
        input_hp.set(INPUT_HP_HZ, rate);
        let combs = COMB_MS
            .iter()
            .map(|_| {
                let mut damp = OnePole::default();
                damp.set(damping_hz(rate), rate);
                Comb {
                    line: DelayLine::new(max),
                    damp,
                    delay: std::array::from_fn(|_| Smoothed::new(1.0, COMB_S, rate)),
                    gain: std::array::from_fn(|_| Smoothed::new(0.0, COMB_S, rate)),
                }
            })
            .collect();
        let mut r = Self {
            rate,
            size: 10.0,
            size_target: 10.0,
            reverb_ms: 500.0,
            size_counter: 0.0,
            send: Smoothed::new(0.0, SEND_S, rate),
            input,
            input_hp,
            combs,
            allpasses: std::array::from_fn(|_| DelayLine::new((0.03 * rate) as usize + 8)),
        };
        r.retune();
        r
    }

    pub fn set_step(&mut self, k: i32) {
        let (size, ms, send) = zone(k);
        self.size_target = size;
        self.reverb_ms = ms;
        self.send.set(send);
        self.retune();
    }

    fn retune(&mut self) {
        let scale = delay_scale(self.size);
        for (comb, (l, r)) in self.combs.iter_mut().zip(COMB_MS) {
            for (ch, ms) in [l, r].into_iter().enumerate() {
                let d = ms * scale;
                comb.delay[ch].set((d / 1000.0 * self.rate) as f32);
                comb.gain[ch].set((-3.0 * d / self.reverb_ms.max(1.0)).exp() as f32);
            }
        }
    }

    pub fn clear(&mut self) {
        for c in &mut self.combs {
            c.line.clear();
        }
        for a in &mut self.allpasses {
            a.clear();
        }
    }

    pub fn frame(&mut self, x: [f32; 2]) -> [f32; 2] {
        self.size_counter += 1.0;
        if self.size_counter >= SIZE_STEP_S * self.rate
            && (self.size - self.size_target).abs() > f64::EPSILON
        {
            self.size_counter = 0.0;
            self.size += (self.size_target - self.size).clamp(-1.0, 1.0);
            self.retune();
        }
        let send = self.send.tick();
        let w = input_gain(self.size) as f32;
        let mut sum = [0.0f32; 2];
        let mut fed = [0.0f32; 2];
        for ch in 0..2 {
            let (low, _) = self.input.tick(ch, x[ch] * send);
            let (_, high) = self.input_hp.tick(ch, low);
            fed[ch] = high * w;
        }
        for comb in &mut self.combs {
            let mut write = [0.0f32; 2];
            for ch in 0..2 {
                let d = comb.delay[ch].tick().max(1.0);
                let g = comb.gain[ch].tick();
                let back = comb.line.read(d)[ch];
                sum[ch] += back;
                let (damped, _) = comb.damp.tick(ch, back * g);
                write[ch] = fed[ch] + damped;
            }
            comb.line.push(write);
        }
        let mut wet = sum;
        for (line, (l, r)) in self.allpasses.iter_mut().zip(ALLPASS_MS) {
            let mut write = [0.0f32; 2];
            for ch in 0..2 {
                let ms = if ch == 0 { l } else { r };
                let delayed = line.read((ms / 1000.0 * self.rate) as f32)[ch];
                let v = wet[ch] + ALLPASS_GAIN * delayed;
                write[ch] = v;
                wet[ch] = delayed - ALLPASS_GAIN * v;
            }
            line.push(write);
        }
        [x[0] + wet[0] * OUT_GAIN, x[1] + wet[1] * OUT_GAIN]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_and_curves_are_right() {
        assert_eq!(zone(127), (10.0, 500.0, 0.0));
        let (s, ms, send) = zone(0);
        assert!((s - 38.0).abs() < 1e-9 && (ms - 4000.0).abs() < 1e-9 && (send - 1.0).abs() < 1e-9);
        let (s, ms, _) = zone(255);
        assert!((s - 100.0).abs() < 1e-9 && (ms - 5090.0).abs() < 1.0);
        assert!((input_gain(10.0) - 0.066).abs() < 1e-3);
        assert!((input_gain(38.0) - 0.181).abs() < 1e-3);
        assert!((input_gain(100.0) - 0.083).abs() < 1e-3);
        assert!((delay_scale(10.0) - 0.463).abs() < 1e-3);
        assert!((delay_scale(100.0) - 2.08).abs() < 1e-2);
    }

    #[test]
    fn the_centre_sends_nothing_and_an_end_rings() {
        let rate = 48_000.0;
        let mut r = ColorReverb::new(rate);
        let impulse = |r: &mut ColorReverb| {
            let mut tail = 0.0f32;
            for i in 0..24_000 {
                let x = if i == 0 { 1.0 } else { 0.0 };
                let y = r.frame([x, x]);
                if i > 2_000 {
                    tail = tail.max(y[0].abs());
                }
            }
            tail
        };
        assert!(impulse(&mut r) < 1e-6, "centre: dry only");
        r.set_step(255);
        for _ in 0..20_000 {
            r.frame([0.0; 2]);
        }
        assert!(impulse(&mut r) > 1e-3, "right end: a long tail");
    }
}
