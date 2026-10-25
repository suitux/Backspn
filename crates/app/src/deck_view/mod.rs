use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use backspn_analysis::{Analysis, Options as AnalysisOptions};
use backspn_control::{MoveMode, MoveSize};
use backspn_engine::{Command, DeckCommand, DeckState, Grid, StemCommand, SyncState};
use backspn_formats::nml::{KeyNotation, MusicalKey};
use backspn_library::CuePoint;
use eframe::egui::{self, Align, Layout, Rect, Ui, vec2};

use crate::browser::TrackDrag;
use crate::covers;
use crate::cue_list::CueList;
use crate::deck_informer::{DeckInformer, Key, Notice};
use crate::grid_edit::GridEdit;
use crate::theme;
use crate::waveform::{self, Peaks};

mod bottom;
mod cue_panel;
mod grid_panel;
mod header;
mod move_panel;
mod stem_row;
mod waveforms;

pub(crate) use header::clock;
use waveforms::OverviewKey;

pub const DEFAULT_ZOOM_S: f64 = 4.0;
pub const MIN_ZOOM_S: f64 = 0.5;
pub const MAX_ZOOM_S: f64 = 32.0;
const CARD_OUTER: f32 = 22.0;
const MIN_ZOOM_HEIGHT: f32 = 40.0;

const COVER: f32 = 58.0;
const TEMPO_COLUMN: f32 = 30.0;
const PLAYHEAD_AT: f64 = 0.35;
const JUMP_BUTTON: f32 = 34.0;
const SIZE_BUTTON: f32 = 34.0;
const ARROW: f32 = 20.0;
const OVERVIEW: f32 = 28.0;

const ZOOM_STEP: f64 = 1.5;
pub(crate) const SIZES: [(f64, &str); 12] = [
    (1.0 / 32.0, "1/32"),
    (0.0625, "1/16"),
    (0.125, "1/8"),
    (0.25, "1/4"),
    (0.5, "1/2"),
    (1.0, "1"),
    (2.0, "2"),
    (4.0, "4"),
    (8.0, "8"),
    (16.0, "16"),
    (32.0, "32"),
    (64.0, "64"),
];
const TEMPO_LOCK_EPSILON: f64 = 0.0005;

#[derive(Debug, Clone, Copy, Default)]
struct TempoLock {
    fader: Option<f32>,
    settled: bool,
}

pub const TEMPO_RANGES: [f32; 14] = [
    0.02, 0.04, 0.06, 0.08, 0.10, 0.12, 0.14, 0.16, 0.18, 0.20, 0.25, 0.35, 0.50, 1.0,
];

enum LoadEvent {
    Decoded(
        Arc<backspn_decode::Track>,
        Peaks,
        Option<egui::ColorImage>,
        Option<Box<crate::stems::StemLoad>>,
        Option<f64>,
    ),
    Analyzed(Box<Analysis>),
    Failed(String),
}

struct Loading {
    request: LoadRequest,
    events: Receiver<LoadEvent>,
    decoded: bool,
}

struct Loaded {
    name: String,
    title: String,
    artist: String,
    comment: String,
    cover: Option<egui::ColorImage>,
    cover_texture: Option<egui::TextureHandle>,
    peaks: Peaks,
    track_id: Option<i64>,
    key_text: Option<String>,
    lufs: Option<f64>,
    cues: CueList,
    locked: bool,
    stems: Option<Vec<backspn_formats::stem::StemInfo>>,
    audio: Arc<backspn_decode::Track>,
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DeckSize {
    Micro,
    Essential,
    #[default]
    Full,
    Advanced,
    ParallelSlim,
    ParallelFull,
}

impl DeckSize {
    pub fn next(self) -> Self {
        match self {
            Self::Micro => Self::Essential,
            Self::Essential => Self::Full,
            Self::Full => Self::Advanced,
            Self::Advanced => Self::Micro,
            Self::ParallelSlim => Self::ParallelFull,
            Self::ParallelFull => Self::ParallelSlim,
        }
    }

    pub fn parallel_wave(self) -> Option<f32> {
        match self {
            Self::ParallelSlim => Some(35.0),
            Self::ParallelFull => Some(91.0),
            _ => None,
        }
    }

