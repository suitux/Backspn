use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use backspn_library::{Library, Node, NodeKind, TrackRow};
use eframe::egui::{self, Align, Layout, ScrollArea, Ui};

use crate::analysis_queue::{AnalysisQueue, Parts};
use crate::columns::{BrowserColumn, ColumnLayout, MAX_SORT_CRITERIA, Sorting, compare_by};
use crate::key_view::KeyView;
use crate::preview::Preview;
use crate::selection::Selection;
use crate::smartlist_editor::SmartlistEditor;
use crate::suggest_panel::{self, Master, SuggestPanel};
use crate::tag_writer::TagWriter;
use crate::track_edit::TrackEdit;
use crate::track_filter::TrackFilter;
use crate::{theme, widgets};

mod analysis;
pub use analysis::AnalysedGrid;
mod dialogs;
mod external;
#[cfg(not(target_os = "macos"))]
pub(crate) use external::LIBRARY_XML_HINT;
pub use external::{Outside, default_music_xml, music_source};
mod files;
pub(crate) use files::reveal;
mod inline;
mod rescan;
mod stems;
mod table;
mod tags;
mod toolbar;
mod tree;
pub use tree::TreeNode;

pub use files::library_path;
use table::drag_preview;

pub fn free_deck(deck_count: usize, playing: [bool; 4]) -> usize {
    (0..deck_count.min(4)).find(|&d| !playing[d]).unwrap_or(0)
}

const COVER_SIDE: f32 = 48.0;

const PAGE_ROWS: i32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackDrag {
    pub id: i64,
    pub title: String,
    pub tracks: Vec<i64>,
    pub entries: Option<(i64, Vec<usize>)>,
}

