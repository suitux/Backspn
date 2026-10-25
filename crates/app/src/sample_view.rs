use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use backspn_control::feedback::{SampleFeedback, SampleSlotFeedback};
use backspn_control::{CellMode, SamplePart, SampleSlotPart, Trigger};
use backspn_engine::{
    CellAudio, CellParams, CellState, Command, PlayType, PlayerState, QUANTIZE_BEATS, SAMPLE_CELLS,
    SAMPLE_PAGES, SAMPLE_SLOTS, SampleBehaviour, SampleCommand, SampleDeckCommand, SampleState,
    StemCommand, StemControls, Track, TriggerType, sample_size_factor,
};
use backspn_formats::nml::{SampleSet, SetCell, SetSlot};
use backspn_library::{SampleSetRow, TrackRow};
use eframe::egui::{self, Align, Color32, ComboBox, Layout, RichText, Sense, Ui, vec2};
use serde::{Deserialize, Serialize};

use crate::browser::{SetDrag, TrackDrag};
use crate::i18n::tr;
use crate::screen::{SampleScreen, ScreenCell, SlotOverlay};
use crate::{theme, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeckKind {
    #[default]
    Track,
    #[serde(alias = "Remix")]
    SampleDeck,
}

const CELL_COLOURS: [[u8; 3]; 16] = [
    [220, 60, 60],
    [230, 120, 50],
    [235, 170, 50],
    [220, 210, 60],
    [150, 210, 60],
    [70, 200, 90],
    [60, 200, 160],
    [60, 190, 220],
    [60, 140, 230],
    [90, 90, 230],
    [150, 80, 230],
    [200, 80, 220],
    [230, 80, 170],
    [230, 100, 120],
    [170, 140, 110],
    [200, 200, 200],
];

#[derive(Debug, Clone, PartialEq)]
pub enum SampleEvent {
    LoadCell {
        slot: usize,
        cell: usize,
        track: i64,
    },
    LoadSelected {
        slot: usize,
        cell: usize,
    },
    SaveSet,
    Capture {
        slot: usize,
        cell: usize,
    },
    CaptureFrom {
        slot: usize,
        cell: usize,
        deck: usize,
    },
    CopyFromSlot {
        slot: usize,
        source: usize,
    },
    FromLoop {
        slot: usize,
        cell: usize,
        from: LoopDrag,
    },
    RotateCaptureSource,
    LoadSet(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopDrag {
    Deck(usize),
    Recorder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotDrag {
    pub deck: usize,
    pub slot: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckDrag(pub usize);

struct CellLoad {
    slot: usize,
    cell: usize,
    result: Receiver<Result<Arc<Track>, String>>,
}

#[allow(clippy::struct_excessive_bools)]
pub struct SampleView {
    deck: usize,
    pub set: SampleSet,
    pub tracks: Vec<Option<i64>>,
    pub durations: Vec<f64>,
    pub set_id: Option<i64>,
    pub modified: bool,
    pub page: usize,
    size_steps: [usize; SAMPLE_SLOTS],
    selected: [usize; SAMPLE_SLOTS],
    loads: Vec<CellLoad>,
    held: Option<(usize, usize)>,
    title_edit: Option<String>,
    events: Vec<SampleEvent>,
    pub library_sets: Vec<(i64, String)>,
    pub confirm_overwrite: Option<i64>,
    pub cell_mode: CellMode,
    pub capture_source: usize,
    pub can_lead: bool,
    pub is_master: bool,
    pub is_synced: bool,
    pages_width: f32,
}

impl std::fmt::Debug for SampleView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SampleView")
            .field("deck", &self.deck)
            .field("set", &self.set.title)
            .finish_non_exhaustive()
    }
}

impl SampleView {
    pub fn new(deck: usize) -> Self {
        Self {
            deck,
            set: SampleSet::default(),
            tracks: vec![None; SAMPLE_SLOTS * SAMPLE_CELLS],
            durations: vec![0.0; SAMPLE_SLOTS * SAMPLE_CELLS],
            set_id: None,
            modified: false,
            page: 0,
            size_steps: [11; SAMPLE_SLOTS],
            selected: [0; SAMPLE_SLOTS],
            loads: Vec::new(),
            held: None,
            title_edit: None,
            events: Vec::new(),
            library_sets: Vec::new(),
            confirm_overwrite: None,
            cell_mode: CellMode::default(),
            capture_source: 0,
            can_lead: true,
            is_master: false,
            is_synced: false,
            pages_width: 0.0,
        }
    }

    fn cmd(&self, c: SampleCommand) -> Command {
        Command::SampleDeck(self.deck, SampleDeckCommand::Deck(c))
    }

    fn at(slot: usize, cell: usize) -> usize {
        slot * SAMPLE_CELLS + cell
    }

    pub fn cell(&self, slot: usize, cell: usize) -> Option<&SetCell> {
        self.set
            .slots
            .get(slot)?
            .cells
            .iter()
            .find(|c| c.index as usize == cell)
    }

    fn cell_mut(&mut self, slot: usize, cell: usize) -> Option<&mut SetCell> {
        self.set
            .slots
            .get_mut(slot)?
            .cells
            .iter_mut()
            .find(|c| c.index as usize == cell)
    }

    pub fn take_events(&mut self) -> Vec<SampleEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn initial_commands(&self, behaviour: SampleBehaviour, out: &mut Vec<Command>) {
        out.push(self.cmd(SampleCommand::SetBehaviour(behaviour)));
        out.push(self.cmd(SampleCommand::SetQuantize {
            on: self.set.quant_on,
            beats: quantize_beats(self.set.quant_index),
        }));
        if let Some(bpm) = self.set.bpm {
            out.push(self.cmd(SampleCommand::SetBaseBpm(bpm)));
        }
        for (slot, s) in self.set.slots.iter().enumerate() {
            out.push(self.cmd(SampleCommand::SetKeylock {
                slot,
                on: s.keylock,
            }));
            out.push(self.cmd(SampleCommand::SetPunch { slot, on: s.punch }));
        }
    }

    pub fn params_of(cell: &SetCell) -> CellParams {
        CellParams {
            play_type: if cell.play_type == 1 {
                PlayType::OneShot
            } else {
                PlayType::Loop
            },
            trigger_type: if cell.trigger_type == 0 {
                TriggerType::Gate
            } else {
                TriggerType::Latch
            },
            synced: cell.sync,
            reverse: cell.reverse,
            speed: cell.speed,
            pitch: (cell.transpose * 12.0) as f32,
            gain: cell.gain as f32,
            offset_beats: cell.offset,
            start_beats: cell.start_marker,
            end_beats: cell.end_marker,
            bpm: cell.bpm,
        }
    }

    pub fn load_cell(
        &mut self,
        slot: usize,
        cell: usize,
        track: &TrackRow,
        auto_gain_db: Option<f32>,
        startup_volume: &str,
    ) {
        if slot >= SAMPLE_SLOTS || cell >= SAMPLE_CELLS {
            return;
        }
        let path = track
            .path
            .split('#')
            .next()
            .unwrap_or(&track.path)
            .to_owned();
        let mut location = backspn_library::paths::location_of(Path::new(&path), startup_volume);
        location.volume_id = None;
        let old = self.cell(slot, cell).cloned();
        let mut new = SetCell::new(cell as u32, location);
        if let Some(old) = old {
            new.sync = old.sync;
            new.reverse = old.reverse;
            new.trigger_type = old.trigger_type;
        }
        new.name.clone_from(&track.title);
        new.bpm = track.bpm.unwrap_or(0.0);
        new.play_type = match track.sample_type {
            Some(0) => 0,
            _ => 1,
        };
        if let Some(db) = auto_gain_db {
            new.gain = f64::from((db / 24.0 + 0.5).clamp(0.0, 1.0));
        }
        self.put_cell(slot, new);
        self.tracks[Self::at(slot, cell)] = Some(track.id);
        self.durations[Self::at(slot, cell)] = track.duration_s.unwrap_or(0.0);
        self.decode(slot, cell, path);
    }

    pub fn copy_cell_attributes(
        &mut self,
        slot: usize,
        cell: usize,
        from: &SetCell,
        out: &mut Vec<Command>,
    ) {
        let Some(c) = self.cell_mut(slot, cell) else {
            return;
        };
        let (index, location) = (c.index, c.location.clone());
        *c = from.clone();
        c.index = index;
        c.location = location;
        self.push_params(slot, cell, out);
    }

    pub fn load_captured(
        &mut self,
        slot: usize,
        cell: usize,
        captured: &crate::sample_capture::Captured,
        track: &TrackRow,
        startup_volume: &str,
        out: &mut Vec<Command>,
    ) {
        if slot >= SAMPLE_SLOTS || cell >= SAMPLE_CELLS {
            return;
        }
        let mut location =
            backspn_library::paths::location_of(Path::new(&track.path), startup_volume);
        location.volume_id = None;
        let old = self.cell(slot, cell).cloned();
        let mut new = SetCell::new(cell as u32, location);
        if let Some(old) = old {
            new.sync = old.sync;
            new.reverse = old.reverse;
            new.trigger_type = old.trigger_type;
        }
        new.name.clone_from(&captured.name);
        new.bpm = captured.bpm;
        new.play_type = 0;
        let params = Self::params_of(&new);
        self.put_cell(slot, new);
        self.tracks[Self::at(slot, cell)] = Some(track.id);
        self.durations[Self::at(slot, cell)] = captured.track.duration_s();
        self.loads.retain(|l| (l.slot, l.cell) != (slot, cell));
        out.push(self.cmd(SampleCommand::LoadCell {
            slot,
            cell,
            audio: CellAudio {
                track: Arc::clone(&captured.track),
                params,
            },
        }));
        if self.set.slots[slot].active_cell as usize == cell {
            out.push(self.cmd(SampleCommand::SetActiveCell { slot, cell }));
        }
    }

    fn put_cell(&mut self, slot: usize, cell: SetCell) {
        let cells = &mut self.set.slots[slot].cells;
        cells.retain(|c| c.index != cell.index);
        cells.push(cell);
        cells.sort_by_key(|c| c.index);
        self.modified = true;
    }

    fn decode(&mut self, slot: usize, cell: usize, path: String) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = backspn_decode::decode_file(&path)
                .map(Arc::new)
                .map_err(|e| e.to_string());
            drop(tx.send(result));
        });
        self.loads.retain(|l| (l.slot, l.cell) != (slot, cell));
        self.loads.push(CellLoad {
            slot,
            cell,
            result: rx,
        });
    }

    pub fn poll(&mut self, out: &mut Vec<Command>) -> Vec<String> {
        let mut failures = Vec::new();
        let mut finished = Vec::new();
        self.loads.retain(|load| match load.result.try_recv() {
            Ok(result) => {
                finished.push((load.slot, load.cell, result));
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => false,
            Err(mpsc::TryRecvError::Empty) => true,
        });
        for (slot, cell, result) in finished {
            match result {
                Ok(track) => {
                    let Some(params) = self.cell(slot, cell).map(Self::params_of) else {
                        continue;
                    };
                    self.durations[Self::at(slot, cell)] = track.duration_s();
                    out.push(self.cmd(SampleCommand::LoadCell {
                        slot,
                        cell,
                        audio: CellAudio { track, params },
                    }));
                    if self.set.slots[slot].active_cell as usize == cell {
                        out.push(self.cmd(SampleCommand::SetActiveCell { slot, cell }));
                    }
                }
                Err(e) => {
                    failures.push(e);
                    self.clear_cell(slot, cell);
                }
            }
        }
        failures
    }

    fn clear_cell(&mut self, slot: usize, cell: usize) {
        if let Some(s) = self.set.slots.get_mut(slot) {
            s.cells.retain(|c| c.index as usize != cell);
        }
        self.tracks[Self::at(slot, cell)] = None;
        self.durations[Self::at(slot, cell)] = 0.0;
        self.modified = true;
    }

    pub fn unload_cell(&mut self, slot: usize, cell: usize, out: &mut Vec<Command>) {
        self.clear_cell(slot, cell);
        out.push(self.cmd(SampleCommand::UnloadCell { slot, cell }));
    }

    fn push_params(&mut self, slot: usize, cell: usize, out: &mut Vec<Command>) {
        self.modified = true;
        if let Some(c) = self.cell(slot, cell) {
            out.push(self.cmd(SampleCommand::SetCellParams {
                slot,
                cell,
                params: Self::params_of(c),
            }));
        }
    }

    pub fn trigger(&mut self, slot: usize, cell: usize, pressed: bool, out: &mut Vec<Command>) {
        out.push(self.cmd(SampleCommand::Trigger {
            slot,
            cell,
            pressed,
        }));
    }

    pub fn alter_size(&mut self, slot: usize, grow: Option<bool>, out: &mut Vec<Command>) {
        let cell = self.set.slots.get(slot).map(|s| s.active_cell as usize);
        let Some(cell) = cell else { return };
        let duration = self.durations[Self::at(slot, cell)];
        let Some(c) = self.cell_mut(slot, cell) else {
            return;
        };
        let changed = if let Some(grow) = grow {
            let mut p = Self::params_of(c);
            let ok = p.alter_range(true, grow, duration);
            if ok {
                c.start_marker = p.start_beats;
                c.end_marker = p.end_beats;
            }
            ok
        } else {
            c.start_marker = 0.0;
            c.end_marker = 0.0;
            self.size_steps[slot] = 11;
            true
        };
        if changed {
            self.push_params(slot, cell, out);
        }
    }

    pub fn set_size_step(&mut self, slot: usize, step: usize, out: &mut Vec<Command>) {
        let cell = self.set.slots.get(slot).map(|s| s.active_cell as usize);
        let Some(cell) = cell else { return };
        let duration = self.durations[Self::at(slot, cell)];
        let Some(c) = self.cell_mut(slot, cell) else {
            return;
        };
        let mut p = Self::params_of(c);
        if p.set_size(sample_size_factor(step), duration) {
            c.end_marker = p.end_beats;
            self.size_steps[slot] = step;
            self.push_params(slot, cell, out);
        }
    }

    pub fn load_set(
        &mut self,
        row: &SampleSetRow,
        tracks: &[Option<TrackRow>],
        out: &mut Vec<Command>,
        behaviour: SampleBehaviour,
    ) {
        for slot in 0..SAMPLE_SLOTS {
            for cell in 0..SAMPLE_CELLS {
                if self.cell(slot, cell).is_some() {
                    out.push(self.cmd(SampleCommand::UnloadCell { slot, cell }));
                }
            }
        }
        self.loads.clear();
        self.set = row.set.clone();
        self.set.slots.resize_with(SAMPLE_SLOTS, SetSlot::default);
        self.set_id = Some(row.id);
        self.modified = false;
        self.tracks = vec![None; SAMPLE_SLOTS * SAMPLE_CELLS];
        self.durations = vec![0.0; SAMPLE_SLOTS * SAMPLE_CELLS];
        self.size_steps = [11; SAMPLE_SLOTS];
        for (slot, s) in self.set.slots.iter().enumerate() {
            for c in &s.cells {
                let cell = c.index as usize;
                if cell >= SAMPLE_CELLS {
                    continue;
                }
                let at = Self::at(slot, cell);
                self.tracks[at] = row.tracks.get(at).copied().flatten();
                if let Some(t) = tracks.get(at).and_then(Option::as_ref) {
                    self.durations[at] = t.duration_s.unwrap_or(0.0);
                }
            }
        }
        self.initial_commands(behaviour, out);
        let to_load: Vec<(usize, usize, String)> = tracks
            .iter()
            .enumerate()
            .filter_map(|(at, t)| {
                let t = t.as_ref()?;
                let path = t.path.split('#').next().unwrap_or(&t.path).to_owned();
                Some((at / SAMPLE_CELLS, at % SAMPLE_CELLS, path))
            })
            .collect();
        for (slot, cell, path) in to_load {
            self.decode(slot, cell, path);
        }
    }

    pub fn clear(&mut self, out: &mut Vec<Command>) {
        for slot in 0..SAMPLE_SLOTS {
            for cell in 0..SAMPLE_CELLS {
                if self.cell(slot, cell).is_some() {
                    out.push(self.cmd(SampleCommand::UnloadCell { slot, cell }));
                }
            }
        }
        self.loads.clear();
        self.set = SampleSet::default();
        self.set_id = None;
        self.modified = false;
        self.tracks = vec![None; SAMPLE_SLOTS * SAMPLE_CELLS];
        self.durations = vec![0.0; SAMPLE_SLOTS * SAMPLE_CELLS];
    }

    pub fn set_to_save(&self, state: &SampleState) -> SampleSet {
        let mut set = self.set.clone();
        set.bpm = Some(state.base_bpm);
        if set.title.trim().is_empty() {
            set.title = SampleSet::default().title;
        }
        set
    }

    pub fn has_cells(&self) -> bool {
        self.set.slots.iter().any(|s| !s.cells.is_empty())
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        state: Option<&SampleState>,
        controls: &mut [StemControls; SAMPLE_SLOTS],
        out: &mut Vec<Command>,
    ) {
        let t = theme::get(ui.ctx());
        let colour = t.deck(self.deck);
        let Some(state) = state else {
            ui.label(RichText::new(tr("Sample deck starting…")).color(t.text_dim));
            return;
        };
        self.set_row(ui, state, out, &t, colour);
        ui.columns(SAMPLE_SLOTS, |columns| {
            for (slot, ui) in columns.iter_mut().enumerate() {
                self.slot_column(ui, slot, state, &mut controls[slot], out, &t, colour);
            }
        });
    }

    #[allow(clippy::too_many_lines)]
    fn set_row(
        &mut self,
        ui: &mut Ui,
        state: &SampleState,
        out: &mut Vec<Command>,
        t: &theme::Theme,
        colour: Color32,
    ) {
        ui.horizontal_wrapped(|ui| {
            if let Some(edit) = &mut self.title_edit {
                let response = ui.add(egui::TextEdit::singleline(edit).desired_width(160.0));
                if response.lost_focus() {
                    let title = edit.trim().to_owned();
                    self.title_edit = None;
                    if !title.is_empty() && title != self.set.title {
                        self.set.title = title;
                        self.modified = true;
                    }
                }
            } else {
                let title = ui
                    .add(
                        egui::Label::new(RichText::new(&self.set.title).strong().size(14.0))
                            .sense(Sense::click()),
                    )
                    .on_hover_text(tr("Double-click to rename"));
                if title.double_clicked() {
                    self.title_edit = Some(self.set.title.clone());
                }
                if let Some(drag) = title.dnd_release_payload::<SetDrag>() {
                    self.events.push(SampleEvent::LoadSet(drag.0));
                }
            }
            if self.modified {
                ui.label(RichText::new("•").color(t.accent));
            }
            let mut load = None;
            ComboBox::from_id_salt(("sample_deck set", self.deck))
                .selected_text(tr("Load…"))
                .width(80.0)
                .show_ui(ui, |ui| {
                    for (id, title) in &self.library_sets {
                        if ui.selectable_label(false, title).clicked() {
                            load = Some(*id);
                        }
                    }
                    if self.library_sets.is_empty() {
                        ui.label(RichText::new(tr("No Sample Sets in the collection")).weak());
                    }
                });
            if let Some(id) = load {
                self.events.push(SampleEvent::LoadSet(id));
            }
            if ui
                .button(tr("Save"))
                .on_hover_text(tr("Save Sample Set to the collection"))
                .clicked()
            {
                self.events.push(SampleEvent::SaveSet);
            }
            if ui
                .button(tr("New"))
                .on_hover_text(tr("New empty Sample Set"))
                .clicked()
            {
                self.clear(out);
            }
            ui.add_space(8.0);
            let play = widgets::toggle(ui, "PLAY", state.playing, t.play)
                .on_hover_text(tr("Deck Play: the sample deck's transport"));
            if play.clicked() {
                out.push(self.cmd(SampleCommand::Play(!state.playing)));
            }
            let sync = widgets::toggle(ui, "SYNC", self.is_synced, t.sync);
            if sync.clicked() {
                out.push(Command::Sync(backspn_engine::SyncCommand::SetSync(
                    self.deck,
                    !self.is_synced,
                )));
            }
            if self.can_lead
                && widgets::toggle(ui, "MASTER", self.is_master, t.master)
                    .on_hover_text(tr("Make this deck the tempo master"))
                    .clicked()
            {
                let master = (!self.is_master).then_some(self.deck);
                out.push(Command::Sync(backspn_engine::SyncCommand::SetMaster(
                    master,
                )));
            }
            ui.label(
                RichText::new(format!("{:.2}", state.bpm))
                    .monospace()
                    .size(15.0)
                    .color(t.text),
            );
            let beat = state.beat.max(0.0);
            ui.label(
                RichText::new(format!("{}.{}", beat as i64 / 4 + 1, beat as i64 % 4 + 1))
                    .monospace()
                    .color(t.text_dim),
            )
            .on_hover_text(tr("Beat position"));
            let quant = widgets::toggle(ui, "Q", self.set.quant_on, t.accent)
                .on_hover_text(tr("Quantize On: triggers wait for the next quantize point"));
            if quant.clicked() {
                self.set.quant_on = !self.set.quant_on;
                self.modified = true;
                out.push(self.cmd(SampleCommand::SetQuantize {
                    on: self.set.quant_on,
                    beats: quantize_beats(self.set.quant_index),
                }));
            }
            let mut index = self.set.quant_index as usize;
            ComboBox::from_id_salt(("sample_deck quant", self.deck))
                .selected_text(quantize_label(index))
                .width(52.0)
                .show_ui(ui, |ui| {
                    for i in 0..QUANTIZE_BEATS.len() {
                        ui.selectable_value(&mut index, i, quantize_label(i));
                    }
                });
            if index != self.set.quant_index as usize {
                self.set.quant_index = index as u32;
                self.modified = true;
                out.push(self.cmd(SampleCommand::SetQuantize {
                    on: self.set.quant_on,
                    beats: quantize_beats(index as u32),
                }));
            }
            if ui
                .button(capture_label(self.capture_source))
                .on_hover_text(tr(
                    "Capture Source: where an empty cell captures from (click: next)",
                ))
                .clicked()
            {
                self.events.push(SampleEvent::RotateCaptureSource);
            }
            let room = ui.available_width();
            if room < self.pages_width {
                ui.end_row();
            }
            let pages = ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for page in (0..SAMPLE_PAGES).rev() {
                    let on = page == self.page;
                    if widgets::toggle(ui, &(page + 1).to_string(), on, colour)
                        .on_hover_text(tr("Sample page"))
                        .clicked()
                    {
                        self.page = page;
                    }
                }
                ui.min_rect().width()
            });
            self.pages_width = pages.inner;
        });
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn slot_column(
        &mut self,
        ui: &mut Ui,
        slot: usize,
        state: &SampleState,
        controls: &mut StemControls,
        out: &mut Vec<Command>,
        t: &theme::Theme,
        colour: Color32,
    ) {
        let slot_state = state.slots[slot];
        let active = self.set.slots[slot].active_cell as usize;
        let name = self
            .cell(slot, active)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let fill = self
            .cell(slot, active)
            .map_or(t.control, |c| Self::colour(c, colour));
        let playing = slot_state.player == PlayerState::Playing;
        let label = egui::Button::new(
            RichText::new(if name.is_empty() { "—" } else { &name })
                .color(theme::Theme::on(fill))
                .strong(),
        )
        .fill(if playing {
            fill
        } else {
            fill.gamma_multiply(0.6)
        })
        .min_size(vec2(ui.available_width(), 18.0));
        let header = ui
            .add(label)
            .interact(Sense::click_and_drag())
            .on_hover_text(tr(
                "Active cell: click to trigger (Slot Capture/Trigger/Mute)",
            ));
        if header.clicked() {
            self.primary(slot, state, controls, out);
        }
        header.dnd_set_drag_payload(SlotDrag {
            deck: self.deck,
            slot,
        });
        if let Some(event) = slot_drop(&header, slot, active) {
            self.events.push(event);
        }
        for row in 0..4 {
            let cell = self.page * 4 + row;
            self.cell_pad(ui, slot, cell, slot_state.cells[cell], out, colour);
        }
        ui.horizontal_wrapped(|ui| {
            let s = self.set.slots[slot].clone();
            let c = self.cell(slot, active).cloned();
            let one_shot = c.as_ref().is_some_and(|c| c.play_type == 1);
            if widgets::toggle(
                ui,
                if one_shot { "1SHOT" } else { "LOOP" },
                !one_shot,
                colour,
            )
            .on_hover_text(tr("Slot Play Mode: loop or one-shot"))
            .clicked()
                && let Some(c) = self.cell_mut(slot, active)
            {
                c.play_type = u32::from(!one_shot);
                self.push_params(slot, active, out);
            }
            let gate = c.as_ref().is_some_and(|c| c.trigger_type == 0);
            if widgets::toggle(ui, if gate { "GATE" } else { "LATCH" }, gate, colour)
                .on_hover_text(tr("Slot Trigger Type: gate stops on release"))
                .clicked()
                && let Some(c) = self.cell_mut(slot, active)
            {
                c.trigger_type = u32::from(gate);
                self.push_params(slot, active, out);
            }
            if widgets::toggle(ui, "KL", s.keylock, colour)
                .on_hover_text(tr("Slot Keylock On"))
                .clicked()
            {
                self.set.slots[slot].keylock = !s.keylock;
                self.modified = true;
                out.push(self.cmd(SampleCommand::SetKeylock {
                    slot,
                    on: !s.keylock,
                }));
            }
            if widgets::toggle(ui, "PUNCH", s.punch, colour)
                .on_hover_text(tr(
                    "Slot Punch On: a new cell goes on from where the last one was",
                ))
                .clicked()
            {
                self.set.slots[slot].punch = !s.punch;
                self.modified = true;
                out.push(self.cmd(SampleCommand::SetPunch { slot, on: !s.punch }));
            }
            if ui
                .small_button("÷2")
                .on_hover_text(tr("Slot Size /2"))
                .clicked()
            {
                self.alter_size(slot, Some(false), out);
            }
            if ui
                .small_button("×2")
                .on_hover_text(tr("Slot Size x2"))
                .clicked()
            {
                self.alter_size(slot, Some(true), out);
            }
            if ui
                .small_button("⟲")
                .on_hover_text(tr("Slot Size Reset: the whole sample"))
                .clicked()
            {
                self.alter_size(slot, None, out);
            }
            let mut step = self.size_steps[slot];
            ComboBox::from_id_salt(("sample_deck size", self.deck, slot))
                .selected_text(size_label(step))
                .width(56.0)
                .show_ui(ui, |ui| {
                    for s in 0..backspn_engine::SAMPLE_SIZE_STEPS {
                        ui.selectable_value(&mut step, s, size_label(s));
                    }
                });
            if step != self.size_steps[slot] {
                self.set_size_step(slot, step, out);
            }
        });
        self.strip(ui, slot, controls, out, colour);
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn cell_pad(
        &mut self,
        ui: &mut Ui,
        slot: usize,
        cell: usize,
        cell_state: CellState,
        out: &mut Vec<Command>,
        colour: Color32,
    ) {
        let set_cell = self.cell(slot, cell).cloned();
        let base = set_cell.as_ref().map(|c| Self::colour(c, colour));
        let fill = match (cell_state, base) {
            (CellState::Empty, _) | (_, None) => None,
            (CellState::Playing, Some(c)) => Some(c),
            (CellState::Queued, Some(c)) => {
                ui.ctx().request_repaint();
                let on = (ui.input(|i| i.time) * 6.0) as i64 % 2 == 0;
                Some(if on { c } else { c.gamma_multiply(0.35) })
            }
            (CellState::Played | CellState::BeforeStart, Some(c)) => Some(c.gamma_multiply(0.45)),
            (CellState::Loaded, Some(c)) => Some(c.gamma_multiply(0.6)),
        };
        let name = set_cell.as_ref().map_or("", |c| c.name.as_str());
        let marked = self.set.slots[slot].active_cell as usize == cell;
        let text = if marked && !name.is_empty() {
            format!("▸ {name}")
        } else {
            name.to_owned()
        };
        let size = vec2(ui.available_width(), 26.0);
        let response = widgets::pad(ui, &text, fill, size).on_hover_text(tr(
            "Click: trigger (empty: select) · Right-click: options · Drop a track to load",
        ));
        let down = response.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
        match (self.held, down) {
            (None, true) => {
                self.held = Some((slot, cell));
                self.trigger(slot, cell, true, out);
            }
            (Some(h), false) if h == (slot, cell) => {
                self.held = None;
                self.trigger(slot, cell, false, out);
            }
            _ => {}
        }
        if let Some(drag) = response.dnd_release_payload::<TrackDrag>() {
            self.events.extend(cell_drop(slot, cell, &drag));
        }
        if let Some(from) = response.dnd_release_payload::<LoopDrag>() {
            self.events.push(SampleEvent::FromLoop {
                slot,
                cell,
                from: *from,
            });
        }
        if response.dnd_hover_payload::<TrackDrag>().is_some()
            || response.dnd_hover_payload::<LoopDrag>().is_some()
        {
            ui.painter().rect_stroke(
                response.rect,
                2.0,
                egui::Stroke::new(2.0, colour),
                egui::StrokeKind::Inside,
            );
        }
        response.context_menu(|ui| {
            if ui.button(tr("Load selected track")).clicked() {
                self.events.push(SampleEvent::LoadSelected { slot, cell });
                ui.close();
            }
            if set_cell.is_none() {
                return;
            }
            if ui.button(tr("Delete")).clicked() {
                self.unload_cell(slot, cell, out);
                ui.close();
            }
            if ui.button(tr("Make active")).clicked() {
                self.set.slots[slot].active_cell = cell as u32;
                self.modified = true;
                out.push(self.cmd(SampleCommand::SetActiveCell { slot, cell }));
                ui.close();
            }
            ui.separator();
            let mut changed = false;
            if let Some(c) = self.cell_mut(slot, cell) {
                let one_shot = c.play_type == 1;
                if ui.selectable_label(!one_shot, tr("Loop")).clicked() {
                    c.play_type = 0;
                    changed = true;
                }
                if ui.selectable_label(one_shot, tr("One-shot")).clicked() {
                    c.play_type = 1;
                    changed = true;
                }
                ui.separator();
                let gate = c.trigger_type == 0;
                if ui.selectable_label(!gate, tr("Latch")).clicked() {
                    c.trigger_type = 1;
                    changed = true;
                }
                if ui.selectable_label(gate, tr("Gate")).clicked() {
                    c.trigger_type = 0;
                    changed = true;
                }
                ui.separator();
                changed |= ui.checkbox(&mut c.sync, tr("Sync")).changed();
                changed |= ui.checkbox(&mut c.reverse, tr("Reverse")).changed();
                ui.separator();
                let mut gain = c.gain as f32;
                if ui
                    .add(egui::Slider::new(&mut gain, 0.0..=1.0).text(tr("Gain")))
                    .changed()
                {
                    c.gain = f64::from(gain);
                    changed = true;
                }
                let mut speed = c.speed as f32;
                if ui
                    .add(egui::Slider::new(&mut speed, 0.25..=4.0).text(tr("Speed")))
                    .changed()
                {
                    c.speed = f64::from(speed);
                    changed = true;
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    for (i, rgb) in CELL_COLOURS.iter().enumerate() {
                        let swatch = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                        let on = c.color as usize == i + 1;
                        if widgets::dot(ui, on, swatch, tr("Colour")).clicked() {
                            c.color = i as u32 + 1;
                            changed = true;
                        }
                    }
                });
            }
            if changed {
                self.push_params(slot, cell, out);
            }
        });
    }

    pub fn primary(
        &mut self,
        slot: usize,
        state: &SampleState,
        controls: &mut StemControls,
        out: &mut Vec<Command>,
    ) {
        let active = self.set.slots[slot].active_cell as usize;
        let Some(cell) = self.cell(slot, active).cloned() else {
            self.events
                .push(SampleEvent::Capture { slot, cell: active });
            return;
        };
        match state.slots[slot].cells[active] {
            CellState::Empty => self
                .events
                .push(SampleEvent::Capture { slot, cell: active }),
            CellState::Playing | CellState::Played | CellState::BeforeStart => {
                if cell.trigger_type == 0 {
                    self.trigger(slot, active, true, out);
                } else if cell.play_type == 1 {
                    out.push(self.cmd(SampleCommand::Stop { slot, cell: active }));
                } else {
                    controls.mute = !controls.mute;
                    out.push(Command::Stem(
                        self.deck,
                        StemCommand::Controls(slot, *controls),
                    ));
                }
            }
            CellState::Loaded | CellState::Queued => self.trigger(slot, active, true, out),
        }
    }

    fn strip(
        &self,
        ui: &mut Ui,
        slot: usize,
        controls: &mut StemControls,
        out: &mut Vec<Command>,
        colour: Color32,
    ) {
        let t = theme::get(ui.ctx());
        let c = *controls;
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            let mut volume = c.volume;
            if widgets::knob_compact(ui, &mut volume, 0.0..=1.0, 1.0, "VOL").changed() {
                controls.volume = volume;
                changed = true;
            }
            if widgets::toggle(ui, "FLT", c.filter_on, colour).clicked() {
                controls.filter_on = !c.filter_on;
                changed = true;
            }
            let mut filter = c.filter;
            if widgets::knob_compact(ui, &mut filter, 0.0..=1.0, 0.5, "FLTR").changed() {
                controls.filter = filter;
                changed = true;
            }
            if widgets::toggle(ui, "FX", c.fx_on, colour).clicked() {
                controls.fx_on = !c.fx_on;
                changed = true;
            }
            let mut send = c.fx_send;
            if widgets::knob_compact(ui, &mut send, 0.0..=1.0, 1.0, "FX").changed() {
                controls.fx_send = send;
                changed = true;
            }
            if widgets::toggle(ui, "M", c.mute, t.danger)
                .on_hover_text(tr("Slot Mute On"))
                .clicked()
            {
                controls.mute = !c.mute;
                changed = true;
            }
        });
        if changed {
            out.push(Command::Stem(
                self.deck,
                StemCommand::Controls(slot, *controls),
            ));
        }
    }

    fn colour(cell: &SetCell, deck: Color32) -> Color32 {
        cell_rgb(cell.color).map_or(deck, |[r, g, b]| Color32::from_rgb(r, g, b))
    }
}