    fn without_wave(self) -> bool {
        self == Self::Micro || self.parallel_wave().is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelTab {
    Move,
    Cue,
    Grid,
}

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub path: PathBuf,
    pub name: String,
    pub title: String,
    pub artist: String,
    pub comment: String,
    pub track_id: Option<i64>,
    pub grid: Option<Grid>,
    pub cue_s: Option<f64>,
    pub cues: Vec<CuePoint>,
    pub key_text: Option<String>,
    pub lufs: Option<f64>,
    pub locked: bool,
    pub duplicate_of: Option<usize>,
    pub analysis: Option<AnalysisOptions>,
    pub cover: covers::Source,
    pub stems: Option<PathBuf>,
    pub output_rate: u32,
}

impl LoadRequest {
    pub fn file(path: PathBuf) -> Self {
        let name = path.file_stem().map_or_else(
            || path.display().to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        Self {
            cover: covers::Source::File(path.clone()),
            path,
            title: name.clone(),
            name,
            artist: String::new(),
            comment: String::new(),
            track_id: None,
            grid: None,
            cue_s: None,
            cues: Vec::new(),
            key_text: None,
            lufs: None,
            locked: false,
            duplicate_of: None,
            analysis: Some(AnalysisOptions::default()),
            stems: None,
            output_rate: 44_100,
        }
    }
}

pub enum DeckEvent {
    Analyzed {
        track_id: i64,
        analysis: Box<Analysis>,
        keep_grid: bool,
    },
    Overview {
        track_id: i64,
        overview: Vec<u8>,
    },
    ShowInBrowser(i64),
    Duplicate {
        deck: usize,
        from: usize,
    },
    Bend {
        deck: usize,
        direction: f64,
        down: bool,
    },
    Action(backspn_control::Action),
    Grid {
        deck: usize,
        edit: GridEdit,
    },
    Lock {
        deck: usize,
        locked: bool,
    },
    Reanalyse {
        deck: usize,
        grid: bool,
    },
    CuePick {
        deck: usize,
        index: usize,
    },
    CueRename {
        deck: usize,
        name: String,
    },
}

#[derive(Default)]
pub struct Hold {
    down: bool,
}

impl Hold {
    pub fn update(&mut self, down: bool) -> Option<bool> {
        (down != self.down).then(|| {
            self.down = down;
            down
        })
    }

    pub fn button(&mut self, response: &egui::Response) -> Option<bool> {
        let held = self.down && response.ctx.input(|i| i.pointer.primary_down());
        self.update(response.is_pointer_button_down_on() || held)
    }

    pub fn down(&self) -> bool {
        self.down
    }
}

#[allow(clippy::struct_excessive_bools)]
pub struct DeckView {
    pub index: usize,
    pub stem_controls: [backspn_engine::StemControls; backspn_engine::STEMS],
    loading: Option<Loading>,
    loaded: Option<Loaded>,
    informer: DeckInformer,
    cue_button: Hold,
    cup_button: Hold,
    tempo_fader: f32,
    tempo_lock: Option<TempoLock>,
    tempo_range: f32,
    key_notation: KeyNotation,
    rev_button: Hold,
    bend: [Hold; 2],
    slice_buttons: [Hold; backspn_engine::SLICES],
    events: Vec<DeckEvent>,
    loop_beats: f64,
    size_offset: usize,
    shown_size: Option<(f64, usize)>,
    pub show_pads: bool,
    pub panel: Option<PanelTab>,
    cue_map: bool,
    cue_name: Option<String>,
    confirm_autogrid: bool,
    pub tick: bool,
    pub focused: bool,
    move_mode: MoveMode,
    move_size: MoveSize,
    move_held: Option<(i32, std::time::Instant, u32)>,
    zoom_texture: Option<egui::TextureHandle>,
    overview_texture: Option<(egui::TextureHandle, OverviewKey)>,
    rect: Option<Rect>,
    wave_rect: Option<Rect>,
    zoom_height: f32,
    pub height: f32,
    chrome: f32,
    pub zoom_s: f64,
    pub end_warning_s: f64,
    pub auto_cue_db: Option<i32>,
    end_warning_since: Option<std::time::Instant>,
    pub compact: bool,
    size: DeckSize,
    last_tab: PanelTab,
    pub pending_open: Option<PathBuf>,
    pub pending_track: Option<i64>,
    pub pending_save_loop: bool,
    pub sample_deck: Option<Box<crate::sample_view::SampleView>>,
    pub sample_state: Option<backspn_engine::SampleState>,
    pub live: bool,
}

impl DeckView {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            stem_controls: [backspn_engine::StemControls::default(); backspn_engine::STEMS],
            loading: None,
            loaded: None,
            informer: DeckInformer::default(),
            cue_button: Hold::default(),
            cup_button: Hold::default(),
            tempo_fader: 0.0,
            tempo_lock: None,
            tempo_range: 0.08,
            key_notation: KeyNotation::default(),
            compact: false,
            size: DeckSize::Full,
            last_tab: PanelTab::Cue,
            rev_button: Hold::default(),
            bend: [Hold::default(), Hold::default()],
            slice_buttons: std::array::from_fn(|_| Hold::default()),
            events: Vec::new(),
            loop_beats: crate::status::DeckStatus::default().loop_beats,
            size_offset: 0,
            shown_size: None,
            show_pads: false,
            panel: None,
            cue_map: false,
            cue_name: None,
            confirm_autogrid: false,
            tick: false,
            focused: false,
            move_mode: crate::status::DeckStatus::default().move_mode,
            move_size: crate::status::DeckStatus::default().move_size,
            move_held: None,
            zoom_texture: None,
            overview_texture: None,
            rect: None,
            wave_rect: None,
            zoom_height: 80.0,
            height: 300.0,
            chrome: 200.0,
            zoom_s: DEFAULT_ZOOM_S,
            end_warning_s: crate::track_end::DEFAULT_WARNING_S,
            auto_cue_db: None,
            end_warning_since: None,
            pending_open: None,
            pending_track: None,
            pending_save_loop: false,
            sample_deck: None,
            live: false,
            sample_state: None,
        }
    }

