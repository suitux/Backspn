use std::path::PathBuf;

use backspn_analysis::Analysis;
use backspn_control::{Action, Trigger};
use backspn_engine::{Command, DECKS, DeckCommand, DeckState, EngineState, Grid};

use crate::browser::{AnalysedGrid, BrowserAction};
use crate::deck_view::{DeckEvent, LoadRequest};
use crate::grid_edit::{self, GridEdit};
use crate::stems::{self, LoadAs};
use crate::{App, tap_tempo, track_store};

impl App {
    fn may_load(&mut self, deck: usize) -> bool {
        use crate::deck_informer::{Key, Notice};
        if self.decks.get(deck).is_some_and(|d| d.live) {
            self.decks[deck].notify_for(Notice::deck_type(), Key::DeckType);
            return false;
        }
        let blocked = self.settings.load_only_stopped && self.playing.get(deck) == Some(&true);
        if blocked {
            let notice = Notice::deck_locked();
            crate::toast::warning(crate::deck_informer::toast_text(deck, &notice));
            self.decks[deck].notify_for(notice, Key::DeckLocked);
        }
        !blocked
    }

    pub(crate) fn open_file(&mut self, deck: usize, path: PathBuf) {
        if !self.may_load(deck) {
            return;
        }
        if let Some(&id) = self.browser.add_files(std::slice::from_ref(&path)).first() {
            self.load_from_library(deck, id);
        } else {
            let mut request = LoadRequest::file(path);
            request.analysis = self.settings.load_analysis();
            self.decks[deck].load(request);
        }
    }

    pub(crate) fn output_rate(&self) -> u32 {
        self.engine
            .as_ref()
            .map_or(44_100, |e| e.info().sample_rate)
    }

    pub(crate) fn load_from_library(&mut self, deck: usize, id: i64) {
        self.load_track(deck, id, None, LoadAs::Primary);
    }

    pub(crate) fn load_from_library_as(&mut self, deck: usize, id: i64, how: LoadAs) {
        self.load_track(deck, id, None, how);
    }

    pub(crate) fn duplicate_deck(&mut self, deck: usize, from: usize) {
        if deck == from || from >= DECKS {
            return;
        }
        if let Some(id) = self.decks[from].track_id() {
            let how = if self.engine_state().decks[from].stems {
                LoadAs::Stems
            } else {
                LoadAs::Track
            };
            self.load_track(deck, id, Some(from), how);
        }
    }

    fn load_track(&mut self, deck: usize, id: i64, duplicate_of: Option<usize>, how: LoadAs) {
        if !self.may_load(deck) {
            return;
        }
        let Some(id) = self.browser.resolve(id) else {
            return;
        };
        let Some(lib) = self.browser.library() else {
            return;
        };
        let (Ok(Some(track)), Ok(cues)) = (lib.track(id), lib.cues(id)) else {
            return;
        };
        let mut request = track_store::load_request(&track, &cues, self.settings.cue_to_marker);
        request.duplicate_of = duplicate_of;
        if duplicate_of.is_none() && self.settings.duplicate_same_track {
            request.duplicate_of = track_store::duplicate_source(
                deck,
                (0..DECKS).map(|d| (self.decks[d].track_id(), self.playing[d])),
                id,
            );
        }
        request.analysis = self.settings.load_analysis();
        let generated = self.browser.generated_stems(id);
        request.stems = stems::stems_to_play(
            &request.path,
            generated.as_deref(),
            self.settings.stems_default_load,
            how,
        );
        request.output_rate = self.output_rate();
        let generation = self.engine_state().decks[deck].generation;
        self.watch[deck] = Some(track_store::Watch::new(id, generation));
        self.decks[deck].load(request);
    }

    pub(crate) fn handle_browser_actions(&mut self, actions: Vec<BrowserAction>) {
        for action in actions {
            match action {
                BrowserAction::Load { deck, track } => self.load_from_library(deck, track),
                BrowserAction::LoadAs { deck, track, mode } => {
                    self.load_from_library_as(deck, track, mode.into());
                }
                BrowserAction::LoadSet { deck, set } => self.load_sample_set(deck, set),
                BrowserAction::ExportSet(set) => self.export_set(set),
                BrowserAction::ImportSet => self.import_set(),
                BrowserAction::FoldersChanged(folders) => self.settings.music_folders = folders,
                BrowserAction::LibraryXmlChanged(path) => self.settings.library_xml = path,
                BrowserAction::Preview(commands) => {
                    self.outbox
                        .extend(commands.into_iter().map(Command::Preview));
                }
                BrowserAction::SaveLoop(from) => self.save_loop_from(from),
            }
        }
    }

