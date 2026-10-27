pub mod freeze;
mod grid;
mod keylock;
mod loop_move;

use std::sync::Arc;

pub use backspn_decode::Track;
pub use backspn_formats::nml::CueKind;
use backspn_stretch::Stretcher;
pub use grid::{Grid, wrap_phase};
pub use keylock::KeylockRange;
pub use loop_move::LoopEdge;

use freeze::{Freeze, PadChange, SLICES, round_beat};

pub const DECLICK_S: f64 = 0.004;
pub const HOTCUES: usize = 8;
pub const MAX_KEY_ADJUST: f32 = 12.0;
pub const MIN_TEMPO: f64 = 0.05;
pub const MAX_TEMPO: f64 = 4.0;
const DEFAULT_BEAT_S: f64 = 0.5;
const MIN_LOOP_BEATS: f64 = 1.0 / 32.0;
const MIN_LOOP_S: f64 = MIN_LOOP_BEATS * 60.0 / 300.0;
const SUB_BLOCK: usize = 64;
const SCRATCH_LAG_S: f64 = 0.02;
const SCRATCH_SMOOTH_S: f64 = 0.004;
const MAX_SCRATCH_SPEED: f64 = 8.0;
const NUDGE_S: f64 = 0.08;
const MAX_NUDGE_SPEED: f64 = 16.0;

pub const STEMS: usize = 4;

#[derive(Debug)]
pub struct Stems(pub [Track; STEMS]);

#[derive(Debug, Clone)]
pub enum Material {
    Track(Arc<Track>),
    Stems(Arc<Stems>),
}

impl Material {
    pub fn main(&self) -> &Track {
        match self {
            Self::Track(t) => t,
            Self::Stems(s) => &s.0[0],
        }
    }

