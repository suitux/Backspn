use std::sync::Arc;

use backspn_dsp::Smoothed;

use crate::Context;

pub const STEPS: usize = 16;
pub const PATTERNS: usize = 8;
const CYCLE: f64 = 4.0;
pub const RESOLUTIONS: [u32; 3] = [4, 8, 16];
pub const MAX_STEPS: usize = 64;
const SILENT: f32 = 1e-4;
const CHOKE: f32 = 0.99;
const DUCK_DEPTH: f32 = 0.683_776;
const LN_1_PERCENT: f32 = -4.605_17;
const DUCK_ATTACK_S: f32 = 0.01;
const JUMP: f64 = 1.1;

#[derive(Debug, Clone)]
pub struct Sound {
    pub name: String,
    pub samples: Arc<[f32]>,
    pub rate: f32,
    pub level: f32,
    pub patterns: [Steps; PATTERNS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Steps {
    pub bits: u64,
    pub per_beat: u32,
}

impl Default for Steps {
    fn default() -> Self {
        Self {
            bits: 0,
            per_beat: RESOLUTIONS[0],
        }
    }
}

impl Steps {
    pub fn cells(self) -> usize {
        CYCLE as usize * self.per_beat as usize
    }

    pub fn on(self, step: usize) -> bool {
        step < self.cells() && self.bits & (1 << step) != 0
    }

    pub fn set(&mut self, step: usize, on: bool) {
        if step < self.cells() {
            if on {
                self.bits |= 1 << step;
            } else {
                self.bits &= !(1 << step);
            }
        }
    }

    #[must_use]
    pub fn rescaled(self, per_beat: u32) -> Self {
        let mut out = Self { bits: 0, per_beat };
        if !RESOLUTIONS.contains(&per_beat) {
            return self;
        }
        for i in (0..self.cells()).filter(|&i| self.on(i)) {
            out.set(i * per_beat as usize / self.per_beat as usize, true);
        }
        out
    }

    pub fn parse(text: &str) -> Option<Self> {
        let cells = text.chars().count();
        let per_beat = RESOLUTIONS
            .into_iter()
            .find(|&r| CYCLE as usize * r as usize == cells)?;
        let mut steps = Self { bits: 0, per_beat };
        for (i, c) in text.chars().enumerate() {
            match c {
                'x' | 'X' => steps.set(i, true),
                '.' | '-' => {}
                _ => return None,
            }
        }
        Some(steps)
    }
}

impl std::fmt::Display for Steps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (0..self.cells()).try_for_each(|i| f.write_str(if self.on(i) { "x" } else { "." }))
    }
}

impl serde::Serialize for Steps {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Steps {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("bad steps {text:?}")))
    }
}

pub fn pattern_index(knob: f32) -> usize {
    ((knob.clamp(0.0, 1.0) * PATTERNS as f32) as usize).min(PATTERNS - 1)
}

pub fn pitch_ratio(knob: f32) -> f32 {
    (2.0 * knob.clamp(0.0, 1.0) - 1.0).exp2()
}

pub fn pitch_semitones(knob: f32) -> f32 {
    (2.0 * knob.clamp(0.0, 1.0) - 1.0) * 12.0
}