fn cell_rgb(color: u32) -> Option<[u8; 3]> {
    (color != 0).then(|| CELL_COLOURS[(color as usize - 1) % CELL_COLOURS.len()])
}

fn slot_drop(response: &egui::Response, slot: usize, active: usize) -> Option<SampleEvent> {
    if let Some(drag) = response.dnd_release_payload::<TrackDrag>() {
        return Some(SampleEvent::LoadCell {
            slot,
            cell: active,
            track: drag.id,
        });
    }
    if let Some(deck) = response.dnd_release_payload::<DeckDrag>() {
        return Some(SampleEvent::CaptureFrom {
            slot,
            cell: active,
            deck: deck.0,
        });
    }
    if let Some(from) = response.dnd_release_payload::<LoopDrag>() {
        return Some(SampleEvent::FromLoop {
            slot,
            cell: active,
            from: *from,
        });
    }
    let from = response.dnd_release_payload::<SlotDrag>()?;
    Some(SampleEvent::CopyFromSlot {
        slot,
        source: from.deck * SAMPLE_SLOTS + from.slot,
    })
}

fn cell_drop(slot: usize, cell: usize, drag: &TrackDrag) -> Vec<SampleEvent> {
    let tracks: &[i64] = if drag.tracks.is_empty() {
        std::slice::from_ref(&drag.id)
    } else {
        &drag.tracks
    };
    let mut target = cell;
    tracks
        .iter()
        .map(|&track| {
            let event = SampleEvent::LoadCell {
                slot,
                cell: target,
                track,
            };
            if target < SAMPLE_CELLS - 1 {
                target += 1;
            }
            event
        })
        .collect()
}