    fn layers(&self) -> &[Track] {
        match self {
            Self::Track(t) => std::slice::from_ref(&**t),
            Self::Stems(s) => &s.0,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    Load(Arc<Track>),
    LoadStems(Arc<Stems>),
    Unload,
    SetGrid(Option<Grid>),
    Play,
    Pause,
    TogglePlay,
    Seek {
        seconds: f64,
    },
    CueDown,
    CueUp,
    CupDown,
    CupUp,
    SetTempo(f64),
    SetKeylock(bool),
    SetKeylockRange(KeylockRange),
    SetKeyAdjust(f32),
    SetSnap(bool),
    SetQuantize(bool),
    Hotcue(usize),
    DeleteHotcue(usize),
    LoopIn,
    LoopOut,
    AutoLoop {
        beats: f64,
    },
    LoopToggle,
    FastLoop {
        beats: f64,
    },
    LoopScale(f64),
    SetLoop(Option<LoopRange>),
    BeatJump {
        beats: f64,
        stay_in_loop: bool,
    },
    MoveLoop {
        edge: LoopEdge,
        beats: Option<f64>,
        steps: i32,
    },
    ScratchStart,
    ScratchMove(f64),
    ScratchEnd,
    Nudge(f64),
    SetHotcues([Option<HotcuePoint>; HOTCUES]),
    JumpToCue(HotcuePoint),
    RestoreCues {
        cue_s: Option<f64>,
        hotcues: [Option<HotcuePoint>; HOTCUES],
    },
    HotcueRelease(usize),
    SlipJump(bool),
    SetSlip(bool),
    SetSlipPause(bool),
    Reverse(bool),
    SetFreeze {
        on: bool,
        loop_beats: f64,
    },
    FreezeSlice {
        slice: usize,
        pressed: bool,
    },
    SetSliceSize(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopRange {
    pub start_s: f64,
    pub end_s: f64,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FreezeView {
    pub bounds_s: [f64; SLICES + 1],
    pub current: Option<usize>,
    pub held: u16,
    pub size_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct DeckState {
    pub generation: u64,
    pub loaded: bool,
    pub stems: bool,
    pub playing: bool,
    pub scratching: bool,
    pub position_s: f64,
    pub duration_s: f64,
    pub cue_s: f64,
    pub tempo: f64,
    pub keylock: bool,
    pub key_adjust: f32,
    pub snap: bool,
    pub quantize: bool,
    pub grid: Option<Grid>,
    pub loop_start_s: Option<f64>,
    pub loop_end_s: Option<f64>,
    pub loop_active: bool,
    pub hotcues: [Option<HotcuePoint>; HOTCUES],
    pub slip: bool,
    pub reverse: bool,
    pub slip_position_s: Option<f64>,
    pub freeze: Option<FreezeView>,
}

impl DeckState {
    pub fn effective_bpm(&self) -> Option<f64> {
        self.grid
            .map(|g| 60.0 / g.period_at(self.position_s) * self.tempo)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transport {
    Stopped,
    Playing,
    CuePreview,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pending {
    jump_to: Option<f64>,
    then: Transport,
    keylock: Option<bool>,
    issued_at: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HotcuePoint {
    pub start_s: f64,
    pub kind: CueKind,
    pub loop_s: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Hotcue {
    position: f64,
    kind: CueKind,
    loop_len: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Loop {
    start: f64,
    end: Option<f64>,
    active: bool,
}

impl Loop {
    fn len(&self) -> Option<f64> {
        self.end.map(|e| e - self.start)
    }

    fn wrap(&self, prev: f64, pos: f64) -> f64 {
        match (self.active, self.end) {
            (true, Some(end))
                if end > self.start
                    && (self.start..=end).contains(&prev)
                    && !(self.start..end).contains(&pos) =>
            {
                self.start + (pos - self.start).rem_euclid(end - self.start)
            }
            _ => pos,
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
pub struct Deck {
    output_rate: u32,
    track: Option<Material>,
    generation: u64,
    position: f64,
    cue: f64,
    transport: Transport,
    tempo: f64,
    grid: Option<Grid>,
    snap: bool,
    quantize: bool,
    hotcues: [Option<Hotcue>; HOTCUES],
    looping: Option<Loop>,
    gain: f32,
    gain_step: f32,
    pending: Option<Pending>,
    scratch: Option<f64>,
    scratch_speed: f64,
    nudge: f64,
    slip: bool,
    reverse: bool,
    slip_hotcue: Option<usize>,
    slip_jump: bool,
    freeze_jump: bool,
    slip_pause: bool,
    slip_paused: bool,
    freeze: Freeze,
    shadow: Option<f64>,
    keylock: bool,
    key_adjust: f32,
    keylock_range: KeylockRange,
    transpose: f32,
    stretching: bool,
    stretchers: Vec<Stretcher>,
    read: f64,
    carry: f64,
    stretch_in: Vec<Box<[f32]>>,
    prime_buf: Vec<Box<[f32]>>,
    layer_out: Vec<Box<[[f32; 2]]>>,
}

impl std::fmt::Debug for Deck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deck")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl Deck {
    pub fn new(output_rate: u32) -> Self {
        Self::with_layers(output_rate, 1)
    }

    pub fn with_stems(output_rate: u32) -> Self {
        Self::with_layers(output_rate, STEMS)
    }

    fn with_layers(output_rate: u32, layers: usize) -> Self {
        assert!(output_rate > 0, "output rate must be positive");
        let ramp_frames = (DECLICK_S * f64::from(output_rate)).max(1.0) as f32;
        let stretchers: Vec<Stretcher> = (0..layers).map(|_| Stretcher::new(output_rate)).collect();
        let prime_frames = stretchers[0].input_latency() + 1;
        let buffer = |len: usize| vec![0.0; len].into_boxed_slice();
        Self {
            output_rate,
            track: None,
            generation: 0,
            position: 0.0,
            cue: 0.0,
            transport: Transport::Stopped,
            tempo: 1.0,
            grid: None,
            snap: false,
            quantize: false,
            hotcues: [None; HOTCUES],
            looping: None,
            gain: 0.0,
            gain_step: 1.0 / ramp_frames,
            pending: None,
            scratch: None,
            scratch_speed: 0.0,
            nudge: 0.0,
            slip: false,
            reverse: false,
            slip_hotcue: None,
            slip_jump: false,
            freeze_jump: false,
            slip_pause: false,
            slip_paused: false,
            freeze: Freeze::default(),
            shadow: None,
            keylock: false,
            key_adjust: 0.0,
            keylock_range: KeylockRange::default(),
            transpose: 0.0,
            stretching: false,
            stretchers,
            read: 0.0,
            carry: 0.0,
            stretch_in: (0..layers)
                .map(|_| buffer(2 * (SUB_BLOCK * MAX_TEMPO.ceil() as usize + 2)))
                .collect(),
            prime_buf: (0..layers).map(|_| buffer(2 * prime_frames)).collect(),
            layer_out: (0..layers)
                .map(|_| vec![[0.0; 2]; SUB_BLOCK].into_boxed_slice())
                .collect(),
        }
    }

    fn load(&mut self, material: Material) -> Option<Material> {
        if material.layers().len() > self.capacity() {
            return Some(material);
        }
        self.reset_for_track();
        self.track.replace(material)
    }

    pub fn has_stems(&self) -> bool {
        matches!(self.track, Some(Material::Stems(_)))
    }

    fn capacity(&self) -> usize {
        self.stretchers.len()
    }

    pub fn handle(&mut self, command: Command) -> Option<Material> {
        let before = self.in_excursion();
        let replaced = self.apply(command);
        self.update_slip(before);
        replaced
    }

    #[allow(clippy::too_many_lines)]
    fn apply(&mut self, command: Command) -> Option<Material> {
        match command {
            Command::Load(track) => return self.load(Material::Track(track)),
            Command::LoadStems(stems) => return self.load(Material::Stems(stems)),
            Command::Unload => {
                self.reset_for_track();
                return self.track.take();
            }
            Command::SetGrid(grid) => self.grid = grid.filter(|g| g.bpm > 0.0 && g.bpm.is_finite()),
            Command::SetTempo(tempo) => self.set_tempo(tempo),
            Command::SetKeylock(on) => self.set_keylock(on),
            Command::SetKeylockRange(range) => self.keylock_range = range,
            Command::SetKeyAdjust(semitones) => {
                if semitones.is_finite() {
                    self.key_adjust = semitones.clamp(-MAX_KEY_ADJUST, MAX_KEY_ADJUST);
                }
            }
            Command::SetSnap(on) => self.snap = on,
            Command::SetQuantize(on) => self.quantize = on,
            Command::SetSlip(on) => self.set_slip(on),
            Command::SetSlipPause(on) => self.slip_pause = on,
            _ if self.track.is_none() => {}
            Command::HotcueRelease(slot) => {
                if self.slip_hotcue == Some(slot) {
                    self.slip_hotcue = None;
                }
            }
            Command::SlipJump(held) => self.slip_jump = held && self.slip && self.is_playing(),
            Command::Reverse(on) => self.reverse = on && self.is_playing(),
            Command::SetFreeze { .. } | Command::FreezeSlice { .. } | Command::SetSliceSize(_) => {
                self.freeze_command(&command);
            }
            Command::Play | Command::CupUp => self.resume(),
            Command::Pause => self.pause(),
            Command::TogglePlay => match self.target_transport() {
                Transport::Stopped => self.resume(),
                Transport::Playing | Transport::CuePreview => self.pause(),
            },
            Command::Seek { seconds } => {
                let target = self.clamp(seconds * self.source_rate());
                self.jump(target, self.target_transport());
            }
            Command::CueDown => self.cue_down(),
            Command::CueUp => {
                if self.target_transport() == Transport::CuePreview {
                    self.stop_at(Some(self.cue));
                }
            }
            Command::CupDown => self.stop_at(Some(self.cue)),
            Command::Hotcue(slot) => {
                self.press_hotcue(slot);
                self.leave_freeze_if_outside();
            }
            Command::DeleteHotcue(slot) => {
                if let Some(h) = self.hotcues.get_mut(slot) {
                    *h = None;
                }
            }
            Command::LoopIn => self.loop_in(),
            Command::LoopOut => self.loop_out(),
            Command::AutoLoop { beats } => self.auto_loop(beats),
            Command::LoopToggle => self.loop_toggle(),
            Command::FastLoop { beats } => self.fast_loop(beats),
            Command::LoopScale(factor) => self.loop_scale(factor),
            Command::SetLoop(range) => {
                let rate = self.source_rate();
                self.looping = range
                    .filter(|r| r.end_s > r.start_s && r.start_s.is_finite() && r.end_s.is_finite())
                    .map(|r| Loop {
                        start: self.clamp(r.start_s * rate),
                        end: Some(self.clamp(r.end_s * rate)),
                        active: r.active,
                    });
            }
            Command::BeatJump {
                beats,
                stay_in_loop,
            } => {
                self.beat_jump(beats, stay_in_loop);
                self.leave_freeze_if_outside();
            }
            Command::MoveLoop { edge, beats, steps } => self.move_loop(edge, beats, steps),
            Command::ScratchStart | Command::ScratchMove(_) | Command::ScratchEnd => {
                self.scratch_command(&command);
            }
            Command::Nudge(seconds) => {
                if self.is_playing() && seconds.is_finite() {
                    self.nudge += seconds * self.source_rate();
                }
            }
            Command::SetHotcues(hotcues) => self.set_hotcues(hotcues),
            Command::JumpToCue(point) => {
                self.jump_to_cue(self.hotcue_of(point));
                self.leave_freeze_if_outside();
            }
            Command::RestoreCues { cue_s, hotcues } => {
                if let Some(cue) = cue_s {
                    self.cue = self.clamp(cue * self.source_rate());
                    if !self.is_playing() && self.position == 0.0 {
                        self.jump(self.cue, Transport::Stopped);
                    }
                }
                self.set_hotcues(hotcues);
            }
        }
        None
    }

    pub fn process(&mut self, out: &mut [f32]) {
        let (frames, _) = out.as_chunks_mut::<2>();
        for chunk in frames.chunks_mut(SUB_BLOCK) {
            let layers = self.render_chunk(chunk.len());
            for (i, frame) in chunk.iter_mut().enumerate() {
                *frame = self.layer_out[..layers]
                    .iter()
                    .fold([0.0; 2], |a, l| [a[0] + l[i][0], a[1] + l[i][1]]);
            }
        }
    }

    pub fn process_layers(&mut self, outs: &mut [&mut [f32]]) -> usize {
        let frames = outs.first().map_or(0, |o| o.len() / 2);
        let mut played = 0;
        let mut done = 0;
        while done < frames {
            let n = SUB_BLOCK.min(frames - done);
            let layers = self.render_chunk(n);
            played = layers;
            for (l, out) in outs.iter_mut().enumerate() {
                let (out, _) = out[2 * done..2 * (done + n)].as_chunks_mut::<2>();
                match self.layer_out.get(l).filter(|_| l < layers) {
                    Some(layer) => out.copy_from_slice(&layer[..n]),
                    None => out.fill([0.0; 2]),
                }
            }
            done += n;
        }
        played
    }

    pub fn state(&self) -> DeckState {
        let rate = self.source_rate();
        let hotcues = self.hotcues.map(|h| {
            h.map(|h| HotcuePoint {
                start_s: h.position / rate,
                kind: h.kind,
                loop_s: h.loop_len.map(|l| l / rate),
            })
        });
        DeckState {
            generation: self.generation,
            loaded: self.track.is_some(),
            stems: self.has_stems(),
            playing: self.target_transport() != Transport::Stopped,
            scratching: self.scratch.is_some(),
            position_s: self.position / rate,
            duration_s: self.track.as_ref().map_or(0.0, |t| t.main().duration_s()),
            cue_s: self.cue / rate,
            tempo: self.tempo,
            keylock: self.pending.and_then(|p| p.keylock).unwrap_or(self.keylock),
            key_adjust: self.key_adjust,
            snap: self.snap,
            quantize: self.quantize,
            grid: self.grid,
            loop_start_s: self.looping.map(|l| l.start / rate),
            loop_end_s: self.looping.and_then(|l| l.end).map(|e| e / rate),
            loop_active: self.looping.is_some_and(|l| l.active),
            hotcues,
            slip: self.slip,
            reverse: self.reverse,
            slip_position_s: self.shadow.map(|s| s / rate),
            freeze: self.freeze_view(),
        }
    }

    fn in_excursion(&self) -> bool {
        self.reverse
            || (self.slip
                && (self.looping.is_some_and(|l| l.active)
                    || self.scratch.is_some()
                    || self.slip_hotcue.is_some()
                    || self.slip_jump
                    || self.freeze_jump))
    }

    fn update_slip(&mut self, before: bool) {
        let now = self.in_excursion();
        if !before && now && self.is_playing() && self.shadow.is_none() {
            self.shadow = Some(self.position);
        } else if before
            && !now
            && !self.slip_paused
            && let Some(shadow) = self.shadow.take()
            && self.is_playing()
        {
            self.jump(shadow, self.target_transport());
        }
    }

    fn set_slip(&mut self, on: bool) {
        self.slip = on;
        if !on {
            self.slip_hotcue = None;
            self.slip_jump = false;
            self.freeze_jump = false;
            if std::mem::take(&mut self.slip_paused) {
                self.shadow = None;
            }
        }
    }

    fn press_hotcue(&mut self, slot: usize) {
        if self.slip && self.is_playing() && self.hotcues.get(slot).is_some_and(Option::is_some) {
            self.slip_hotcue = Some(slot);
        }
        self.hotcue(slot);
    }

    pub fn beat_position(&self) -> Option<f64> {
        let grid = self.grid?;
        self.track.as_ref()?;
        Some(grid.beat_at(self.position / self.source_rate()))
    }

    pub fn is_playing(&self) -> bool {
        self.target_transport() != Transport::Stopped
    }

    pub fn set_tempo(&mut self, tempo: f64) {
        if tempo.is_finite() {
            self.tempo = tempo.clamp(MIN_TEMPO, MAX_TEMPO);
        }
    }

    pub fn shift_beats(&mut self, beats: f64) {
        if self.track.is_none() {
            return;
        }
        let target = self.clamp(self.position + beats * self.beat_frames());
        self.jump(target, self.target_transport());
    }

    fn render_chunk(&mut self, n: usize) -> usize {
        let Some(material) = self.track.clone() else {
            for layer in &mut self.layer_out {
                layer[..n].fill([0.0; 2]);
            }
            return 1;
        };
        let layers = material.layers();
        let mut out = std::mem::take(&mut self.layer_out);
        self.render_layers(layers, &mut out, n);
        self.layer_out = out;
        layers.len()
    }

    fn render_layers(&mut self, layers: &[Track], out: &mut [Box<[[f32; 2]]>], n: usize) {
        let frames = layers[0].frames();
        let step = n as f64 * self.rate_ratio() * self.tempo;
        if let Some(shadow) = &mut self.shadow
            && (self.transport != Transport::Stopped || self.slip_paused)
        {
            *shadow = (*shadow + step).min(frames as f64);
        }
        if let Some(target) = self.scratch {
            self.stretching = false;
            self.render_scratch(layers, target, out, n);
            return;
        }
        let mut done = 0;
        while done < n {
            if self.transport == Transport::Stopped && self.gain == 0.0 {
                self.apply_pending();
                if self.transport == Transport::Stopped {
                    self.stretching = false;
                    for o in out.iter_mut() {
                        o[done..n].fill([0.0; 2]);
                    }
                    return;
                }
            }
            let fading_out = self.pending.is_some() || self.transport == Transport::Stopped;
            let remaining = n - done;
            let len = if fading_out {
                ((self.gain / self.gain_step).ceil() as usize).clamp(1, remaining)
            } else {
                remaining
            };
            let segment = done..done + len;
            let transpose = if self.keylock && !self.reverse {
                self.keylock_transpose(len)
            } else {
                None
            };
            if let Some(semitones) = transpose {
                if (semitones - self.transpose).abs() > 1e-4 {
                    for stretcher in &mut self.stretchers {
                        stretcher.set_transpose(semitones);
                    }
                    self.transpose = semitones;
                }
                if self.gain == 0.0 || !self.stretching {
                    self.prime_keylock(self.position);
                }
                self.stretching = true;
                self.render_keylock(layers, out, segment.clone());
            } else {
                self.stretching = false;
                self.render_direct(layers, out, segment.clone());
            }
            for i in segment {
                self.gain = if fading_out {
                    (self.gain - self.gain_step).max(0.0)
                } else {
                    (self.gain + self.gain_step).min(1.0)
                };
                for o in &mut out[..layers.len()] {
                    o[i][0] *= self.gain;
                    o[i][1] *= self.gain;
                }
            }
            done += len;
            if self.gain == 0.0 {
                self.apply_pending();
            }
            if self.position >= frames as f64 && self.looping.is_none_or(|l| !l.active) {
                self.position = frames as f64;
                self.transport = Transport::Stopped;
                self.pending = None;
                self.gain = 0.0;
            }
        }
    }

    fn render_scratch(
        &mut self,
        layers: &[Track],
        mut target: f64,
        out: &mut [Box<[[f32; 2]]>],
        n: usize,
    ) {
        let looping = self.looping.unwrap_or_default();
        let rate = f64::from(self.output_rate);
        let normal = self.rate_ratio();
        let max = MAX_SCRATCH_SPEED * normal;
        let smooth = 1.0 / (SCRATCH_SMOOTH_S * rate).max(1.0);
        let end = layers[0].frames() as f64;
        for i in 0..n {
            let wanted = ((target - self.position) / (SCRATCH_LAG_S * rate)).clamp(-max, max);
            self.scratch_speed += (wanted - self.scratch_speed) * smooth;
            let moved = (self.position + self.scratch_speed).min(end);
            let wrapped = looping.wrap(self.position, moved);
            target += wrapped - moved;
            self.position = wrapped;
            let loud = (self.scratch_speed.abs() / (0.05 * normal)).min(1.0) as f32;
            self.gain = if self.gain < loud {
                (self.gain + self.gain_step).min(loud)
            } else {
                (self.gain - self.gain_step).max(loud)
            };
            for (o, track) in out.iter_mut().zip(layers) {
                let [l, r] = interpolate(track, self.position);
                o[i] = [l * self.gain, r * self.gain];
            }
        }
        if self.scratch.is_some() {
            self.scratch = Some(target);
        }
    }

    fn nudge_share(&self, frames: usize) -> f64 {
        if self.nudge.abs() < 1e-3 || self.transport == Transport::Stopped {
            return 0.0;
        }
        let share = (frames as f64 / (NUDGE_S * f64::from(self.output_rate))).min(1.0);
        let limit = MAX_NUDGE_SPEED * frames as f64 * self.rate_ratio() * self.tempo;
        (self.nudge * share).clamp(-limit, limit)
    }

    fn take_nudge(&mut self, frames: usize) -> f64 {
        if self.nudge.abs() < 1e-3 {
            self.nudge = 0.0;
            return 0.0;
        }
        let taken = self.nudge_share(frames);
        self.nudge -= taken;
        taken
    }

    fn render_direct(
        &mut self,
        layers: &[Track],
        out: &mut [Box<[[f32; 2]]>],
        segment: std::ops::Range<usize>,
    ) {
        let len = segment.len();
        let nudge = if self.transport == Transport::Stopped {
            0.0
        } else {
            self.take_nudge(len)
        };
        let step = self.rate_ratio() * self.tempo + nudge / len as f64;
        let looping = self.looping.unwrap_or_default();
        let step = if self.reverse { -step } else { step };
        for i in segment {
            for (o, track) in out.iter_mut().zip(layers) {
                o[i] = interpolate(track, self.position);
            }
            self.position = looping.wrap(self.position, self.position + step);
        }
    }

    fn render_keylock(
        &mut self,
        layers: &[Track],
        out: &mut [Box<[[f32; 2]]>],
        segment: std::ops::Range<usize>,
    ) {
        let len = segment.len();
        let nudge = if self.transport == Transport::Stopped {
            0.0
        } else {
            self.take_nudge(len)
        };
        let wanted = len as f64 * self.tempo + nudge / self.rate_ratio() + self.carry;
        let n_in = (wanted.floor() as usize).min(self.stretch_in[0].len() / 2);
        self.carry = wanted - n_in as f64;
        let step = self.rate_ratio();
        let looping = self.looping.unwrap_or_default();
        let start = self.read;
        for (l, track) in layers.iter().enumerate() {
            let mut read = start;
            let (input, _) = self.stretch_in[l].as_chunks_mut::<2>();
            for frame in &mut input[..n_in] {
                *frame = interpolate(track, read);
                read = looping.wrap(read, read + step);
            }
            self.read = read;
            let out_flat = out[l][segment.clone()].as_flattened_mut();
            self.stretchers[l].process(&self.stretch_in[l][..2 * n_in], out_flat);
        }
        let advanced = self.position + len as f64 * self.tempo * step + nudge;
        self.position = looping.wrap(self.position, advanced);
    }

    fn keylock_transpose(&self, frames: usize) -> Option<f32> {
        let bend = self.nudge_share(frames) / (frames.max(1) as f64 * self.rate_ratio());
        let speed = self.tempo + bend;
        if speed <= 0.0 {
            return None;
        }
        let key_scale = 2f64.powf(f64::from(self.key_adjust) / 12.0);
        keylock::heard_pitch(self.keylock_range, speed, key_scale)
            .map(|pitch| (12.0 * pitch.log2()) as f32)
    }

    fn prime_keylock(&mut self, target: f64) {
        let Some(material) = self.track.clone() else {
            return;
        };
        let step = self.rate_ratio();
        let processing = target + self.stretchers[0].output_latency() as f64 * self.tempo * step;
        let frames = self.stretchers[0].input_latency();
        let mut pos = processing;
        for (l, track) in material.layers().iter().enumerate() {
            if !self.stretching {
                self.stretchers[l].reset();
            }
            let (buf, _) = self.prime_buf[l].as_chunks_mut::<2>();
            pos = processing;
            for frame in &mut buf[..frames] {
                *frame = interpolate(track, pos);
                pos += step;
            }
            self.stretchers[l].prime(&self.prime_buf[l][..2 * frames], self.tempo);
        }
        self.read = pos;
        self.carry = 0.0;
        self.position = target;
    }

    fn apply_pending(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if let Some(on) = pending.keylock {
            self.keylock = on;
        }
        let elapsed = self.position - pending.issued_at;
        let target = match pending.jump_to {
            Some(t)
                if pending.then != Transport::Stopped
                    && (0.0..self.source_rate()).contains(&elapsed) =>
            {
                self.clamp(t + elapsed)
            }
            Some(t) => t,
            None => self.position,
        };
        self.position = target;
        if self.keylock && (pending.jump_to.is_some() || pending.keylock.is_some()) {
            self.prime_keylock(target);
        }
        self.transport = pending.then;
    }

    fn play(&mut self) {
        match &mut self.pending {
            Some(pending) => pending.then = Transport::Playing,
            None => self.transport = Transport::Playing,
        }
        if self.at_end() {
            self.jump(self.cue, Transport::Playing);
        }
    }

    fn pause(&mut self) {
        if self.slip && self.slip_pause && self.is_playing() {
            self.shadow.get_or_insert(self.position);
            self.slip_paused = true;
        }
        self.stop_at(None);
    }

    fn resume(&mut self) {
        if std::mem::take(&mut self.slip_paused)
            && let Some(shadow) = self.shadow.take()
        {
            self.jump(self.clamp(shadow), Transport::Playing);
            return;
        }
        self.play();
    }

    fn cue_down(&mut self) {
        match self.target_transport() {
            Transport::Playing => self.stop_at(Some(self.cue)),
            Transport::CuePreview => {}
            Transport::Stopped => {
                if self.pending.is_none() {
                    self.cue = self.snapped(self.position);
                    if (self.cue - self.position).abs() > 1e-9 {
                        self.jump(self.cue, Transport::CuePreview);
                        return;
                    }
                }
                match &mut self.pending {
                    Some(pending) => pending.then = Transport::CuePreview,
                    None => self.transport = Transport::CuePreview,
                }
            }
        }
    }

    fn stop_at(&mut self, jump_to: Option<f64>) {
        let jump_to = jump_to.or(self.pending.and_then(|p| p.jump_to));
        let keylock = self.pending.and_then(|p| p.keylock);
        self.pending = Some(Pending {
            jump_to,
            then: Transport::Stopped,
            keylock,
            issued_at: self.position,
        });
        if self.gain == 0.0 {
            self.apply_pending();
        }
    }

    fn jump(&mut self, target: f64, then: Transport) {
        let keylock = self.pending.and_then(|p| p.keylock);
        self.pending = Some(Pending {
            jump_to: Some(target),
            then,
            keylock,
            issued_at: self.position,
        });
        if self.gain == 0.0 {
            self.apply_pending();
        }
    }

    fn quantized_jump(&mut self, target: f64) {
        let target = match self.grid {
            Some(grid) if self.quantize && self.is_playing() => {
                let rate = self.source_rate();
                let phase = grid.offset_from_beat(self.position / rate)
                    - grid.offset_from_beat(target / rate);
                self.clamp(target + phase * rate)
            }
            _ => target,
        };
        self.jump(target, self.target_transport());
    }

    fn set_keylock(&mut self, on: bool) {
        if on == self.state().keylock {
            return;
        }
        let jump_to = self.pending.and_then(|p| p.jump_to);
        let then = self.target_transport();
        self.pending = Some(Pending {
            jump_to,
            then,
            keylock: Some(on),
            issued_at: self.position,
        });
        if self.gain == 0.0 {
            self.apply_pending();
        }
    }

    fn target_transport(&self) -> Transport {
        self.pending.map_or(self.transport, |p| p.then)
    }

    fn reset_for_track(&mut self) {
        self.generation += 1;
        self.position = 0.0;
        self.read = 0.0;
        self.carry = 0.0;
        self.cue = 0.0;
        self.transport = Transport::Stopped;
        self.gain = 0.0;
        self.pending = None;
        self.scratch = None;
        self.nudge = 0.0;
        self.reverse = false;
        self.slip_hotcue = None;
        self.slip_jump = false;
        self.freeze_jump = false;
        self.slip_paused = false;
        self.freeze = Freeze {
            size_index: self.freeze.size_index,
            ..Freeze::default()
        };
        self.shadow = None;
        self.grid = None;
        self.hotcues = [None; HOTCUES];
        self.looping = None;
        for stretcher in &mut self.stretchers {
            stretcher.reset();
        }
    }

    fn hotcue(&mut self, slot: usize) {
        if slot >= HOTCUES {
            return;
        }
        match self.hotcues[slot] {
            None => {
                let looping = self.looping.filter(|l| l.active);
                let position = looping.map_or_else(|| self.snapped(self.position), |l| l.start);
                let loop_len = looping.and_then(|l| l.len());
                self.hotcues[slot] = Some(Hotcue {
                    position,
                    kind: if loop_len.is_some() {
                        CueKind::Loop
                    } else {
                        CueKind::Cue
                    },
                    loop_len,
                });
            }
            Some(h) => self.jump_to_cue(h),
        }
    }

    fn jump_to_cue(&mut self, h: Hotcue) {
        self.cue = h.position;
        if let Some(len) = h.loop_len {
            self.looping = Some(Loop {
                start: h.position,
                end: Some(h.position + len),
                active: true,
            });
        }
        self.quantized_jump(h.position);
    }

    fn set_hotcues(&mut self, hotcues: [Option<HotcuePoint>; HOTCUES]) {
        self.hotcues = hotcues.map(|h| h.map(|h| self.hotcue_of(h)));
    }

    fn hotcue_of(&self, h: HotcuePoint) -> Hotcue {
        let rate = self.source_rate();
        Hotcue {
            position: h.start_s * rate,
            kind: h.kind,
            loop_len: h
                .loop_s
                .filter(|&l| h.kind == CueKind::Loop && l > 0.0)
                .map(|l| l * rate),
        }
    }

    fn loop_in(&mut self) {
        self.looping = Some(Loop {
            start: self.snapped(self.position),
            end: None,
            active: false,
        });
    }

    fn loop_out(&mut self) {
        let Some(mut l) = self.looping else { return };
        let mut end = self.snapped(self.position);
        if end <= l.start {
            end = l.start + self.beat_frames();
        }
        l.end = Some(end);
        l.active = true;
        self.looping = Some(l);
    }

    fn auto_loop(&mut self, beats: f64) {
        let len = beats.max(MIN_LOOP_BEATS) * self.beat_frames();
        if let Some(l) = self.looping.filter(|l| l.active)
            && l.len().is_some_and(|current| (current - len).abs() < 1.0)
        {
            self.looping = Some(Loop { active: false, ..l });
            return;
        }
        let start = match self.grid {
            Some(grid) if self.snap => {
                let rate = self.source_rate();
                (grid.time_of(grid.beat_at(self.position / rate).round()) * rate).max(0.0)
            }
            _ => self.position,
        };
        self.looping = Some(Loop {
            start,
            end: Some(start + len),
            active: true,
        });
    }

    fn fast_loop(&mut self, beats: f64) {
        match self.looping {
            Some(l)
                if l.active
                    && l.end
                        .is_some_and(|end| (l.start..=end).contains(&self.position)) =>
            {
                self.looping = Some(Loop { active: false, ..l });
            }
            _ => {
                self.looping = None;
                self.auto_loop(beats);
            }
        }
    }

    fn loop_toggle(&mut self) {
        let Some(mut l) = self.looping else { return };
        let Some(end) = l.end else { return };
        if l.active {
            l.active = false;
            self.looping = Some(l);
            return;
        }
        l.active = true;
        self.looping = Some(l);
        if self.position >= end || self.position < l.start {
            self.quantized_jump(l.start);
        }
    }

    fn loop_scale(&mut self, factor: f64) {
        let min = MIN_LOOP_BEATS * self.beat_frames();
        let Some(l) = self.looping else { return };
        let Some(old_end) = l.end else { return };
        if factor <= 0.0 {
            return;
        }
        let end = l.start + ((old_end - l.start) * factor).max(min);
        self.looping = Some(Loop {
            end: Some(end),
            ..l
        });
        self.fold_into_loop(l, l.start, end);
    }

    fn fold_into_loop(&mut self, old: Loop, start: f64, end: f64) {
        let pos = self.position;
        if old.active
            && old
                .end
                .is_some_and(|old_end| (old.start..=old_end).contains(&pos))
            && !(start..=end).contains(&pos)
        {
            let folded = start + (pos - start).rem_euclid(end - start);
            self.jump(folded, self.target_transport());
        }
    }

    fn beat_jump(&mut self, beats: f64, stay_in_loop: bool) {
        let mut target = self.position + beats * self.beat_frames();
        if stay_in_loop && let Some(l) = self.looping {
            target = l.wrap(self.position, target);
        }
        let target = self.clamp(target);
        self.jump(target, self.target_transport());
    }

    fn move_loop(&mut self, edge: LoopEdge, beats: Option<f64>, steps: i32) {
        let Some(l) = self.looping else { return };
        let Some(old_end) = l.end else { return };
        let step = beats.map(|b| b * self.beat_frames());
        let (start, end) = loop_move::moved(edge, step, steps, l.start, old_end);
        if start < 0.0 || end < 0.0 || end - start < MIN_LOOP_S * self.source_rate() {
            return;
        }
        self.looping = Some(Loop {
            start,
            end: Some(end),
            ..l
        });
        self.fold_into_loop(l, start, end);
    }

    fn beats_of(&self, seconds: f64) -> f64 {
        self.grid
            .map_or(seconds / DEFAULT_BEAT_S, |g| g.beat_at(seconds))
    }

    fn seconds_of(&self, beats: f64) -> f64 {
        self.grid
            .map_or(beats * DEFAULT_BEAT_S, |g| g.time_of(beats))
    }

    fn loop_active(&self) -> bool {
        self.looping.is_some_and(|l| l.active)
    }

    fn scratch_command(&mut self, command: &Command) {
        match *command {
            Command::ScratchStart => {
                self.apply_pending();
                self.scratch = Some(self.position);
                self.scratch_speed = 0.0;
            }
            Command::ScratchMove(seconds) => {
                let delta = seconds * self.source_rate();
                if let Some(target) = self.scratch {
                    self.scratch = Some(self.clamp(target + delta));
                }
            }
            Command::ScratchEnd => {
                let ended = self.scratch.take().is_some();
                if ended && self.keylock {
                    self.prime_keylock(self.position);
                }
            }
            _ => {}
        }
    }

    fn freeze_command(&mut self, command: &Command) {
        match *command {
            Command::SetFreeze { on, loop_beats } => self.set_freeze(on, loop_beats),
            Command::FreezeSlice { slice, pressed } => self.freeze_slice(slice, pressed),
            Command::SetSliceSize(index) => {
                self.freeze.size_index = index.min(freeze::SLICE_SIZES.len() - 1);
            }
            _ => {}
        }
    }

    fn set_freeze(&mut self, on: bool, loop_beats: f64) {
        if !on || self.shadow.is_some() {
            self.freeze.enabled = false;
            return;
        }
        self.freeze.enabled = true;
        if self.freeze.held == 0 {
            let rate = self.source_rate();
            self.freeze.loop_beats = loop_beats;
            self.freeze.start_s = match self.looping.filter(|l| l.active) {
                Some(l) => l.start / rate,
                None => self.seconds_of(round_beat(self.beats_of(self.position / rate))),
            };
        }
    }

    fn freeze_slice(&mut self, slice: usize, pressed: bool) {
        if slice >= SLICES {
            return;
        }
        let before = self.freeze.held;
        let bit = 1 << slice;
        if pressed && self.freeze.enabled {
            self.freeze.held |= bit;
        } else if !pressed {
            self.freeze.held &= !bit;
        }
        self.freeze_pads(PadChange::new(before, self.freeze.held));
    }

    fn freeze_pads(&mut self, change: PadChange) {
        if !change.changed {
            return;
        }
        let rate = self.source_rate();
        let loop_active = self.loop_active();
        let size = self.freeze.slice_beats(loop_active);
        let start = self.beats_of(self.freeze.start_s);
        match (change.first, change.last) {
            (Some(first), Some(last)) if loop_active => {
                let from = self.seconds_of(first as f64 * size + start) * rate;
                let to = self.seconds_of((last + 1) as f64 * size + start) * rate;
                self.looping = Some(Loop {
                    start: from,
                    end: Some(to),
                    active: true,
                });
                if !change.was_held {
                    self.jump(self.clamp(from), self.target_transport());
                }
                self.freeze.last_slice = Some(first);
            }
            (Some(_), _) => {
                let Some(newest) = change.newest.filter(|_| self.freeze.enabled) else {
                    return;
                };
                let now = self.beats_of(self.position / rate);
                let mut jump = newest as f64 * size + start - now;
                if self.quantize && self.grid.is_some() {
                    jump = round_beat(jump);
                }
                self.freeze_jump = self.slip && self.is_playing();
                self.beat_jump(jump, false);
                self.freeze.last_slice = Some(newest);
            }
            (None, _) if loop_active => {
                let from = self.freeze.start_s;
                let to = from + self.seconds_of(self.freeze.loop_beats);
                self.looping = Some(Loop {
                    start: from * rate,
                    end: Some(to * rate),
                    active: true,
                });
            }
            (None, _) => self.freeze_jump = false,
        }
    }

    fn leave_freeze_if_outside(&mut self) {
        if !self.freeze.enabled {
            return;
        }
        let loop_active = self.loop_active();
        let len = self.seconds_of(self.freeze.slice_beats(loop_active) * SLICES as f64);
        let slack = if loop_active {
            0.0
        } else {
            self.seconds_of(2.0)
        };
        let start = self.freeze.start_s;
        let target = self
            .pending
            .and_then(|p| p.jump_to)
            .unwrap_or(self.position);
        let pos = target / self.source_rate();
        if pos < start - slack || pos > start + len + slack {
            self.freeze.enabled = false;
        }
    }

    fn freeze_view(&self) -> Option<FreezeView> {
        if !self.freeze.enabled || self.track.is_none() {
            return None;
        }
        let size = self.freeze.slice_beats(self.loop_active());
        let start = self.beats_of(self.freeze.start_s);
        let bounds_s = std::array::from_fn(|i| self.seconds_of(start + i as f64 * size));
        let now = self.beats_of(self.position / self.source_rate());
        let index = ((now * 1e5 + 0.5).floor() / 1e5 - (start * 1e5 + 0.5).floor() / 1e5) / size;
        let current = (size > 0.0 && (0.0..SLICES as f64).contains(&index.floor()))
            .then(|| index.floor() as usize);
        Some(FreezeView {
            bounds_s,
            current,
            held: self.freeze.held,
            size_index: self.freeze.size_index,
        })
    }

    fn snapped(&self, pos: f64) -> f64 {
        match self.grid {
            Some(grid) if self.snap => {
                let rate = self.source_rate();
                self.clamp(grid.nearest_beat(pos / rate) * rate)
            }
            _ => pos,
        }
    }

    fn beat_frames(&self) -> f64 {
        self.grid.map_or(DEFAULT_BEAT_S, |g| g.period_s()) * self.source_rate()
    }

    fn source_rate(&self) -> f64 {
        self.track
            .as_ref()
            .map_or(f64::from(self.output_rate), |t| {
                f64::from(t.main().sample_rate())
            })
    }

    fn rate_ratio(&self) -> f64 {
        self.source_rate() / f64::from(self.output_rate)
    }

    fn clamp(&self, frames: f64) -> f64 {
        let end = self
            .track
            .as_ref()
            .map_or(0.0, |t| t.main().frames() as f64);
        frames.min(end)
    }

    fn at_end(&self) -> bool {
        self.track
            .as_ref()
            .is_some_and(|t| self.position >= t.main().frames() as f64)
    }
}

#[inline]
fn interpolate(track: &Track, pos: f64) -> [f32; 2] {
    let base = pos.floor();
    let frac = (pos - base) as f32;
    let index = base as isize;
    let points = [-1, 0, 1, 2].map(|offset| track.frame(index + offset));
    std::array::from_fn(|ch| {
        let [y0, y1, y2, y3] = points.map(|p| p[ch]);
        let cubic = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
        let quadratic = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let linear = -0.5 * y0 + 0.5 * y2;
        ((cubic * frac + quadratic) * frac + linear) * frac + y1
    })
}

#[cfg(test)]
mod tests;
