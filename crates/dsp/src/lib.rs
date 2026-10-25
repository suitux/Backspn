use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z: [[f32; 2]; 2],
}

impl Biquad {
    pub fn lowpass(freq: f64, rate: f64) -> Self {
        Self::rbj(freq, rate, false)
    }

    pub fn highpass(freq: f64, rate: f64) -> Self {
        Self::rbj(freq, rate, true)
    }

    fn rbj(freq: f64, rate: f64, high: bool) -> Self {
        let w0 = 2.0 * PI * freq.min(0.49 * rate) / rate;
        let alpha = w0.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high {
            (f64::midpoint(1.0, cos), -(1.0 + cos))
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos)
        };
        Self {
            b0: (b0 / a0) as f32,
            b1: (b1 / a0) as f32,
            b2: (b0 / a0) as f32,
            a1: (-2.0 * cos / a0) as f32,
            a2: ((1.0 - alpha) / a0) as f32,
            z: [[0.0; 2]; 2],
        }
    }

    #[inline]
    pub fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let z = &mut self.z[ch];
        let y = self.b0 * x + z[0];
        z[0] = self.b1 * x - self.a1 * y + z[1];
        z[1] = self.b2 * x - self.a2 * y;
        y
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Lr4 {
    a: Biquad,
    b: Biquad,
}

impl Lr4 {
    pub fn lowpass(freq: f64, rate: f64) -> Self {
        let s = Biquad::lowpass(freq, rate);
        Self { a: s, b: s }
    }

    pub fn highpass(freq: f64, rate: f64) -> Self {
        let s = Biquad::highpass(freq, rate);
        Self { a: s, b: s }
    }

    #[inline]
    pub fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let y = self.a.tick(ch, x);
        self.b.tick(ch, y)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Isolator {
    low_lp: Lr4,
    low_hp: Lr4,
    high_lp: Lr4,
    high_hp: Lr4,
    low_ap_lp: Lr4,
    low_ap_hp: Lr4,
}

impl Isolator {
    pub fn new(low_hz: f64, high_hz: f64, rate: f64) -> Self {
        Self {
            low_lp: Lr4::lowpass(low_hz, rate),
            low_hp: Lr4::highpass(low_hz, rate),
            high_lp: Lr4::lowpass(high_hz, rate),
            high_hp: Lr4::highpass(high_hz, rate),
            low_ap_lp: Lr4::lowpass(high_hz, rate),
            low_ap_hp: Lr4::highpass(high_hz, rate),
        }
    }

