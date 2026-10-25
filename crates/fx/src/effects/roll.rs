use backspn_dsp::{DelayLine, Isolator, Smoothed};

use super::echo::{ECHO_BEATS, GROUP_BEATS, HIGH_BAND_HZ, LOW_BAND_HZ, centred};
use super::{EffectKind, beats_per_sample, pick};
use crate::{Context, Effect};

const HISTORY_S: f64 = 12.0;
const MIN_MS: f64 = 10.0;
const MAX_MS: f64 = 4000.0;
const MAX_OFFSET_MS: f64 = 2000.0;
const RAMP_S: f64 = 0.003_333;
const SLIP_S: f64 = 0.000_1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Roll,
    SlipRoll,
    ReverseRoll,
}

pub(crate) fn roll_mix(level: f32) -> (f32, f32) {
    let level = level.clamp(0.0, 1.0);
    if level > 0.5 {
        return (0.0, 1.0);
    }
    let i = (256.0 * level).floor();
    let curve = |x: f32| (1.0 - (x / 127.5).powi(2)).max(0.0);
    (curve(i), curve(128.0 - i))
}

fn offset_beats(v: f32) -> f64 {
    f64::from(((v.clamp(0.0, 1.0) * 9.0) as i32).min(8)) - 4.0
}

fn duty_of(v: f32) -> (f64, f64) {
    let c = f64::from(centred(v));
    if c < 0.0 {
        (1.0 + c, -c)
    } else {
        (1.0 - 0.95 * c, 0.0)
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Roll {
    variant: Variant,
    rate: f64,
    history: DelayLine,
    written: u64,
    loop_bands: Isolator,
    live_bands: Isolator,
    active: bool,
    beats: f64,
    offset: f64,
    duty: (f64, f64),
    lo_cut: bool,
    hi_cut: bool,
    dry: Smoothed,
    wet: Smoothed,
    start: Option<u64>,
    len: f64,
    head: f64,
    next_point: Option<f64>,
    last_point: f64,
    valid: bool,
    gate: f32,
    jumping: bool,
    loop_open: Smoothed,
    live_open: Smoothed,
}

impl Roll {
    pub fn new(variant: Variant, rate: f64) -> Self {
        let ramp = |v| Smoothed::new(v, RAMP_S / 3.0, rate);
        let mut roll = Self {
            variant,
            rate,
            history: DelayLine::new((HISTORY_S * rate) as usize),
            written: 0,
            loop_bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            live_bands: Isolator::new(LOW_BAND_HZ, HIGH_BAND_HZ, rate),
            active: false,
            beats: GROUP_BEATS,
            offset: 0.0,
            duty: (1.0, 0.0),
            lo_cut: false,
            hi_cut: false,
            dry: Smoothed::new(1.0, 0.01, rate),
            wet: Smoothed::new(0.0, 0.01, rate),
            start: None,
            len: 1.0,
            head: 0.0,
            next_point: None,
            last_point: 0.0,
            valid: false,
            gate: 1.0,
            jumping: false,
            loop_open: ramp(1.0),
            live_open: ramp(0.0),
        };
        roll.set_level(0.0);
        roll
    }

    fn set_level(&mut self, level: f32) {
        let (dry, wet) = roll_mix(level);
        self.dry.set(dry);
        self.wet.set(wet);
    }

    fn reverse(&self) -> bool {
        self.variant == Variant::ReverseRoll
    }

    fn lengths(&self, bpm: f64) -> (f64, f64) {
        let beat_ms = 60_000.0 / bpm.max(20.0);
        let len = (self.beats * beat_ms).clamp(MIN_MS, MAX_MS) / 1000.0 * self.rate;
        let offset =
            (self.offset * beat_ms).clamp(-MAX_OFFSET_MS, MAX_OFFSET_MS) / 1000.0 * self.rate;
        (len, offset)
    }

    fn start_at(&self, now: u64, offset: f64) -> u64 {
        let oldest = now.saturating_sub(self.history.capacity() as u64 - 1);
        (now as f64 + offset).max(oldest as f64) as u64
    }

    fn read(&self, at: f64, now: u64, live: [f32; 2]) -> [f32; 2] {
        let at = at.floor();
        if at < 0.0 || at as u64 >= now {
            return live;
        }
        if now - at as u64 >= self.history.capacity() as u64 {
            return [0.0; 2];
        }
        self.history.at(at as usize)
    }

    fn jump(&mut self, now: u64, bpm: f64) {
        let (len, offset) = self.lengths(bpm);
        let Some(start) = self.start else {
            return;
        };
        let start =
            if self.variant == Variant::SlipRoll && (len - self.len).abs() > SLIP_S * self.rate {
                let s = self.start_at(now, offset);
                self.start = Some(s);
                s
            } else {
                start
            };
        self.len = len;
        self.head = start as f64;
        self.valid = true;
    }

    fn frame(&mut self, x: [f32; 2], beat: f64, bpm: f64) -> [f32; 2] {
        self.history.push(x);
        let now = self.written;
        self.written += 1;
        if !self.active {
            return x;
        }
        if self.next_point.is_none() {
            let len_beats = self.beats;
            self.next_point = Some((beat / len_beats).ceil() * len_beats);
            if self.reverse() {
                let (len, offset) = self.lengths(bpm);
                self.start = Some(self.start_at(now, offset));
                self.len = len;
                self.head = self.start.unwrap_or(now) as f64;
                self.valid = true;
                self.last_point = beat;
            }
        }
        let point = self.next_point.unwrap_or(beat);
        if beat >= point {
            self.next_point = Some(point + self.beats);
            self.last_point = point;
            if self.start.is_none() {
                let (len, offset) = self.lengths(bpm);
                self.start = Some(self.start_at(now, offset));
                self.len = len;
            }
            self.jumping = true;
        }
        let step = 1.0 / (RAMP_S * self.rate) as f32;
        if self.jumping {
            self.gate -= step;
            if self.gate <= 0.0 || !self.valid {
                self.gate = 0.0;
                self.jumping = false;
                self.jump(now, bpm);
            }
        } else {
            self.gate = (self.gate + step).min(1.0);
        }
        let roll = self.read(self.head, now, x);
        self.head += if self.reverse() { -1.0 } else { 1.0 };
        let phase = ((beat - self.last_point) / self.beats).clamp(0.0, 1.0);
        let (roll_duty, live_tail) = self.duty;
        self.loop_open
            .set(if phase < roll_duty { 1.0 } else { 0.0 });
        self.live_open
            .set(if phase >= 1.0 - live_tail && live_tail > 0.0 {
                1.0
            } else {
                0.0
            });
        let (open, fill) = (self.loop_open.tick() * self.gate, self.live_open.tick());
        let (dry, wet) = if self.valid {
            (self.dry.tick(), self.wet.tick())
        } else {
            (1.0, 0.0)
        };
        let mut out = x;
        for ch in 0..2 {
            let [rl, rm, rh] = self.loop_bands.split(ch, roll[ch] * open + x[ch] * fill);
            let [ll, _, lh] = self.live_bands.split(ch, x[ch]);
            let low = if self.lo_cut { ll } else { rl };
            let high = if self.hi_cut { lh } else { rh };
            out[ch] = x[ch] * dry + (low + rm + high) * wet;
        }
        out
    }
}

impl Effect for Roll {
    fn kind(&self) -> EffectKind {
        match self.variant {
            Variant::Roll => EffectKind::Roll,
            Variant::SlipRoll => EffectKind::SlipRoll,
            Variant::ReverseRoll => EffectKind::ReverseRoll,
        }
    }

    fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let per_beat = beats_per_sample(ctx, self.rate);
        let (frames, _) = buf.as_chunks_mut::<2>();
        for (i, frame) in frames.iter_mut().enumerate() {
            *frame = self.frame(*frame, ctx.beat + i as f64 * per_beat, ctx.bpm);
        }
    }

    fn set_amount(&mut self, amount: f32) {
        self.beats = GROUP_BEATS;
        self.offset = 0.0;
        self.duty = (1.0, 0.0);
        self.lo_cut = false;
        self.hi_cut = false;
        self.set_level(amount);
    }

    fn set_pad(&mut self, pad: &crate::PadParams) {
        self.set_amount(pad.level);
        self.beats = pad.beats;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.beats = pick(&ECHO_BEATS, value),
            1 => self.duty = duty_of(value),
            2 => self.offset = offset_beats(value),
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
        if on != self.active {
            self.active = on;
            self.start = None;
            self.next_point = None;
            self.valid = false;
            self.jumping = false;
            self.gate = 1.0;
        }
    }

    fn listens(&self) -> bool {
        true
    }

    fn set_dry_wet(&mut self, dry_wet: f32) -> bool {
        self.set_level(dry_wet);
        true
    }

    fn reset(&mut self) {
        self.history.clear();
        self.start = None;
        self.next_point = None;
        self.valid = false;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    fn roll(variant: Variant) -> Roll {
        let mut r = Roll::new(variant, f64::from(RATE));
        r.set_dry_wet(1.0);
        r.set_param(0, 3.0 / 9.0);
        r
    }

    fn ramp(frames: usize) -> Vec<f32> {
        (0..frames).flat_map(|i| [i as f32 / 96_000.0; 2]).collect()
    }

    #[test]
    fn knobs_follow_their_curves() {
        assert_eq!(roll_mix(0.0), (1.0, 0.0));
        assert_eq!(roll_mix(0.75), (0.0, 1.0));
        let (dry, wet) = roll_mix(0.5);
        assert!(
            dry.abs() < 0.01 && (wet - 1.0).abs() < 1e-6,
            "full at the centre"
        );
        assert!((offset_beats(0.0) + 4.0).abs() < 1e-9);
        assert!(offset_beats(0.5).abs() < 1e-9 && (offset_beats(1.0) - 4.0).abs() < 1e-9);
        assert_eq!(duty_of(0.5), (1.0, 0.0));
        let (roll, fill) = duty_of(0.0);
        assert!(roll.abs() < 1e-6 && (fill - 1.0).abs() < 1e-6);
        assert!((duty_of(1.0).0 - 0.05).abs() < 1e-6);
    }

    #[test]
    fn roll_repeats_the_slice_from_the_beat_point() {
        let mut r = roll(Variant::Roll);
        r.set_active(true);
        let input = ramp(RATE as usize);
        let out = run(&mut r, &input, ctx());
        let at = 12_000 + 6_000;
        assert!(
            (out[2 * at] - input[2 * 6_000]).abs() < 0.01,
            "{}",
            out[2 * at]
        );
        let at = 36_000 + 6_000;
        assert!((out[2 * at] - input[2 * 6_000]).abs() < 0.01);
    }

    #[test]
    fn slip_roll_starts_again_from_the_track_when_the_length_changes() {
        let mut r = roll(Variant::SlipRoll);
        r.set_active(true);
        let input = ramp(RATE as usize);
        let _ = run(&mut r, &input[..2 * 24_000], ctx());
        r.set_param(0, 2.0 / 9.0);
        let mut c = ctx();
        c.beat = 1.0;
        let out = run(&mut r, &input[2 * 24_000..], c);
        let at = 6_000 + 3_000;
        assert!(out[2 * at] > 0.2, "from the slipped place: {}", out[2 * at]);
    }

    #[test]
    fn reverse_roll_plays_backwards_at_once() {
        let mut r = roll(Variant::ReverseRoll);
        let input = ramp(RATE as usize);
        let _ = run(&mut r, &input[..2 * 24_000], ctx());
        r.set_active(true);
        let mut c = ctx();
        c.beat = 1.0;
        let out = run(&mut r, &input[2 * 24_000..2 * 36_000], c);
        assert!(out[2 * 6_000] < out[2 * 2_000], "falling");
    }
}
