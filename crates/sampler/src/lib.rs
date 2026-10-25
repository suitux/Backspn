use std::sync::Arc;

use backspn_deck::{Command as DeckCommand, Deck, Grid, LoopRange, Track, wrap_phase};
use backspn_dsp::Smoothed;
use backspn_sync::grid_multiple;

mod sequencer;
pub use sequencer::{PatternMode, STEPS, Sequencer};

pub const SLOTS: usize = 4;
pub const CELLS: usize = 16;
pub const PAGES: usize = CELLS / 4;
const QUEUE: usize = 8;
pub const QUANTIZE_BEATS: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
const QUANTIZE_TOLERANCE_BEATS: f64 = 1.0 / 16.0;
const FOLLOW_TOLERANCE_S: f64 = 0.0005;
const GAIN_SMOOTH_S: f64 = 0.01;
pub const DEFAULT_BPM: f64 = 120.0;
pub const SIZE_STEPS: usize = 23;
const MIN_RANGE_FACTOR: f32 = 1.0 / 64.0;
const MAX_RANGE_FACTOR: f32 = 64.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayType {
    #[default]
    Loop,
    OneShot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerType {
    Gate,
    #[default]
    Latch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellParams {
    pub play_type: PlayType,
    pub trigger_type: TriggerType,
    pub synced: bool,
    pub reverse: bool,
    pub speed: f64,
    pub pitch: f32,
    pub gain: f32,
    pub offset_beats: f64,
    pub start_beats: f64,
    pub end_beats: f64,
    pub bpm: f64,
}

impl Default for CellParams {
    fn default() -> Self {
        Self {
            play_type: PlayType::Loop,
            trigger_type: TriggerType::Latch,
            synced: true,
            reverse: false,
            speed: 1.0,
            pitch: 0.0,
            gain: 0.5,
            offset_beats: 0.0,
            start_beats: 0.0,
            end_beats: 0.0,
            bpm: DEFAULT_BPM,
        }
    }
}

impl CellParams {
    pub fn linear_gain(&self) -> f32 {
        (4.0 * self.gain.clamp(0.0, 1.0) - 2.0).exp2()
    }

    fn beat_s(&self) -> f64 {
        if self.bpm > 0.0 && self.bpm.is_finite() {
            60.0 / self.bpm
        } else {
            60.0 / DEFAULT_BPM
        }
    }

    pub fn range_s(&self, duration_s: f64) -> (f64, f64) {
        let start = (self.start_beats * self.beat_s()).max(0.0);
        let end = duration_s + self.end_beats * self.beat_s();
        (start, end.max(start))
    }

    fn length_beats(&self, duration_s: f64) -> f32 {
        (duration_s / self.beat_s()) as f32
    }

    pub fn set_size(&mut self, factor: f32, duration_s: f64) -> bool {
        let len = self.length_beats(duration_s);
        let span = factor * len;
        if !(span < MAX_RANGE_FACTOR * len && span > MIN_RANGE_FACTOR * len) {
            return false;
        }
        self.end_beats = f64::from(self.start_beats as f32 + span - len);
        true
    }

    pub fn alter_range(&mut self, end_marker: bool, grow: bool, duration_s: f64) -> bool {
        let len = self.length_beats(duration_s);
        let start = self.start_beats as f32;
        let end_abs = self.end_beats as f32 + len;
        let span = end_abs - start;
        let (new_start, new_end) = match (end_marker, grow) {
            (true, true) => (start, end_abs + span),
            (true, false) => (start, end_abs - span * 0.5),
            (false, true) => (start - span, end_abs),
            (false, false) => (start + span * 0.5, end_abs),
        };
        if new_end < new_start || new_start < 0.0 {
            return false;
        }
        let new_span = new_end - new_start;
        if !(new_span < MAX_RANGE_FACTOR * len && new_span > MIN_RANGE_FACTOR * len) {
            return false;
        }
        self.start_beats = f64::from(new_start);
        self.end_beats = f64::from(new_end - len);
        true
    }
}

pub fn size_factor(step: usize) -> f32 {
    2f32.powi(step.min(SIZE_STEPS - 1) as i32 - 11)
}

#[derive(Debug, Clone)]
pub struct CellAudio {
    pub track: Arc<Track>,
    pub params: CellParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CellState {
    #[default]
    Empty,
    Loaded,
    Playing,
    Queued,
    Played,
    BeforeStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerState {
    #[default]
    Empty,
    Stopped,
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Behaviour {
    pub auto_play: bool,
    pub one_shot_ignores_quantize: bool,
    pub one_shot_ignores_punch: bool,
}

impl Default for Behaviour {
    fn default() -> Self {
        Self {
            auto_play: true,
            one_shot_ignores_quantize: true,
            one_shot_ignores_punch: true,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    LoadCell {
        slot: usize,
        cell: usize,
        audio: CellAudio,
    },
    UnloadCell {
        slot: usize,
        cell: usize,
    },
    SetCellParams {
        slot: usize,
        cell: usize,
        params: CellParams,
    },
    SetActiveCell {
        slot: usize,
        cell: usize,
    },
    Trigger {
        slot: usize,
        cell: usize,
        pressed: bool,
    },
    Stop {
        slot: usize,
        cell: usize,
    },
    TriggerNext {
        slot: usize,
        next: bool,
    },
    Restart {
        slot: usize,
    },
    SetKeylock {
        slot: usize,
        on: bool,
    },
    SetPunch {
        slot: usize,
        on: bool,
    },
    Play(bool),
    SetBaseBpm(f64),
    SetTempo(f64),
    SetQuantize {
        on: bool,
        beats: f64,
    },
    PhaseSync,
    SetBehaviour(Behaviour),
    SetSelectedCell {
        slot: usize,
        cell: usize,
    },
    SequencerOn(bool),
    SequencerRecord(bool),
    SequencerTrigger {
        slot: usize,
    },
    DeletePattern {
        slot: usize,
    },
    SetPattern {
        slot: usize,
        length: usize,
        speed: f64,
        mode: PatternMode,
    },
    SetSwing(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SlotState {
    pub player: PlayerState,
    pub active: usize,
    pub position_s: f64,
    pub keylock: bool,
    pub punch: bool,
    pub cells: [CellState; CELLS],
    pub selected: usize,
    pub steps: u16,
    pub current_step: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct SampleState {
    pub playing: bool,
    pub base_bpm: f64,
    pub bpm: f64,
    pub beat: f64,
    pub quantize_on: bool,
    pub quantize_beats: f64,
    pub synced: bool,
    pub sequencer_on: bool,
    pub recording: bool,
    pub slots: [SlotState; SLOTS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Trigger,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Event {
    action: Action,
    cell: usize,
    beat: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Follow {
    origin_s: f64,
}

struct Slot {
    player: Deck,
    cells: [Option<CellAudio>; CELLS],
    loaded: Option<usize>,
    active: usize,
    last_triggered: Option<usize>,
    held: u16,
    keylock: bool,
    punch: bool,
    queue: [Option<Event>; QUEUE],
    queued: usize,
    follow: Option<Follow>,
    gain: Smoothed,
    sequencer: Sequencer,
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slot")
            .field("loaded", &self.loaded)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl Slot {
    fn new(rate: u32) -> Self {
        Self {
            player: Deck::new(rate),
            cells: std::array::from_fn(|_| None),
            loaded: None,
            active: 0,
            last_triggered: None,
            held: 0,
            keylock: true,
            punch: false,
            queue: [None; QUEUE],
            queued: 0,
            follow: None,
            gain: Smoothed::new(1.0, GAIN_SMOOTH_S, f64::from(rate)),
            sequencer: Sequencer::default(),
        }
    }

    fn push(&mut self, event: Event) {
        if self.queued < QUEUE {
            self.queue[self.queued] = Some(event);
            self.queued += 1;
        }
    }

    fn pop(&mut self) {
        if self.queued > 0 {
            self.queue.copy_within(1..self.queued, 0);
            self.queued -= 1;
            self.queue[self.queued] = None;
        }
    }

    fn is_queued(&self, cell: usize) -> bool {
        self.queue[..self.queued]
            .iter()
            .flatten()
            .any(|e| e.action == Action::Trigger && e.cell == cell)
    }

    fn player_state(&self) -> PlayerState {
        match self.loaded {
            None => PlayerState::Empty,
            Some(_) if self.player.is_playing() => PlayerState::Playing,
            Some(_) => PlayerState::Stopped,
        }
    }

    fn cell_state(&self, cell: usize) -> CellState {
        let Some(audio) = self.cells.get(cell).and_then(Option::as_ref) else {
            return CellState::Empty;
        };
        if self.loaded == Some(cell) && self.player.is_playing() {
            if audio.params.play_type == PlayType::OneShot {
                let (start, end) = audio.params.range_s(audio.track.duration_s());
                let pos = self.player.state().position_s;
                if pos < start {
                    return CellState::BeforeStart;
                }
                if pos > end {
                    return CellState::Played;
                }
            }
            return CellState::Playing;
        }
        if self.is_queued(cell) {
            return CellState::Queued;
        }
        CellState::Loaded
    }

    fn load(&mut self, cell: usize) -> Option<backspn_deck::Material> {
        let audio = self.cells.get(cell).and_then(Option::as_ref)?.clone();
        let old = self
            .player
            .handle(DeckCommand::Load(Arc::clone(&audio.track)));
        self.loaded = Some(cell);
        self.active = cell;
        self.follow = None;
        self.apply_params(&audio);
        old
    }

    fn unload(&mut self) -> Option<backspn_deck::Material> {
        self.loaded = None;
        self.follow = None;
        self.player.handle(DeckCommand::Unload)
    }

    fn apply_params(&mut self, audio: &CellAudio) {
        let p = &audio.params;
        let (start_s, end_s) = p.range_s(audio.track.duration_s());
        self.player
            .handle(DeckCommand::SetGrid(Some(Grid::new(p.bpm, 0.0))));
        self.player.handle(DeckCommand::SetKeylock(self.keylock));
        self.player.handle(DeckCommand::SetKeyAdjust(p.pitch));
        self.player.handle(DeckCommand::SetLoop(Some(LoopRange {
            start_s,
            end_s,
            active: p.play_type == PlayType::Loop,
        })));
        self.gain.set(p.linear_gain());
    }

    fn loaded_audio(&self) -> Option<&CellAudio> {
        self.cells.get(self.loaded?)?.as_ref()
    }

    fn rate(&self, deck_bpm: f64) -> f64 {
        let Some(audio) = self.loaded_audio() else {
            return 1.0;
        };
        let p = &audio.params;
        let factor = if p.synced && p.bpm > 0.0 && deck_bpm > 0.0 {
            deck_bpm / p.bpm
        } else {
            1.0
        };
        p.speed.abs() * factor
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct SampleDeck {
    rate: u32,
    slots: [Slot; SLOTS],
    playing: bool,
    base_bpm: f64,
    bpm_set: bool,
    tempo: f64,
    bend: f64,
    beat: f64,
    time_s: f64,
    quantize_on: bool,
    quantize_beats: f64,
    synced: bool,
    align: bool,
    was_playing: bool,
    behaviour: Behaviour,
    recording: bool,
    random: u32,
}

impl SampleDeck {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            slots: std::array::from_fn(|_| Slot::new(rate)),
            playing: false,
            base_bpm: DEFAULT_BPM,
            bpm_set: false,
            tempo: 1.0,
            bend: 1.0,
            beat: 0.0,
            time_s: 0.0,
            quantize_on: true,
            quantize_beats: 1.0,
            synced: false,
            align: false,
            was_playing: false,
            behaviour: Behaviour::default(),
            recording: false,
            random: 0x9e37_79b9,
        }
    }

    pub fn effective_bpm(&self) -> f64 {
        self.base_bpm * self.tempo * self.bend
    }

    pub fn beat_position(&self) -> f64 {
        self.beat
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn has_audio(&self) -> bool {
        self.slots
            .iter()
            .any(|s| s.cells.iter().any(Option::is_some))
    }

    pub fn set_bend(&mut self, bend: f64) {
        self.bend = bend;
    }

    #[allow(clippy::too_many_lines)]
    pub fn handle(&mut self, command: Command) -> Option<backspn_deck::Material> {
        match command {
            Command::LoadCell { slot, cell, audio } => {
                let s = self.slots.get_mut(slot)?;
                let old_cell = s.cells.get_mut(cell)?.replace(audio.clone());
                if !self.bpm_set
                    && audio.params.bpm > 0.0
                    && audio.params.play_type == PlayType::Loop
                {
                    self.base_bpm = audio.params.bpm;
                    self.bpm_set = true;
                }
                if s.loaded == Some(cell)
                    && let Some(old) = s.load(cell)
                {
                    return Some(old);
                }
                return old_cell.map(|a| backspn_deck::Material::Track(a.track));
            }
            Command::UnloadCell { slot, cell } => {
                let s = self.slots.get_mut(slot)?;
                let old = s.cells.get_mut(cell)?.take();
                if s.loaded == Some(cell) {
                    s.unload();
                }
                return old.map(|a| backspn_deck::Material::Track(a.track));
            }
            Command::SetCellParams { slot, cell, params } => {
                let s = self.slots.get_mut(slot)?;
                let audio = s.cells.get_mut(cell)?.as_mut()?;
                audio.params = params;
                if s.loaded == Some(cell) {
                    let audio = audio.clone();
                    s.apply_params(&audio);
                }
            }
            Command::SetActiveCell { slot, cell } => {
                let s = self.slots.get_mut(slot)?;
                if cell >= CELLS {
                    return None;
                }
                if s.cells[cell].is_some() {
                    s.player.handle(DeckCommand::Pause);
                    return s.load(cell);
                }
                s.active = cell;
            }
            Command::Trigger {
                slot,
                cell,
                pressed,
            } => self.trigger(slot, cell, pressed),
            Command::Stop { slot, cell } => {
                let beat = self.event_beat(slot, self.is_one_shot(slot, cell));
                let s = self.slots.get_mut(slot)?;
                if s.active == cell && s.loaded == Some(cell) {
                    s.push(Event {
                        action: Action::Stop,
                        cell,
                        beat,
                    });
                }
            }
            Command::TriggerNext { slot, next } => {
                let s = self.slots.get(slot)?;
                let cell = if next {
                    (s.active + 1) % CELLS
                } else {
                    s.active
                };
                self.trigger(slot, cell, next);
            }
            Command::Restart { slot } => {
                let s = self.slots.get_mut(slot)?;
                if let Some(audio) = s.loaded_audio() {
                    let (start, end) = audio.params.range_s(audio.track.duration_s());
                    let target = if audio.params.reverse { end } else { start };
                    s.player.handle(DeckCommand::Seek { seconds: target });
                    s.follow = Some(Follow {
                        origin_s: self.time_s,
                    });
                }
            }
            Command::SetKeylock { slot, on } => {
                let s = self.slots.get_mut(slot)?;
                s.keylock = on;
                s.player.handle(DeckCommand::SetKeylock(on));
            }
            Command::SetPunch { slot, on } => self.slots.get_mut(slot)?.punch = on,
            Command::Play(on) => self.playing = on,
            Command::SetBaseBpm(bpm) => {
                if bpm > 0.0 && bpm.is_finite() {
                    self.base_bpm = bpm;
                    self.bpm_set = true;
                }
            }
            Command::SetTempo(tempo) => {
                if tempo > 0.0 && tempo.is_finite() {
                    self.tempo = tempo;
                }
            }
            Command::SetQuantize { on, beats } => {
                self.quantize_on = on;
                if beats > 0.0 && beats.is_finite() {
                    self.quantize_beats = beats;
                }
            }
            Command::PhaseSync => self.align = true,
            Command::SetBehaviour(b) => self.behaviour = b,
            Command::SetSelectedCell { slot, cell } => {
                if cell < CELLS {
                    self.slots.get_mut(slot)?.sequencer.selected = cell;
                }
            }
            Command::SequencerOn(on) => {
                for s in &mut self.slots {
                    s.sequencer.on = on;
                }
            }
            Command::SequencerRecord(on) => self.recording = on,
            Command::SequencerTrigger { slot } => {
                let s = self.slots.get_mut(slot)?;
                if !s.sequencer.on {
                    return None;
                }
                let cell = s.sequencer.selected;
                if self.recording {
                    s.sequencer.record();
                }
                self.trigger(slot, cell, true);
            }
            Command::DeletePattern { slot } => self.slots.get_mut(slot)?.sequencer.delete_pattern(),
            Command::SetPattern {
                slot,
                length,
                speed,
                mode,
            } => {
                let q = &mut self.slots.get_mut(slot)?.sequencer;
                q.length = length.clamp(1, STEPS);
                if speed > 0.0 && speed.is_finite() {
                    q.speed = speed;
                }
                q.mode = mode;
            }
            Command::SetSwing(swing) => {
                for s in &mut self.slots {
                    s.sequencer.swing = swing.clamp(0.0, 1.0);
                }
            }
        }
        None
    }

    fn sequence(&mut self, slot: usize) {
        let beat = self.beat;
        let random = &mut self.random;
        let next = || {
            *random ^= *random << 13;
            *random ^= *random >> 17;
            *random ^= *random << 5;
            *random
        };
        if let Some(cell) = self.slots[slot].sequencer.tick(beat, next) {
            self.trigger(slot, cell, true);
            self.trigger(slot, cell, false);
        }
    }

    fn trigger(&mut self, slot: usize, cell: usize, pressed: bool) {
        let playing = self.playing;
        let beat = self.event_beat(slot, self.is_one_shot(slot, cell));
        let auto_play = self.behaviour.auto_play;
        let Some(s) = self.slots.get_mut(slot) else {
            return;
        };
        if cell >= CELLS {
            return;
        }
        if s.cells[cell].is_none() {
            if pressed && playing {
                s.push(Event {
                    action: Action::Trigger,
                    cell,
                    beat,
                });
            } else if pressed {
                s.active = cell;
            }
            return;
        }
        let bit = 1u16 << cell;
        if pressed {
            s.held |= bit;
            s.push(Event {
                action: Action::Trigger,
                cell,
                beat,
            });
            s.last_triggered = Some(cell);
            if auto_play {
                self.playing = true;
            }
        } else {
            s.held &= !bit;
            let gate = s.cells[cell]
                .as_ref()
                .is_some_and(|a| a.params.trigger_type == TriggerType::Gate);
            if gate && s.last_triggered == Some(cell) {
                s.push(Event {
                    action: Action::Stop,
                    cell,
                    beat,
                });
            }
        }
    }

    fn event_beat(&self, slot: usize, one_shot: bool) -> Option<f64> {
        let sequencing = self.slots.get(slot).is_some_and(|s| s.sequencer.on);
        if !self.quantize_on || sequencing || (one_shot && self.behaviour.one_shot_ignores_quantize)
        {
            return None;
        }
        Some(quantize(self.beat, self.quantize_beats))
    }

    fn is_one_shot(&self, slot: usize, cell: usize) -> bool {
        self.slots
            .get(slot)
            .and_then(|s| s.cells.get(cell))
            .and_then(Option::as_ref)
            .is_some_and(|a| a.params.play_type == PlayType::OneShot)
    }

    pub fn follow_clock(&mut self, synced: bool, clock_bpm: f64, clock_beat: f64, bend: f64) {
        self.bend = bend;
        if synced && !self.synced {
            self.align = true;
        }
        self.synced = synced;
        if !self.synced || self.base_bpm <= 0.0 {
            return;
        }
        let multiple = grid_multiple(clock_bpm, self.base_bpm);
        self.tempo = clock_bpm * multiple / self.base_bpm;
        if !self.playing {
            return;
        }
        if self.align || !self.was_playing {
            let error = wrap_phase(clock_beat * multiple - self.beat);
            self.beat += error;
            self.align = false;
        }
    }

    pub fn process(&mut self, outs: &mut [&mut [f32]; SLOTS]) {
        let frames = outs[0].len() / 2;
        let seconds = frames as f64 / f64::from(self.rate);
        let bpm = self.effective_bpm();
        for slot in 0..SLOTS {
            self.sequence(slot);
            self.run_events(slot);
            self.follow_transport(slot, bpm);
            let playing = self.playing;
            let s = &mut self.slots[slot];
            let out = &mut outs[slot][..2 * frames];
            if !playing {
                out.fill(0.0);
                continue;
            }
            s.player.set_tempo(s.rate(bpm));
            s.player.process(out);
            let (stereo, _) = out.as_chunks_mut::<2>();
            for frame in stereo {
                let g = s.gain.tick();
                frame[0] *= g;
                frame[1] *= g;
            }
        }
        if self.playing {
            self.beat += seconds * bpm / 60.0;
            self.time_s += seconds;
        }
        self.was_playing = self.playing;
    }

    fn run_events(&mut self, slot: usize) {
        let (beat, time_s, bpm, playing) =
            (self.beat, self.time_s, self.effective_bpm(), self.playing);
        let behaviour = self.behaviour;
        let s = &mut self.slots[slot];
        while let Some(event) = s.queue[0] {
            let one_shot = s.cells[event.cell]
                .as_ref()
                .is_some_and(|a| a.params.play_type == PlayType::OneShot);
            let origin_s = match event.beat {
                Some(target) if !(one_shot && behaviour.one_shot_ignores_quantize) => {
                    if target > beat {
                        return;
                    }
                    time_s - (beat - target) * 60.0 / bpm.max(1.0)
                }
                _ => time_s,
            };
            s.pop();
            match event.action {
                Action::Stop => {
                    if s.loaded == Some(event.cell) {
                        s.player.handle(DeckCommand::Pause);
                        s.follow = None;
                    }
                }
                Action::Trigger => {
                    if s.cells[event.cell].is_none() {
                        s.unload();
                        s.active = event.cell;
                        continue;
                    }
                    if s.loaded != Some(event.cell) {
                        drop(s.load(event.cell));
                    }
                    let Some(audio) = s.loaded_audio() else {
                        continue;
                    };
                    let p = audio.params;
                    let duration = audio.track.duration_s();
                    let punch = s.punch
                        && s.player.is_playing()
                        && !(one_shot && behaviour.one_shot_ignores_punch);
                    if !punch {
                        let (start, end) = p.range_s(duration);
                        s.player.handle(DeckCommand::Seek {
                            seconds: if p.reverse { end } else { start },
                        });
                        s.follow = Some(Follow { origin_s });
                    }
                    s.player.handle(DeckCommand::Play);
                    if p.reverse {
                        s.player.handle(DeckCommand::Reverse(true));
                    }
                    let _ = playing;
                }
            }
        }
    }

    fn follow_transport(&mut self, slot: usize, bpm: f64) {
        if !self.playing {
            return;
        }
        let time_s = self.time_s;
        let s = &mut self.slots[slot];
        let (Some(follow), Some(audio)) = (s.follow, s.loaded_audio()) else {
            return;
        };
        if !s.player.is_playing() {
            return;
        }
        let p = audio.params;
        let (start, end) = p.range_s(audio.track.duration_s());
        let rate = s.rate(bpm);
        let elapsed = (time_s - follow.origin_s) * rate + p.offset_beats * p.beat_s();
        let mut expected = if p.reverse {
            end - elapsed
        } else {
            start + elapsed
        };
        if p.play_type == PlayType::Loop && end > start {
            expected = start + (expected - start).rem_euclid(end - start);
        }
        let actual = s.player.state().position_s;
        if (actual - expected).abs() > FOLLOW_TOLERANCE_S {
            s.player.handle(DeckCommand::Seek { seconds: expected });
        }
    }

    pub fn state(&self) -> SampleState {
        SampleState {
            playing: self.playing,
            base_bpm: self.base_bpm,
            bpm: self.effective_bpm(),
            beat: self.beat,
            quantize_on: self.quantize_on,
            quantize_beats: self.quantize_beats,
            synced: self.synced,
            sequencer_on: self.slots[0].sequencer.on,
            recording: self.recording,
            slots: std::array::from_fn(|i| {
                let s = &self.slots[i];
                SlotState {
                    player: s.player_state(),
                    active: s.active,
                    position_s: s.player.state().position_s,
                    keylock: s.keylock,
                    punch: s.punch,
                    cells: std::array::from_fn(|c| s.cell_state(c)),
                    selected: s.sequencer.selected,
                    steps: s.sequencer.steps,
                    current_step: s.sequencer.current,
                }
            }),
        }
    }
}

pub fn quantize(beat: f64, q: f64) -> f64 {
    if q <= 0.0 {
        return beat;
    }
    let tolerance = (QUANTIZE_TOLERANCE_BEATS.min(q) / q).clamp(0.0, 1.0);
    let units = beat / q;
    let snapped = if units <= 0.0 {
        (units + 1.0 - tolerance).floor()
    } else {
        (units - tolerance).ceil()
    };
    snapped * q
}

#[cfg(test)]
mod tests;