fn load_event(
    slot: usize,
    active: usize,
    part: SampleSlotPart,
    trigger: Trigger,
) -> Option<SampleEvent> {
    let pressed = trigger == Trigger::Press;
    let cell = active;
    match part {
        SampleSlotPart::LoadFromList if pressed => Some(SampleEvent::LoadSelected { slot, cell }),
        SampleSlotPart::Capture if pressed => Some(SampleEvent::Capture { slot, cell }),
        SampleSlotPart::CaptureLoopRecorder if pressed => Some(SampleEvent::FromLoop {
            slot,
            cell,
            from: LoopDrag::Recorder,
        }),
        SampleSlotPart::CopyFromSlot => {
            copy_source(trigger).map(|source| SampleEvent::CopyFromSlot { slot, source })
        }
        _ => None,
    }
}

fn copy_source(trigger: Trigger) -> Option<usize> {
    let Trigger::Set(v) = trigger else {
        return None;
    };
    let steps = (SAMPLE_SLOTS * backspn_engine::DECKS) as f32;
    let value = (v.clamp(0.0, 1.0) * steps).round() as usize;
    value.checked_sub(1)
}

fn list_index(current: usize, len: usize, trigger: Trigger) -> Option<usize> {
    match trigger {
        Trigger::Set(v) => {
            Some(((v.clamp(0.0, 1.0) * (len - 1) as f32).round() as usize).min(len - 1))
        }
        Trigger::Delta(d) => {
            let step = if d > 0.0 {
                1
            } else if d < 0.0 {
                -1
            } else {
                return None;
            };
            Some((current as i64 + step).clamp(0, len as i64 - 1) as usize)
        }
        Trigger::Press => Some((current + 1) % len),
        Trigger::Release | Trigger::Reset => None,
    }
}

