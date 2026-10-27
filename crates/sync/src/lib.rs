pub mod midi_clock;
pub mod midi_clock_in;

use backspn_deck::{Deck, wrap_phase};

pub const MAX_DECKS: usize = 4;
pub const MIN_CLOCK_BPM: f64 = 40.0;
pub const MAX_CLOCK_BPM: f64 = 300.0;
pub const DEFAULT_CLOCK_BPM: f64 = 120.0;
const MAX_CORRECTION: f64 = 0.01;
const CORRECTION_TIME_S: f64 = 1.0;
const JUMP_THRESHOLD_BEATS: f64 = 0.25;
const PHASE_TOLERANCE_BEATS: f64 = 0.001;

const LINK_PHASE_EPSILON: f64 = 1e-4;

const LEAD_BPM: std::ops::RangeInclusive<f64> = 40.0..=300.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    SetSync(usize, bool),
    SetAutoMaster(bool),
    SetMaster(Option<usize>),
    SetOnAirOnly(bool),
    SetSampleDecksLead(bool),
    TempoSync(usize),
    PhaseSync(usize),
    SetClockBpm(f64),
    Bend(usize, f64),
    ReleasePhase(usize),
    SetExternal(bool),
    SetLink(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct SyncState {
    pub auto: bool,
    pub master: Option<usize>,
    pub synced: [bool; MAX_DECKS],
    pub clock_bpm: f64,
    pub clock_beat: f64,
    pub phase: [f32; MAX_DECKS],
    pub phase_locked: [bool; MAX_DECKS],
    pub external: bool,
    pub external_playing: bool,
    pub link: bool,
}

#[derive(Debug, Clone)]
pub struct Sync {
    state: SyncState,
    was_playing: [bool; MAX_DECKS],
    needs_alignment: [bool; MAX_DECKS],
    loaded: [(u64, bool); MAX_DECKS],
    on_air_only: bool,
    sample_decks_lead: bool,
    pending_tempo: [bool; MAX_DECKS],
    pending_phase: [bool; MAX_DECKS],
    bend: [f64; MAX_DECKS],
    clock_master: Option<usize>,
    follower: midi_clock_in::Follower,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lead {
    pub loaded: bool,
    pub playing: bool,
    pub bpm: f64,
    pub beat: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Candidate {
    loaded: bool,
    playing: bool,
    bpm: Option<f64>,
    beat: Option<f64>,
    away: bool,
}

impl Candidate {
    fn of(deck: &Deck, lead: Option<&Lead>) -> Self {
        match lead {
            Some(l) => Self {
                loaded: l.loaded,
                playing: l.playing,
                bpm: Some(l.bpm).filter(|b| *b > 0.0),
                beat: Some(l.beat),
                away: false,
            },
            None => Self {
                loaded: deck.state().loaded,
                playing: deck.is_playing(),
                bpm: deck.state().effective_bpm(),
                beat: deck.beat_position(),
                away: deck.state().reverse || deck.state().scratching,
            },
        }
    }
}

impl Default for Sync {
    fn default() -> Self {
        Self {
            state: SyncState {
                auto: true,
                clock_bpm: DEFAULT_CLOCK_BPM,
                ..SyncState::default()
            },
            was_playing: [false; MAX_DECKS],
            needs_alignment: [false; MAX_DECKS],
            loaded: [(0, false); MAX_DECKS],
            on_air_only: false,
            sample_decks_lead: true,
            pending_tempo: [false; MAX_DECKS],
            pending_phase: [false; MAX_DECKS],
            bend: [1.0; MAX_DECKS],
            clock_master: None,
            follower: midi_clock_in::Follower::default(),
        }
    }
}

impl Sync {
    pub fn handle(&mut self, command: Command) {
        match command {
            Command::SetSync(deck, on) if deck < MAX_DECKS => {
                self.state.synced[deck] = on;
                self.state.phase_locked[deck] = on;
                self.needs_alignment[deck] = on;
            }
            Command::Bend(deck, factor)
                if deck < MAX_DECKS && factor.is_finite() && factor > 0.0 =>
            {
                self.bend[deck] = factor;
                if (factor - 1.0).abs() > f64::EPSILON {
                    self.state.phase_locked[deck] = false;
                }
            }
            Command::ReleasePhase(deck) if deck < MAX_DECKS => {
                self.state.phase_locked[deck] = false;
            }
            Command::SetAutoMaster(on) => self.state.auto = on,
            Command::SetMaster(master) => {
                self.state.master = master.filter(|&i| i < MAX_DECKS);
                if let Some(i) = self.state.master {
                    self.state.synced[i] = false;
                }
            }
            Command::SetOnAirOnly(on) => self.on_air_only = on,
            Command::SetSampleDecksLead(on) => self.sample_decks_lead = on,
            Command::TempoSync(deck) if deck < MAX_DECKS => self.pending_tempo[deck] = true,
            Command::PhaseSync(deck) if deck < MAX_DECKS => self.pending_phase[deck] = true,
            Command::SetClockBpm(bpm) if bpm.is_finite() && bpm > 0.0 && !self.state.external => {
                self.state.clock_bpm = bpm.clamp(MIN_CLOCK_BPM, MAX_CLOCK_BPM);
            }
            Command::SetExternal(on) => {
                self.state.external = on;
                self.follower = midi_clock_in::Follower::default();
                if on {
                    self.state.master = None;
                }
            }
            Command::SetLink(on) => {
                self.state.link = on;
                if on {
                    self.state.master = None;
                }
            }
            _ => {}
        }
    }

    pub fn follow_external(&mut self, tempo: Option<f64>, beat: Option<f64>) {
        self.state.external_playing = beat.is_some();
        if !self.state.external {
            return;
        }
        if let (Some(tempo), Some(beat)) = (tempo, beat) {
            let phase = wrap_phase(self.state.clock_beat - beat);
            self.state.clock_bpm = self.follower.step(self.state.clock_bpm, tempo, phase);
        }
    }

    pub fn follow_link(&mut self, phase: f64) {
        if !self.state.link {
            return;
        }
        let delta = phase - self.state.clock_beat.rem_euclid(1.0);
        if delta > LINK_PHASE_EPSILON {
            self.state.clock_beat += delta;
        }
    }

    pub fn state(&self) -> SyncState {
        self.state
    }

    pub fn bend(&self, i: usize) -> f64 {
        self.bend.get(i).copied().unwrap_or(1.0)
    }

    pub fn update(&mut self, decks: &mut [Deck], on_air: &[bool], frames: usize, rate: u32) {
        self.update_with(decks, &[], on_air, frames, rate);
    }

    pub fn update_with(
        &mut self,
        decks: &mut [Deck],
        leads: &[Option<Lead>],
        on_air: &[bool],
        frames: usize,
        rate: u32,
    ) {
        let n = decks.len().min(MAX_DECKS);
        let mut candidates = [Candidate::default(); MAX_DECKS];
        for (i, c) in candidates.iter_mut().enumerate().take(n) {
            *c = match leads.get(i).and_then(Option::as_ref) {
                Some(_) if !self.sample_decks_lead => Candidate::default(),
                lead => Candidate::of(&decks[i], lead),
            };
        }
        let candidates = &candidates[..n];
        self.state.master = if self.state.external || self.state.link {
            None
        } else {
            self.check_master(candidates, on_air)
        };
        self.advance_clock(candidates, frames, rate);
        let reference = self.phase_reference(candidates);
        for (i, deck) in decks[..n].iter_mut().enumerate() {
            let tempo_sync = std::mem::take(&mut self.pending_tempo[i]);
            let phase_sync = std::mem::take(&mut self.pending_phase[i]);
            self.notice_new_track(i, deck);
            if Some(i) != self.state.master {
                self.sync_once(deck, tempo_sync, phase_sync && reference);
            }
            if self.state.synced[i] && Some(i) != self.state.master {
                let phase_lock = reference && self.state.phase_locked[i];
                self.follow(i, deck, phase_lock);
            }
            self.state.phase[i] = if reference && deck.is_playing() && Some(i) != self.state.master
            {
                self.phase_of(deck).unwrap_or(0.0) as f32
            } else {
                0.0
            };
            self.was_playing[i] = candidates[i].playing;
        }
    }

    fn notice_new_track(&mut self, i: usize, deck: &Deck) {
        let state = deck.state();
        let loaded = (state.generation, state.grid.is_some());
        let before = std::mem::replace(&mut self.loaded[i], loaded);
        if before == loaded {
            return;
        }
        if before.0 != loaded.0 {
            self.state.phase_locked[i] = self.state.synced[i];
        }
        self.needs_alignment[i] = self.state.synced[i];
    }

    fn phase_reference(&self, candidates: &[Candidate]) -> bool {
        self.state.master.is_none_or(|m| candidates[m].playing)
    }

    fn phase_of(&self, deck: &Deck) -> Option<f64> {
        let grid = deck.state().grid.filter(|_| deck.state().loaded)?;
        let beat = deck.beat_position()?;
        let bpm = 60.0 / grid.period_at(deck.state().position_s);
        let multiple = grid_multiple(self.state.clock_bpm, bpm);
        Some(wrap_phase(beat - self.state.clock_beat * multiple))
    }

    fn can_lead(&self, i: usize, candidates: &[Candidate], on_air: &[bool]) -> bool {
        let c = &candidates[i];
        c.loaded
            && c.playing
            && (!self.on_air_only || on_air.get(i).copied().unwrap_or(true))
            && c.bpm.is_some_and(|bpm| LEAD_BPM.contains(&bpm))
    }

    fn check_master(&self, candidates: &[Candidate], on_air: &[bool]) -> Option<usize> {
        if let Some(m) = self.state.master.filter(|&m| m < candidates.len()) {
            let c = &candidates[m];
            let running = c.loaded && c.playing;
            if (!self.state.auto && running) || self.can_lead(m, candidates, on_air) {
                return Some(m);
            }
        }
        if self.state.auto {
            self.closest_deck(candidates, on_air).or_else(|| {
                self.state
                    .master
                    .filter(|&m| m < candidates.len())
                    .filter(|&m| candidates[m].loaded && !candidates[m].playing)
            })
        } else {
            None
        }
    }

    fn closest_deck(&self, candidates: &[Candidate], on_air: &[bool]) -> Option<usize> {
        let clock_phase = self.state.clock_beat;
        let mut best = None;
        let (mut best_tempo, mut best_phase) = (*LEAD_BPM.end(), f64::MAX);
        for i in 0..candidates.len() {
            if !self.can_lead(i, candidates, on_air) {
                continue;
            }
            let Some(bpm) = candidates[i].bpm else {
                continue;
            };
            let tempo = (bpm - self.state.clock_bpm).abs();
            if tempo > best_tempo {
                continue;
            }
            if tempo < best_tempo {
                best_phase = f64::MAX;
            }
            best_tempo = tempo;
            let beat = candidates[i].beat.unwrap_or(0.0);
            let phase = wrap_phase(beat - clock_phase).abs();
            if phase <= best_phase {
                best_phase = phase;
                best = Some(i);
            }
        }
        best
    }

    fn advance_clock(&mut self, candidates: &[Candidate], frames: usize, rate: u32) {
        let lead = self.state.master.and_then(|i| {
            let c = &candidates[i];
            Some((c.bpm?, c.beat?, c.playing))
        });
        let away = self.state.master.is_some_and(|i| candidates[i].away);
        let lead = match lead {
            Some((bpm, _, true)) if away => {
                self.state.clock_bpm = bpm;
                None
            }
            lead => lead,
        };
        let master = self.state.master;
        let realign = master.is_some_and(|m| self.clock_master != Some(m) || !self.was_playing[m]);
        self.clock_master = master;
        let seconds = frames as f64 / f64::from(rate);
        match lead {
            Some((bpm, beat, playing)) => {
                self.state.clock_bpm = bpm;
                if realign || !playing {
                    self.state.clock_beat = beat;
                } else {
                    self.state.clock_beat += seconds * bpm / 60.0;
                }
            }
            None => {
                self.state.clock_beat += seconds * self.state.clock_bpm / 60.0;
            }
        }
    }

    fn sync_once(&self, deck: &mut Deck, tempo: bool, phase: bool) {
        let Some(grid) = deck.state().grid.filter(|_| deck.state().loaded) else {
            return;
        };
        let bpm = 60.0 / grid.period_at(deck.state().position_s);
        if tempo && bpm >= MIN_CLOCK_BPM {
            deck.set_tempo(self.state.clock_bpm / bpm);
        }
        if phase && let Some(offset) = self.phase_of(deck) {
            deck.shift_beats(-offset);
        }
    }

    fn follow(&mut self, i: usize, deck: &mut Deck, phase_lock: bool) {
        let Some(grid) = deck.state().grid else {
            return;
        };
        let Some(beat) = deck.beat_position() else {
            return;
        };
        let clock_bpm = self.state.clock_bpm;
        let bpm = 60.0 / grid.period_at(deck.state().position_s);
        let multiple = grid_multiple(clock_bpm, bpm);
        let tempo = clock_bpm * multiple / bpm * self.bend[i];
        let away = deck.state().reverse || deck.state().scratching;
        if !deck.is_playing() || !phase_lock || away {
            deck.set_tempo(tempo);
            return;
        }
        let error = wrap_phase(self.state.clock_beat * multiple - beat);
        let just_started = !self.was_playing[i];
        if just_started || self.needs_alignment[i] || error.abs() > JUMP_THRESHOLD_BEATS {
            deck.set_tempo(tempo);
            if error.abs() > PHASE_TOLERANCE_BEATS {
                deck.shift_beats(error);
            }
            self.needs_alignment[i] = false;
            return;
        }
        let beats_per_s = bpm * tempo / 60.0;
        let correction = if error.abs() > PHASE_TOLERANCE_BEATS {
            (error / (CORRECTION_TIME_S * beats_per_s)).clamp(-MAX_CORRECTION, MAX_CORRECTION)
        } else {
            0.0
        };
        deck.set_tempo(tempo * (1.0 + correction));
    }
}

pub fn grid_multiple(clock_bpm: f64, grid_bpm: f64) -> f64 {
    [0.5, 1.0, 2.0]
        .into_iter()
        .min_by(|a, b| {
            let da = (clock_bpm * a / grid_bpm).ln().abs();
            let db = (clock_bpm * b / grid_bpm).ln().abs();
            da.total_cmp(&db)
        })
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests;