    pub fn resend_stem_controls(&self, out: &mut Vec<Command>) {
        for (stem, controls) in self.stem_controls.iter().enumerate() {
            out.push(Command::Stem(
                self.index,
                StemCommand::Controls(stem, *controls),
            ));
        }
    }

    pub fn apply_stem(
        &mut self,
        stem: usize,
        part: backspn_control::StemPart,
        trigger: backspn_control::Trigger,
        out: &mut Vec<Command>,
    ) {
        if let Some(controls) = self.stem_controls.get_mut(stem)
            && crate::stems::apply(controls, part, trigger)
        {
            out.push(Command::Stem(
                self.index,
                StemCommand::Controls(stem, *controls),
            ));
        }
    }

    pub fn loop_beats(&self) -> f64 {
        self.loop_beats
    }

    pub fn set_loop_beats(&mut self, beats: f64) {
        self.loop_beats = beats;
    }

    pub fn move_mode(&self) -> MoveMode {
        self.move_mode
    }

    pub fn set_move_mode(&mut self, mode: MoveMode) {
        self.move_mode = mode;
    }

    pub fn move_size(&self) -> MoveSize {
        self.move_size
    }

    pub fn set_move_size(&mut self, size: MoveSize) {
        self.move_size = size;
    }

    pub fn move_command(&self, steps: i32) -> Option<DeckCommand> {
        crate::moves::command(self.move_mode, self.move_size, steps, self.loop_beats)
    }

    pub fn letter(&self) -> char {
        backspn_control::deck_letter(self.index)
    }

    fn deck(&self, command: DeckCommand) -> Command {
        Command::Deck(self.index, command)
    }

    pub fn load(&mut self, request: LoadRequest) {
        let (tx, rx) = mpsc::channel();
        let path = request.path.clone();
        let cover = request.cover.clone();
        let options = request.analysis;
        let (stems, output_rate) = (request.stems.clone(), request.output_rate);
        let missing =
            request.grid.is_none() || request.key_text.is_none() || request.lufs.is_none();
        let auto_cue = self
            .auto_cue_db
            .filter(|_| request.cue_s.is_none() && request.duplicate_of.is_none());
        std::thread::spawn(move || {
            let track = match backspn_decode::decode_file(&path) {
                Ok(track) => Arc::new(track),
                Err(e) => {
                    drop(tx.send(LoadEvent::Failed(e.to_string())));
                    return;
                }
            };
            let peaks = Peaks::compute(&track);
            let cover = cover.read().and_then(|b| covers::thumbnail(&b, COVER));
            let stems = stems
                .and_then(|file| {
                    crate::stems::load(&file, output_rate)
                        .map_err(|e| log::warn!("stems of {}: {e}", file.display()))
                        .ok()
                })
                .map(Box::new);
            let cue_s = auto_cue
                .and_then(|db| crate::auto_cue::position(track.samples(), track.sample_rate(), db));
            if tx
                .send(LoadEvent::Decoded(
                    Arc::clone(&track),
                    peaks,
                    cover,
                    stems,
                    cue_s,
                ))
                .is_err()
            {
                return;
            }
            if let Some(options) = options.filter(|_| missing) {
                let analysis = backspn_analysis::analyze(&track, &options);
                drop(tx.send(LoadEvent::Analyzed(Box::new(analysis))));
            }
        });
        for key in [Key::DeckLocked, Key::DeckType, Key::Unplayable] {
            self.informer.remove(key);
        }
        self.loading = Some(Loading {
            request,
            events: rx,
            decoded: false,
        });
    }