    pub(crate) fn handle_deck_events(&mut self, events: Vec<DeckEvent>) {
        for event in events {
            match event {
                DeckEvent::Analyzed {
                    track_id,
                    analysis,
                    keep_grid,
                } => {
                    self.save_analysis(track_id, &analysis, keep_grid);
                }
                DeckEvent::ShowInBrowser(track) => self.browser.jump_to_track(track),
                DeckEvent::Overview { track_id, overview } => {
                    self.browser.store_overview(track_id, &overview);
                }
                DeckEvent::Bend {
                    deck,
                    direction,
                    down,
                } => {
                    let trigger = if down {
                        Trigger::Press
                    } else {
                        Trigger::Release
                    };
                    let state = self.engine_state();
                    self.execute(Action::TempoBend { deck, direction }, trigger, &state);
                }
                DeckEvent::Action(action) => {
                    let state = self.engine_state();
                    self.execute(action, Trigger::Press, &state);
                }
                DeckEvent::Grid { deck, edit } => self.edit_grid(deck, edit),
                DeckEvent::Lock { deck, locked } => self.lock_track(deck, locked),
                DeckEvent::Reanalyse { deck, grid } => {
                    if let Some(track) = self.decks[deck]
                        .track_id()
                        .filter(|_| !self.decks[deck].locked())
                    {
                        self.browser.reanalyse_grid(track, grid);
                    }
                }
                DeckEvent::CuePick { deck, index } => self.pick_cue(deck, index),
                DeckEvent::CueRename { deck, name } => self.rename_cue(deck, &name),
                DeckEvent::Duplicate { deck, from } => {
                    let source = self.engine_state().decks[from];
                    if source.loaded {
                        self.outbox
                            .push(Command::Deck(deck, DeckCommand::SetTempo(source.tempo)));
                        self.outbox
                            .push(Command::Deck(deck, DeckCommand::SetKeylock(source.keylock)));
                        self.outbox.push(Command::Deck(
                            deck,
                            DeckCommand::Seek {
                                seconds: source.position_s,
                            },
                        ));
                        if source.playing {
                            self.outbox.push(Command::Deck(deck, DeckCommand::Play));
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn beat_tap(&mut self, deck: usize, state: &DeckState) {
        if !state.loaded {
            return;
        }
        let Some(seed) =
            tap_tempo::deck_tap(&mut self.beat_taps[deck], state.position_s, state.tempo)
        else {
            return;
        };
        let Some(track_id) = self.decks[deck].track_id() else {
            return;
        };
        let transients = self
            .browser
            .library()
            .and_then(|lib| lib.transients(track_id).ok())
            .unwrap_or_default();
        if transients.is_empty() {
            crate::toast::warning(
                "Analyze the track first: detecting its BPM needs its transients.",
            );
            return;
        }
        let transients: Vec<_> = transients.iter().map(track_store::onset).collect();
        let taps: Vec<f32> = self.beat_taps[deck]
            .taps()
            .iter()
            .map(|&t| t as f32)
            .collect();
        let (bpm, anchor_s) = backspn_analysis::tap::tapped_grid(&transients, seed as f32, &taps);
        let bpm = f64::from(bpm);
        if state.grid.is_some() {
            let grid = Grid {
                bpm,
                anchor_s: f64::from(anchor_s),
                ..Grid::FIXED
            };
            self.outbox.push(Command::Deck(
                deck,
                DeckCommand::SetGrid(Some(self.decks[deck].flex(grid))),
            ));
            self.store_grid(deck, grid);
        } else if let Some(lib) = self.browser.library()
            && let Err(e) = lib.save_analysis(
                track_id,
                &backspn_library::AnalysisUpdate {
                    bpm: Some(bpm),
                    ..Default::default()
                },
            )
        {
            log::warn!("cannot store tempo: {e}");
        } else {
            self.browser.refresh_track(track_id);
        }
    }

    pub(crate) fn edit_grid(&mut self, deck: usize, edit: GridEdit) {
        let state = self.engine_state().decks[deck];
        if self.decks[deck].locked() {
            return;
        }
        if let Some(grid) = state
            .grid
            .and_then(|g| grid_edit::apply(g, edit, state.position_s))
        {
            self.outbox.push(Command::Deck(
                deck,
                DeckCommand::SetGrid(Some(self.decks[deck].flex(grid))),
            ));
            self.store_grid(deck, grid);
        }
    }

    pub(crate) fn background_work(&mut self, ctx: &eframe::egui::Context, state: &EngineState) {
        self.persist_hotcues(state);
        self.watch_audio(ctx);
        self.midi_clock.watch(
            self.settings.midi_clock_send,
            self.settings.midi_clock_output.as_deref(),
            &mut self.outbox,
        );
        self.update_bends(state);
        self.update_key_resets();
        self.update_auto_gain();
        self.sync_status(state);
        self.update_auto_fade(ctx, self.last_frame.elapsed().as_secs_f64());
        for (d, (view, &tick)) in self.decks.iter_mut().zip(&state.ticks).enumerate() {
            view.tick = tick;
            view.focused = d == self.focus.deck;
        }
        for (d, (view, deck)) in self.decks.iter_mut().zip(&state.decks).enumerate() {
            view.end_warning_s = self.settings.end_warning_s;
            view.auto_cue_db = self.settings.auto_cue[d].then_some(self.settings.auto_cue_level_db);
            view.track_end_warning(deck);
        }
        self.browser.set_context(ctx);
        let grids = self.browser.collect_analysis();
        self.browser.collect_stems();
        self.apply_analysed_grids(&grids, state);
        self.tap.expire(self.epoch.elapsed().as_secs_f64());
    }

    pub(crate) fn apply_analysed_grids(&mut self, grids: &[AnalysedGrid], state: &EngineState) {
        for g in grids {
            for deck in 0..DECKS {
                if self.decks[deck].track_id() != Some(g.track) || self.decks[deck].locked() {
                    continue;
                }
                if let Some(grid) = track_store::analysed_grid(g, state.decks[deck].grid) {
                    self.outbox.push(Command::Deck(
                        deck,
                        DeckCommand::SetGrid(Some(self.decks[deck].flex(grid))),
                    ));
                }
            }
        }
    }

    pub(crate) fn lock_track(&mut self, deck: usize, locked: bool) {
        let Some(track_id) = self.decks[deck].track_id() else {
            return;
        };
        if let Some(lib) = self.browser.library() {
            if let Err(e) = lib.set_locked(&[track_id], locked) {
                log::warn!("cannot lock the track: {e}");
                return;
            }
            self.decks[deck].set_locked(locked);
            self.browser.refresh_track(track_id);
        }
    }

    pub(crate) fn store_grid(&mut self, deck: usize, grid: Grid) {
        let Some(track_id) = self.decks[deck].track_id() else {
            return;
        };
        let Some(lib) = self.browser.library() else {
            return;
        };
        if let Err(e) = lib.save_analysis(track_id, &track_store::grid_update(grid)) {
            log::warn!("cannot store grid: {e}");
        }
        self.browser.refresh_track(track_id);
        self.browser.tags_changed(&[track_id]);
    }

    fn save_analysis(&mut self, track_id: i64, analysis: &Analysis, keep_grid: bool) {
        let Some(lib) = self.browser.library() else {
            return;
        };
        let known_key = lib
            .track(track_id)
            .ok()
            .flatten()
            .is_some_and(|t| !t.key_text.is_empty());
        let update = track_store::analysis_update(analysis, keep_grid, known_key);
        if let Err(e) = lib.save_analysis(track_id, &update) {
            log::warn!("cannot store analysis: {e}");
        }
        self.browser.refresh_track(track_id);
        self.browser.tags_changed(&[track_id]);
    }

    pub(crate) fn persist_hotcues(&mut self, state: &EngineState) {
        for deck in 0..DECKS {
            if self.decks[deck].is_decoding() {
                continue;
            }
            let Some(track_id) = self.decks[deck].track_id() else {
                continue;
            };
            let d = &state.decks[deck];
            let watch = self.watch[deck]
                .get_or_insert_with(|| track_store::Watch::new(track_id, d.generation));
            if let Some(changed) = watch.observe(track_id, d.generation, d.loaded, d.hotcues)
                && self.decks[deck]
                    .cues_mut()
                    .is_some_and(|cues| cues.sync_hotcues(&changed))
            {
                self.save_cues(deck);
            }
        }
    }

    pub(crate) fn save_cues(&mut self, deck: usize) {
        let view = &self.decks[deck];
        let (Some(track_id), Some(cues)) = (view.track_id(), view.cues()) else {
            return;
        };
        let cues: Vec<_> = cues.cues().cloned().collect();
        if let Some(lib) = self.browser.library()
            && let Err(e) = lib.save_cues(track_id, &cues)
        {
            log::warn!("cannot store cue points: {e}");
        }
        self.browser.refresh_track(track_id);
    }
}