impl SampleView {
    pub fn feedback(
        &self,
        state: &SampleState,
        controls: &[StemControls; SAMPLE_SLOTS],
    ) -> SampleFeedback {
        let cell_state = |c: CellState| match c {
            CellState::Empty => 0,
            CellState::Loaded | CellState::Played | CellState::BeforeStart => 1,
            CellState::Playing => 2,
            CellState::Queued => 3,
        };
        SampleFeedback {
            playing: state.playing,
            quantize_on: self.set.quant_on,
            quantize_index: self.set.quant_index.min(7) as u8,
            page: self.page as u8,
            sequencer_on: state.sequencer_on,
            recording: state.recording,
            slots: std::array::from_fn(|slot| {
                let s = &self.set.slots[slot];
                let active = self.cell(slot, s.active_cell as usize);
                SampleSlotFeedback {
                    state: match state.slots[slot].player {
                        PlayerState::Empty => 0,
                        PlayerState::Stopped => 1,
                        PlayerState::Playing => 2,
                    },
                    keylock: s.keylock,
                    punch: s.punch,
                    one_shot: active.is_some_and(|c| c.play_type == 1),
                    gate: active.is_some_and(|c| c.trigger_type == 0),
                    cells: std::array::from_fn(|c| cell_state(state.slots[slot].cells[c])),
                    selected: self.selected[slot] as u8,
                    selected_colour: self
                        .cell(slot, self.selected[slot])
                        .and_then(|c| cell_rgb(c.color)),
                    muted: controls[slot].mute,
                    filter_on: controls[slot].filter_on,
                }
            }),
        }
    }