    pub fn apply_tempo(&mut self, trigger: backspn_control::Trigger, out: &mut Vec<Command>) {
        let range = self.tempo_range;
        self.move_tempo_fader(trigger, out, |f| 1.0 + f64::from(f * range));
    }

    pub fn apply_platter_tempo(
        &mut self,
        trigger: backspn_control::Trigger,
        out: &mut Vec<Command>,
    ) {
        self.move_tempo_fader(trigger, out, crate::platter::tempo_ratio);
    }

    pub fn apply_relative_tempo(&mut self, steps: f32, out: &mut Vec<Command>) {
        if self.tempo_lock.is_some() {
            return;
        }
        self.tempo_fader = relative_tempo_fader(self.tempo_fader, steps, self.tempo_range);
        let ratio = 1.0 + f64::from(self.tempo_fader * self.tempo_range);
        out.push(self.deck(DeckCommand::SetTempo(ratio)));
    }

    fn move_tempo_fader(
        &mut self,
        trigger: backspn_control::Trigger,
        out: &mut Vec<Command>,
        ratio: impl Fn(f32) -> f64,
    ) {
        if let Some(lock) = &mut self.tempo_lock {
            if let backspn_control::Trigger::Set(v) = trigger {
                lock.fader = Some(v);
            }
            return;
        }
        let fader = match trigger {
            backspn_control::Trigger::Set(v) => v * 2.0 - 1.0,
            backspn_control::Trigger::Delta(d) => self.tempo_fader + d / 100.0,
            _ => return,
        };
        self.tempo_fader = fader.clamp(-1.0, 1.0);
        out.push(self.deck(DeckCommand::SetTempo(ratio(self.tempo_fader))));
    }

    pub fn toggle_tempo_lock(&mut self, absolute: bool, out: &mut Vec<Command>) {
        if let Some(lock) = self.tempo_lock.take() {
            if let (true, Some(v)) = (absolute, lock.fader) {
                self.apply_tempo(backspn_control::Trigger::Set(v), out);
            }
        } else {
            self.tempo_lock = Some(TempoLock::default());
            self.tempo_fader = 0.0;
            out.push(self.deck(DeckCommand::SetTempo(1.0)));
        }
    }

    pub fn tempo_locked(&self) -> bool {
        self.tempo_lock.is_some()
    }

    pub fn watch_tempo_lock(&mut self, tempo: f64) {
        let Some(lock) = &mut self.tempo_lock else {
            return;
        };
        let at_one = (tempo - 1.0).abs() <= TEMPO_LOCK_EPSILON;
        if lock.settled && !at_one {
            self.tempo_lock = None;
        } else if at_one {
            lock.settled = true;
        }
    }

    pub fn tempo_fader_position(&self) -> f32 {
        self.tempo_fader
    }

    pub fn notify(&mut self, notice: Notice) {
        self.informer
            .post(notice, None, None, crate::deck_informer::now());
    }

    pub fn notify_for(&mut self, notice: Notice, key: Key) {
        let now = crate::deck_informer::now();
        let until = now + crate::deck_informer::LOADING_WARNING_S;
        self.informer.post(notice, Some(key), Some(until), now);
    }

    pub fn clear_notice(&mut self, key: Key) {
        self.informer.remove(key);
    }

    pub fn tempo_range(&self) -> f32 {
        self.tempo_range
    }

    pub fn tempo_range_index(&self) -> u8 {
        let at = TEMPO_RANGES
            .iter()
            .position(|r| (r - self.tempo_range).abs() < 1e-6)
            .unwrap_or(0);
        u8::try_from(at).unwrap_or(0)
    }

    pub fn set_tempo_range(&mut self, range: f32) {
        self.tempo_range = range;
    }

    pub fn apply_tempo_range(&mut self, trigger: backspn_control::Trigger) {
        if let Some(i) = tempo_range_after(self.tempo_range_index(), trigger) {
            self.tempo_range = TEMPO_RANGES[i];
        }
    }

    pub fn key_label(&self, state: &DeckState) -> Option<String> {
        header::key_label(
            self.key_text(),
            self.key_notation,
            state.keylock,
            state.key_adjust,
        )
    }

    pub fn screen_info(&self) -> Option<(&str, &Peaks)> {
        self.loaded.as_ref().map(|l| (l.name.as_str(), &l.peaks))
    }

    pub fn apply_key_adjust(
        &self,
        state: &DeckState,
        trigger: backspn_control::Trigger,
        out: &mut Vec<Command>,
    ) {
        if let Some(semitones) = key_adjust(state.key_adjust, trigger) {
            out.push(self.deck(DeckCommand::SetKeyAdjust(semitones)));
        }
    }