impl TrackDrag {
    pub fn single(id: i64, title: String) -> Self {
        Self {
            id,
            title,
            tracks: vec![id],
            entries: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Collection,
    Node(i64),
    Folder(PathBuf),
    History(i64),
    Recordings,
    Loops,
    Set(i64),
    External(Outside, Vec<usize>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetDrag(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeDrag(pub i64);

pub enum BrowserAction {
    Load {
        deck: usize,
        track: i64,
    },
    LoadAs {
        deck: usize,
        track: i64,
        mode: backspn_control::LoadMode,
    },
    LoadSet {
        deck: usize,
        set: i64,
    },
    ExportSet(i64),
    ImportSet,
    FoldersChanged(Vec<PathBuf>),
    LibraryXmlChanged(Option<PathBuf>),
    Preview(Vec<backspn_engine::DeckCommand>),
    SaveLoop(crate::sample_view::LoopDrag),
}

#[allow(clippy::struct_excessive_bools)]
pub struct Browser {
    lib: Result<Library, String>,
    rows: Vec<TrackRow>,
    index: HashMap<i64, usize>,
    nodes: Vec<Node>,
    recordings: Option<i64>,
    loops: Option<i64>,
    hidden_nodes: Vec<TreeNode>,
    source: Source,
    folders: Vec<PathBuf>,
    histories: Vec<backspn_library::HistorySession>,
    current_history: Option<i64>,
    played: HashSet<i64>,
    history_positions: Vec<(i64, i64)>,
    external: Vec<TrackRow>,
    external_origin: Vec<Option<usize>>,
    library_xml: Option<external::OutsideLibrary>,
    music: Option<external::OutsideLibrary>,
    programs: Vec<external::OutsideLibrary>,
    by_path: HashMap<String, i64>,
    subdirs: HashMap<PathBuf, Vec<PathBuf>>,
    tree_actions: Vec<BrowserAction>,
    search: String,
    filter: TrackFilter,
    search_error: Option<String>,
    search_history: Vec<String>,
    view: Vec<i64>,
    view_source: Vec<usize>,
    sort: Sorting,
    pub columns: ColumnLayout,
    pub favorites: Vec<String>,
    pub show_favorites: bool,
    pub show_preview_player: bool,
    pub row_height: f32,
    pub inplace_edit: bool,
    inline: Option<inline::InlineEdit>,
    folder_import: Option<files::FolderImport>,
    columns_menu_at: Option<(egui::Pos2, u64)>,
    columns_query: String,
    analysis: AnalysisQueue,
    pub activity: crate::activity::Activity,
    rescan: crate::rescan_queue::RescanQueue,
    analysis_dialog: Option<(Vec<i64>, Parts)>,
    stem_jobs: crate::stem_jobs::StemJobs,
    stem_dirs: crate::stems::StemDirs,
    generated: std::cell::RefCell<HashMap<i64, Option<PathBuf>>>,
    stem_model_ask: Option<i64>,
    analysis_range: backspn_analysis::BpmRange,
    analyze_imports: bool,
    tag_writer: TagWriter,
    ctx: Option<egui::Context>,
    keys: KeyView,
    editing: Option<TrackEdit>,
    found_in: Option<(i64, Vec<i64>)>,
    preview: Preview,
    preview_state: backspn_engine::DeckState,
    loaded: Vec<i64>,
    sets: Vec<backspn_library::SampleSetRow>,
    sample_decks: [bool; 4],
    deleting: Option<(Vec<i64>, bool)>,
    ask: Option<crate::modal::Ask<dialogs::BrowserAsk>>,
    consistency: Option<backspn_library::maintenance::ConsistencyReport>,
    smart_editor: Option<SmartlistEditor>,
    renaming: Option<(i64, String, Option<String>)>,
    folder_playlist: Option<(PathBuf, String, Option<String>)>,
    cue_marks: std::cell::RefCell<HashMap<i64, std::rc::Rc<[crate::columns::CueMark]>>>,
    covers: std::cell::RefCell<Option<crate::covers::CoverCache>>,
    pub suggestions: SuggestPanel,
    selection: Selection,
    scroll_to_selected: bool,
    message: Option<String>,
}

const PREPARATION: &str = "Preparation";

impl Browser {
    pub fn open(folders: Vec<PathBuf>) -> Self {
        let path = library_path();
        let lib = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .map_err(|e| e.to_string())
            .and_then(|()| Library::open(&path).map_err(|e| e.to_string()));
        if let Err(e) = &lib {
            crate::toast::error(format!(
                "Cannot open the library at {}: {e}",
                path.display()
            ));
        }
        let mut browser = Self::with_library(lib);
        browser.folders = folders;
        browser
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self::with_library(Library::open_in_memory().map_err(|e| e.to_string()))
    }

    fn with_library(lib: Result<Library, String>) -> Self {
        let mut browser = Self {
            lib,
            rows: Vec::new(),
            index: HashMap::new(),
            nodes: Vec::new(),
            recordings: None,
            loops: None,
            hidden_nodes: Vec::new(),
            source: Source::Collection,
            folders: Vec::new(),
            histories: Vec::new(),
            current_history: None,
            sets: Vec::new(),
            sample_decks: [false; 4],
            history_positions: Vec::new(),
            played: HashSet::new(),
            external: Vec::new(),
            external_origin: Vec::new(),
            library_xml: None,
            music: None,
            programs: Vec::new(),
            by_path: HashMap::new(),
            subdirs: HashMap::new(),
            tree_actions: Vec::new(),
            search: String::new(),
            search_error: None,
            filter: TrackFilter::default(),
            search_history: Vec::new(),
            view: Vec::new(),
            view_source: Vec::new(),
            sort: Sorting::new(),
            columns: ColumnLayout::default(),
            favorites: crate::favorites::defaults(),
            show_favorites: true,
            show_preview_player: true,
            row_height: 24.0,
            inplace_edit: true,
            inline: None,
            folder_import: None,
            columns_menu_at: None,
            columns_query: String::new(),
            suggestions: SuggestPanel::default(),
            covers: std::cell::RefCell::default(),
            cue_marks: std::cell::RefCell::default(),
            renaming: None,
            folder_playlist: None,
            smart_editor: None,
            consistency: None,
            deleting: None,
            ask: None,
            loaded: Vec::new(),
            keys: KeyView::default(),
            editing: None,
            found_in: None,
            preview: Preview::default(),
            preview_state: backspn_engine::DeckState::default(),
            analysis: AnalysisQueue::default(),
            activity: crate::activity::Activity::default(),
            rescan: crate::rescan_queue::RescanQueue::default(),
            analysis_dialog: None,
            stem_jobs: crate::stem_jobs::StemJobs::new(crate::prefs::stem_model_path()),
            stem_dirs: crate::stems::StemDirs {
                own: crate::stems::default_dir(),
                theirs: None,
            },
            generated: std::cell::RefCell::default(),
            stem_model_ask: None,
            analysis_range: backspn_analysis::BpmRange::AUTOMATIC,
            analyze_imports: false,
            tag_writer: TagWriter::default(),
            ctx: None,
            selection: Selection::default(),
            scroll_to_selected: false,
            message: None,
        };
        browser.refresh();
        browser.show(Source::Collection);
        browser
    }

    pub fn library(&mut self) -> Option<&mut Library> {
        self.lib.as_mut().ok()
    }

    pub fn refresh(&mut self) {
        self.cue_marks.borrow_mut().clear();
        let Ok(lib) = &mut self.lib else { return };
        self.recordings = lib.recordings_list().ok();
        self.loops = lib.loops_list().ok();
        match (lib.tracks(), lib.nodes(), lib.history_sessions()) {
            (Ok(rows), Ok(nodes), Ok(histories)) => {
                self.index = rows.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
                self.by_path = rows.iter().map(|r| (r.path.clone(), r.id)).collect();
                self.rows = rows;
                self.nodes = backspn_library::playlists::visible(&nodes);
                self.histories = histories;
                let gone = match &self.source {
                    Source::Node(id) => !self.nodes.iter().any(|n| n.id == *id),
                    Source::History(id) => !self.histories.iter().any(|h| h.id == *id),
                    _ => false,
                };
                if gone {
                    self.source = Source::Collection;
                }
            }
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => crate::toast::error(e.to_string()),
        }
        self.refresh_sets();
        self.recompute();
    }

    fn select_track(&mut self, track: i64) {
        if let Some(row) = self.view.iter().position(|&v| v == track) {
            self.selection.select(row);
            self.scroll_to_selected = true;
        }
    }

    pub fn jump_to_track(&mut self, track: i64) {
        if !self.view.contains(&track) {
            self.search.clear();
            self.show(Source::Collection);
        }
        self.select_track(track);
    }

    pub fn set_preview_state(&mut self, state: &backspn_engine::DeckState) {
        self.preview_state = *state;
    }

    pub fn set_key_view(&mut self, keys: KeyView) {
        self.keys = keys;
    }

    pub fn set_loaded(&mut self, tracks: Vec<i64>) {
        self.loaded = tracks;
    }

    pub(crate) fn open_set(&mut self, id: i64) {
        self.show(Source::Set(id));
    }

    pub(crate) fn shown_set(&self) -> Option<i64> {
        match self.source {
            Source::Set(id) => Some(id),
            _ => None,
        }
    }

    pub fn set_sample_decks(&mut self, decks: [bool; 4]) {
        self.sample_decks = decks;
    }

    fn refresh_sets(&mut self) {
        self.sets = self
            .lib
            .as_ref()
            .ok()
            .and_then(|l| l.sample_sets().ok())
            .unwrap_or_default();
        if matches!(self.source, Source::Set(id) if !self.sets.iter().any(|s| s.id == id)) {
            self.source = Source::Collection;
        }
    }

    pub fn mark_played(&mut self, track: i64) {
        self.played.insert(track);
        self.refresh_track(track);
        self.refresh_played_lists();
    }

    fn reset_played(&mut self, tracks: &[i64]) {
        for t in tracks {
            self.played.remove(t);
        }
        self.refresh_played_lists();
    }

    fn refresh_played_lists(&mut self) {
        if matches!(self.source, Source::Node(_)) {
            self.recompute();
        }
    }

    pub fn history_changed(&mut self, current: i64) {
        self.current_history = Some(current);
        if let Some(Ok(histories)) = self.lib.as_ref().ok().map(Library::history_sessions) {
            self.histories = histories;
        }
        if matches!(self.source, Source::History(_)) {
            self.recompute();
        }
    }

    pub fn set_context(&mut self, ctx: &egui::Context) {
        self.ctx = Some(ctx.clone());
    }

    pub fn refresh_track(&mut self, id: i64) {
        self.cue_marks.borrow_mut().remove(&id);
        let Ok(lib) = &self.lib else { return };
        if let (Ok(Some(row)), Some(&i)) = (lib.track(id), self.index.get(&id)) {
            self.rows[i] = row;
        }
    }

    pub(crate) fn cue_marks(&self, id: i64) -> std::rc::Rc<[crate::columns::CueMark]> {
        if let Some(marks) = self.cue_marks.borrow().get(&id) {
            return std::rc::Rc::clone(marks);
        }
        let marks: std::rc::Rc<[_]> = match (&self.lib, self.row(id)) {
            (Ok(lib), Some(track)) if id >= 0 => {
                crate::columns::cue_marks(&lib.cues(id).unwrap_or_default(), track.duration_s)
                    .into()
            }
            _ => std::rc::Rc::from([]),
        };
        self.cue_marks
            .borrow_mut()
            .insert(id, std::rc::Rc::clone(&marks));
        marks
    }

    pub(crate) fn library_track(&self, id: i64) -> Option<TrackRow> {
        self.lib.as_ref().ok()?.track(id).ok()?
    }

    pub(crate) fn library_sets(&self) -> Vec<backspn_library::SampleSetRow> {
        self.sets.clone()
    }

    pub(crate) fn library_set(&self, id: i64) -> Option<backspn_library::SampleSetRow> {
        self.lib.as_ref().ok()?.sample_set(id).ok()?
    }

    pub(crate) fn library_set_by_title(&self, title: &str) -> Option<i64> {
        self.lib.as_ref().ok()?.sample_set_by_title(title).ok()?
    }

    pub(crate) fn save_library_set(
        &mut self,
        target: Option<i64>,
        set: &backspn_formats::nml::SampleSet,
        tracks: &[Option<i64>],
    ) -> Option<i64> {
        let result = self.with_lib_value(|lib| lib.save_sample_set(target, set, tracks));
        self.refresh_sets();
        self.recompute();
        match result {
            Ok(id) => Some(id),
            Err(e) => {
                crate::toast::error(e.to_string());
                None
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn library_cues(&self, id: i64) -> Vec<backspn_library::CuePoint> {
        self.lib
            .as_ref()
            .ok()
            .and_then(|lib| lib.cues(id).ok())
            .unwrap_or_default()
    }

    fn row(&self, id: i64) -> Option<&TrackRow> {
        if id < 0 {
            return self.external.get(usize::try_from(-id - 1).ok()?);
        }
        self.index.get(&id).map(|&i| &self.rows[i])
    }

    pub fn resolve(&mut self, id: i64) -> Option<i64> {
        if id >= 0 {
            return Some(id);
        }
        let new_id = if let Some(track) = self.external_track_to_import(id) {
            self.lib.as_mut().ok()?.import_external_track(&track).ok()?
        } else {
            let path = PathBuf::from(&self.row(id)?.path);
            self.lib.as_mut().ok()?.add_file(&path).ok()?
        };
        let (mut ids, mut cursor) = self.remember_selection();
        for i in ids.iter_mut().chain(cursor.iter_mut()) {
            if *i == id {
                *i = new_id;
            }
        }
        self.refresh();
        self.restore_selection(&ids, cursor);
        Some(new_id)
    }

    pub fn set_folders(&mut self, folders: Vec<PathBuf>) {
        if let Source::Folder(f) = &self.source
            && !folders.iter().any(|root| f.starts_with(root))
        {
            self.source = Source::Collection;
            self.recompute();
        }
        self.folders = folders;
    }

    pub fn add_folder(&mut self, folder: PathBuf) -> Option<BrowserAction> {
        if !folder.is_dir() || self.folders.contains(&folder) {
            return None;
        }
        self.folders.push(folder.clone());
        self.show(Source::Folder(folder));
        Some(BrowserAction::FoldersChanged(self.folders.clone()))
    }

    fn remember_selection(&self) -> (Vec<i64>, Option<i64>) {
        let ids = self
            .selection
            .rows()
            .filter_map(|r| self.view.get(r).copied())
            .collect();
        (ids, self.selected())
    }

    fn restore_selection(&mut self, ids: &[i64], cursor: Option<i64>) {
        self.selection.clear();
        let mut wanted: Vec<i64> = ids.to_vec();
        for (row, id) in self.view.iter().enumerate() {
            if let Some(i) = wanted.iter().position(|w| w == id) {
                wanted.swap_remove(i);
                self.selection.toggle(row);
            }
        }
        if let Some(row) = cursor.and_then(|c| self.view.iter().position(|&v| v == c)) {
            self.selection.set_cursor(row);
        }
    }

    fn recompute(&mut self) {
        let (ids, cursor) = self.remember_selection();
        self.rebuild_view();
        self.restore_selection(&ids, cursor);
    }

    fn rebuild_view(&mut self) {
        if self.lib.is_err() {
            return;
        }
        self.history_positions.clear();
        let source: Vec<i64> = match self.source.clone() {
            Source::Folder(dir) => self.list_folder(&dir),
            Source::External(kind, at) => self.list_external(kind, &at),
            Source::Collection => self.rows.iter().map(|r| r.id).collect(),
            Source::Node(node) => {
                let cx = backspn_library::smart::Context {
                    played: Some(&self.played),
                    ..backspn_library::smart::Context::default()
                };
                self.lib
                    .as_ref()
                    .map(|lib| lib.node_tracks_in(node, &cx).unwrap_or_default())
                    .unwrap_or_default()
            }
            Source::Recordings => self.recorded_tracks(),
            Source::Loops => self.loop_tracks(),
            Source::Set(id) => self
                .sets
                .iter()
                .find(|s| s.id == id)
                .map(backspn_library::SampleSetRow::distinct_tracks)
                .unwrap_or_default(),
            Source::History(session) => {
                let entries = self
                    .lib
                    .as_ref()
                    .map(|lib| lib.history_entries(session).unwrap_or_default())
                    .unwrap_or_default();
                self.history_positions = entries
                    .iter()
                    .filter_map(|e| Some((e.track_id?, e.position)))
                    .collect();
                self.history_positions.iter().map(|&(t, _)| t).collect()
            }
        };
        let mut order: Vec<usize> = (0..source.len()).collect();
        match backspn_library::Query::parse_search(&self.search) {
            Ok(None) => self.search_error = None,
            Ok(Some(query)) => {
                self.search_error = None;
                let cx = backspn_library::smart::Context {
                    played: Some(&self.played),
                    ..backspn_library::smart::Context::default()
                };
                order.retain(|&i| {
                    self.row(source[i])
                        .is_some_and(|t| query.matches_in(t, &cx))
                });
            }
            Err(e) => {
                self.search_error = Some(e.message);
                order.clear();
            }
        }
        if self.filter.active() {
            order.retain(|&i| self.row(source[i]).is_some_and(|t| self.filter.matches(t)));
        }
        if !self.sort.is_empty() {
            let (rows, index, external) = (&self.rows, &self.index, &self.external);
            let get = |id: i64| {
                if id < 0 {
                    &external[(-id - 1) as usize]
                } else {
                    &rows[index[&id]]
                }
            };
            order.sort_by(|&a, &b| compare_by(&self.sort, get(source[a]), get(source[b])));
        }
        self.view = order.iter().map(|&i| source[i]).collect();
        self.view_source = order;
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        (deck_count, playing): (usize, [bool; 4]),
        master: Option<Master>,
    ) -> Vec<BrowserAction> {
        let mut actions = Vec::new();
        let t = theme::get(ui.ctx());
        if let Err(e) = &self.lib {
            ui.colored_label(t.danger, format!("Library unavailable: {e}"));
            return actions;
        }
        ui.style_mut().interaction.selectable_labels = false;
        self.ctx = Some(ui.ctx().clone());
        self.collect_tag_writes();
        self.continue_folder_import();
        if self.folder_import.is_some() {
            ui.ctx().request_repaint();
        }
        let loaded = self.preview.poll();
        if !loaded.is_empty() {
            actions.push(BrowserAction::Preview(loaded));
        }
        if let Some(title) = self.preview.take_failure() {
            crate::toast::error(format!(
                "This file cannot be played: {title}. It may be missing, damaged or in a format the app cannot read."
            ));
        }
        self.keyboard(ui, deck_count, playing, &mut actions);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(210.0, ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| {
                    widgets::caption(ui, "LIBRARY");
                    ScrollArea::vertical()
                        .id_salt("tree")
                        .show(ui, |ui| self.tree(ui));
                },
            );
            widgets::divider(ui, ui.available_height());
            ui.vertical(|ui| {
                self.toolbar(ui);
                if self.show_favorites {
                    self.favorites_bar(ui);
                }
                if self.filter.open && self.filter.ui(ui) {
                    self.recompute();
                }
                if self.show_preview_player {
                    let state = self.preview_state;
                    let commands = self.preview.ui(ui, &state);
                    if !commands.is_empty() {
                        actions.push(BrowserAction::Preview(commands));
                    }
                }
                ui.horizontal_top(|ui| {
                    let panel = if self.suggestions.open {
                        suggest_panel::WIDTH + 2.0 * ui.spacing().item_spacing.x + 1.0
                    } else {
                        0.0
                    };
                    let size = egui::vec2(ui.available_width() - panel, ui.available_height());
                    ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
                        ScrollArea::horizontal().id_salt("columns").show(ui, |ui| {
                            self.table(ui, deck_count, &mut actions);
                        });
                    });
                    if self.suggestions.open {
                        widgets::divider(ui, ui.available_height());
                        let cx = suggest_panel::Context {
                            rows: &self.rows,
                            index: &self.index,
                            master,
                            played: &self.played,
                            deck_count,
                            playing,
                        };
                        ui.vertical(|ui| self.suggestions.ui(ui, &cx, &mut actions));
                    }
                });
            });
        });
        drag_preview(ui.ctx());
        self.columns_popup(ui.ctx());
        self.rename_dialog(ui.ctx());
        self.folder_playlist_dialog(ui.ctx());
        self.smartlist_dialog(ui.ctx());
        self.consistency_dialog(ui.ctx());
        self.delete_dialog(ui.ctx());
        self.ask_dialog(ui.ctx());
        self.analysis_options_dialog(ui.ctx());
        self.stem_model_dialog(ui.ctx());
        self.edit_dialog(ui.ctx());
        self.found_in_dialog(ui.ctx());
        actions.append(&mut self.tree_actions);
        actions
    }

    fn keyboard(
        &mut self,
        ui: &Ui,
        deck_count: usize,
        playing: [bool; 4],
        actions: &mut Vec<BrowserAction>,
    ) {
        if ui.ctx().text_edit_focused() || self.view.is_empty() {
            return;
        }
        let none = egui::Modifiers::NONE;
        let (up, down, page_up, page_down, home, end, enter, f2) = ui.input_mut(|i| {
            (
                i.count_and_consume_key(none, egui::Key::ArrowUp),
                i.count_and_consume_key(none, egui::Key::ArrowDown),
                i.count_and_consume_key(none, egui::Key::PageUp),
                i.count_and_consume_key(none, egui::Key::PageDown),
                i.consume_key(none, egui::Key::Home),
                i.consume_key(none, egui::Key::End),
                i.consume_key(none, egui::Key::Enter),
                i.consume_key(none, egui::Key::F2),
            )
        });
        let delta = (down as i32 - up as i32) + PAGE_ROWS * (page_down as i32 - page_up as i32);
        if delta != 0 {
            self.move_selection(delta);
        }
        if home {
            self.move_selection(i32::MIN);
        }
        if end {
            self.move_selection(i32::MAX);
        }
        if (enter || f2) && self.inplace_edit && self.edit_selected() {
            return;
        }
        if enter && let Some(track) = self.selected() {
            let deck = free_deck(deck_count, playing);
            actions.push(BrowserAction::Load { deck, track });
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.view.is_empty() {
            return;
        }
        let next = match self.selection.cursor() {
            Some(i) => (i as i64 + i64::from(delta)).clamp(0, self.view.len() as i64 - 1) as usize,
            None if delta == i32::MAX => self.view.len() - 1,
            None => 0,
        };
        self.selection.select(next);
        self.scroll_to_selected = true;
    }

    pub fn step_selection(&mut self, step: i32) -> Option<i64> {
        let row = match self.selection.cursor() {
            Some(c) => c.checked_add_signed(step as isize)?,
            None if step > 0 => 0,
            None => return None,
        };
        let id = *self.view.get(row)?;
        self.selection.select(row);
        self.scroll_to_selected = true;
        Some(id)
    }

    fn cover(&self, ctx: &egui::Context, id: i64) -> Option<egui::TextureId> {
        let row = self.row(id)?;
        let mut covers = self.covers.borrow_mut();
        let covers = covers.get_or_insert_with(|| crate::covers::CoverCache::new(ctx, COVER_SIDE));
        covers.get(ctx, row).map(egui::TextureHandle::id)
    }

    fn sources(&self) -> Vec<Source> {
        fn walk(nodes: &[Node], parent: Option<i64>, out: &mut Vec<Source>) {
            let mut children: Vec<&Node> = nodes.iter().filter(|n| n.parent == parent).collect();
            children.sort_by_key(|n| n.position);
            for node in children {
                if node.kind == NodeKind::Folder {
                    walk(nodes, Some(node.id), out);
                } else {
                    out.push(Source::Node(node.id));
                }
            }
        }
        let mut out = vec![Source::Collection];
        walk(&self.nodes, None, &mut out);
        out.extend(self.folders.iter().cloned().map(Source::Folder));
        for (node, source) in [
            (TreeNode::Recordings, Source::Recordings),
            (TreeNode::Loops, Source::Loops),
        ] {
            if self.shown(node) {
                out.push(source);
            }
        }
        out
    }

    fn show(&mut self, source: Source) {
        self.source = source;
        self.sort = self
            .sorting_path()
            .and_then(|path| self.lib.as_ref().ok()?.sorting(&path).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(attribute, descending)| {
                Some((BrowserColumn::from_attribute(attribute)?, !descending))
            })
            .take(MAX_SORT_CRITERIA)
            .collect();
        self.recompute();
    }

    fn sorting_path(&self) -> Option<String> {
        match self.source {
            Source::Collection => Some("$COLLECTION".to_owned()),
            Source::Node(id) => self.lib.as_ref().ok()?.sorting_path(id).ok(),
            Source::Recordings => self.lib.as_ref().ok()?.sorting_path(self.recordings?).ok(),
            Source::Loops => self.lib.as_ref().ok()?.sorting_path(self.loops?).ok(),
            Source::Folder(_) | Source::History(_) | Source::Set(_) | Source::External(..) => None,
        }
    }

    fn remember_sorting(&mut self) {
        let Some(path) = self.sorting_path() else {
            return;
        };
        let sorting: Vec<(u32, bool)> = self
            .sort
            .iter()
            .map_while(|&(c, ascending)| Some((c.attribute()?, !ascending)))
            .collect();
        let result = self.with_lib(|lib| lib.set_sorting(&path, &sorting));
        Self::report(result);
    }

    pub fn move_source(&mut self, delta: i32) {
        let sources = self.sources();
        let at = sources.iter().position(|s| *s == self.source).unwrap_or(0) as i64;
        let next = (at + i64::from(delta)).clamp(0, sources.len() as i64 - 1) as usize;
        self.selection.clear();
        self.show(sources[next].clone());
    }

    fn preparation(&mut self) -> Option<i64> {
        let existing = self
            .nodes
            .iter()
            .find(|n| n.parent.is_none() && n.kind == NodeKind::Playlist && n.name == PREPARATION)
            .map(|n| n.id);
        if existing.is_some() {
            return existing;
        }
        let id = self
            .library()?
            .create_node(None, NodeKind::Playlist, PREPARATION, None)
            .ok()?;
        self.refresh();
        Some(id)
    }

    pub fn toggle_preparation(&mut self) {
        let Some(track) = self.selected().and_then(|id| self.resolve(id)) else {
            return;
        };
        let Some(prep) = self.preparation() else {
            return;
        };
        let Some(lib) = self.library() else { return };
        let result = match lib.node_tracks(prep) {
            Ok(tracks) => match tracks.iter().position(|&t| t == track) {
                Some(index) => lib.remove_from_playlist(prep, index),
                None => lib.add_to_playlist(prep, &[track]),
            },
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            crate::toast::error(e.to_string());
        }
        if self.source == Source::Node(prep) {
            self.recompute();
        }
    }

    pub fn show_preparation(&mut self) {
        if let Some(prep) = self.preparation() {
            self.show(Source::Node(prep));
        }
    }

    pub fn preview_selected(&mut self) -> Option<BrowserAction> {
        let id = self.selected()?;
        self.preview(id, Some(0.0))
    }

    pub fn unload_preview(&mut self) -> BrowserAction {
        BrowserAction::Preview(self.preview.unload())
    }

    pub fn seek_preview(&self, steps: f32) -> Option<BrowserAction> {
        let s = &self.preview_state;
        (s.loaded && s.duration_s > 0.0).then(|| {
            let at = (s.position_s / s.duration_s + f64::from(steps) * 0.01).clamp(0.0, 1.0);
            BrowserAction::Preview(vec![backspn_engine::DeckCommand::Seek {
                seconds: at * s.duration_s,
            }])
        })
    }

    pub fn step_favorite(&mut self, step: i32) {
        use crate::favorites::{SLOTS, resolve, step as next};
        self.favorites.resize(SLOTS, String::new());
        let shown =
            (0..SLOTS).find(|&slot| self.shows(resolve(&self.nodes, &self.favorites[slot])));
        let target = resolve(&self.nodes, &self.favorites[next(shown, step)]);
        self.show_favorite(target);
    }

    fn shows(&self, target: Option<crate::favorites::Target>) -> bool {
        use crate::favorites::Target;
        match target {
            Some(Target::Collection) => self.source == Source::Collection,
            Some(Target::Node(id)) => self.source == Source::Node(id),
            Some(Target::Recordings) => self.source == Source::Recordings,
            Some(Target::History) => {
                matches!(self.source, Source::History(h) if Some(h) == self.current_history)
            }
            None => false,
        }
    }

    fn show_favorite(&mut self, target: Option<crate::favorites::Target>) {
        use crate::favorites::Target;
        match target {
            Some(Target::Collection) => self.show(Source::Collection),
            Some(Target::Node(id)) => self.show(Source::Node(id)),
            Some(Target::Recordings) => self.show(Source::Recordings),
            Some(Target::History) => {
                if let Some(h) = self.current_history {
                    self.show(Source::History(h));
                }
            }
            None => {}
        }
    }

    pub fn selected(&self) -> Option<i64> {
        self.selection
            .cursor()
            .and_then(|r| self.view.get(r).copied())
    }

    fn selected_rows(&self) -> Vec<(usize, i64)> {
        self.selection
            .rows()
            .filter_map(|r| Some((r, *self.view.get(r)?)))
            .collect()
    }

    pub(crate) fn with_lib_value<T>(
        &mut self,
        f: impl FnOnce(&mut Library) -> backspn_library::Result<T>,
    ) -> backspn_library::Result<T> {
        match &mut self.lib {
            Ok(lib) => f(lib),
            Err(e) => Err(backspn_library::Error::Invalid(e.clone())),
        }
    }

    fn resolve_selection(&mut self) -> Vec<i64> {
        let ids: Vec<i64> = self.selected_rows().into_iter().map(|(_, id)| id).collect();
        ids.into_iter().filter_map(|id| self.resolve(id)).collect()
    }

    fn selected_entries(&self) -> Vec<usize> {
        self.selection
            .rows()
            .filter_map(|r| self.view_source.get(r).copied())
            .collect()
    }

    fn with_lib(
        &mut self,
        f: impl FnOnce(&mut Library) -> backspn_library::Result<()>,
    ) -> backspn_library::Result<()> {
        self.lib.as_mut().map_or(Ok(()), f)
    }

    fn report(result: backspn_library::Result<()>) {
        if let Err(e) = result {
            crate::toast::error(e.to_string());
        }
    }

    pub fn take_message(&mut self) -> Option<String> {
        self.message.take()
    }
}

#[cfg(test)]
mod tests;
