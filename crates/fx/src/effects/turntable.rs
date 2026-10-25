use backspn_dsp::{DelayLine, Smoothed};

use super::EffectKind;
use crate::{Context, Effect};

const HISTORY_S: f64 = 6.8;
const REF_RATE: f64 = 44_100.0;
const REWIND_SPEED: f64 = -16.0;
const TO_LIVE: f64 = 2_000.0;
const ROCK_OUT: f64 = 1_000.0;
const ROCK_PER_BEAT: [f64; 7] = [0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0];
const GROUP_AMOUNT: f32 = 0.33;

fn brake_frames(v: f32) -> f64 {
    let v = f64::from(v.clamp(0.0, 1.0));
    (5_000.0 + 200_000.0 * (1.0 - v).powi(2)).round()
}

fn rewind_coef(v: f32) -> f64 {
    let v = f64::from(v.clamp(0.0, 1.0));
    0.000_3 * v * v + 1e-6
}

fn rock_strength(v: f32) -> f64 {
    3_000.0 + 8_000.0 * f64::from(v.clamp(0.0, 1.0))
}

fn swing(phase: f64) -> f64 {
    let x = phase - phase.round();
    x * (8.0 - 16.0 * x.abs())
}

fn rewind_gain(speed: f64) -> f64 {
    let g = (speed.abs() / 8.0).min(1.0);
    if g > 0.2 {
        1.0 - 0.4 * (g - 0.2) * 1.25
    } else {
        1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    Brake,
    Restart,
    Rewind,
    RewindEnd,
    Rock { anchor: f64, phase: f64 },
    ToLive { left: f64, len: f64 },
}

#[derive(Debug)]
pub struct Turntable {
    rate: f64,
    history: DelayLine,
    state: State,
    speed: f64,
    lag: f64,
    brake_speed: f32,
    rock_rate: f32,
    amount: Smoothed,
    wet: Smoothed,
    level: Smoothed,
    buttons: [bool; 3],
    active: bool,
    single: bool,
}

impl Turntable {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            history: DelayLine::new((HISTORY_S * rate) as usize),
            state: State::Idle,
            speed: 1.0,
            lag: 1.0,
            brake_speed: 0.502,
            rock_rate: 1.0,
            amount: Smoothed::new(rock_strength(GROUP_AMOUNT) as f32, 2_000.0 / REF_RATE, rate),
            wet: Smoothed::new(0.0, 0.001 / std::f64::consts::LN_2, rate),
            level: Smoothed::new(1.0, 0.001 / std::f64::consts::LN_2, rate),
            buttons: [false; 3],
            active: false,
            single: false,
        }
    }

    fn brake_len(&self) -> f64 {
        brake_frames(self.brake_speed) * self.rate / REF_RATE
    }

    fn brake(&mut self) {
        self.state = State::Brake;
    }

    fn restart(&mut self, ctx: &Context) {
        let n = self.brake_len();
        let missing = (1.0 - self.speed).max(0.0);
        let end = self.lag + missing * missing * n / 2.0;
        let bar = (4.0 * 60.0 / ctx.bpm.max(20.0) * self.rate).round();
        let whole = (end / bar).ceil() * bar;
        if whole < self.history.capacity() as f64 - n {
            self.lag += whole - end;
        }
        self.state = State::Restart;
    }

    fn fade_to_live(&mut self, len: f64) {
        let len = len * self.rate / REF_RATE;
        self.state = State::ToLive { left: len, len };
    }

    fn press(&mut self, ctx: &Context) {
        let [rewind, rock, _] = self.buttons;
        let brake = self.active;
        let state = self.state;
        match state {
            _ if brake => {
                if !matches!(state, State::Brake) {
                    self.brake();
                }
            }
            State::Brake => self.restart(ctx),
            _ if rewind => {
                if !matches!(state, State::Rewind) {
                    self.speed = self.speed.max(0.001);
                    self.state = State::Rewind;
                }
            }
            State::Rewind => self.state = State::RewindEnd,
            _ if rock => {
                if !matches!(state, State::Rock { .. }) {
                    self.state = State::Rock {
                        anchor: self.lag,
                        phase: 0.0,
                    };
                }
            }
            State::Rock { .. } => self.fade_to_live(ROCK_OUT),
            _ => {}
        }
    }

    fn step(&mut self, ctx: &Context) -> (f64, f64) {
        let n = self.brake_len();
        let k = rewind_coef(self.brake_speed) * REF_RATE / self.rate;
        match self.state {
            State::Idle => {
                self.speed = 1.0;
                self.lag = 1.0;
                return (0.0, 1.0);
            }
            State::Brake => self.speed = (self.speed - 1.0 / n).max(0.0),
            State::Restart => {
                self.speed = (self.speed + 1.0 / n).min(1.0);
                if self.speed >= 1.0 {
                    self.fade_to_live(TO_LIVE);
                }
            }
            State::Rewind => {
                self.speed += -64.0 * k * (self.speed - REWIND_SPEED);
            }
            State::RewindEnd => {
                self.speed += 3.0 * k * 64.0;
                if self.speed > 0.0 {
                    self.speed = 1.0;
                    self.fade_to_live(TO_LIVE);
                }
            }
            State::Rock { anchor, phase } => {
                let freq = ROCK_PER_BEAT[(6.0 * self.rock_rate).round() as usize]
                    * ctx.bpm.max(20.0)
                    / 60.0;
                let reach = f64::from(self.amount.tick()) * self.rate / REF_RATE / freq.max(0.001);
                let anchor = anchor + 1.0;
                let phase = phase + freq / self.rate;
                let lag = anchor - reach * swing(phase);
                self.speed = 1.0 - (lag - self.lag);
                self.lag = lag;
                self.state = State::Rock { anchor, phase };
                return (1.0, 1.0);
            }
            State::ToLive { left, len } => {
                let left = left - 1.0;
                if left <= 0.0 {
                    self.state = State::Idle;
                    return (0.0, 1.0);
                }
                self.state = State::ToLive { left, len };
                self.lag += 1.0 - self.speed;
                return (left / len, 1.0);
            }
        }
        self.lag += 1.0 - self.speed;
        let max = self.history.capacity() as f64 - 2.0;
        if self.lag > max {
            self.lag = max;
            return (1.0, 0.0);
        }
        if self.speed <= 0.0 && self.state == State::Brake {
            (1.0, 0.0)
        } else if self.speed < 0.0 {
            (1.0, rewind_gain(self.speed))
        } else {
            (1.0, 1.0)
        }
    }
}