    pub fn set_key_notation(&mut self, notation: KeyNotation) {
        self.key_notation = notation;
    }

    pub fn key_text(&self) -> Option<&str> {
        self.loaded.as_ref().and_then(|l| l.key_text.as_deref())
    }

    pub fn track_end_warning(&mut self, state: &DeckState) {
        let on = crate::track_end::warning(state, self.end_warning_s);
        match (on, self.end_warning_since) {
            (true, None) => self.end_warning_since = Some(std::time::Instant::now()),
            (false, Some(_)) => self.end_warning_since = None,
            _ => {}
        }
    }

    pub fn lufs(&self) -> Option<f64> {
        self.loaded.as_ref().and_then(|l| l.lufs)
    }

    pub fn locked(&self) -> bool {
        self.loaded.as_ref().is_some_and(|l| l.locked)
    }

    pub fn set_locked(&mut self, locked: bool) {
        if let Some(l) = &mut self.loaded {
            l.locked = locked;
        }
    }

    pub fn cues(&self) -> Option<&CueList> {
        self.loaded.as_ref().map(|l| &l.cues)
    }

    pub fn cues_mut(&mut self) -> Option<&mut CueList> {
        self.loaded.as_mut().map(|l| &mut l.cues)
    }

    pub fn track_id(&self) -> Option<i64> {
        self.loaded.as_ref().and_then(|l| l.track_id)
    }

    pub fn audio(&self) -> Option<(Arc<backspn_decode::Track>, String)> {
        let l = self.loaded.as_ref()?;
        let name = l
            .path
            .file_stem()
            .map_or_else(|| l.name.clone(), |s| s.to_string_lossy().into_owned());
        Some((Arc::clone(&l.audio), name))
    }

    pub fn is_decoding(&self) -> bool {
        self.loading.as_ref().is_some_and(|l| !l.decoded)
    }

    pub fn is_busy(&self) -> bool {
        self.loading.is_some()
    }

    fn send_load(
        &self,
        track: Arc<backspn_decode::Track>,
        stems: Option<Box<crate::stems::StemLoad>>,
        out: &mut Vec<Command>,
    ) -> Option<Vec<backspn_formats::stem::StemInfo>> {
        let Some(load) = stems else {
            out.push(self.deck(DeckCommand::Load(track)));
            out.push(Command::Stem(self.index, StemCommand::Mastering(None)));
            return None;
        };
        let crate::stems::StemLoad {
            stems,
            master,
            info,
        } = *load;
        out.push(self.deck(DeckCommand::LoadStems(stems)));
        out.push(Command::Stem(
            self.index,
            StemCommand::Mastering(Some(master)),
        ));
        for (stem, controls) in self.stem_controls.iter().enumerate() {
            out.push(Command::Stem(
                self.index,
                StemCommand::Controls(stem, *controls),
            ));
        }
        Some(info)
    }