    #[inline]
    pub fn split(&mut self, ch: usize, x: f32) -> [f32; 3] {
        let low = self.low_lp.tick(ch, x);
        let rest = self.low_hp.tick(ch, x);
        let low = self.low_ap_lp.tick(ch, low) + self.low_ap_hp.tick(ch, low);
        let mid = self.high_lp.tick(ch, rest);
        let high = self.high_hp.tick(ch, rest);
        [low, mid, high]
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Svf {
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
    ic1: [f32; 2],
    ic2: [f32; 2],
}

impl Svf {
    pub fn set(&mut self, freq: f64, q: f64, rate: f64) {
        let g = (PI * freq.min(0.49 * rate) / rate).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        self.a1 = a1 as f32;
        self.a2 = (g * a1) as f32;
        self.a3 = (g * g * a1) as f32;
        self.k = k as f32;
    }

    pub fn reset(&mut self) {
        self.ic1 = [0.0; 2];
        self.ic2 = [0.0; 2];
    }

    #[inline]
    pub fn tick(&mut self, ch: usize, x: f32) -> (f32, f32) {
        let v3 = x - self.ic2[ch];
        let v1 = self.a1 * self.ic1[ch] + self.a2 * v3;
        let v2 = self.ic2[ch] + self.a2 * self.ic1[ch] + self.a3 * v3;
        self.ic1[ch] = 2.0 * v1 - self.ic1[ch];
        self.ic2[ch] = 2.0 * v2 - self.ic2[ch];
        (v2, x - self.k * v1 - v2)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Smoothed {
    value: f32,
    target: f32,
    coef: f32,
}

impl Smoothed {
    pub fn new(value: f32, time_s: f64, rate: f64) -> Self {
        Self {
            value,
            target: value,
            coef: (1.0 - (-1.0 / (time_s * rate)).exp()) as f32,
        }
    }

    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        self.value += (self.target - self.value) * self.coef;
        if (self.value - self.target).abs() < 1e-6 {
            self.value = self.target;
        }
        self.value
    }

    pub fn value(&self) -> f32 {
        self.value
    }
}

#[derive(Debug, Clone)]
pub struct Limiter {
    delay: Box<[[f32; 2]]>,
    pos: usize,
    gain: f32,
    attack: f32,
    release: f32,
    threshold: f32,
    pub min_gain: f32,
}

impl Limiter {
    pub fn new(rate: f64) -> Self {
        let lookahead = ((0.0015 * rate).round() as usize).max(1);
        Self {
            delay: vec![[0.0; 2]; lookahead].into_boxed_slice(),
            pos: 0,
            gain: 1.0,
            attack: (1.0 - (-3.0 / lookahead as f64).exp()) as f32,
            release: (1.0 - (-1.0 / (0.1 * rate)).exp()) as f32,
            threshold: 10f32.powf(-0.3 / 20.0),
            min_gain: 1.0,
        }
    }

    #[inline]
    pub fn tick(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let peak = frame[0].abs().max(frame[1].abs());
        let target = if peak > self.threshold {
            self.threshold / peak
        } else {
            1.0
        };
        let coef = if target < self.gain {
            self.attack
        } else {
            self.release
        };
        self.gain += (target - self.gain) * coef;
        self.min_gain = self.min_gain.min(self.gain);
        let delayed = std::mem::replace(&mut self.delay[self.pos], frame);
        self.pos = (self.pos + 1) % self.delay.len();
        delayed.map(|s| (s * self.gain).clamp(-1.0, 1.0))
    }
}

#[derive(Debug, Clone)]
pub struct DelayLine {
    buffer: Box<[[f32; 2]]>,
    mask: usize,
    write: usize,
}

impl DelayLine {
    pub fn new(max_samples: usize) -> Self {
        let len = (max_samples + 4).next_power_of_two();
        Self {
            buffer: vec![[0.0; 2]; len].into_boxed_slice(),
            mask: len - 1,
            write: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.buffer.len() - 4
    }

    pub fn clear(&mut self) {
        self.buffer.fill([0.0; 2]);
    }

    #[inline]
    pub fn push(&mut self, frame: [f32; 2]) {
        self.buffer[self.write] = frame;
        self.write = (self.write + 1) & self.mask;
    }

    #[inline]
    pub fn read(&self, delay: f32) -> [f32; 2] {
        let delay = delay.clamp(1.0, self.capacity() as f32);
        let whole = delay.floor();
        let frac = delay - whole;
        let i = (self.write + self.buffer.len() - whole as usize) & self.mask;
        let j = (i + self.buffer.len() - 1) & self.mask;
        let (a, b) = (self.buffer[i], self.buffer[j]);
        [a[0] + (b[0] - a[0]) * frac, a[1] + (b[1] - a[1]) * frac]
    }

    #[inline]
    pub fn at(&self, index: usize) -> [f32; 2] {
        self.buffer[index & self.mask]
    }

    pub fn write_index(&self) -> usize {
        self.write
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OnePole {
    coef: f32,
    state: [f32; 2],
}

impl OnePole {
    pub fn set(&mut self, freq: f64, rate: f64) {
        self.coef = (1.0 - (-2.0 * PI * freq.min(0.49 * rate) / rate).exp()) as f32;
    }

    #[inline]
    pub fn tick(&mut self, ch: usize, x: f32) -> (f32, f32) {
        self.state[ch] += (x - self.state[ch]) * self.coef;
        (self.state[ch], x - self.state[ch])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn sine(freq: f64, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / RATE).sin() as f32)
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn isolator_bands_sum_flat() {
        for freq in [50.0, 250.0, 1_000.0, 2_500.0, 10_000.0] {
            let mut iso = Isolator::new(250.0, 2_500.0, RATE);
            let x = sine(freq, 48_000);
            let y: Vec<f32> = x.iter().map(|&s| iso.split(0, s).iter().sum()).collect();
            let ratio = rms(&y[24_000..]) / rms(&x[24_000..]);
            assert!((ratio - 1.0).abs() < 0.01, "{freq} Hz: {ratio}");
        }
    }

    #[test]
    fn isolator_separates_bands() {
        let mut iso = Isolator::new(250.0, 2_500.0, RATE);
        let x = sine(60.0, 48_000);
        let bands: Vec<[f32; 3]> = x.iter().map(|&s| iso.split(0, s)).collect();
        let band = |i: usize| rms(&bands[24_000..].iter().map(|b| b[i]).collect::<Vec<_>>());
        assert!(band(0) > 0.65 && band(1) < 0.05 && band(2) < 0.001);
    }

    #[test]
    fn svf_low_and_high_pass() {
        let mut f = Svf::default();
        f.set(1_000.0, std::f64::consts::FRAC_1_SQRT_2, RATE);
        let low: Vec<(f32, f32)> = sine(100.0, 48_000).iter().map(|&s| f.tick(0, s)).collect();
        assert!(rms(&low[24_000..].iter().map(|v| v.0).collect::<Vec<_>>()) > 0.69);
        assert!(rms(&low[24_000..].iter().map(|v| v.1).collect::<Vec<_>>()) < 0.02);
    }

    #[test]
    fn limiter_holds_ceiling() {
        let mut l = Limiter::new(RATE);
        let out: Vec<[f32; 2]> = sine(100.0, 48_000)
            .iter()
            .map(|&s| l.tick([s * 3.0, s * 3.0]))
            .collect();
        let peak = out[1_000..].iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        assert!(peak <= 1.0 && peak > 0.9, "{peak}");
        assert!(l.min_gain < 0.4);
    }

    #[test]
    fn delay_line_reads_back() {
        let mut d = DelayLine::new(100);
        for i in 0..50 {
            d.push([i as f32, -(i as f32)]);
        }
        assert_eq!(d.read(1.0), [49.0, -49.0]);
        assert_eq!(d.read(10.0), [40.0, -40.0]);
        assert_eq!(d.read(10.5), [39.5, -39.5]);
        assert!(d.capacity() >= 100);
    }

    #[test]
    fn smoother_converges() {
        let mut s = Smoothed::new(0.0, 0.01, RATE);
        s.set(1.0);
        for _ in 0..4_800 {
            s.tick();
        }
        assert!((s.value() - 1.0).abs() < 1e-3);
    }
}