impl Effect for Turntable {
    fn kind(&self) -> EffectKind {
        EffectKind::Turntable
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        self.press(ctx);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for frame in frames {
            self.history.push(*frame);
            let (mix, level) = self.step(ctx);
            self.wet.set(mix as f32);
            self.level.set(level as f32);
            let (wet, level) = (self.wet.tick(), self.level.tick());
            if wet == 0.0 {
                continue;
            }
            let played = self.history.read(self.lag.max(1.0) as f32);
            for ch in 0..2 {
                frame[ch] += (played[ch] * level - frame[ch]) * wet;
            }
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.single = false;
        self.brake_speed = amount;
        self.rock_rate = 1.0;
        self.amount.set(rock_strength(GROUP_AMOUNT) as f32);
        self.buttons = [false; 3];
    }

    fn set_param(&mut self, index: usize, value: f32) {
        self.single = true;
        match index {
            0 => self.brake_speed = value,
            1 => self.rock_rate = value.clamp(0.0, 1.0),
            2 => self.amount.set(rock_strength(value) as f32),
            _ => {}
        }
    }

    fn set_button(&mut self, index: usize, on: bool) {
        self.single = true;
        if let Some(b) = self.buttons.get_mut(index) {
            *b = on;
        }
    }

    fn set_active(&mut self, on: bool) {
        self.active = on;
    }

    fn listens(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.history.clear();
        self.state = State::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    fn turntable() -> Turntable {
        Turntable::new(f64::from(RATE))
    }

    #[test]
    fn knobs_follow_their_curves() {
        assert!((brake_frames(0.0) - 205_000.0).abs() < 1e-9);
        assert!((brake_frames(1.0) - 5_000.0).abs() < 1e-9);
        assert!((rewind_coef(1.0) - 0.000_301).abs() < 1e-9);
        assert!((swing(0.25) - 1.0).abs() < 1e-9 && swing(0.0).abs() < 1e-9);
        assert!((swing(0.75) + 1.0).abs() < 1e-9);
        assert!((rewind_gain(-16.0) - 0.6).abs() < 1e-9);
        assert!((rewind_gain(-1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn group_on_brakes_to_silence_and_off_comes_back() {
        let mut t = turntable();
        t.set_amount(1.0);
        let input = sine(440.0, RATE as usize / 2, 0.5);
        let out = run(&mut t, &input, ctx());
        assert!((rms(&out) - rms(&input)).abs() < 1e-3, "off: untouched");
        t.set_active(true);
        let out = run(&mut t, &input, ctx());
        assert!(rms(&out[2 * 12_000..]) < 1e-3, "stopped");
        t.set_active(false);
        let out = run(&mut t, &input, ctx());
        assert!(rms(&out[2 * 12_000..]) > 0.3, "running again");
    }

    #[test]
    fn rewind_plays_backwards_and_rock_swings_round_a_point() {
        let mut t = turntable();
        t.set_param(0, 0.0);
        let ramp: Vec<f32> = (0..2 * RATE as usize)
            .flat_map(|i| [i as f32 / 96_000.0; 2])
            .collect();
        let _ = run(&mut t, &ramp, ctx());
        t.set_button(0, true);
        let out = run(&mut t, &ramp[..2 * 9_600], ctx());
        assert!(out[2 * 9_500] < out[2 * 9_000], "backwards");

        let mut t = turntable();
        t.set_param(1, 0.0);
        let (before, after) = ramp.split_at(ramp.len() / 2);
        let _ = run(&mut t, before, ctx());
        t.set_button(1, true);
        let out = run(&mut t, after, ctx());
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let (lo, hi) = left[1_000..]
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(lo > 0.4 && hi < 0.6, "stays near the middle: {lo} … {hi}");
    }
}