    #[allow(clippy::too_many_lines)]
    pub fn poll(&mut self, out: &mut Vec<Command>) -> Vec<DeckEvent> {
        let mut events = std::mem::take(&mut self.events);
        while let Some(loading) = &mut self.loading {
            let Ok(event) = loading.events.try_recv() else {
                return events;
            };
            match event {
                LoadEvent::Decoded(track, peaks, cover, stems, auto_cue_s) => {
                    loading.decoded = true;
                    let request = loading.request.clone();
                    let cues = CueList::new(&request.cues);
                    let audio = Arc::clone(&track);
                    let stem_info = self.send_load(track, stems, out);
                    let markers: Vec<f64> = cues
                        .cues()
                        .filter(|c| c.kind == CuePoint::GRID)
                        .map(|c| c.start_s)
                        .collect();
                    if let Some(grid) = request.grid {
                        let grid = Grid::new(grid.bpm, grid.anchor_s).with_markers(&markers);
                        out.push(self.deck(DeckCommand::SetGrid(Some(grid))));
                    }
                    out.push(self.deck(DeckCommand::RestoreCues {
                        cue_s: request.cue_s.or(auto_cue_s),
                        hotcues: cues.hotcues(),
                    }));
                    if let Some(track_id) = request.track_id {
                        events.push(DeckEvent::Overview {
                            track_id,
                            overview: peaks.overview(waveform::OVERVIEW_COLUMNS),
                        });
                    }
                    if let Some(from) = request.duplicate_of {
                        events.push(DeckEvent::Duplicate {
                            deck: self.index,
                            from,
                        });
                    }
                    let complete = request.grid.is_some()
                        && request.key_text.is_some()
                        && request.lufs.is_some();
                    self.loaded = Some(Loaded {
                        name: request.name,
                        title: request.title,
                        artist: request.artist,
                        comment: request.comment,
                        cover,
                        cover_texture: None,
                        peaks,
                        track_id: request.track_id,
                        key_text: request.key_text,
                        lufs: request.lufs,
                        locked: request.locked,
                        cues,
                        stems: stem_info,
                        audio,
                        path: request.path,
                    });
                    if complete {
                        self.loading = None;
                    }
                }
                LoadEvent::Analyzed(analysis) => {
                    let known_grid = loading.request.grid.is_some();
                    if !known_grid {
                        let grid = analysis.beatgrid.map(|g| Grid {
                            bpm: g.bpm,
                            anchor_s: g.anchor_s,
                            ..Grid::FIXED
                        });
                        out.push(self.deck(DeckCommand::SetGrid(grid)));
                    }
                    if let Some(loaded) = &mut self.loaded {
                        if loaded.key_text.is_none() {
                            loaded.key_text = analysis
                                .key
                                .and_then(|k| MusicalKey::from_value(k.nml_value()))
                                .map(MusicalKey::open_key);
                        }
                        if loaded.lufs.is_none() {
                            loaded.lufs = analysis
                                .loudness
                                .map(|l| l.integrated_lufs)
                                .filter(|l| l.is_finite());
                        }
                        if let Some(track_id) = loaded.track_id {
                            events.push(DeckEvent::Analyzed {
                                track_id,
                                analysis,
                                keep_grid: known_grid,
                            });
                        }
                    }
                    self.loading = None;
                }
                LoadEvent::Failed(e) => {
                    log::warn!("cannot load {}: {e}", loading.request.name);
                    self.notify_for(Notice::unplayable(), Key::Unplayable);
                    self.loading = None;
                }
            }
        }
        events
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &DeckState, sync: &SyncState, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        let frame = theme::card(&t);
        let inner = self.height - CARD_OUTER;
        self.zoom_height = (inner - self.chrome).max(MIN_ZOOM_HEIGHT);
        let response = frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(inner);
            let start = ui.cursor().top();
            self.header(ui, state, sync, out);
            if let Some(sample_deck) = &mut self.sample_deck {
                sample_deck.ui(ui, self.sample_state.as_ref(), &mut self.stem_controls, out);
            } else if self.size == DeckSize::Micro {
                self.overview_row(ui, state, true, out);
            } else {
                ui.horizontal_top(|ui| {
                    let gap = ui.spacing().item_spacing.x;
                    let left = (ui.available_width() - TEMPO_COLUMN - gap - 2.0).max(120.0);
                    let body = ui
                        .allocate_ui_with_layout(
                            vec2(left, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                self.control_row(ui, state, sync, out);
                                if self.size.parallel_wave().is_some() {
                                    self.overview_row(ui, state, false, out);
                                } else {
                                    self.waveforms(ui, state, out);
                                }
                            },
                        )
                        .response
                        .rect
                        .height();
                    ui.allocate_ui_with_layout(
                        vec2(TEMPO_COLUMN, body),
                        Layout::top_down(Align::Center),
                        |ui| self.tempo_fader(ui, state, sync, body, out),
                    );
                });
                self.bottom_bar(ui, state, out);
                if state.stems && !self.compact {
                    self.stem_row(ui, out);
                }
                if self.show_pads && !self.compact {
                    self.hotcue_row(ui, state, out);
                }
                if let Some(tab) = self.panel.filter(|_| !self.compact) {
                    self.advanced_panel(ui, state, tab, out);
                }
            }
            ui.cursor().top() - start - ui.spacing().item_spacing.y
        });
        let card = &response.response;
        if response.inner > 0.0 {
            self.chrome = if self.without_wave() {
                response.inner
            } else {
                response.inner - self.zoom_height
            };
        }
        self.track_drop(ui, card);
        self.rect = Some(card.rect);
    }

    pub fn wave_drop(&mut self, ui: &Ui, response: &egui::Response) {
        self.track_drop(ui, response);
        self.wave_rect = Some(response.rect);
    }

    fn track_drop(&mut self, ui: &Ui, response: &egui::Response) {
        if response.dnd_hover_payload::<TrackDrag>().is_some() {
            let t = theme::get(ui.ctx());
            let color = t.deck(self.index);
            let painter = ui.painter();
            painter.rect_filled(response.rect, 0.0, color.gamma_multiply(0.12));
            painter.rect_stroke(
                response.rect,
                0.0,
                egui::Stroke::new(2.0, color),
                egui::StrokeKind::Inside,
            );
            painter.text(
                response.rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("Load to deck {}", self.letter()),
                egui::FontId::proportional(20.0),
                color,
            );
        }
        if let Some(drag) = response.dnd_release_payload::<TrackDrag>() {
            self.pending_track = Some(drag.id);
        }
    }

    pub fn contains(&self, pos: egui::Pos2) -> bool {
        let parallel = self
            .wave_rect
            .filter(|_| self.size.parallel_wave().is_some());
        [self.rect, parallel]
            .into_iter()
            .flatten()
            .any(|r| r.contains(pos))
    }

    fn without_wave(&self) -> bool {
        self.sample_deck.is_some() || self.size.without_wave()
    }

    pub fn min_height(&self) -> f32 {
        let zoom = if self.without_wave() {
            0.0
        } else {
            MIN_ZOOM_HEIGHT
        };
        self.chrome + zoom + CARD_OUTER
    }

    pub fn flex(&self, grid: Grid) -> Grid {
        let markers: Vec<f64> = self
            .cues()
            .into_iter()
            .flat_map(CueList::cues)
            .filter(|c| c.kind == CuePoint::GRID)
            .map(|c| c.start_s)
            .collect();
        Grid::new(grid.bpm, grid.anchor_s).with_markers(&markers)
    }

    pub fn size(&self) -> DeckSize {
        self.size
    }

    pub fn set_size(&mut self, size: DeckSize) {
        if let Some(tab) = self.panel {
            self.last_tab = tab;
        }
        self.size = size;
        self.panel = (size == DeckSize::Advanced).then_some(self.last_tab);
    }

    pub fn zoom_s(&self) -> f64 {
        self.zoom_s
    }

    pub fn set_zoom(&mut self, half_window_s: f64) {
        if half_window_s.is_finite() {
            self.zoom_s = half_window_s.clamp(MIN_ZOOM_S, MAX_ZOOM_S);
        }
    }
}

