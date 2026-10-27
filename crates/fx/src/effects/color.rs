use std::f64::consts::PI;

use backspn_dsp::{DelayLine, Isolator, OnePole, Smoothed, Svf};

use super::color_reverb::ColorReverb;
use super::echo::Shifter;

const STEP_S: f64 = 0.000_333_3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Jet,
    Crush,
    Noise,
    Pitch,
    Space,
    DubEcho,
    Echo,
    Sweep,
    GateComp,
    KeyTune,
    Reverb,
}

fn step_of(knob: f32) -> i32 {
    (knob.clamp(0.0, 1.0) * 255.0) as i32
}

#[derive(Debug, Clone, Copy, Default)]
struct Rbj {
    b: [f32; 3],
    a: [f32; 2],
    z: [[f32; 2]; 2],
}

impl Rbj {
    fn set(&mut self, hz: f64, quality: f64, rate: f64, high: bool, band: bool) {
        let omega = 2.0 * PI * hz.clamp(5.0, 0.45 * rate) / rate;
        let (sin, cos) = omega.sin_cos();
        let alpha = sin / (2.0 * quality.max(0.05));
        let a0 = 1.0 + alpha;
        let b = if band {
            [alpha, 0.0, -alpha]
        } else if high {
            let half = f64::midpoint(1.0, cos);
            [half, -(1.0 + cos), half]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        self.b = b.map(|v| (v / a0) as f32);
        self.a = [(-2.0 * cos / a0) as f32, ((1.0 - alpha) / a0) as f32];
    }

    fn tick(&mut self, ch: usize, x: f32) -> f32 {
        let z = &mut self.z[ch];
        let y = self.b[0] * x + z[0];
        z[0] = self.b[1] * x - self.a[0] * y + z[1];
        z[1] = self.b[2] * x - self.a[1] * y;
        y
    }

    fn clear(&mut self) {
        self.z = [[0.0; 2]; 2];
    }
}

fn fade_dry(v: i32, gate_comp: bool) -> f32 {
    let x = if v <= 123 {
        1.0 - f64::from(v) / 246.0
    } else if v <= 131 {
        0.5
    } else {
        0.5 + f64::from(v - 132) / 246.0
    };
    let g = if gate_comp {
        let y = 2.0 * x - 1.0;
        if y < 0.25 { 1.0 - 4.0 * y } else { 0.0 }
    } else if 2.0 * x - 1.0 < 0.25 {
        5.0 - 8.0 * x
    } else {
        0.0
    };
    (g * g).min(1.0) as f32
}

fn noise_level(w: i32) -> f64 {
    let w = f64::from(w);
    let line = 1.391_3 - 0.011_22 * w;
    match w {
        w if w <= 40.0 => line,
        w if w <= 123.0 => line * (1.0 - 0.001_4 * (w - 40.0)).powi(8),
        w if w <= 131.0 => 0.0,
        w if w <= 156.0 => 0.8 - 0.001_28 * (157.0 - w).powi(2),
        w if w <= 181.0 => 0.8 - (0.8 - 0.547) * (w - 157.0) / 24.0,
        w => (1.0 - 0.004 * (w - 180.0)).max(0.0).powi(8) * 0.011_22 * (w - 131.0),
    }
}

fn pitch_speed(v: i32, range: f64) -> f64 {
    let v = f64::from(v);
    let s = if v <= 124.0 {
        (v / 248.0 - 0.5) * (2.0 - 2.0 * 2f64.powf(-range / 12.0))
    } else if v >= 131.0 {
        (2f64.powf(range / 12.0) - 1.0) * (v - 131.0) / 124.0
    } else {
        0.0
    };
    s.clamp(-2.0, 3.0)
}

fn sweep_hz(p: i32) -> f64 {
    let p = f64::from(p);
    if p < 128.0 {
        60.0 + 7.401_6 * p
    } else {
        1_000.0 + 149.606 * (p - 128.0)
    }
}

fn pitch_range(p: i32) -> f64 {
    let p = f64::from(p);
    if p <= 126.0 {
        2.0 + p * 10.0 / 127.0
    } else if p <= 128.0 {
        12.0
    } else {
        12.0 + (p - 128.0) * 12.0 / 127.0
    }
}

fn jet_scale(p: i32) -> f64 {
    let v = f64::from(p);
    match p {
        p if p <= 123 => v / 124.0,
        p if p <= 131 => 1.0,
        _ => 0.647_85 + 0.002_688 * v,
    }
}

fn noise_param(p: i32) -> f64 {
    let u = f64::from(p) / 128.0;
    if p < 128 {
        0.15 * u * u
    } else {
        0.15 * (u.powi(3) + 4.0 * (u - 1.0).powi(3))
    }
}

fn space_feedback(fb: f64, p: i32, step: f64) -> f64 {
    match p {
        p if p < 116 => fb * f64::from(p) / 116.0,
        p if p <= 139 => fb,
        p => (fb + f64::from(p - 139) * step).min(0.99),
    }
}

fn dub_feedback(p: i32) -> (f64, f64, f64) {
    let v = f64::from(p);
    match p {
        p if p <= 115 => (0.007_5 * v, 0.007_586 * v, 1.0),
        p if p <= 139 => (0.87, 0.88, 1.0),
        _ => {
            let t = (v - 139.0) / 116.0;
            (0.87 + 0.12 * t, 0.88 + 0.11 * t, (255.0 - v) / 116.0)
        }
    }
}

fn key_of(v: i32) -> u8 {
    let k = match v {
        v if v < 3 => 6,
        v if v < 253 => {
            let zone = (v - 3) / 25;
            [7, 8, 9, 10, 11, 0, 1, 2, 3, 4][zone.min(9) as usize]
        }
        _ => 5,
    };
    k as u8
}

fn scale_periods(key: u8, rate: f64) -> Vec<f64> {
    const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
    (55..=71)
        .filter(|n: &u8| MAJOR.contains(&((n + 12 - key) % 12)))
        .map(|n| rate / (440.0 * 2f64.powf((f64::from(n) - 69.0) / 12.0)))
        .collect()
}

#[derive(Debug)]
pub struct ColorFx {
    kind: Kind,
    rate: f64,
    value: i32,
    target: i32,
    param: i32,
    param_target: i32,
    counter: f64,
    step_frames: f64,
    line: DelayLine,
    line_b: DelayLine,
    time: Smoothed,
    time_b: Smoothed,
    filters: [Rbj; 4],
    shifter: Shifter,
    bands: Isolator,
    noise: u32,
    damp: [OnePole; 2],
    allpasses: [DelayLine; 4],
    held: [f32; 2],
    hold_left: f64,
    env: [f32; 4],
    gate: [f32; 4],
    detector: Detector,
    mix: (f32, f32),
    reverb: Box<ColorReverb>,
}

#[derive(Debug, Clone)]
struct Detector {
    band: [Svf; 2],
    last: f32,
    since: f64,
    periods: [f64; 6],
    count: usize,
    ratio: f64,
    until: f64,
}

impl ColorFx {
    pub fn new(kind: Kind, rate: f64) -> Self {
        let step = match kind {
            Kind::Space | Kind::Sweep | Kind::GateComp | Kind::Reverb => 2.0 * STEP_S,
            _ => STEP_S,
        };
        let mut band = [Svf::default(); 2];
        band[0].set(430.0, 0.707, rate);
        band[1].set(241.0, 0.707, rate);
        let mut damp = OnePole::default();
        damp.set(19_000.0, rate);
        let mut fx = Self {
            kind,
            rate,
            value: 127,
            target: 127,
            param: 127,
            param_target: 127,
            counter: 0.0,
            step_frames: step * rate,
            line: DelayLine::new((0.32 * rate) as usize + 8),
            line_b: DelayLine::new((0.32 * rate) as usize + 8),
            time: Smoothed::new((0.055 * rate) as f32, 0.05, rate),
            time_b: Smoothed::new((0.055 * rate) as f32, 0.05, rate),
            filters: [Rbj::default(); 4],
            shifter: Shifter::new(rate),
            bands: Isolator::new(284.0, 4_649.0, rate),
            noise: 0x2468_ace1,
            damp: [damp; 2],
            allpasses: std::array::from_fn(|_| DelayLine::new((0.08 * rate) as usize)),
            held: [0.0; 2],
            hold_left: 0.0,
            env: [0.0; 4],
            gate: [1.0; 4],
            detector: Detector {
                band,
                last: 0.0,
                since: 0.0,
                periods: [0.0; 6],
                count: 0,
                ratio: 1.0,
                until: 0.0,
            },
            mix: (1.0, 0.0),
            reverb: Box::new(ColorReverb::new(rate)),
        };
        fx.update();
        fx
    }