    pub fn screen(
        &self,
        state: &SampleState,
        controls: &[StemControls; SAMPLE_SLOTS],
        held: Option<(usize, bool)>,
        loop_beats: f64,
        deck_colour: Color32,
    ) -> SampleScreen {
        let cell = |slot: usize, index: usize| {
            let c = self.cell(slot, index);
            ScreenCell {
                number: index + 1,
                name: c.map(|c| c.name.clone()).unwrap_or_default(),
                colour: c.map(|c| Self::colour(c, deck_colour)),
                looped: c.is_some_and(|c| c.play_type == 0),
                playing: state.slots[slot].cells[index] == CellState::Playing,
            }
        };
        SampleScreen {
            title: self.set.title.clone(),
            bpm: state.bpm,
            quantize: quantize_label(self.set.quant_index as usize),
            quantize_on: self.set.quant_on,
            beat: beat_position_label(state.beat),
            loop_beats,
            loop_active: false,
            recording: state.recording,
            cells: std::array::from_fn(|slot| cell(slot, self.selected[slot])),
            slot: held
                .filter(|&(s, _)| s < SAMPLE_SLOTS)
                .map(|(slot, browsing)| {
                    let selected = self.selected[slot];
                    let c = controls[slot];
                    SlotOverlay {
                        browsing,
                        volume: c.volume,
                        muted: c.mute,
                        filter: c.filter,
                        filter_on: c.filter_on,
                        around: [
                            selected.checked_sub(1).map(|i| cell(slot, i)),
                            Some(cell(slot, selected)),
                            (selected + 1 < SAMPLE_CELLS).then(|| cell(slot, selected + 1)),
                        ],
                    }
                }),
        }
    }

