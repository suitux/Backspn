pub const SIZES: [f64; 4] = [4.0, 8.0, 16.0, 32.0];
const EXTRA_FRAMES: usize = 64;
const RING_SECONDS: f64 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Main,
    Cue,
    Ext,
    Aux,
}

impl Source {
    pub const ALL: [Self; 4] = [Self::Main, Self::Cue, Self::Ext, Self::Aux];

    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "MAIN",
            Self::Cue => "CUE",
            Self::Ext => "EXT",
            Self::Aux => "AUX",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Empty,
    Stopped,
    Playing,
    Recording,
    Overdubbing,
}

impl State {
    pub fn code(self) -> u8 {
        match self {
            Self::Empty => 0,
            Self::Stopped => 1,
            Self::Playing => 2,
            Self::Recording => 3,
            Self::Overdubbing => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Record(bool),
    Play(bool),
    Delete,
    Undo,
    Size(usize),
    Source(Source),
    DryWet(f32),
    Decay(f32),
    Latency(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LoopRecorderState {
    pub state: State,
    pub undo: u8,
    pub position: f32,
    pub size: usize,
    pub source: Source,
    pub dry_wet: f32,
    pub frames: usize,
}

impl LoopRecorderState {
    pub fn has_loop(&self) -> bool {
        matches!(
            self.state,
            State::Stopped | State::Playing | State::Overdubbing
        )
    }
}

#[derive(Debug, Default)]
pub struct LoopCopy {
    pub frames: Vec<[f32; 2]>,
    pub bpm: f64,
    pub rate: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Clock {
    pub bpm: f64,
    pub beat: f64,
}

#[derive(Debug)]
pub struct LoopRecorder {
    rate: f64,
    ring: Vec<[f32; 2]>,
    written: usize,
    loop_: Vec<[f32; 2]>,
    undo_: Vec<[f32; 2]>,
    frames: usize,
    beats: f64,
    play_pos: f64,
    take_from: usize,
    taken: usize,
    pub state: LoopRecorderState,
    play_wanted: bool,
    decay: f32,
    latency_ms: f32,
    overdub_wait: usize,
}

impl LoopRecorder {
    pub fn new(rate: u32) -> Self {
        let rate = f64::from(rate);
        let capacity = (RING_SECONDS * 1.5 * rate).ceil() as usize + EXTRA_FRAMES;
        Self {
            rate,
            ring: vec![[0.0; 2]; capacity],
            written: 0,
            loop_: Vec::with_capacity(capacity),
            undo_: Vec::with_capacity(capacity),
            frames: 0,
            beats: 0.0,
            play_pos: 0.0,
            take_from: 0,
            taken: 0,
            state: LoopRecorderState {
                dry_wet: 0.5,
                ..LoopRecorderState::default()
            },
            play_wanted: false,
            decay: 0.0,
            latency_ms: 0.0,
            overdub_wait: 0,
        }
    }

    pub fn handle(&mut self, c: Command, clock: Clock) {
        match c {
            Command::Record(on) => self.record(on, clock),
            Command::Play(on) => {
                self.play_wanted = on;
                if on {
                    self.play(clock);
                } else {
                    if self.state.state == State::Overdubbing {
                        self.stop_recording();
                    }
                    if self.state.state == State::Playing {
                        self.state.state = State::Stopped;
                    }
                }
            }
            Command::Delete => self.clear(),
            Command::Undo => {
                if self.state.undo != 0 {
                    self.stop_recording();
                    std::mem::swap(&mut self.loop_, &mut self.undo_);
                    self.state.undo = 3 - self.state.undo;
                }
            }
            Command::Size(i) if self.state.state == State::Empty && i < SIZES.len() => {
                self.state.size = i;
            }
            Command::Source(s) => self.state.source = s,
            Command::DryWet(p) => self.state.dry_wet = p.clamp(0.0, 1.0),
            Command::Decay(d) => self.decay = d.clamp(0.0, 1.0),
            Command::Latency(ms) => self.latency_ms = ms.max(0.0),
            Command::Size(_) => {}
        }
    }

    fn spb(&self, bpm: f64) -> f64 {
        60.0 / bpm * self.rate
    }

    fn latency(&self) -> usize {
        if self.state.source == Source::Ext {
            (f64::from(self.latency_ms) * 0.001 * self.rate).round() as usize
        } else {
            0
        }
    }

    fn taking(&self) -> bool {
        self.frames > 0 && self.taken < self.frames + EXTRA_FRAMES
    }

    fn record(&mut self, on: bool, clock: Clock) {
        if !on {
            self.stop_recording();
            return;
        }
        match self.state.state {
            State::Empty => {
                let beats = SIZES[self.state.size];
                let frames = (self.spb(clock.bpm) * beats).round() as usize;
                if frames == 0 || frames + EXTRA_FRAMES > self.ring.len() {
                    return;
                }
                let offset = clock.beat - clock.beat.round();
                let shift = (offset * self.spb(clock.bpm)).round() as i64;
                self.take_from = (self.written as i64 - shift).max(0) as usize + self.latency();
                self.taken = 0;
                self.frames = frames;
                self.state.frames = frames;
                self.beats = beats;
                self.loop_.clear();
                self.state.state = State::Recording;
                self.play_wanted = true;
            }
            State::Stopped | State::Playing => {
                self.undo_.clone_from(&self.loop_);
                self.state.undo = 1;
                if self.state.state == State::Stopped {
                    self.play(clock);
                }
                self.overdub_wait = self.latency();
                self.state.state = State::Overdubbing;
            }
            _ => {}
        }
    }

    fn stop_recording(&mut self) {
        match self.state.state {
            State::Overdubbing => {
                self.state.state = if self.play_wanted {
                    State::Playing
                } else {
                    State::Stopped
                };
            }
            State::Recording => self.clear(),
            _ => {}
        }
    }

    fn play(&mut self, clock: Clock) {
        if self.frames == 0 || self.state.state == State::Empty {
            return;
        }
        if self.state.state == State::Stopped {
            self.play_pos = clock.beat.rem_euclid(1.0) / self.beats * self.frames as f64;
            self.state.state = State::Playing;
        }
    }

    fn clear(&mut self) {
        self.frames = 0;
        self.state.frames = 0;
        self.taken = 0;
        self.loop_.clear();
        self.state.state = State::Empty;
        self.state.undo = 0;
        self.play_wanted = false;
    }

    pub fn copy_into(&self, out: &mut LoopCopy) {
        out.frames.clear();
        out.rate = self.rate as u32;
        if !self.state.has_loop() || self.frames == 0 {
            return;
        }
        let take = self.loop_.len().min(out.frames.capacity());
        out.frames.extend_from_slice(&self.loop_[..take]);
        out.bpm = self.beats * 60.0 * self.rate / self.frames as f64;
    }

    fn speed(&self, bpm: f64) -> f64 {
        bpm / 60.0 * self.beats.recip() * self.frames as f64 / self.rate
    }

    fn frame_at(&self, back: f64) -> usize {
        ((self.play_pos - back).rem_euclid(self.frames as f64) as usize).min(self.frames - 1)
    }

    pub fn process(&mut self, source: &[f32], master: &mut [f32], clock: Clock, external: bool) {
        let n = source.len() / 2;
        let cap = self.ring.len();
        for i in 0..n {
            self.ring[(self.written + i) % cap] = [source[2 * i], source[2 * i + 1]];
        }
        self.written += n;
        if self.taking() {
            self.take(self.written - n);
        }
        let playing = matches!(self.state.state, State::Playing | State::Overdubbing);
        if !playing || self.frames == 0 {
            self.state.position = 0.0;
            return;
        }
        let (dry, wet) = gains(self.state.dry_wet);
        let wet = if external {
            2.0 * self.state.dry_wet * self.state.dry_wet
        } else {
            wet
        };
        let speed = self.speed(clock.bpm);
        let overdub = self.state.state == State::Overdubbing;
        let lag = self.latency() as f64 * speed;
        for i in 0..n {
            if overdub {
                if self.overdub_wait > 0 {
                    self.overdub_wait -= 1;
                } else {
                    let at = self.frame_at(lag);
                    if let Some(s) = self.loop_.get_mut(at) {
                        let keep = 1.0 - self.decay;
                        s[0] = s[0] * keep + source[2 * i];
                        s[1] = s[1] * keep + source[2 * i + 1];
                    }
                }
            }
            let s = self
                .loop_
                .get(self.frame_at(0.0))
                .copied()
                .unwrap_or([0.0; 2]);
            master[2 * i] = master[2 * i] * dry + s[0] * wet;
            master[2 * i + 1] = master[2 * i + 1] * dry + s[1] * wet;
            self.play_pos = (self.play_pos + speed).rem_euclid(self.frames as f64);
        }
        self.state.position = (self.play_pos / self.frames as f64) as f32;
    }

    fn take(&mut self, block_start: usize) {
        let cap = self.ring.len();
        let want = self.frames + EXTRA_FRAMES;
        while self.taken < want && self.take_from + self.taken < self.written {
            let f = self.ring[(self.take_from + self.taken) % cap];
            if self.taken < self.frames {
                self.loop_.push(f);
            }
            self.taken += 1;
        }
        if self.state.state == State::Recording && self.taken + self.latency() >= want {
            self.play_pos =
                (block_start as f64 - self.take_from as f64).rem_euclid(self.frames as f64);
            self.state.state = if self.play_wanted {
                State::Playing
            } else {
                State::Stopped
            };
        }
    }
}

pub fn gains(p: f32) -> (f32, f32) {
    backspn_mixer::crossfader_gains(p, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 1000;

    fn clock(beat: f64) -> Clock {
        Clock { bpm: 120.0, beat }
    }

    fn run(r: &mut LoopRecorder, value: f32, blocks: usize, beat: &mut f64) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..blocks {
            let src = vec![value; 200];
            let mut master = vec![0.0; 200];
            r.process(&src, &mut master, clock(*beat), false);
            *beat += 100.0 / 500.0;
            out.extend(master);
        }
        out
    }

    #[test]
    fn a_take_records_four_beats_then_plays_locked_to_the_clock() {
        let mut r = LoopRecorder::new(RATE);
        let mut beat = 0.0;
        run(&mut r, 0.5, 5, &mut beat);
        r.handle(Command::Record(true), clock(beat));
        assert_eq!(r.state.state, State::Recording);
        run(&mut r, 0.5, 21, &mut beat);
        assert_eq!(r.state.state, State::Playing);
        let out = run(&mut r, 0.0, 1, &mut beat);
        assert!((out[0] - 0.5).abs() < 1e-6, "{}", out[0]);
        assert!(r.state.position > 0.0);
    }

    fn ramp(r: &mut LoopRecorder, blocks: usize, t: &mut usize, clock_runs: bool) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..blocks {
            let src: Vec<f32> = (0..200).map(|i| (*t + i / 2) as f32 / 100_000.0).collect();
            let mut master = vec![0.0; 200];
            let beat = if clock_runs { *t as f64 / 500.0 } else { 0.0 };
            r.process(&src, &mut master, clock(beat), false);
            *t += 100;
            out.extend(master.iter().step_by(2));
        }
        out
    }

    #[test]
    fn the_loop_plays_on_in_phase_with_what_it_recorded() {
        let mut r = LoopRecorder::new(RATE);
        let mut t = 0;
        ramp(&mut r, 5, &mut t, true);
        r.handle(Command::DryWet(1.0), clock(t as f64 / 500.0));
        r.handle(Command::Record(true), clock(t as f64 / 500.0));
        let start = t;
        while r.state.state == State::Recording {
            ramp(&mut r, 1, &mut t, true);
        }
        let from = t;
        let out = ramp(&mut r, 30, &mut t, true);
        for (i, &v) in out.iter().enumerate() {
            let k = (from + i - start) % 2000;
            let want = (start + k) as f32 / 100_000.0;
            assert!((v - want).abs() < 1e-6, "frame {i}: {v} vs {want}");
        }
    }

    #[test]
    fn a_clock_standing_still_does_not_freeze_the_loop() {
        let mut r = LoopRecorder::new(RATE);
        let mut t = 0;
        r.handle(Command::DryWet(1.0), clock(0.0));
        r.handle(Command::Record(true), clock(0.0));
        ramp(&mut r, 21, &mut t, false);
        assert_eq!(r.state.state, State::Playing);
        let out = ramp(&mut r, 3, &mut t, false);
        assert!(
            out.windows(2).filter(|w| w[1] < w[0]).count() <= 1,
            "{out:?}"
        );
        assert!(out[299] > out[0]);
    }

    #[test]
    fn with_an_external_mixer_the_loop_joins_fx_return_at_two_p_squared() {
        let mut r = LoopRecorder::new(RATE);
        let mut beat = 0.0;
        r.handle(Command::Record(true), clock(beat));
        run(&mut r, 0.5, 22, &mut beat);
        assert_eq!(r.state.state, State::Playing);
        r.handle(Command::DryWet(1.0), clock(beat));
        let src = vec![0.0; 200];
        let mut out = vec![0.25; 200];
        r.process(&src, &mut out, clock(beat), true);
        assert!((out[0] - 1.0).abs() < 1e-6, "{}", out[0]);
    }

    #[test]
    fn stopping_a_first_take_throws_it_away_and_size_is_fixed_once_recorded() {
        let mut r = LoopRecorder::new(RATE);
        r.handle(Command::Size(2), clock(0.0));
        r.handle(Command::Record(true), clock(0.0));
        r.handle(Command::Record(false), clock(0.1));
        assert_eq!(r.state.state, State::Empty);
        assert_eq!(r.state.size, 2);
    }

    #[test]
    fn overdub_decays_the_loop_and_undo_swaps_it_back() {
        let mut r = LoopRecorder::new(RATE);
        let mut beat = 0.0;
        r.handle(Command::Record(true), clock(beat));
        run(&mut r, 0.5, 22, &mut beat);
        r.handle(Command::Decay(0.5), clock(beat));
        r.handle(Command::Record(true), clock(beat));
        assert_eq!((r.state.state, r.state.undo), (State::Overdubbing, 1));
        run(&mut r, 0.0, 1, &mut beat);
        r.handle(Command::Record(false), clock(beat));
        assert_eq!(r.state.state, State::Playing);
        let at = r.frame_at(100.0);
        assert!((r.loop_[at][0] - 0.25).abs() < 1e-6, "{}", r.loop_[at][0]);
        r.handle(Command::Undo, clock(beat));
        assert_eq!(r.state.undo, 2);
        assert!((r.loop_[at][0] - 0.5).abs() < 1e-6);
        r.handle(Command::Delete, clock(beat));
        assert_eq!((r.state.state, r.state.undo), (State::Empty, 0));
    }

    fn block(r: &mut LoopRecorder, value: f32, beat: &mut f64) {
        run(r, value, 1, beat);
    }

    #[test]
    fn rec_latency_shifts_an_ext_take_but_not_when_it_is_ready() {
        for (source, first) in [(Source::Ext, 0.5), (Source::Main, 1.0)] {
            let mut r = LoopRecorder::new(RATE);
            r.handle(Command::Source(source), clock(0.0));
            r.handle(Command::Latency(100.0), clock(0.0));
            let mut beat = 0.0;
            r.handle(Command::Record(true), clock(beat));
            block(&mut r, 1.0, &mut beat);
            run(&mut r, 0.5, 20, &mut beat);
            assert_eq!(r.state.state, State::Playing, "{source:?}");
            assert!((r.loop_[0][0] - first).abs() < 1e-6, "{source:?}");
        }
    }

    #[test]
    fn rec_latency_overdubs_ext_input_where_it_was_played() {
        let mut r = LoopRecorder::new(RATE);
        r.handle(Command::Source(Source::Ext), clock(0.0));
        let mut beat = 0.0;
        r.handle(Command::Record(true), clock(beat));
        run(&mut r, 0.0, 22, &mut beat);
        r.handle(Command::Latency(100.0), clock(beat));
        let start = r.frame_at(0.0);
        r.handle(Command::Record(true), clock(beat));
        block(&mut r, 9.0, &mut beat);
        block(&mut r, 0.25, &mut beat);
        assert!((r.loop_[start][0] - 0.25).abs() < 1e-6);
        assert!((r.loop_[start + 99][0] - 0.25).abs() < 1e-6);
        assert!(r.loop_.iter().all(|s| s[0] < 1.0));
    }

    #[test]
    fn a_copy_is_the_loop_exactly_at_its_tempo() {
        let mut r = LoopRecorder::new(RATE);
        let mut copy = LoopCopy {
            frames: Vec::with_capacity(4000),
            ..LoopCopy::default()
        };
        r.copy_into(&mut copy);
        assert_eq!(copy.frames.len(), 0);
        let mut beat = 0.0;
        r.handle(Command::Record(true), clock(beat));
        run(&mut r, 0.5, 22, &mut beat);
        assert!(r.state.has_loop());
        r.copy_into(&mut copy);
        assert_eq!((copy.frames.len(), r.state.frames), (2000, 2000));
        assert!((copy.bpm - 120.0).abs() < 1e-9 && copy.rate == RATE);
    }

    #[test]
    fn dry_wet_crossfades_with_a_cosine() {
        assert_eq!(gains(0.0), (1.0, 0.0));
        assert_eq!(gains(0.5), (1.0, 1.0));
        let (dry, wet) = gains(1.0);
        assert!(dry.abs() < 1e-6 && (wet - 1.0).abs() < 1e-6);
        let (_, w) = gains(0.25);
        assert!((w - 0.5).abs() < 1e-6);
    }
}