    pub fn set_knob(&mut self, knob: f32) {
        self.target = step_of(0.5 + 0.5 * knob);
    }

    pub fn set_parameter(&mut self, value: f32) {
        self.param_target = step_of(value);
    }

    pub fn reset(&mut self) {
        self.line.clear();
        self.line_b.clear();
        self.shifter.clear();
        self.reverb.clear();
        for f in &mut self.filters {
            f.clear();
        }
        for a in &mut self.allpasses {
            a.clear();
        }
        for d in &mut self.damp {
            d.clear();
        }
        self.held = [0.0; 2];
        self.hold_left = 0.0;
        self.env = [0.0; 4];
        self.gate = [1.0; 4];
    }

    /// Jumps to the knob and parameter positions instead of stepping there,
    /// for an effect that starts again after resting.
    pub fn settle(&mut self) {
        self.value = self.target;
        self.param = self.param_target;
        self.counter = 0.0;
        self.update();
    }

    fn update(&mut self) {
        let v = self.value;
        match self.kind {
            Kind::Sweep => self.update_sweep(v),
            Kind::Crush => {
                let d = 2.0 * (f64::from(v) / 255.0 - 0.5).abs();
                let idx = d * 255.0;
                if v > 131 && idx >= 4.0 {
                    let hz = 20.0 * (4_130.0f64 / 20.0).powf(idx / 255.0);
                    let res = 1.3 * (1.0 - (1.0 - (idx - 3.0) / 252.0).powi(16));
                    self.filters[0].set(hz, 0.707 + res, self.rate, true, false);
                }
            }
            Kind::Noise => {
                let side = if v <= 127 { v } else { v - 128 };
                let hz = 100.0 * 2f64.powf(0.056_919_2 * f64::from(side)) * 2.0 / 1.5;
                self.filters[0].set(hz, 2.0, self.rate, false, true);
            }
            Kind::Space => {
                let (lp, hp) = if v <= 123 {
                    (1_000.0, 200.0)
                } else {
                    (7_000.0, 3_000.0)
                };
                self.filters[0].set(lp, 0.707, self.rate, false, false);
                self.filters[1].set(hp, 0.707, self.rate, true, false);
            }
            Kind::Reverb => self.reverb.set_step(v),
            Kind::DubEcho => {
                self.filters[0].set(1_375.0, 0.707, self.rate, false, true);
                self.filters[1].set(2_500.0, 0.707, self.rate, true, false);
                self.filters[2].set(12_000.0, 0.707, self.rate, false, false);
            }
            _ => {}
        }
    }

