use backspn_dsp::{Isolator, Smoothed};

use super::echo::{ECHO_BEATS, GROUP_BEATS, HIGH_BAND_HZ, LOW_BAND_HZ, centred};
use super::{EffectKind, pick};
use crate::{Context, Effect};

const MAX_MS: f64 = 4000.0;
const MIN_MS: f64 = 10.0;
const RAMP_S: f64 = 0.003_333;
const FADE_BEATS: f64 = 3.0;

fn gains_of(x: f32) -> (f32, f32) {
    let x = x.clamp(0.0, 1.0);
    if x < 0.5 {
        let u = (1.0 - 2.0 * x).powi(2);
        (0.85 * (1.0 - u), 0.45 + 0.55 * u)
    } else {
        (0.7 + 0.3 * x, 0.9 - 0.9 * x)
    }
}

fn trim_of(v: f32) -> f64 {
    let c = f64::from(centred(v));
    if c < 0.0 {
        1.0 - 3.0 * c
    } else {
        1.0 / (1.0 + 3.0 * c)
    }
}

fn glide_s(v: f32) -> (f64, f64) {
    let v = f64::from(v.clamp(0.0, 1.0));
    (1.78 * v + 0.01, 1.38 * v + 0.01)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Off,
    Capture,
    Loop,
    Glide,
    Commit(usize),
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct RisingEcho {
    rate: f64,
    current: Box<[[f32; 2]]>,
    next: Box<[[f32; 2]]>,
    bands: Isolator,
    state: State,
    len: usize,
    target: usize,
    pos: f64,
    speed: f64,
    beats: f64,
    trim: f64,
    track_speed: f32,
    loop_gain: Smoothed,
    input_gain: Smoothed,
    fade: f32,
    active: bool,
    lo_cut: bool,
    hi_cut: bool,
}

impl RisingEcho {
    pub fn new(rate: f64) -> Self {
        let size = (MAX_MS / 1000.0 * rate) as usize + 1;
        let (a, b) = gains_of(0.5);
        Self {
            rate,
            current: vec![[0.0; 2]; size].into_boxed_slice(),
            next: vec![[0.0; 2]; size].into_boxed_slice(),
            bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            state: State::Off,
            len: 1,
            target: 1,
            pos: 0.0,
            speed: 1.0,
            beats: GROUP_BEATS,
            trim: 1.0,
            track_speed: 0.5,
            loop_gain: Smoothed::new(a, RAMP_S, rate),
            input_gain: Smoothed::new(b, RAMP_S, rate),
            fade: 0.0,
            active: false,
            lo_cut: false,
            hi_cut: false,
        }
    }

    fn set_level(&mut self, level: f32) {
        let (a, b) = gains_of(level);
        self.loop_gain.set(a);
        self.input_gain.set(b);
    }

    fn wanted(&self, bpm: f64) -> usize {
        let ms = (self.beats * 60_000.0 / bpm.max(20.0) * self.trim).clamp(MIN_MS, MAX_MS);
        ((ms / 1000.0 * self.rate) as usize).clamp(1, self.current.len() - 1)
    }

    fn tick(&mut self, input: [f32; 2]) -> [f32; 2] {
        let (loop_gain, input_gain) = (self.loop_gain.tick(), self.input_gain.tick());
        match self.state {
            State::Off => input,
            State::Capture => {
                let i = self.pos as usize;
                self.current[i] = input;
                self.pos += 1.0;
                if self.pos as usize >= self.target {
                    self.len = self.target;
                    self.pos = 0.0;
                    self.state = State::Loop;
                }
                input
            }
            State::Loop | State::Glide | State::Commit(_) => {
                let i = (self.pos as usize).min(self.len - 1);
                let looped = self.current[i];
                let gain = if self.speed > 2.0 {
                    loop_gain * (1.0 / (self.speed - 1.0)).max(0.25) as f32
                } else {
                    loop_gain
                };
                let kept = [
                    looped[0] * gain + input[0] * input_gain,
                    looped[1] * gain + input[1] * input_gain,
                ];
                let out = [
                    looped[0] * gain * self.fade + input[0] * input_gain,
                    looped[1] * gain * self.fade + input[1] * input_gain,
                ];
                match self.state {
                    State::Loop => self.current[i] = kept,
                    State::Commit(n) => {
                        self.next[n] = kept;
                        if n + 1 >= self.target {
                            std::mem::swap(&mut self.current, &mut self.next);
                            self.len = self.target;
                            self.pos = -self.speed;
                            self.speed = 1.0;
                            self.state = State::Loop;
                        } else {
                            self.state = State::Commit(n + 1);
                        }
                    }
                    _ => {}
                }
                self.pos = (self.pos + self.speed).rem_euclid(self.len as f64);
                out
            }
        }
    }

    fn glide(&mut self) {
        let ratio = self.len as f64 / self.target as f64;
        let (up, down) = glide_s(self.track_speed);
        let rising = ratio > self.speed;
        let per_frame = 2f64.powf(1.0 / ((if rising { up } else { down }) * self.rate));
        self.speed = if rising {
            (self.speed * per_frame).min(ratio)
        } else {
            (self.speed / per_frame).max(ratio)
        };
        if (self.speed - ratio).abs() < 1e-9 && self.loop_gain.value() < 1.0 {
            self.state = State::Commit(0);
        }
    }
}

impl Effect for RisingEcho {
    fn kind(&self) -> EffectKind {
        EffectKind::RisingEcho
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let wanted = self.wanted(ctx.bpm);
        match self.state {
            State::Capture => self.target = wanted,
            State::Loop if wanted != self.len => {
                self.target = wanted;
                self.state = State::Glide;
            }
            _ => {}
        }
        let fade_step = 1.0 / (FADE_BEATS * 60.0 / ctx.bpm.max(20.0) * self.rate) as f32;
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            if self.active {
                self.fade = 1.0;
            } else if self.state != State::Off {
                self.fade -= fade_step;
                if self.fade <= 0.0 {
                    self.fade = 0.0;
                    self.state = State::Off;
                }
            }
            if self.state == State::Glide {
                self.glide();
            }
            let mut fx_in = *frame;
            let mut thru = [0.0; 2];
            if self.lo_cut || self.hi_cut {
                for ch in 0..2 {
                    let [low, mid, high] = self.bands.split(ch, frame[ch]);
                    let (lo_thru, hi_thru) = (
                        if self.lo_cut { low } else { 0.0 },
                        if self.hi_cut { high } else { 0.0 },
                    );
                    thru[ch] = lo_thru + hi_thru;
                    fx_in[ch] = mid + low + high - thru[ch];
                }
            }
            let out = self.tick(fx_in);
            frame[0] = out[0] + thru[0];
            frame[1] = out[1] + thru[1];
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.beats = GROUP_BEATS;
        self.trim = 1.0;
        self.track_speed = 0.5;
        self.lo_cut = false;
        self.hi_cut = false;
        self.set_level(amount);
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.beats = pick(&ECHO_BEATS, value),
            1 => self.trim = trim_of(value),
            2 => self.track_speed = value,
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        match index {
            0 => self.lo_cut = on,
            1 => self.hi_cut = on,
            _ => {}
        }
    }

    fn set_active(&mut self, on: bool) {
        if on == self.active {
            return;
        }
        self.active = on;
        if on {
            let resume =
                self.state != State::Off && self.fade >= 0.5 && self.loop_gain.value() > 0.0;
            if !resume {
                self.state = State::Capture;
                self.pos = 0.0;
                self.speed = 1.0;
                self.target = self.len.max(1);
            }
        }
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.set_level(dry_wet);
        true
    }

    fn has_tail(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.state = State::Off;
        self.fade = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn knobs_follow_their_curves() {
        assert_eq!(gains_of(0.0), (0.0, 1.0));
        let (a, b) = gains_of(0.5);
        assert!((a - 0.85).abs() < 1e-6 && (b - 0.45).abs() < 1e-6);
        let (a, b) = gains_of(1.0);
        assert!((a - 1.0).abs() < 1e-6 && b.abs() < 1e-6);
        assert!((trim_of(0.0) - 4.0).abs() < 1e-6 && (trim_of(1.0) - 0.25).abs() < 1e-6);
        assert!((trim_of(0.5) - 1.0).abs() < 1e-9);
        let (up, down) = glide_s(0.5);
        assert!((up - 0.9).abs() < 1e-9 && (down - 0.7).abs() < 1e-9);
    }

    #[test]
    fn loops_what_it_captured() {
        let mut h = RisingEcho::new(f64::from(RATE));
        h.set_dry_wet(1.0);
        h.set_active(true);
        let mut input = sine(440.0, 12_000, 0.5);
        input.resize(2 * RATE as usize, 0.0);
        let out = run(&mut h, &input, ctx());
        assert!(
            rms(&out[2 * 12_000..2 * 24_000]) > 0.3,
            "the half beat again"
        );
        assert!(rms(&out[2 * 36_000..2 * 48_000]) > 0.3, "and again");
    }

    #[test]
    fn a_shorter_length_raises_the_pitch_then_settles() {
        let mut h = RisingEcho::new(f64::from(RATE));
        h.set_dry_wet(0.75);
        h.set_param(2, 0.0);
        h.set_active(true);
        let mut input = sine(440.0, 12_000, 0.5);
        input.resize(2 * RATE as usize, 0.0);
        let _ = run(&mut h, &input, ctx());
        h.set_param(0, 2.0 / 9.0);
        let _ = run(&mut h, &vec![0.0; 2 * RATE as usize / 2], ctx());
        assert_eq!(h.state, State::Loop, "committed");
        assert_eq!(h.len, 6_000);
    }

    #[test]
    fn off_fades_out_over_three_beats() {
        let mut h = RisingEcho::new(f64::from(RATE));
        h.set_dry_wet(1.0);
        h.set_active(true);
        let _ = run(&mut h, &sine(440.0, RATE as usize, 0.5), ctx());
        h.set_active(false);
        let silence = vec![0.0; 2 * 2 * RATE as usize];
        let out = run(&mut h, &silence, ctx());
        assert!(rms(&out[..2 * 12_000]) > 0.1, "still there");
        assert!(rms(&out[2 * 73_000..]) < 1e-4, "gone after 1.5 s");
    }
}