    pub fn slot_action(
        &mut self,
        slot: usize,
        part: SampleSlotPart,
        trigger: Trigger,
        state: &SampleState,
        controls: &mut StemControls,
        out: &mut Vec<Command>,
    ) {
        if slot >= SAMPLE_SLOTS {
            return;
        }
        let pressed = trigger == Trigger::Press;
        let active = self.set.slots[slot].active_cell as usize;
        match part {
            SampleSlotPart::Primary => match trigger {
                Trigger::Press => self.primary(slot, state, controls, out),
                Trigger::Release => self.trigger(slot, active, false, out),
                _ => {}
            },
            SampleSlotPart::Secondary if pressed => self.secondary(slot, state, out),
            SampleSlotPart::Retrigger | SampleSlotPart::RetriggerPlay => match trigger {
                Trigger::Press => {
                    if self.cell(slot, active).is_none() {
                        self.events
                            .push(SampleEvent::Capture { slot, cell: active });
                        return;
                    }
                    if part == SampleSlotPart::RetriggerPlay && controls.mute {
                        controls.mute = false;
                        out.push(Command::Stem(
                            self.deck,
                            StemCommand::Controls(slot, *controls),
                        ));
                    }
                    self.trigger(slot, active, true, out);
                }
                Trigger::Release => self.trigger(slot, active, false, out),
                _ => {}
            },
            SampleSlotPart::Play => match trigger {
                Trigger::Press => self.trigger(slot, active, true, out),
                Trigger::Release => self.trigger(slot, active, false, out),
                _ => {}
            },
            SampleSlotPart::PlayMode => {
                let one_shot = self.cell(slot, active).is_some_and(|c| c.play_type == 1);
                if let Some(i) = list_index(usize::from(!one_shot), 2, trigger)
                    && let Some(c) = self.cell_mut(slot, active)
                {
                    c.play_type = u32::from(i == 0);
                    self.push_params(slot, active, out);
                }
            }
            SampleSlotPart::TriggerType => {
                let gate = self.cell(slot, active).is_some_and(|c| c.trigger_type == 0);
                if let Some(i) = list_index(usize::from(gate), 2, trigger)
                    && let Some(c) = self.cell_mut(slot, active)
                {
                    c.trigger_type = u32::from(i == 0);
                    self.push_params(slot, active, out);
                }
            }
            SampleSlotPart::Keylock if pressed => {
                let on = !self.set.slots[slot].keylock;
                self.set.slots[slot].keylock = on;
                self.modified = true;
                out.push(self.cmd(SampleCommand::SetKeylock { slot, on }));
            }
            SampleSlotPart::Punch if pressed => {
                let on = !self.set.slots[slot].punch;
                self.set.slots[slot].punch = on;
                self.modified = true;
                out.push(self.cmd(SampleCommand::SetPunch { slot, on }));
            }
            SampleSlotPart::SizeDouble if pressed => self.alter_size(slot, Some(true), out),
            SampleSlotPart::SizeHalf if pressed => self.alter_size(slot, Some(false), out),
            SampleSlotPart::SizeReset if pressed => self.alter_size(slot, None, out),
            SampleSlotPart::SizeAdjust => {
                if let Some(step) = list_index(
                    self.size_steps[slot],
                    backspn_engine::SAMPLE_SIZE_STEPS,
                    trigger,
                ) {
                    self.set_size_step(slot, step, out);
                }
            }
            SampleSlotPart::Unload if pressed => self.unload_cell(slot, active, out),
            SampleSlotPart::LoadFromList
            | SampleSlotPart::Capture
            | SampleSlotPart::CopyFromSlot
            | SampleSlotPart::CaptureLoopRecorder => {
                if let Some(event) = load_event(slot, active, part, trigger) {
                    self.events.push(event);
                }
            }
            SampleSlotPart::SelectedCell
            | SampleSlotPart::TriggerSelected
            | SampleSlotPart::StopSelected
            | SampleSlotPart::DeletePattern => {
                self.selected_action(slot, part, trigger, state.sequencer_on, out);
            }
            _ => {}
        }
    }

    fn selected_action(
        &mut self,
        slot: usize,
        part: SampleSlotPart,
        trigger: Trigger,
        sequencing: bool,
        out: &mut Vec<Command>,
    ) {
        let cell = self.selected[slot];
        match (part, trigger) {
            (SampleSlotPart::SelectedCell, _) => {
                if let Some(i) = list_index(cell, SAMPLE_CELLS, trigger) {
                    self.selected[slot] = i;
                    out.push(self.cmd(SampleCommand::SetSelectedCell { slot, cell: i }));
                }
            }
            (SampleSlotPart::TriggerSelected, Trigger::Press) if sequencing => {
                out.push(self.cmd(SampleCommand::SequencerTrigger { slot }));
            }
            (SampleSlotPart::TriggerSelected, Trigger::Press) => {
                self.trigger(slot, cell, true, out);
            }
            (SampleSlotPart::TriggerSelected, Trigger::Release) if !sequencing => {
                self.trigger(slot, cell, false, out);
            }
            (SampleSlotPart::DeletePattern, Trigger::Press) => {
                out.push(self.cmd(SampleCommand::DeletePattern { slot }));
            }
            (SampleSlotPart::StopSelected, Trigger::Press) => {
                out.push(self.cmd(SampleCommand::Stop { slot, cell }));
            }
            _ => {}
        }
    }

    pub fn secondary(&mut self, slot: usize, state: &SampleState, out: &mut Vec<Command>) {
        let active = self.set.slots[slot].active_cell as usize;
        match state.slots[slot].cells[active] {
            CellState::Empty => self
                .events
                .push(SampleEvent::LoadSelected { slot, cell: active }),
            CellState::Playing | CellState::Played | CellState::BeforeStart => {
                out.push(self.cmd(SampleCommand::Stop { slot, cell: active }));
            }
            CellState::Loaded | CellState::Queued => self.unload_cell(slot, active, out),
        }
    }