    fn update_sweep(&mut self, w: i32) {
        let f = sweep_hz(self.param);
        let rate = self.rate;
        if w <= 123 {
            let t = f64::from(w) / 123.0;
            self.filters[0].set(20_000.0 * (f / 20_000.0).powf(t), 1.0, rate, true, false);
            self.filters[1].set(60.0 * (f / 60.0).powf(t), 1.0, rate, false, false);
        } else if w >= 131 {
            let r = 1.4 + 0.001_007_8 * f64::from(256 - w).powi(2);
            let q = 0.707_95
                + if w <= 161 {
                    0.009_375 * f64::from(162 - w)
                } else {
                    0.0
                };
            self.filters[0].set(f / r, q, rate, true, false);
            self.filters[1].set(f * r, q, rate, false, false);
        } else {
            self.filters[0].set(0.057_5 * f, 0.707, rate, true, false);
            self.filters[1].set(17.4 * f, 0.707, rate, false, false);
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            self.counter += 1.0;
            let moving = self.value != self.target || self.param != self.param_target;
            if self.counter >= self.step_frames && moving {
                self.counter = 0.0;
                let before = self.value;
                self.value += (self.target - self.value).signum();
                self.param += (self.param_target - self.param).signum();
                if self.kind == Kind::Sweep && (before <= 127) != (self.value <= 127) {
                    for f in &mut self.filters {
                        f.clear();
                    }
                }
                self.update();
            }
            *frame = self.frame(*frame);
        }
    }

