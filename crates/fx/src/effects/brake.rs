use backspn_dsp::{DelayLine, Isolator, OnePole, Smoothed};

use super::echo::{ECHO_BEATS, HIGH_BAND_HZ, LOW_BAND_HZ, Shifter};
use super::{EffectKind, pick};
use crate::{Context, Effect};

const BRAKE_S: f64 = 6.0;
const MIN_DECAY_MS: f64 = 10.0;
const MAX_DECAY_MS: f64 = 12_000.0;
const TAIL_FROM: f64 = 0.125;
const TAIL_SLOPE: f64 = 0.205_59;
const LEVEL_ON: f32 = 0.009_766;
const RECOVERY_S: f64 = 0.2;
const SMOOTH_S: f64 = 0.003_333;
const PRE_START_S: f64 = 0.002;
const ECHO_SEND: f32 = 0.7;
const ECHO_MIN_MS: f64 = 10.0;
const ECHO_MAX_MS: f64 = 4000.0;
const GROUP_BEATS: f64 = 1.0;
const SLIDER_BEATS: [f64; 5] = [8.0, 4.0, 2.0, 1.0, 0.5];
const SPIN_HISTORY_S: f64 = 6.0;
const SPIN_PEAK: f64 = 8.0;
const SPIN_END_FROM: f64 = 0.282_35;
const SPIN_END_S: f64 = 0.186_1;
const SPIN_GATE_S: f64 = 0.003_333;

fn brake_gain(v: f64) -> f32 {
    (if v <= 0.01 {
        0.0
    } else if v <= 0.05 {
        12.5 * v - 0.125
    } else if v <= 0.125 {
        0.5
    } else if v <= 1.0 {
        4.0 / 7.0 * v + 3.0 / 7.0
    } else {
        1.0
    }) as f32
}

fn stop_beats(level: f32) -> f64 {
    let l = f64::from(level.clamp(0.0, 1.0));
    if l < 0.25 {
        4.0
    } else if l < 0.5 {
        3.25 * (2.0 - 4.0 * l).powi(2) + 0.75
    } else if l < 0.990_2 {
        1.387_45 - 1.274_9 * l
    } else {
        0.125
    }
}

fn echo_of(v: f32) -> Option<(f64, f32)> {
    let beats = match v {
        v if v < 0.000_5 => 0.03,
        v if v < 0.025 => 1.0 / 8.0,
        v if v < 0.125 => 1.0 / 6.0,
        v if v < 0.225 => 1.0 / 3.0,
        v if v < 0.325 => 1.0 / 4.0,
        v if v < 0.425 => 1.0 / 2.0,
        v if v <= 0.575 => return None,
        v if v < 0.7 => 3.0 / 4.0,
        v if v < 0.825 => 1.0,
        v if v < 0.95 => 3.0 / 2.0,
        _ => 2.0,
    };
    let feedback = match beats {
        b if b <= 1.0 / 6.0 => 0.8,
        b if b <= 3.0 / 4.0 => 0.7,
        b if b <= 1.0 => 0.6,
        b if b <= 1.5 => 0.5,
        _ => 0.4,
    };
    Some((beats, feedback))
}

fn repeat_beats(v: f32) -> f64 {
    match v {
        v if v < 0.000_5 => 1.0 / 16.0,
        v if v < 0.11 => 1.0 / 8.0,
        v if v < 0.22 => 1.0 / 6.0,
        v if v < 0.33 => 1.0 / 3.0,
        v if v < 0.44 => 1.0 / 4.0,
        v if v < 0.56 => 1.0 / 2.0,
        v if v < 0.67 => 3.0 / 4.0,
        v if v < 0.78 => 1.0,
        v if v < 0.89 => 3.0 / 2.0,
        v if v < 0.999_5 => 2.0,
        _ => 4.0,
    }
}

fn slider_beats(v: f32) -> f64 {
    SLIDER_BEATS[((v.clamp(0.0, 1.0) * 5.0) as usize).min(4)]
}

fn slider_for(beats: f64) -> f32 {
    let b = super::nearest_beat(&SLIDER_BEATS, beats);
    let i = SLIDER_BEATS
        .iter()
        .position(|&s| (s - b).abs() < 1e-9)
        .unwrap_or(0);
    (i as f32 + 0.5) / SLIDER_BEATS.len() as f32
}

#[derive(Debug)]
struct Core {
    ring: Box<[[f32; 2]]>,
    written: usize,
    read: f64,
    speed: f64,
    decay_frames: f64,
    running: bool,
}