    pub fn cell_action(&mut self, slot: usize, cell: usize, pressed: bool, out: &mut Vec<Command>) {
        if slot >= SAMPLE_SLOTS || cell >= SAMPLE_CELLS {
            return;
        }
        match self.cell_mode {
            CellMode::Trigger => self.trigger(slot, cell, pressed, out),
            CellMode::Load if pressed => self.events.push(SampleEvent::LoadSelected { slot, cell }),
            CellMode::Delete if pressed => self.unload_cell(slot, cell, out),
            CellMode::Reverse => {
                if let Some(c) = self.cell_mut(slot, cell) {
                    c.reverse = pressed;
                    self.push_params(slot, cell, out);
                }
            }
            CellMode::Capture if pressed => self.events.push(SampleEvent::Capture { slot, cell }),
            _ => {}
        }
    }

    pub fn stop_or_load(&mut self, slot: usize, cell: usize, out: &mut Vec<Command>) {
        if slot >= SAMPLE_SLOTS || cell >= SAMPLE_CELLS {
            return;
        }
        if self.cell(slot, cell).is_some() {
            out.push(self.cmd(SampleCommand::Stop { slot, cell }));
        } else {
            self.events.push(SampleEvent::LoadSelected { slot, cell });
        }
    }

    pub fn deck_action(
        &mut self,
        part: SamplePart,
        trigger: Trigger,
        state: &SampleState,
        controls: &mut [StemControls; SAMPLE_SLOTS],
        out: &mut Vec<Command>,
    ) {
        let pressed = trigger == Trigger::Press;
        let modifier = |mode: CellMode, this: &mut Self| match trigger {
            Trigger::Press if this.cell_mode == CellMode::Trigger => this.cell_mode = mode,
            Trigger::Release if this.cell_mode == mode => this.cell_mode = CellMode::Trigger,
            _ => {}
        };
        match part {
            SamplePart::QuantizeSelector => {
                if let Some(i) =
                    list_index(self.set.quant_index as usize, QUANTIZE_BEATS.len(), trigger)
                {
                    self.set.quant_index = i as u32;
                    self.modified = true;
                    out.push(self.cmd(SampleCommand::SetQuantize {
                        on: self.set.quant_on,
                        beats: quantize_beats(i as u32),
                    }));
                }
            }
            SamplePart::QuantizeOn if pressed => {
                self.set.quant_on = !self.set.quant_on;
                self.modified = true;
                out.push(self.cmd(SampleCommand::SetQuantize {
                    on: self.set.quant_on,
                    beats: quantize_beats(self.set.quant_index),
                }));
            }
            SamplePart::SaveSet if pressed => self.events.push(SampleEvent::SaveSet),
            SamplePart::Page => {
                if let Some(p) = list_index(self.page, SAMPLE_PAGES, trigger) {
                    self.page = p;
                }
            }
            SamplePart::PlayAll if pressed => {
                for slot in 0..SAMPLE_SLOTS {
                    self.primary(slot, state, &mut controls[slot], out);
                }
            }
            SamplePart::TriggerAll => match trigger {
                Trigger::Press | Trigger::Release => {
                    for slot in 0..SAMPLE_SLOTS {
                        let active = self.set.slots[slot].active_cell as usize;
                        if self.cell(slot, active).is_some() {
                            self.trigger(slot, active, pressed, out);
                        }
                    }
                }
                _ => {}
            },
            SamplePart::PlayModeAll => {
                for slot in 0..SAMPLE_SLOTS {
                    self.slot_action(
                        slot,
                        SampleSlotPart::PlayMode,
                        trigger,
                        state,
                        &mut controls[slot],
                        out,
                    );
                }
            }
            SamplePart::CaptureSource => {
                if let Some(i) = list_index(self.capture_source, 5, trigger) {
                    self.capture_source = i;
                }
            }
            SamplePart::CellLoadModifier => modifier(CellMode::Load, self),
            SamplePart::CellDeleteModifier => modifier(CellMode::Delete, self),
            SamplePart::CellReverseModifier => modifier(CellMode::Reverse, self),
            SamplePart::CellCaptureModifier => modifier(CellMode::Capture, self),
            SamplePart::SequencerOn => {
                let on = match trigger {
                    Trigger::Press => !state.sequencer_on,
                    Trigger::Set(v) => v >= 0.5,
                    _ => return,
                };
                out.push(self.cmd(SampleCommand::SequencerOn(on)));
            }
            SamplePart::SequencerRecord if pressed => {
                out.push(self.cmd(SampleCommand::SequencerRecord(!state.recording)));
            }
            _ => {}
        }
    }
}

pub fn quantize_beats(index: u32) -> f64 {
    QUANTIZE_BEATS
        .get(index as usize)
        .copied()
        .unwrap_or(QUANTIZE_BEATS[2])
}

pub fn capture_label(source: usize) -> String {
    match source {
        0..=3 => format!("Deck {}", char::from(b'A' + source as u8)),
        _ => "Loop Rec".to_owned(),
    }
}

pub fn default_capture_source(deck: usize, capturable: [bool; 5]) -> usize {
    match deck {
        2 if capturable[0] => 0,
        3 if capturable[1] => 1,
        _ => next_capture_source(deck, capturable),
    }
}

pub fn next_capture_source(from: usize, capturable: [bool; 5]) -> usize {
    (1..=4)
        .map(|step| (from + step) % 5)
        .find(|&i| capturable[i])
        .unwrap_or(4)
}

fn beat_position_label(beat: f64) -> String {
    let v = beat as f32;
    let group = (v * 0.0625) as i64 + 1;
    let bar = (v * 0.25) as i64 % 4 + 1;
    let at = v as i64 % 4 + 1;
    format!("{group}.{bar}.{at}")
}

fn quantize_label(index: usize) -> String {
    match QUANTIZE_BEATS.get(index).copied().unwrap_or(1.0) {
        0.25 => "1/4".to_owned(),
        0.5 => "1/2".to_owned(),
        b => format!("{b}"),
    }
}

pub fn behaviour(s: &crate::prefs::Settings) -> SampleBehaviour {
    SampleBehaviour {
        auto_play: s.sample_auto_play,
        one_shot_ignores_quantize: s.sample_one_shot_ignores_quantize,
        one_shot_ignores_punch: s.sample_one_shot_ignores_punch,
    }
}