    fn frame(&mut self, x: [f32; 2]) -> [f32; 2] {
        let v = self.value;
        match self.kind {
            Kind::Jet => self.jet(x, v),
            Kind::Crush => self.crush(x, v),
            Kind::Noise => self.noise(x, v),
            Kind::Pitch => {
                let speed = pitch_speed(v, pitch_range(self.param));
                if (124..=131).contains(&v) {
                    let _ = self.shifter.tick(x, 1.0);
                    x
                } else {
                    self.shifter.tick(x, 1.0 + speed)
                }
            }
            Kind::Space => self.space(x, v),
            Kind::DubEcho => self.dub_echo(x, v),
            Kind::Echo => self.echo(x, v),
            Kind::Sweep => {
                let dry = fade_dry(v, false);
                let mut out = [0.0; 2];
                for ch in 0..2 {
                    let wet = if v <= 123 {
                        self.filters[0].tick(ch, x[ch]) + self.filters[1].tick(ch, x[ch])
                    } else {
                        let h = self.filters[0].tick(ch, x[ch]);
                        self.filters[1].tick(ch, h)
                    };
                    out[ch] = x[ch] * dry + wet * (1.0 - dry);
                }
                out
            }
            Kind::GateComp => self.gate_comp(x, v),
            Kind::KeyTune => self.key_tune(x, v),
            Kind::Reverb => self.reverb.frame(x),
        }
    }

    fn jet(&mut self, x: [f32; 2], c: i32) -> [f32; 2] {
        let (ms, fb, n) = if c >= 132 {
            let n = f64::from(c - 131);
            let ms = if c > 140 {
                (2.862_3 - 0.021_739 * n).max(0.166_7)
            } else {
                2.667
            };
            let fb = if c <= 193 {
                0.6
            } else {
                (1.344 - 0.012 * n).max(0.3)
            };
            (ms, fb, n)
        } else if c <= 123 {
            let n = f64::from(124 - c);
            let fb = if c >= 24 {
                0.55
            } else {
                0.3 + 0.25 * (f64::from(c) - 7.0).max(0.0) / 17.0
            };
            ((2.667 + 0.139_78 * n).min(20.0), fb, n)
        } else {
            (2.667, 0.6, 0.0)
        };
        self.mix = if n == 0.0 {
            (1.0, 0.0)
        } else {
            (
                (1.0 - 0.015 * n).max(0.7) as f32,
                (0.035 * n).min(0.7) as f32,
            )
        };
        let fb = fb * jet_scale(self.param);
        self.time.set((ms / 1000.0 * self.rate) as f32);
        let t = self.time.tick();
        let tap = self.line.read(t);
        self.line
            .push([x[0] + tap[0] * fb as f32, x[1] + tap[1] * fb as f32]);
        let (dry, wet) = self.mix;
        [x[0] * dry + tap[0] * wet, x[1] * dry + tap[1] * wet]
    }