fn crosses_points(
    per_beat: f64,
    old: f64,
    new: f64,
    delta: f64,
    active: impl Fn(usize) -> bool,
) -> bool {
    if delta == 0.0 {
        return false;
    }
    let points = CYCLE * per_beat;
    let from = old * per_beat;
    let mut to = new * per_beat;
    let at = |k: f64| active(k.rem_euclid(points) as usize);
    if delta > 0.0 {
        if to < from {
            to += points;
        }
        let mut k = from.ceil();
        while k < to && k < from + points {
            if at(k) {
                return true;
            }
            k += 1.0;
        }
    } else {
        if to > from {
            to -= points;
        }
        let mut k = from.floor();
        while k > to && k > from - points {
            if at(k) {
                return true;
            }
            k -= 1.0;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, Default)]
struct Voice {
    pos: usize,
    frac: f32,
    gain: f32,
}

#[derive(Debug, Clone)]
struct Player {
    samples: Arc<[f32]>,
    level: f32,
    saved: [Steps; PATTERNS],
    live: [Steps; PATTERNS],
    pattern: usize,
    phase: f64,
    voices: [Voice; 2],
    current: usize,
    ratio: f32,
    pitch: f32,
    decay: f32,
    decay_step: f32,
    choke: f32,
}

impl Player {
    fn new(sound: &Sound, rate: f32) -> Self {
        let ratio = sound.rate / rate;
        let mut p = Self {
            samples: Arc::clone(&sound.samples),
            level: sound.level,
            saved: sound.patterns,
            live: sound.patterns,
            pattern: 0,
            phase: 0.0,
            voices: [Voice::default(); 2],
            current: 0,
            ratio,
            pitch: 1.0,
            decay: 1.0,
            decay_step: 1.0,
            choke: CHOKE.powf(ratio),
        };
        p.set_decay(1.0);
        p
    }

    fn set_decay(&mut self, knob: f32) {
        let floor = 0.01 + 0.99 * knob.clamp(0.0, 1.0);
        let len = self.samples.len().max(1) as f32;
        self.decay = floor.powf(8.0 / len);
        self.decay_step = self.decay.powf(self.ratio);
    }

    fn step_rate(&self) -> f32 {
        self.pitch * self.ratio
    }

    fn advance(&mut self, delta: f64, mute: bool, roll: bool) -> bool {
        let old = self.phase;
        self.phase = (old + delta).rem_euclid(CYCLE);
        let steps = self.live[self.pattern];
        let hit = (roll || steps.bits != 0)
            && crosses_points(f64::from(steps.per_beat), old, self.phase, delta, |k| {
                roll || steps.on(k)
            });
        if mute || !hit {
            return false;
        }
        self.current = 1 - self.current;
        self.voices[self.current] = Voice {
            pos: 0,
            frac: 0.0,
            gain: 1.0,
        };
        true
    }

    fn sync_phase(&mut self, beat: f64) {
        let d = beat - self.phase;
        self.phase = (self.phase + d - d.round()).rem_euclid(CYCLE);
    }

    fn process(&mut self) -> f32 {
        let rate = self.step_rate();
        let mut out = 0.0;
        for (i, v) in self.voices.iter_mut().enumerate() {
            if v.gain < SILENT || v.pos + 1 >= self.samples.len() {
                continue;
            }
            let a = self.samples[v.pos];
            let b = self.samples[v.pos + 1];
            out += (a + (b - a) * v.frac) * v.gain;
            let next = v.frac + rate;
            v.pos += next.floor() as usize;
            v.frac = next.fract();
            v.gain *= self.decay_step;
            if i != self.current {
                v.gain *= self.choke;
            }
        }
        out * self.level
    }

    fn silent(&self) -> bool {
        self.voices
            .iter()
            .all(|v| v.gain < SILENT || v.pos + 1 >= self.samples.len())
    }

    fn step(&self) -> usize {
        let steps = self.live[self.pattern];
        (self.phase * f64::from(steps.per_beat)) as usize % steps.cells()
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct DrumMachine {
    players: Vec<Player>,
    active: usize,
    on: bool,
    restart: bool,
    duck: bool,
    roll: bool,
    rate: f64,
    volume: Smoothed,
    depth: Smoothed,
    env: f32,
    env2: f32,
    release: f32,
    attack2: f32,
    last_delta: f64,
    jumped: bool,
    last_block: Option<u64>,
}

impl DrumMachine {
    pub fn new(sounds: &[Sound], rate: u32) -> Self {
        let rate = f64::from(rate);
        let smooth = |v| Smoothed::new(v, 0.01 / 20f64.ln(), rate);
        let mut p = Self {
            players: sounds.iter().map(|s| Player::new(s, rate as f32)).collect(),
            active: 0,
            on: false,
            restart: false,
            duck: false,
            roll: false,
            rate,
            volume: smooth(0.5),
            depth: smooth(0.5f32.sqrt()),
            env: 0.0,
            env2: 0.0,
            release: 0.0,
            attack2: env_coef(DUCK_ATTACK_S, rate),
            last_delta: 0.0,
            jumped: false,
            last_block: None,
        };
        p.set_decay(1.0);
        p
    }

    pub fn sounds(&self) -> usize {
        self.players.len()
    }

    pub fn set_on(&mut self, on: bool) {
        if on && !self.on {
            self.restart = true;
        }
        self.on = on;
    }

    pub fn set_volume(&mut self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        self.volume.set(v);
        self.depth.set(v.sqrt());
    }

    pub fn set_decay(&mut self, knob: f32) {
        let knob = knob.clamp(0.0, 1.0);
        for p in &mut self.players {
            p.set_decay(knob);
        }
        self.release = env_coef(0.2 + 0.3 * knob.powi(3), self.rate);
    }

    pub fn set_pitch(&mut self, knob: f32) {
        let ratio = pitch_ratio(knob);
        for p in &mut self.players {
            p.pitch = ratio;
        }
    }

    pub fn set_pattern(&mut self, knob: f32) {
        let index = pattern_index(knob);
        for p in &mut self.players {
            p.pattern = index;
        }
    }

    pub fn set_duck(&mut self, on: bool) {
        self.duck = on;
    }

    pub fn set_roll(&mut self, on: bool) {
        self.roll = on;
    }

    pub fn set_sound(&mut self, index: usize) {
        if index < self.players.len() {
            self.active = index;
        }
    }

    fn current(&mut self) -> Option<&mut Player> {
        self.players.get_mut(self.active)
    }

    pub fn set_step(&mut self, step: usize, on: bool) {
        if let Some(p) = self.current() {
            p.live[p.pattern].set(step, on);
        }
    }

    pub fn set_steps(&mut self, sound: usize, pattern: usize, steps: Steps) {
        if let Some(p) = self.players.get_mut(sound)
            && pattern < PATTERNS
            && steps.per_beat == p.saved[pattern].per_beat
        {
            p.live[pattern] = steps;
        }
    }

    pub fn reset_pattern(&mut self) {
        if let Some(p) = self.current() {
            p.live[p.pattern] = p.saved[p.pattern];
        }
    }

    pub fn clear_pattern(&mut self) {
        if let Some(p) = self.current() {
            p.live[p.pattern].bits = 0;
        }
    }

    pub fn step(&self) -> Option<usize> {
        self.players.get(self.active).map(Player::step)
    }

    pub(crate) fn on(&self) -> bool {
        self.on
    }

    pub(crate) fn running(&self, block: u64) -> bool {
        self.on && self.last_block.is_some_and(|b| b + 1 >= block)
    }

    pub(crate) fn cycle_at(&self, block: u64) -> f64 {
        let phase = self.players.first().map_or(0.0, |p| p.phase);
        if self.last_block == Some(block) {
            (phase - self.last_delta).rem_euclid(CYCLE)
        } else {
            phase
        }
    }

    pub(crate) fn align(&mut self, beat: f64, cycle: Option<f64>) {
        let fraction = beat - beat.round();
        let phase = cycle.map_or(fraction, |c| fraction + (c - fraction).round());
        for p in &mut self.players {
            p.phase = phase.rem_euclid(CYCLE);
        }
        self.restart = false;
    }

    pub(crate) fn process_in(&mut self, block: u64, buf: &mut [f32], ctx: &Context) {
        self.last_block = Some(block);
        self.process(buf, ctx);
    }

    pub fn idle(&self) -> bool {
        !self.on && self.env2 <= 0.0 && self.players.iter().all(Player::silent)
    }

    pub fn process(&mut self, buf: &mut [f32], ctx: &Context) {
        let frames = buf.len() / 2;
        if frames == 0 {
            return;
        }
        let expected = ctx.bpm / 60.0 * frames as f64 / self.rate;
        let observed = ctx.beat_end - ctx.beat;
        let jumped = expected > 0.0 && observed.abs() > JUMP * expected;
        let mut delta = observed;
        if jumped && !self.jumped {
            delta = expected.copysign(self.last_delta);
            for p in &mut self.players {
                p.sync_phase(ctx.beat_end - delta);
            }
        }
        let mute_all = !self.on || delta == 0.0;
        let per_frame = delta / frames as f64;
        let (out, _) = buf.as_chunks_mut::<2>();
        for o in out {
            let mut wet = 0.0;
            let mut hit = false;
            for (i, p) in self.players.iter_mut().enumerate() {
                let active = i == self.active;
                hit |= p.advance(per_frame, mute_all || !active, self.roll && active);
                wet += p.process();
            }
            let depth = self.depth.tick();
            let volume = self.volume.tick();
            let push = if self.duck && hit {
                depth * DUCK_DEPTH
            } else {
                0.0
            };
            if push > self.env {
                self.env = push;
            } else {
                self.env += (push - self.env) * self.release;
            }
            if self.env > self.env2 {
                self.env2 += (self.env - self.env2) * self.attack2;
            } else {
                self.env2 = self.env;
            }
            let dry = (1.0 - self.env2).max(0.0);
            let wet = wet * volume;
            o[0] = o[0] * dry + wet;
            o[1] = o[1] * dry + wet;
        }
        if self.restart {
            self.restart = false;
            let phase = ctx.beat_end - (ctx.beat_end + 0.5).floor();
            for p in &mut self.players {
                p.phase = phase.rem_euclid(CYCLE);
            }
        }
        self.last_delta = delta;
        self.jumped = jumped;
    }
}

fn env_coef(time_s: f32, rate: f64) -> f32 {
    if time_s <= 0.0 {
        return 1.0;
    }
    1.0 - (LN_1_PERCENT / (time_s * rate as f32)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn click(len: usize) -> Arc<[f32]> {
        (0..len).map(|i| if i < 4 { 1.0 } else { 0.5 }).collect()
    }

    fn steps(bits: u16) -> Steps {
        Steps {
            bits: u64::from(bits),
            per_beat: 4,
        }
    }

    fn crosses(mask: u64, old: f64, new: f64, delta: f64) -> bool {
        crosses_points(4.0, old, new, delta, |k| mask & (1 << k) != 0)
    }

    fn sound(patterns: [u16; PATTERNS]) -> Sound {
        Sound {
            name: "click".into(),
            samples: click(4800),
            rate: RATE as f32,
            level: 1.0,
            patterns: patterns.map(steps),
        }
    }

    fn onsets(p: &mut DrumMachine, beat: f64, blocks: usize) -> Vec<usize> {
        let frames = 480;
        let per_block = 2.0 * frames as f64 / f64::from(RATE);
        let mut out = Vec::new();
        let mut last = 0.0f32;
        for b in 0..blocks {
            let start = beat + b as f64 * per_block;
            let ctx = Context {
                bpm: 120.0,
                beat: start,
                beat_end: start + per_block,
            };
            let mut buf = vec![0.0; 2 * frames];
            p.process(&mut buf, &ctx);
            for (i, f) in buf.chunks(2).enumerate() {
                if f[0] > last + 0.4 {
                    out.push(b * frames + i);
                }
                last = f[0];
            }
        }
        out
    }

    #[test]
    fn knob_scales() {
        assert_eq!(pattern_index(0.0), 0);
        assert_eq!(pattern_index(0.124), 0);
        assert_eq!(pattern_index(0.125), 1);
        assert_eq!(pattern_index(1.0), 7);
        assert!((pitch_ratio(0.5) - 1.0).abs() < 1e-6);
        assert!((pitch_ratio(1.0) - 2.0).abs() < 1e-6);
        assert!((pitch_semitones(0.0) + 12.0).abs() < 1e-6);
    }

    #[test]
    fn steps_are_crossed_in_either_direction() {
        let four = 0b0001_0001_0001_0001;
        assert!(crosses(four, 0.0, 0.01, 0.01), "a step at the start counts");
        assert!(!crosses(four, 0.01, 0.02, 0.01));
        assert!(crosses(four, 0.99, 1.01, 0.02));
        assert!(crosses(four, 3.99, 0.01, 0.02), "across the wrap");
        assert!(crosses(four, 1.01, 0.99, -0.02), "backwards");
        assert!(!crosses(four, 0.5, 0.5, 0.0), "standing still");
        assert!(!crosses(0b10, 0.0, 0.2, 0.2), "step 1 sits at 0.25");
    }

    #[test]
    fn four_on_the_floor_hits_each_beat_once_on() {
        let mut p = DrumMachine::new(&[sound([0b0001_0001_0001_0001; PATTERNS])], RATE);
        p.set_volume(1.0);
        assert!(onsets(&mut p, 0.0, 100).is_empty(), "off: silent");
        p.set_on(true);
        let hits = onsets(&mut p, 0.0, 200);
        let beats: Vec<_> = hits.iter().filter(|&&h| h > 0).collect();
        assert_eq!(beats.len(), 4, "{hits:?}");
        for (n, h) in beats.into_iter().enumerate() {
            assert!(h.abs_diff((n + 1) * 24_000) <= 1, "{hits:?}");
        }
    }

    #[test]
    fn only_the_selected_sound_plays_and_patterns_switch_together() {
        let a = sound([0b0001; PATTERNS]);
        let mut b = sound([0; PATTERNS]);
        b.patterns[1] = steps(0b0001_0001_0001_0001);
        let mut p = DrumMachine::new(&[a, b], RATE);
        p.set_volume(1.0);
        p.set_on(true);
        p.set_sound(1);
        assert!(
            onsets(&mut p, 0.0, 200).is_empty(),
            "sound 2, pattern 1 empty"
        );
        p.set_pattern(0.2);
        assert_eq!(onsets(&mut p, 0.0, 100).len(), 2, "pattern 2 of sound 2");
    }

    #[test]
    fn roll_repeats_the_playing_sound_on_its_grid() {
        let mut p = DrumMachine::new(&[sound([0b1; PATTERNS])], RATE);
        p.set_volume(1.0);
        p.set_on(true);
        onsets(&mut p, 0.0, 1);
        p.set_roll(true);
        let hits: Vec<_> = onsets(&mut p, 0.02, 100)
            .into_iter()
            .filter(|&h| h > 0)
            .collect();
        assert_eq!(hits.len(), 8, "{hits:?}");
        for w in hits.windows(2) {
            assert!(w[1].abs_diff(w[0] + 6_000) <= 1, "{hits:?}");
        }
        p.set_roll(false);
        let after = onsets(&mut p, 2.02, 50);
        assert!(
            after.iter().all(|&h| h == 0),
            "pattern again: no hit {after:?}"
        );
        let mut eighths = sound([0b1; PATTERNS]);
        eighths.patterns[0] = Steps {
            bits: 1,
            per_beat: 8,
        };
        let mut p = DrumMachine::new(&[eighths], RATE);
        p.set_on(true);
        p.set_volume(1.0);
        onsets(&mut p, 0.0, 1);
        p.set_roll(true);
        let hits: Vec<_> = onsets(&mut p, 0.02, 50)
            .into_iter()
            .filter(|&h| h > 0)
            .collect();
        assert_eq!(hits.len(), 8, "one beat at 1/8: {hits:?}");
    }

    #[test]
    fn steps_text_and_resolutions() {
        let s = Steps::parse("x...x...x...x..X").unwrap();
        assert_eq!((s.bits, s.per_beat), (0b1001_0001_0001_0001, 4));
        assert_eq!(s.to_string(), "x...x...x...x..x");
        assert_eq!(Steps::parse(&"x.".repeat(16)).unwrap().per_beat, 8);
        assert_eq!(Steps::parse(&".".repeat(64)).unwrap().per_beat, 16);
        assert!(Steps::parse("x...").is_none());
        assert!(Steps::parse("x...x...x...x..?").is_none());
        let finer = s.rescaled(8);
        assert_eq!(finer.to_string(), "x.......x.......x.......x.....x.");
        assert_eq!(finer.rescaled(4), s, "back again");
        let json = serde_json::to_string(&finer).unwrap();
        assert_eq!(serde_json::from_str::<Steps>(&json).unwrap(), finer);
    }

    #[test]
    fn live_steps_reset_and_clear() {
        let mut p = DrumMachine::new(&[sound([0b1; PATTERNS])], RATE);
        p.set_step(4, true);
        assert_eq!(p.players[0].live[0].bits, 0b1_0001);
        p.set_step(0, false);
        assert_eq!(p.players[0].live[0].bits, 0b1_0000);
        p.reset_pattern();
        assert_eq!(p.players[0].live[0].bits, 0b1);
        p.clear_pattern();
        assert_eq!(p.players[0].live[0].bits, 0);
        assert_eq!(p.players[0].live[1].bits, 0b1, "other patterns untouched");
    }

    #[test]
    fn a_stopped_beat_triggers_nothing() {
        let mut p = DrumMachine::new(&[sound([u16::MAX; PATTERNS])], RATE);
        p.set_on(true);
        let ctx = Context {
            bpm: 120.0,
            beat: 1.0,
            beat_end: 1.0,
        };
        let mut buf = vec![0.0; 960];
        for _ in 0..50 {
            p.process(&mut buf, &ctx);
        }
        assert!(buf.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn a_jump_keeps_the_cycle_and_takes_the_beat_fraction() {
        let mut p = DrumMachine::new(&[sound([0; PATTERNS])], RATE);
        p.players[0].phase = 1.0;
        let ctx = Context {
            bpm: 120.0,
            beat: 10.0,
            beat_end: 30.3,
        };
        p.last_delta = 0.02;
        p.process(&mut [0.0; 960], &ctx);
        assert!(
            (p.players[0].phase - 1.3).abs() < 1e-9,
            "{}",
            p.players[0].phase
        );
    }

    #[test]
    fn decay_shortens_a_hit() {
        let mut short = Player::new(&sound([1; PATTERNS]), RATE as f32);
        short.set_decay(0.0);
        let mut long = short.clone();
        long.set_decay(1.0);
        for p in [&mut short, &mut long] {
            p.advance(0.01, false, false);
        }
        let tail = |p: &mut Player| (0..2400).map(|_| p.process()).last().unwrap();
        assert!(tail(&mut short) < 1e-3);
        assert!((tail(&mut long) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn duck_dips_the_input_on_each_hit() {
        let mut silent = sound([u16::MAX; PATTERNS]);
        silent.samples = vec![0.0; 4800].into();
        let mut p = DrumMachine::new(&[silent], RATE);
        p.set_volume(1.0);
        p.set_duck(true);
        let mut lowest = 1.0f32;
        let mut beat = 0.0;
        for block in 0..40 {
            if block == 20 {
                p.set_on(true);
            }
            let ctx = Context {
                bpm: 120.0,
                beat,
                beat_end: beat + 0.02,
            };
            beat += 0.02;
            let mut buf = vec![1.0; 960];
            p.process(&mut buf, &ctx);
            lowest = lowest.min(buf.iter().copied().fold(1.0, f32::min));
        }
        assert!(
            (lowest - (1.0 - DUCK_DEPTH)).abs() < 0.05,
            "ducked to {lowest}"
        );
    }
}