fn tempo_range_after(at: u8, trigger: backspn_control::Trigger) -> Option<usize> {
    let n = TEMPO_RANGES.len();
    match trigger {
        backspn_control::Trigger::Delta(d) if d.round() != 0.0 => {
            let step = d.round() as i64;
            Some((i64::from(at) + step).rem_euclid(n as i64) as usize)
        }
        backspn_control::Trigger::Set(v) => {
            Some(((v.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize).min(n - 1))
        }
        _ => None,
    }
}

fn relative_tempo_fader(fader: f32, steps: f32, range: f32) -> f32 {
    let lowest = ((backspn_engine::MIN_TEMPO - 1.0) as f32) / range;
    let highest = ((backspn_engine::MAX_TEMPO - 1.0) as f32) / range;
    (fader + steps / 100.0).clamp(lowest, highest)
}

fn key_adjust(current: f32, trigger: backspn_control::Trigger) -> Option<f32> {
    let max = backspn_engine::MAX_KEY_ADJUST;
    let semitones = match trigger {
        backspn_control::Trigger::Set(v) => (v * 2.0 - 1.0) * max,
        backspn_control::Trigger::Delta(d) => current + d,
        _ => return None,
    };
    Some(semitones.clamp(-max, max))
}

#[cfg(test)]
mod tests {
    use super::header::{clock, key_label};
    use super::*;

    #[test]
    fn tempo_lock_holds_the_original_tempo_until_released() {
        use backspn_control::Trigger;
        let mut v = DeckView::new(0);
        let mut out = Vec::new();
        v.apply_tempo(Trigger::Set(0.75), &mut out);
        v.toggle_tempo_lock(true, &mut out);
        assert!(v.tempo_locked());
        assert_eq!(v.tempo_fader_position(), 0.0);
        out.clear();
        v.apply_tempo(Trigger::Set(0.9), &mut out);
        assert!(out.is_empty());
        v.watch_tempo_lock(1.06);
        assert!(v.tempo_locked());
        v.watch_tempo_lock(1.0);
        v.watch_tempo_lock(1.0004);
        assert!(v.tempo_locked(), "within 0.0005");
        v.toggle_tempo_lock(true, &mut out);
        assert!(!v.tempo_locked());
        assert!((v.tempo_fader_position() - 0.8).abs() < 1e-6);
        v.toggle_tempo_lock(true, &mut out);
        v.watch_tempo_lock(1.0);
        v.watch_tempo_lock(1.05);
        assert!(!v.tempo_locked());
    }

    #[test]
    fn a_relative_tempo_fader_goes_past_its_range() {
        let mut v = DeckView::new(0);
        let mut out = Vec::new();
        v.apply_relative_tempo(100.0, &mut out);
        v.apply_relative_tempo(50.0, &mut out);
        match out.last() {
            Some(Command::Deck(0, DeckCommand::SetTempo(t))) => {
                assert!((t - 1.12).abs() < 1e-5, "{t}");
            }
            other => panic!("{other:?}"),
        }
        assert!((relative_tempo_fader(0.0, -1e6, 0.08) - (-0.95 / 0.08)).abs() < 1e-3);
        assert!((relative_tempo_fader(0.0, 1e6, 0.08) - (3.0 / 0.08)).abs() < 1e-3);
        v.apply_tempo(backspn_control::Trigger::Delta(1.0), &mut out);
        assert!((v.tempo_fader_position() - 1.0).abs() < 1e-6);
        v.toggle_tempo_lock(false, &mut out);
        out.clear();
        v.apply_relative_tempo(10.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn tempo_range_steps_wrap_round() {
        use backspn_control::Trigger;
        assert_eq!(tempo_range_after(3, Trigger::Delta(1.0)), Some(4));
        assert_eq!(tempo_range_after(13, Trigger::Delta(1.0)), Some(0));
        assert_eq!(tempo_range_after(0, Trigger::Delta(-1.0)), Some(13));
        assert_eq!(tempo_range_after(3, Trigger::Set(1.0)), Some(13));
        assert_eq!(tempo_range_after(3, Trigger::Set(3.0 / 13.0)), Some(3));
        assert_eq!(tempo_range_after(3, Trigger::Press), None);
    }

    #[test]
    fn key_adjust_from_knob_and_encoder() {
        use backspn_control::Trigger;
        assert_eq!(key_adjust(0.0, Trigger::Set(1.0)), Some(12.0));
        assert_eq!(key_adjust(0.0, Trigger::Set(0.5)), Some(0.0));
        assert_eq!(key_adjust(0.0, Trigger::Set(0.0)), Some(-12.0));
        assert_eq!(key_adjust(0.3, Trigger::Delta(1.0)), Some(1.3));
        assert_eq!(key_adjust(11.5, Trigger::Delta(2.0)), Some(12.0));
        assert_eq!(key_adjust(0.0, Trigger::Press), None);
    }

    #[test]
    fn key_label_shows_the_shifted_key() {
        let n = KeyNotation::Sharps;
        assert_eq!(key_label(Some("1m"), n, false, 2.0).as_deref(), Some("Am"));
        assert_eq!(key_label(Some("1m"), n, true, 0.0).as_deref(), Some("Am"));
        assert_eq!(
            key_label(Some("1m"), n, true, 2.0).as_deref(),
            Some("Bm +2")
        );
        assert_eq!(
            key_label(Some("1m"), n, true, -1.0).as_deref(),
            Some("G#m -1")
        );
        assert_eq!(
            key_label(Some("1m"), n, true, 0.3).as_deref(),
            Some("+0.30")
        );
        assert_eq!(key_label(None, n, true, 0.0), None);
        assert_eq!(key_label(None, n, true, -2.0).as_deref(), Some("-2.00"));
    }

    #[test]
    fn the_key_control_colours_the_key_heard() {
        let am = MusicalKey::parse("1m");
        assert_eq!(header::resulting_key(Some("1m"), false, 2.0), am);
        assert_eq!(
            header::resulting_key(Some("1m"), true, 2.0),
            am.and_then(|k| k.shifted(2.0))
        );
        assert_eq!(header::resulting_key(Some("1m"), true, 0.3), None);
        assert_eq!(header::resulting_key(None, true, 0.0), None);
    }

    #[test]
    fn header_clock() {
        assert_eq!(clock(0.0), "00:00");
        assert_eq!(clock(316.9), "05:16");
        assert_eq!(clock(-3.0), "00:00");
    }

    #[test]
    fn sizes_cycle_and_advanced_shows_the_panel() {
        let mut deck = DeckView::new(0);
        assert_eq!(deck.size(), DeckSize::Full);
        deck.set_size(deck.size().next());
        assert_eq!(deck.size(), DeckSize::Advanced);
        assert_eq!(deck.panel, Some(PanelTab::Cue));
        deck.panel = Some(PanelTab::Grid);
        deck.set_size(deck.size().next());
        assert_eq!((deck.size(), deck.panel), (DeckSize::Micro, None));
        deck.set_size(DeckSize::Advanced);
        assert_eq!(deck.panel, Some(PanelTab::Grid), "remembers the tab");
    }

    #[test]
    fn hold_reports_edges_only() {
        let mut hold = Hold::default();
        assert_eq!(hold.update(false), None);
        assert_eq!(hold.update(true), Some(true));
        assert_eq!(hold.update(true), None);
        assert_eq!(hold.update(false), Some(false));
    }
}