    fn crush(&mut self, x: [f32; 2], c: i32) -> [f32; 2] {
        let param = f64::from(self.param);
        let scale = if param < 128.0 {
            0.104_17 + 0.003_092_4 * param
        } else {
            0.5 + (param - 128.0) / 512.0
        };
        let d = 2.0 * (f64::from(c) / 255.0 - 0.5).abs();
        let hold = (scale * d).min(0.5) / 1000.0 * self.rate;
        self.hold_left -= 1.0;
        if self.hold_left < 1.0 {
            self.held = x;
            self.hold_left += hold.max(1.0);
        }
        let mut crushed = if hold < 1.0 { x } else { self.held };
        if scale * d > 0.104_17 && self.param > 144 {
            let shift = ((8 * self.param - 1024) >> 7).clamp(0, 15) as u32;
            let mask = 0xFFFF_0000u32 << shift;
            crushed = crushed.map(|v| f32::from_bits(v.to_bits() & mask));
        }
        let idx = d * 255.0;
        if c <= 131 || idx < 4.0 {
            return crushed;
        }
        let dry = if idx < 130.0 {
            (1.0 - idx / 126.0).max(0.0).powi(2) as f32
        } else {
            0.0
        };
        let mut out = [0.0; 2];
        for ch in 0..2 {
            let hp = self.filters[0].tick(ch, crushed[ch]);
            out[ch] = crushed[ch] * dry + hp * (1.0 - dry);
        }
        out
    }