fn size_label(step: usize) -> String {
    let factor = sample_size_factor(step);
    if factor >= 1.0 {
        format!("×{factor}")
    } else {
        format!("×1/{}", (1.0 / factor).round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backspn_formats::nml::Location;

    fn track(id: i64, sample_type: Option<i32>) -> TrackRow {
        TrackRow {
            id,
            path: format!("/Volumes/USB/Samples/kick{id}.wav"),
            title: format!("Kick {id}"),
            bpm: Some(128.0),
            duration_s: Some(0.5),
            sample_type,
            ..TrackRow::default()
        }
    }

    #[test]
    fn loading_a_track_resets_the_cell_but_keeps_sync_reverse_and_trigger() {
        let mut v = SampleView::new(0);
        let mut old = SetCell::new(3, Location::default());
        old.sync = false;
        old.reverse = true;
        old.trigger_type = 0;
        old.gain = 0.9;
        v.put_cell(1, old);
        v.load_cell(1, 3, &track(7, None), None, "Macintosh HD");
        let c = v.cell(1, 3).unwrap();
        assert_eq!((c.sync, c.reverse, c.trigger_type), (false, true, 0));
        assert!((c.gain - 0.5).abs() < 1e-9, "reset");
        assert_eq!(c.play_type, 1, "no sample type: one-shot");
        assert_eq!(c.name, "Kick 7");
        assert_eq!(c.location.file, "kick7.wav");
        assert_eq!(v.tracks[SAMPLE_CELLS + 3], Some(7));
        assert!(v.modified);
    }

    #[test]
    fn the_beat_position_is_group_bar_and_beat() {
        assert_eq!(beat_position_label(0.0), "1.1.1");
        assert_eq!(beat_position_label(17.5), "2.1.2");
        assert_eq!(beat_position_label(-1.5), "1.1.0");
        assert_eq!(beat_position_label(-5.0), "1.0.0");
        assert_eq!(beat_position_label(-20.0), "0.0.1");
    }

    #[test]
    fn the_selected_cell_is_what_the_slot_pads_trigger() {
        let mut v = SampleView::new(0);
        let state = SampleState::default();
        let mut controls = StemControls::default();
        let mut out = Vec::new();
        let mut act = |v: &mut SampleView, part, trigger, out: &mut Vec<Command>| {
            v.slot_action(2, part, trigger, &state, &mut controls, out);
        };
        act(
            &mut v,
            SampleSlotPart::SelectedCell,
            Trigger::Delta(-1.0),
            &mut out,
        );
        assert_eq!(v.selected[2], 0, "held at the first");
        for _ in 0..20 {
            act(
                &mut v,
                SampleSlotPart::SelectedCell,
                Trigger::Delta(1.0),
                &mut out,
            );
        }
        assert_eq!(v.selected[2], 15, "held at the last");
        assert!(matches!(
            out.last(),
            Some(Command::SampleDeck(
                0,
                SampleDeckCommand::Deck(SampleCommand::SetSelectedCell { slot: 2, cell: 15 })
            ))
        ));
        out.clear();
        act(
            &mut v,
            SampleSlotPart::TriggerSelected,
            Trigger::Press,
            &mut out,
        );
        act(
            &mut v,
            SampleSlotPart::StopSelected,
            Trigger::Press,
            &mut out,
        );
        let Command::SampleDeck(
            0,
            SampleDeckCommand::Deck(SampleCommand::Trigger {
                slot,
                cell,
                pressed,
            }),
        ) = out[0]
        else {
            panic!("{:?}", out[0]);
        };
        assert_eq!((slot, cell, pressed), (2, 15, true));
        assert!(matches!(
            out[1],
            Command::SampleDeck(
                0,
                SampleDeckCommand::Deck(SampleCommand::Stop { slot: 2, cell: 15 })
            )
        ));
        let mut cell = SetCell::new(15, Location::default());
        cell.color = 3;
        v.put_cell(2, cell);
        let f = v.feedback(&state, &[StemControls::default(); SAMPLE_SLOTS]);
        assert_eq!(f.slots[2].selected, 15);
        assert_eq!(f.slots[2].selected_colour, Some(CELL_COLOURS[2]));
    }

    #[test]
    fn with_the_sequencer_on_the_slot_pads_play_it_and_record() {
        let mut v = SampleView::new(0);
        let mut state = SampleState::default();
        let mut controls = [StemControls::default(); SAMPLE_SLOTS];
        let mut out = Vec::new();
        v.deck_action(
            SamplePart::SequencerOn,
            Trigger::Set(1.0),
            &state,
            &mut controls,
            &mut out,
        );
        v.deck_action(
            SamplePart::SequencerRecord,
            Trigger::Press,
            &state,
            &mut controls,
            &mut out,
        );
        state.sequencer_on = true;
        for trigger in [Trigger::Press, Trigger::Release] {
            v.slot_action(
                1,
                SampleSlotPart::TriggerSelected,
                trigger,
                &state,
                &mut controls[1],
                &mut out,
            );
        }
        v.slot_action(
            1,
            SampleSlotPart::DeletePattern,
            Trigger::Press,
            &state,
            &mut controls[1],
            &mut out,
        );
        let deck = |c: &Command| match c {
            Command::SampleDeck(0, SampleDeckCommand::Deck(c)) => Some(c.clone()),
            _ => None,
        };
        let sent: Vec<String> = out
            .iter()
            .filter_map(deck)
            .map(|c| format!("{c:?}"))
            .collect();
        assert_eq!(
            sent,
            [
                "SequencerOn(true)",
                "SequencerRecord(true)",
                "SequencerTrigger { slot: 1 }",
                "DeletePattern { slot: 1 }"
            ],
            "the release sends nothing"
        );
        let f = v.feedback(&state, &controls);
        assert!(f.sequencer_on);
    }

    #[test]
    fn a_loop_sample_loads_as_a_loop_with_its_auto_gain() {
        let mut v = SampleView::new(0);
        v.load_cell(0, 0, &track(1, Some(0)), Some(-6.0), "Macintosh HD");
        let c = v.cell(0, 0).unwrap();
        assert_eq!(c.play_type, 0);
        assert!((c.gain - 0.25).abs() < 1e-9, "−6 dB → 0.25");
        let p = SampleView::params_of(c);
        assert_eq!(p.play_type, PlayType::Loop);
        assert_eq!(p.trigger_type, TriggerType::Latch);
        assert!((p.bpm - 128.0).abs() < 1e-9);
    }

    #[test]
    fn quantize_indexes_map_to_their_values() {
        assert!((quantize_beats(0) - 0.25).abs() < 1e-9);
        assert!((quantize_beats(7) - 32.0).abs() < 1e-9);
        assert!(
            (quantize_beats(99) - 1.0).abs() < 1e-9,
            "out of range: 1 beat"
        );
        assert_eq!(capture_label(2), "Deck C");
        assert_eq!(capture_label(4), "Loop Rec");
        let all = [true, true, true, true, false];
        assert_eq!(default_capture_source(0, all), 1);
        assert_eq!(default_capture_source(1, all), 2);
        assert_eq!(default_capture_source(2, all), 0);
        assert_eq!(default_capture_source(3, all), 1);
        let only_d = [false, false, false, true, false];
        assert_eq!(default_capture_source(2, only_d), 3);
        assert_eq!(
            next_capture_source(3, only_d),
            4,
            "no other: the loop recorder"
        );
        assert_eq!(
            next_capture_source(3, [false; 5]),
            4,
            "none: the loop recorder"
        );
        assert_eq!(quantize_label(1), "1/2");
        assert_eq!(quantize_label(4), "4");
    }

    #[test]
    fn dropped_tracks_fill_the_slot_downwards() {
        let drag = TrackDrag {
            id: 1,
            title: String::new(),
            tracks: vec![1, 2, 3],
            entries: None,
        };
        let cells: Vec<(usize, i64)> = cell_drop(2, 14, &drag)
            .into_iter()
            .map(|e| match e {
                SampleEvent::LoadCell { slot, cell, track } => {
                    assert_eq!(slot, 2);
                    (cell, track)
                }
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(cells, [(14, 1), (15, 2), (15, 3)], "held at the last cell");
    }

    #[test]
    fn copy_from_slot_values_name_a_slot_of_any_deck() {
        assert_eq!(copy_source(Trigger::Set(0.0)), None, "−1: nothing");
        assert_eq!(copy_source(Trigger::Set(1.0 / 16.0)), Some(0));
        assert_eq!(copy_source(Trigger::Set(1.0)), Some(15), "deck D slot 4");
        assert_eq!(copy_source(Trigger::Press), None);
    }

    #[test]
    fn a_cell_transposes_by_semitones_and_offsets_by_beats() {
        let mut cell = SetCell::new(
            0,
            backspn_formats::nml::Location {
                dir: String::new(),
                file: String::new(),
                volume: String::new(),
                volume_id: None,
            },
        );
        cell.transpose = -0.5;
        cell.offset = 2.0;
        let p = SampleView::params_of(&cell);
        assert!((p.pitch + 6.0).abs() < 1e-6, "−0.5 = 6 semitones down");
        assert!((p.offset_beats - 2.0).abs() < 1e-9);
    }

    #[test]
    fn shift_sampler_pads_stop_a_loaded_cell_or_load_an_empty_one() {
        let mut v = SampleView::new(2);
        v.put_cell(1, SetCell::new(4, Location::default()));
        let mut out = Vec::new();
        v.stop_or_load(1, 4, &mut out);
        assert_eq!(
            format!("{out:?}"),
            format!("{:?}", [v.cmd(SampleCommand::Stop { slot: 1, cell: 4 })])
        );
        out.clear();
        v.stop_or_load(1, 5, &mut out);
        assert!(out.is_empty());
        assert!(matches!(
            v.take_events()[..],
            [SampleEvent::LoadSelected { slot: 1, cell: 5 }]
        ));
    }
}