impl Core {
    fn new(rate: f64) -> Self {
        Self {
            ring: vec![[0.0; 2]; (BRAKE_S * rate) as usize].into_boxed_slice(),
            written: 0,
            read: 0.0,
            speed: 1.0,
            decay_frames: 1.0,
            running: false,
        }
    }

    fn start(&mut self, decay_ms: f64, rate: f64) {
        self.decay_frames = decay_ms.clamp(MIN_DECAY_MS, MAX_DECAY_MS) / 1000.0 * rate;
        self.written = 0;
        self.read = 0.0;
        self.speed = 1.0;
        self.running = true;
    }

    fn tick(&mut self, x: [f32; 2], rate: f64) -> [f32; 2] {
        if !self.running {
            return [0.0; 2];
        }
        if self.written < self.ring.len() {
            self.ring[self.written] = x;
            self.written += 1;
        }
        let i = self.read as usize;
        let frac = (self.read - i as f64) as f32;
        let last = self.written.saturating_sub(1);
        let (a, b) = (self.ring[i.min(last)], self.ring[(i + 1).min(last)]);
        let gain = brake_gain(self.speed);
        let out = [
            (a[0] + (b[0] - a[0]) * frac) * gain,
            (a[1] + (b[1] - a[1]) * frac) * gain,
        ];
        self.read += self.speed;
        self.speed -= if self.speed >= TAIL_FROM {
            1.0 / self.decay_frames
        } else {
            TAIL_SLOPE / rate
        };
        if self.speed <= 0.0 {
            self.speed = 0.0;
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    VinylBrake,
    ReleaseBrake,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    PreStart { left: f64 },
    Braking,
    Recovery,
}

#[derive(Debug, Clone, Copy, Default)]
struct Recovery {
    level: f64,
    speed: f64,
    drift: f64,
}

fn shift_of(v: f64) -> Option<f64> {
    let g = if v >= 0.45 { v } else { 0.25 + 4.0 / 9.0 * v };
    let pct = (100.0 * g.max(1e-9).log2()).max(-200.0);
    if pct <= -200.0 {
        return None;
    }
    Some(if pct < 0.0 {
        0.005 * pct
    } else if pct <= 100.0 {
        0.01 * pct
    } else {
        0.02 * pct - 1.0
    })
}

fn recovery_speed(l: f64, entry: &Recovery) -> f64 {
    let floor = f64::from(LEVEL_ON);
    let down = if (entry.level - floor).abs() < 1e-9 {
        0.0
    } else {
        (entry.speed - 1.0) / (entry.level - floor)
    };
    let up = if (entry.level - 1.0).abs() < 1e-9 {
        0.0
    } else {
        entry.speed / (entry.level - 1.0)
    };
    let v = if l > entry.level && up.abs() > down.abs() {
        entry.speed + up * (l - entry.level)
    } else {
        1.0 + down * (l - floor)
    };
    let k = if l < 0.125 {
        8.678_327 * l - 0.084_743
    } else {
        1.0
    };
    (v - entry.drift * k).clamp(0.0, 1.0)
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Brake {
    kind: Kind,
    rate: f64,
    core: Core,
    bands: Isolator,
    echo: DelayLine,
    shifter: Shifter,
    lowpass: OnePole,
    level: f32,
    moved: bool,
    beats: f64,
    slider: f32,
    echo_knob: f32,
    repeat: f32,
    repeat_on: bool,
    bpf: bool,
    active: bool,
    state: State,
    peak: f32,
    recovery: Recovery,
    speed: f64,
    next_repeat: Option<f64>,
    started_beats: f64,
    ring_gain: Smoothed,
    live_gain: Smoothed,
    filtered: Smoothed,
    send: Smoothed,
    dry: Smoothed,
}

impl Brake {
    pub fn new(kind: Kind, rate: f64) -> Self {
        let fast = |v| Smoothed::new(v, PRE_START_S / 3.0, rate);
        Self {
            kind,
            rate,
            core: Core::new(rate),
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            echo: DelayLine::new((ECHO_MAX_MS / 1000.0 * rate) as usize + 4),
            shifter: Shifter::new(rate),
            lowpass: OnePole::default(),
            level: 0.0,
            moved: false,
            beats: GROUP_BEATS,
            slider: 0.6,
            echo_knob: 0.5,
            repeat: 0.5,
            repeat_on: false,
            bpf: false,
            active: false,
            state: State::Idle,
            peak: 0.0,
            recovery: Recovery::default(),
            speed: 1.0,
            next_repeat: None,
            started_beats: GROUP_BEATS,
            ring_gain: fast(0.0),
            live_gain: fast(0.0),
            filtered: Smoothed::new(0.0, SMOOTH_S / 3.0, rate),
            send: Smoothed::new(ECHO_SEND, SMOOTH_S / 3.0, rate),
            dry: fast(1.0),
        }
    }

    fn on(&self) -> bool {
        match self.kind {
            Kind::VinylBrake => self.active && self.level > LEVEL_ON,
            Kind::ReleaseBrake => self.active,
        }
    }

    fn decay_ms(&self, bpm: f64) -> f64 {
        let beat_ms = 60_000.0 / bpm.max(20.0);
        match self.kind {
            Kind::VinylBrake => self.beats * beat_ms * stop_beats(self.level),
            Kind::ReleaseBrake => slider_beats(self.slider) * beat_ms,
        }
    }

    fn glide(&mut self, seconds: f64) {
        let (ring, live) = (self.ring_gain.value(), self.live_gain.value());
        self.ring_gain = Smoothed::new(ring, seconds / 3.0, self.rate);
        self.live_gain = Smoothed::new(live, seconds / 3.0, self.rate);
    }

    fn arm(&mut self) {
        self.glide(PRE_START_S);
        self.state = State::PreStart {
            left: PRE_START_S * self.rate,
        };
    }

    fn may_recover(&self) -> bool {
        !self.repeat_on || repeat_beats(self.repeat) > 1.0
    }

    fn update(&mut self, ctx: &Context, frames: usize) {
        let on = self.on();
        let l = f64::from(self.level);
        if !on {
            if self.state != State::Idle {
                if self.state == State::Recovery {
                    self.glide(RECOVERY_S);
                }
                self.state = State::Idle;
            }
        } else if self.state == State::Idle {
            self.started_beats = self.beats;
            self.next_repeat = None;
            self.arm();
        } else if self.kind == Kind::VinylBrake {
            if (self.beats - self.started_beats).abs() > 1e-9 {
                self.started_beats = self.beats;
                self.arm();
            } else if self.repeat_on {
                let every = repeat_beats(self.repeat);
                let next = *self
                    .next_repeat
                    .get_or_insert(((ctx.beat / every).floor() + 1.0) * every);
                if ctx.beat_end.max(ctx.beat) >= next {
                    self.next_repeat = Some(next + every);
                    self.arm();
                }
            }
        }
        if self.state == State::Braking && self.kind == Kind::VinylBrake {
            self.peak = self.peak.max(self.level);
            if self.level < self.peak - 0.031_25 && self.may_recover() {
                self.recovery = Recovery {
                    level: l,
                    speed: self.core.speed,
                    drift: 0.0,
                };
                self.glide(RECOVERY_S);
                self.state = State::Recovery;
            }
        }
        self.speed = match self.state {
            State::Braking => self.core.speed,
            State::Recovery => {
                let v = recovery_speed(l, &self.recovery);
                if self.ring_gain.value() < 1e-3 && v > 0.0 && !self.moved {
                    let beat_frames = 60.0 / ctx.bpm.max(20.0) * self.rate;
                    self.recovery.drift += 0.1 * frames as f64 / beat_frames;
                }
                if v == 0.0 {
                    self.recovery = Recovery {
                        level: l,
                        speed: 0.0,
                        drift: 0.0,
                    };
                }
                v
            }
            _ => 1.0,
        };
        self.moved = false;
        let (ring, live) = match self.state {
            State::Braking => (1.0, 0.0),
            State::Recovery => (0.0, brake_gain(self.speed)),
            _ => (0.0, 0.0),
        };
        self.ring_gain.set(ring);
        self.live_gain.set(live);
        self.dry
            .set(if self.state == State::Idle { 1.0 } else { 0.0 });
        let recovering = self.state == State::Recovery;
        if recovering && self.speed < 0.5 {
            self.lowpass
                .set(self.speed * self.speed * 46_000.0 + 2_500.0, self.rate);
            self.filtered.set(1.0);
        } else {
            self.lowpass.set(14_000.0, self.rate);
            self.filtered.set(0.0);
        }
        let send = if echo_of(self.echo_knob).is_none() {
            0.0
        } else if recovering && self.speed > 0.95 {
            14.0 * (1.0 - self.speed) as f32
        } else {
            ECHO_SEND
        };
        self.send.set(send);
    }

    fn frame(&mut self, x: [f32; 2], bpm: f64) -> [f32; 2] {
        if let State::PreStart { left } = self.state {
            if left <= 1.0 {
                self.core.start(self.decay_ms(bpm), self.rate);
                self.glide(PRE_START_S);
                self.peak = self.level;
                self.state = State::Braking;
            } else {
                self.state = State::PreStart { left: left - 1.0 };
            }
        }
        let (mut input, mut thru) = (x, [0.0; 2]);
        if self.bpf {
            for ch in 0..2 {
                let [low, mid, high] = self.bands.split(ch, x[ch]);
                input[ch] = mid;
                thru[ch] = low + high;
            }
        }
        let braked = self.core.tick(input, self.rate);
        let live = if let Some(shift) = shift_of(self.speed) {
            self.shifter.tick(input, 1.0 + shift)
        } else {
            let _ = self.shifter.tick(input, 1.0);
            [0.0; 2]
        };
        let (ring, gain, filtered) = (
            self.ring_gain.tick(),
            self.live_gain.tick(),
            self.filtered.tick(),
        );
        let mut sum = [0.0; 2];
        for ch in 0..2 {
            let (low, _) = self.lowpass.tick(ch, live[ch]);
            let live = live[ch] + (low - live[ch]) * filtered;
            sum[ch] = braked[ch] * ring + live * gain;
        }
        let send = self.send.tick();
        let echo = self.echo_tick([sum[0] * send, sum[1] * send], bpm);
        let dry = self.dry.tick();
        let mut out = [0.0; 2];
        for ch in 0..2 {
            let kept = x[ch] * dry + thru[ch] * (1.0 - dry);
            out[ch] = kept + sum[ch] + echo[ch];
        }
        out
    }

    fn echo_tick(&mut self, x: [f32; 2], bpm: f64) -> [f32; 2] {
        let Some((beats, feedback)) = echo_of(self.echo_knob) else {
            self.echo.push([0.0; 2]);
            return [0.0; 2];
        };
        let ms = (beats * 60_000.0 / bpm.max(20.0)).clamp(ECHO_MIN_MS, ECHO_MAX_MS);
        let out = self.echo.read((ms / 1000.0 * self.rate) as f32);
        self.echo
            .push([x[0] + out[0] * feedback, x[1] + out[1] * feedback]);
        out
    }

    fn set_level(&mut self, level: f32) {
        if (level - self.level).abs() > 1e-6 {
            self.moved = true;
        }
        self.level = level;
    }
}

impl Effect for Brake {
    fn kind(&self) -> EffectKind {
        match self.kind {
            Kind::VinylBrake => EffectKind::VinylBrake,
            Kind::ReleaseBrake => EffectKind::ReleaseBrake,
        }
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let frames = buf.len() / 2;
        self.update(ctx, frames);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            *frame = self.frame(*frame, ctx.bpm);
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.beats = GROUP_BEATS;
        self.echo_knob = 0.5;
        self.repeat_on = false;
        self.bpf = false;
        match self.kind {
            Kind::VinylBrake => self.set_level(amount),
            Kind::ReleaseBrake => self.slider = amount,
        }
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.set_amount(pad.level);
        match self.kind {
            Kind::VinylBrake => self.beats = pad.beats,
            Kind::ReleaseBrake => self.slider = slider_for(pad.beats),
        }
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match (self.kind, index) {
            (Kind::VinylBrake, 0) => self.beats = pick(&ECHO_BEATS, value),
            (Kind::VinylBrake, 1) => {
                if (value - self.repeat).abs() > 1e-6 && self.repeat_on && self.state != State::Idle
                {
                    self.arm();
                }
                self.repeat = value;
            }
            (Kind::VinylBrake, 2) => self.echo_knob = value,
            (Kind::ReleaseBrake, 0) => self.slider = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match (self.kind, index) {
            (Kind::VinylBrake, 0) => {
                if on != self.repeat_on && self.state != State::Idle {
                    self.arm();
                }
                self.repeat_on = on;
                self.next_repeat = None;
            }
            (Kind::VinylBrake, 1) => self.bpf = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        if self.kind == Kind::VinylBrake {
            self.set_level(dry_wet);
        }
        self.kind == Kind::VinylBrake
    }

    fn has_tail(&self) -> bool {
        self.kind == Kind::VinylBrake
    }

    fn reset(&mut self) {
        self.echo.clear();
        self.shifter.clear();
        self.core.running = false;
        self.state = State::Idle;
    }
}

#[derive(Debug)]
pub struct BackSpin {
    rate: f64,
    history: DelayLine,
    written: u64,
    slider: f32,
    active: bool,
    origin: Option<u64>,
    head: f64,
    window: f64,
    speed: f64,
    elapsed: f64,
    up: f64,
    down: f64,
    gate: f32,
}

fn spin_gain(s: f64) -> f32 {
    let g = if s < 0.0 {
        0.0
    } else if s < 0.283_48 {
        18.165 * s * s * (1.0 - 2.0 * s)
    } else if s < 1.0 {
        3.0 * s - 3.0 * s * s + s * s * s
    } else if s <= SPIN_PEAK {
        0.02 * s + 0.98
    } else {
        0.0
    };
    (g * 0.559_8) as f32
}

impl BackSpin {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            history: DelayLine::new((SPIN_HISTORY_S * rate) as usize),
            written: 0,
            slider: 0.6,
            active: false,
            origin: None,
            head: 0.0,
            window: 1.0,
            speed: 0.0,
            elapsed: 0.0,
            up: 1.0,
            down: 1.0,
            gate: 1.0,
        }
    }

    fn start(&mut self, now: u64, bpm: f64) {
        let beat_ms = 60_000.0 / bpm.max(20.0);
        let stop = (slider_beats(self.slider) * beat_ms).clamp(10.0, 100_000.0);
        let two_beats = (2.0 * beat_ms).clamp(10.0, 4000.0);
        let mut up = 0.16 * two_beats;
        let mut down = stop - up;
        if down < up {
            up = stop / 2.0;
            down = stop / 2.0;
        }
        let frames = |ms: f64| ms / 1000.0 * self.rate;
        self.up = frames(up);
        self.down = frames(down);
        self.window = frames(two_beats).min(self.history.capacity() as f64 - 2.0);
        self.origin = Some(now);
        self.head = now as f64;
        self.speed = 0.0;
        self.elapsed = 0.0;
        self.gate = 1.0;
    }

    fn advance(&mut self) {
        self.elapsed += 1.0;
        if self.elapsed <= self.up {
            self.speed = SPIN_PEAK * self.elapsed / self.up;
        } else if self.speed > SPIN_END_FROM {
            self.speed = (self.speed - SPIN_PEAK / self.down).max(SPIN_END_FROM);
            if self.speed <= SPIN_END_FROM {
                self.speed = SPIN_END_FROM - 1e-9;
            }
        } else {
            self.speed = (self.speed - SPIN_END_FROM / (SPIN_END_S * self.rate)).max(0.0);
        }
    }

    fn frame(&mut self, x: [f32; 2], bpm: f64) -> [f32; 2] {
        self.history.push(x);
        let now = self.written;
        self.written += 1;
        if !self.active {
            self.origin = None;
            return x;
        }
        if self.origin.is_none() {
            self.start(now, bpm);
        }
        let origin = self.origin.unwrap_or(now) as f64;
        self.advance();
        self.head -= self.speed;
        let step = 1.0 / (SPIN_GATE_S * self.rate) as f32;
        if self.head < origin - self.window {
            self.gate -= step;
            if self.gate <= 0.0 {
                self.head += self.window;
            }
        } else {
            self.gate = (self.gate + step).min(1.0);
        }
        let at = self.head.floor().max(0.0) as u64;
        let played = if now - at.min(now) >= self.history.capacity() as u64 {
            [0.0; 2]
        } else {
            self.history.at(at as usize)
        };
        let gain = spin_gain(self.speed) * self.gate.max(0.0);
        [played[0] * gain, played[1] * gain]
    }
}

impl Effect for BackSpin {
    fn kind(&self) -> EffectKind {
        EffectKind::BackSpin
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            *frame = self.frame(*frame, ctx.bpm);
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.slider = amount;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if index == 0 {
            self.slider = value;
        }
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.slider = slider_for(pad.beats);
    }

    fn set_button(&mut self, _index: usize, _on: bool) {}

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn listens(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.history.clear();
        self.origin = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn curves_follow_the_spec_values() {
        assert!(brake_gain(0.0).abs() < 1e-9 && (brake_gain(1.0) - 1.0).abs() < 1e-6);
        assert!((brake_gain(0.1) - 0.5).abs() < 1e-6);
        assert!((stop_beats(0.5) - 0.75).abs() < 1e-3, "centre: 3/4 beat");
        assert!((stop_beats(0.0) - 4.0).abs() < 1e-9 && (stop_beats(1.0) - 0.125).abs() < 1e-9);
        assert_eq!(echo_of(0.5), None);
        assert_eq!(echo_of(0.0), Some((0.03, 0.8)));
        assert_eq!(echo_of(0.75), Some((1.0, 0.6)));
        assert!((repeat_beats(0.5) - 0.5).abs() < 1e-9 && (repeat_beats(1.0) - 4.0).abs() < 1e-9);
        assert!((slider_beats(0.0) - 8.0).abs() < 1e-9 && (slider_beats(1.0) - 0.5).abs() < 1e-9);
        assert!((spin_gain(1.0) - 0.559_8).abs() < 1e-3);
        assert!(spin_gain(0.0).abs() < 1e-9);
    }

    #[test]
    fn vinyl_brake_slows_the_track_to_silence() {
        let mut b = Brake::new(Kind::VinylBrake, f64::from(RATE));
        b.set_amount(1.0);
        let input = sine(440.0, RATE as usize, 0.5);
        let out = run(&mut b, &input, ctx());
        assert!((rms(&out) - rms(&input)).abs() < 1e-3, "off: untouched");
        b.set_active(true);
        let out = run(&mut b, &input, ctx());
        assert!(rms(&out[..2 * 2_000]) > 0.1, "still turning");
        assert!(rms(&out[2 * 40_000..]) < 1e-4, "stopped after the tail");
        b.set_active(false);
        let out = run(&mut b, &input, ctx());
        assert!(rms(&out[2 * 1_000..]) > 0.3, "the track again");
    }

    #[test]
    fn recovery_follows_its_curves() {
        assert_eq!(shift_of(1.0), Some(0.0));
        assert!(shift_of(0.0).is_none(), "the bottom mutes");
        let half = shift_of(0.5).unwrap();
        assert!((half + 0.5).abs() < 1e-9, "an octave down: {half}");
        let entry = Recovery {
            level: 0.8,
            speed: 0.3,
            drift: 0.0,
        };
        assert!(
            (recovery_speed(0.8, &entry) - 0.3).abs() < 1e-9,
            "where it entered"
        );
        assert!(
            (recovery_speed(f64::from(LEVEL_ON), &entry) - 1.0).abs() < 1e-9,
            "full speed at the bottom"
        );
        assert!(
            recovery_speed(1.0, &entry).abs() < 1e-9,
            "stopped at the top"
        );
    }

    #[test]
    fn turning_level_down_while_braking_brings_the_track_back() {
        let mut b = Brake::new(Kind::VinylBrake, f64::from(RATE));
        b.set_dry_wet(1.0);
        b.set_param(0, 5.0 / 9.0);
        b.set_active(true);
        let input = sine(440.0, RATE as usize, 0.5);
        let out = run(&mut b, &input, ctx());
        assert!(rms(&out[2 * 40_000..]) < 1e-3, "stopped");
        assert_eq!(b.state, State::Braking);
        b.set_dry_wet(0.25);
        let _ = run(&mut b, &input[..2 * 512], ctx());
        assert_eq!(b.state, State::Recovery);
        b.set_dry_wet(0.1);
        let out = run(&mut b, &input, ctx());
        assert!(b.speed > 0.4 && b.speed < 1.0, "speed {}", b.speed);
        assert!(rms(&out[2 * 24_000..]) > 0.05, "the track again, pitched");
        b.set_dry_wet(0.0);
        let out = run(&mut b, &input, ctx());
        assert_eq!(b.state, State::Idle);
        let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
        assert!(rms(&diff[2 * 24_000..]) < 0.01);
    }

    #[test]
    fn back_spin_plays_what_came_before_backwards() {
        let mut s = BackSpin::new(f64::from(RATE));
        s.set_amount(0.5);
        let ramp: Vec<f32> = (0..RATE as usize)
            .flat_map(|i| [i as f32 / 48_000.0; 2])
            .collect();
        let _ = run(&mut s, &ramp, ctx());
        s.set_active(true);
        let out = run(&mut s, &ramp[..2 * 12_000], ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!(left[4_000] > 0.0 && left[4_000] < 0.6);
        assert!(left[3_000] > left[4_000]);
    }
}