    fn noise(&mut self, x: [f32; 2], w: i32) -> [f32; 2] {
        let param = noise_param(self.param);
        let gain = (noise_level(w) * param) as f32;
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        let white = (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let n0 = self.filters[0].tick(0, white);
        [x[0] + n0 * gain, x[1] + n0 * gain]
    }

    fn space(&mut self, x: [f32; 2], v: i32) -> [f32; 2] {
        let left = v <= 123;
        let send = if left {
            if v <= 93 {
                1.0
            } else {
                1.0 - (1.0 - 1.0 / 31.0) * f64::from(v - 93) / 30.0
            }
        } else if v <= 131 {
            0.0
        } else {
            (f64::from(v - 131) / 31.0).min(1.0)
        };
        let mono_gain = if left { 0.5 } else { 0.7 };
        let w = if left {
            (f64::from(v) / 124.0).powi(2)
        } else {
            (f64::from(255 - v) / 124.0).powi(2)
        };
        let (fb_l, fb_r) = if left {
            (0.255 + 0.663_1 * (1.0 - w), 0.113_66 + 0.759_3 * (1.0 - w))
        } else {
            (0.255 + 0.678_9 * (1.0 - w), 0.113_66 + 0.783_3 * (1.0 - w))
        };
        let (fb_l, fb_r) = (
            space_feedback(fb_l, self.param, 0.000_517),
            space_feedback(fb_r, self.param, 0.000_862),
        );
        let mono = (x[0] + x[1]) * mono_gain as f32 * send as f32;
        let low = self.filters[0].tick(0, mono);
        let filtered = self.filters[1].tick(0, low);
        let scale = if left { 1.0 } else { 0.898 };
        let mut d = filtered;
        for (line, ms) in self.allpasses.iter_mut().zip([27.73, 42.54, 73.61, 38.07]) {
            let delayed = line.read((ms * scale / 1000.0 * self.rate) as f32)[0];
            let fed = d + 0.8 * delayed;
            line.push([fed, 0.0]);
            d = delayed - 0.8 * fed;
        }
        let combs = [0.197_79, 0.314_79].map(|s| (s * self.rate) as f32);
        let back = [self.line.read(combs[0])[0], self.line.read(combs[1])[1]];
        let (damped_l, _) = self.damp[0].tick(0, back[0]);
        let (damped_r, _) = self.damp[1].tick(1, back[1]);
        self.line
            .push([d + damped_l * fb_l as f32, d + damped_r * fb_r as f32]);
        [x[0] + 2.0 * back[0], x[1] + 2.0 * back[1]]
    }

    fn dub_echo(&mut self, x: [f32; 2], c: i32) -> [f32; 2] {
        let (base_l, base_r, knob_share) = dub_feedback(self.param);
        let (send_l, ms_l, send_r, ms_r) = if c <= 115 {
            let d = f64::from(116 - c) / 116.0;
            let ms = if d < 0.2 {
                55.0
            } else {
                55.0 + (d - 0.2) * 167.5
            };
            ((d / 0.3).min(1.0), ms, 0.0, 55.0)
        } else if c >= 139 {
            let n = f64::from(c - 139);
            let d = n / 116.0;
            let ms = if d < 0.2 { 55.0 } else { 31.25 + 1.023_7 * n };
            (0.0, 55.0, (d / 0.3).min(1.0), ms)
        } else {
            (0.0, 55.0, 0.0, 55.0)
        };
        let extra = |d: f64| {
            if d >= 0.2 {
                (d - 0.2) * 0.1 * knob_share
            } else {
                0.0
            }
        };
        let fb_l = (base_l
            + extra(if c <= 115 {
                f64::from(116 - c) / 116.0
            } else {
                0.0
            }))
        .min(0.993) as f32;
        let fb_r = (base_r
            + extra(if c >= 139 {
                f64::from(c - 139) / 116.0
            } else {
                0.0
            }))
        .min(0.993) as f32;
        self.time.set((ms_l / 1000.0 * self.rate) as f32);
        self.time_b.set((ms_r / 1000.0 * self.rate) as f32);
        let (ta, tb) = (self.time.tick(), self.time_b.tick());
        let (a, b) = (self.line.read(ta), self.line_b.read(tb));
        let mut out = x;
        let mut push_a = [0.0; 2];
        let mut push_b = [0.0; 2];
        for ch in 0..2 {
            let in_a = self.filters[0].tick(ch, x[ch]) * send_l as f32;
            let in_b = self.filters[1].tick(ch, x[ch]) * send_r as f32;
            push_a[ch] = in_a + a[ch] * fb_l;
            push_b[ch] = in_b + b[ch] * fb_r;
            let b_out = self.filters[2].tick(ch, b[ch]);
            out[ch] = x[ch] + 0.7 * a[ch] + b_out;
        }
        self.line.push(push_a);
        self.line_b.push(push_b);
        out
    }

    fn echo(&mut self, x: [f32; 2], c: i32) -> [f32; 2] {
        let (send_a, ms_a, send_b, ms_b) = if c <= 123 {
            let n = f64::from(123 - c);
            let ms = (62.5 + 1.524_4 * n).max(100.0);
            (0.7 * (n / 123.0 / 0.3).min(1.0), ms, 0.0, 125.0)
        } else if c >= 132 {
            let n = f64::from(c - 132);
            let ms = (153.125 - 1.143_3 * n).clamp(12.5, 125.0);
            (0.0, 100.0, 0.7 * (n / 123.0 / 0.3).min(1.0), ms)
        } else {
            (0.0, 100.0, 0.0, 125.0)
        };
        self.time.set((ms_a / 1000.0 * self.rate) as f32);
        self.time_b.set((ms_b / 1000.0 * self.rate) as f32);
        let (ta, tb) = (self.time.tick(), self.time_b.tick());
        let (a, b) = (self.line.read(ta), self.line_b.read(tb));
        self.line.push([
            x[0] * send_a as f32 + a[0] * 0.7,
            x[1] * send_a as f32 + a[1] * 0.7,
        ]);
        self.line_b.push([
            x[0] * send_b as f32 + b[0] * 0.7,
            x[1] * send_b as f32 + b[1] * 0.7,
        ]);
        [x[0] + 0.7 * (a[0] + b[0]), x[1] + 0.7 * (a[1] + b[1])]
    }

    fn gate_settings(knob: i32, param: i32, band: usize) -> (f64, f64, f64) {
        let p = f64::from(param);
        let depth = (1.0 - f64::from(knob) / 107.0).max(0.0);
        let amount = (0.28 + p * 1.44 / 255.0) * (1.0 - (1.0 - depth).powi(2));
        let scale = match (p <= 127.0, band) {
            (true, _) => 0.28 + 0.72 * p / 127.0,
            (false, 0) => 0.28 + 1.44 * (p - 128.0) / 127.0,
            (false, _) => 1.0 + 3.0 * (p - 128.0) / 127.0,
        };
        if band == 0 {
            (
                0.24 + 0.36 * amount,
                0.001_221_5,
                0.999_917 - 0.001_356_5 * depth * depth * scale,
            )
        } else {
            (
                0.33 + 0.232_5 * amount,
                1.0,
                0.999_917 - 0.000_328 * depth * depth * scale,
            )
        }
    }

    fn comp_settings(knob: i32, param: i32, band: usize) -> (f64, f64, f64) {
        let p = f64::from(param);
        if knob <= 147 {
            return (if band == 0 { 0.546_5 } else { 0.433_4 }, 0.15, 1.0);
        }
        let turn = (f64::from(knob - 148) / 107.0).min(1.0);
        let shrink = (1.0 - turn).powi(3) * p / 127.5;
        let threshold = if band == 0 {
            0.135_1 + 0.411_4 * shrink
        } else {
            0.107_1 + 0.326_3 * shrink
        };
        (
            threshold,
            0.15 + 0.45 * turn + 0.4 * p / 255.0,
            1.0 + 2.0 * turn + 0.548 * p / 255.0,
        )
    }

    fn gate_comp(&mut self, input: [f32; 2], knob: i32) -> [f32; 2] {
        let dry = fade_dry(knob, true);
        let mut out = [0.0; 2];
        for ch in 0..2 {
            let [low, mid, high] = self.bands.split(ch, input[ch]);
            let mut sum = 0.0;
            for (band, signal) in [low, mid + high].into_iter().enumerate() {
                let slot = ch * 2 + band;
                let level = &mut self.env[slot];
                *level = if signal.abs() > *level {
                    signal.abs()
                } else {
                    *level * 0.999_83
                };
                let level = f64::from(*level);
                sum += if knob <= 123 {
                    let (threshold, attack, release) = Self::gate_settings(knob, self.param, band);
                    let gain = &mut self.gate[slot];
                    *gain = if level > threshold {
                        *gain + (1.0 - *gain) * attack as f32
                    } else {
                        *gain * release as f32
                    };
                    signal * *gain
                } else if knob >= 132 {
                    let (threshold, ratio, trim) = Self::comp_settings(knob, self.param, band);
                    let gain = if level <= threshold {
                        trim
                    } else {
                        trim / ((1.0 - ratio) + ratio * level / threshold).max(1e-6)
                    };
                    signal * gain as f32
                } else {
                    signal
                };
            }
            out[ch] = input[ch] * dry + sum * (1.0 - dry);
        }
        out
    }

    fn key_tune(&mut self, x: [f32; 2], v: i32) -> [f32; 2] {
        let wet = match v {
            127 | 128 => 0.0,
            123..=126 => 0.25 * f32::from((127 - v) as u8),
            129..=132 => 0.25 * f32::from((v - 128) as u8),
            _ => 1.0,
        };
        let mono = f32::midpoint(x[0], x[1]);
        let (lp, _) = self.detector.band[0].tick(0, mono);
        let (_, band) = self.detector.band[1].tick(0, lp);
        self.detector.since += 1.0;
        if self.detector.last <= 0.0 && band > 0.0 && self.detector.since > 1.0 {
            let d = &mut self.detector;
            d.periods[d.count % 6] = d.since;
            d.count += 1;
            d.since = 0.0;
        }
        self.detector.last = band;
        self.detector.until -= 1.0;
        if self.detector.until <= 0.0 {
            self.detector.until = 0.010_667 * self.rate;
            let n = self.detector.count.min(6);
            let measured = if n > 0 {
                self.detector.periods[..n].iter().sum::<f64>() / n as f64
            } else {
                0.0
            };
            let scale = scale_periods(key_of(v), self.rate);
            let nearest = scale
                .iter()
                .min_by(|a, b| (*a - measured).abs().total_cmp(&(*b - measured).abs()))
                .copied()
                .unwrap_or(measured);
            let ratio = if measured > 0.0 {
                measured / nearest
            } else {
                1.0
            };
            self.detector.ratio =
                if (0.87..=1.13).contains(&ratio) && !(0.995..=1.005).contains(&ratio) {
                    ratio
                } else {
                    1.0
                };
        }
        let tuned = self.shifter.tick(x, self.detector.ratio);
        [
            x[0] * (1.0 - wet) + tuned[0] * wet,
            x[1] * (1.0 - wet) + tuned[1] * wet,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn curves_follow_the_spec_values() {
        assert_eq!(step_of(0.5), 127);
        assert!((fade_dry(127, false) - 1.0).abs() < 1e-6 && fade_dry(0, false) < 1e-6);
        assert!(noise_level(127).abs() < 1e-9);
        assert!((noise_level(0) - 1.391_3).abs() < 1e-9);
        assert!((pitch_speed(255, 12.0) - 1.0).abs() < 1e-9, "an octave up");
        assert!((pitch_speed(0, 12.0) + 0.5).abs() < 1e-9, "an octave down");
        assert_eq!(key_of(130), 0, "C right of the centre");
        assert_eq!(key_of(0), 6, "F# at the left");
        assert_eq!(scale_periods(0, 48_000.0).len(), 10, "ten notes, G3 to B4");
        assert!((pitch_range(0) - 2.0).abs() < 1e-9 && (pitch_range(255) - 24.0).abs() < 1e-9);
        assert!((pitch_range(127) - 12.0).abs() < 1e-9);
        assert!(jet_scale(0).abs() < 1e-9 && (jet_scale(255) - 1.333).abs() < 0.01);
        assert!((sweep_hz(0) - 60.0).abs() < 1e-9 && (sweep_hz(255) - 20_000.0).abs() < 20.0);
        assert!((dub_feedback(255).0 - 0.99).abs() < 1e-9);
        assert!(space_feedback(0.9, 0, 0.000_5).abs() < 1e-9);
        assert!((space_feedback(0.9, 127, 0.000_5) - 0.9).abs() < 1e-9);
    }

    #[test]
    fn parameter_widens_the_pitch_range() {
        let pitch = |param: f32| {
            let mut fx = ColorFx::new(Kind::Pitch, f64::from(RATE));
            fx.set_knob(1.0);
            fx.set_parameter(param);
            let mut buf = sine(300.0, RATE as usize, 0.4);
            fx.process(&mut buf);
            let left: Vec<f32> = buf.iter().step_by(2).copied().collect();
            left[24_000..]
                .windows(2)
                .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
                .count()
        };
        assert!(
            pitch(1.0) as f64 > 1.6 * pitch(0.5) as f64,
            "{} {}",
            pitch(1.0),
            pitch(0.5)
        );
    }

    #[test]
    fn every_color_effect_is_dry_at_the_centre_and_bounded_off_it() {
        for kind in [
            Kind::Jet,
            Kind::Crush,
            Kind::Noise,
            Kind::Pitch,
            Kind::Space,
            Kind::DubEcho,
            Kind::Echo,
            Kind::Sweep,
            Kind::GateComp,
            Kind::KeyTune,
            Kind::Reverb,
        ] {
            let input = sine(330.0, RATE as usize / 2, 0.4);
            let mut fx = ColorFx::new(kind, f64::from(RATE));
            let mut buf = input.clone();
            fx.process(&mut buf);
            let diff: Vec<f32> = buf.iter().zip(&input).map(|(a, b)| a - b).collect();
            assert!(rms(&diff) < 0.02, "{kind:?} at the centre: {}", rms(&diff));
            for knob in [-1.0, 1.0] {
                let mut fx = ColorFx::new(kind, f64::from(RATE));
                fx.set_knob(knob);
                let mut buf = input.clone();
                fx.process(&mut buf);
                assert!(
                    buf.iter().all(|v| v.is_finite() && v.abs() < 4.0),
                    "{kind:?} {knob}"
                );
            }
        }
    }
}
