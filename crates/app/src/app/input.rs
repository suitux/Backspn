use std::path::PathBuf;
use std::time::{Duration, Instant};

use backspn_control::{Action, MoveMode, Trigger};
use backspn_engine::{Command, DECKS, DeckCommand, EngineState, Grid, MixerCommand, SyncCommand};
use eframe::egui;

use crate::browser::BrowserAction;
use crate::controls::Controls;
use crate::{App, is_audio, jog, screen, waveform};

const SCREEN_PERIOD: Duration = Duration::from_millis(33);

const SEARCH_SECONDS_PER_TURN: f64 = 20.0;
const GRID_SECONDS_PER_TURN: f64 = 0.1;
const PREVIEW_BLINK: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowseMode {
    List,
    Favorites,
    Preview(Instant),
}

impl BrowseMode {
    fn after(self, favorites: bool, pressed: bool, now: Instant) -> Self {
        match (self, favorites, pressed) {
            (Self::List, false, true) => Self::Preview(now),
            (Self::List, true, true) => Self::Favorites,
            (Self::Preview(_), false, false) | (Self::Favorites, true, false) => Self::List,
            (mode, ..) => mode,
        }
    }

    fn preview_lit(self, now: Instant) -> bool {
        match self {
            Self::Preview(since) => {
                let half = PREVIEW_BLINK.as_millis() / 2;
                (now.duration_since(since).as_millis() / half).is_multiple_of(2)
            }
            _ => false,
        }
    }
}

impl App {
    pub(crate) fn handle_input(&mut self, ctx: &egui::Context) {
        let (dropped, hover) = ctx.input(|i| {
            let paths: Vec<PathBuf> = i
                .raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect();
            (paths, i.pointer.hover_pos())
        });
        let (folders, files): (Vec<PathBuf>, Vec<PathBuf>) =
            dropped.into_iter().partition(|p| p.is_dir());
        for folder in folders {
            if let Some(BrowserAction::FoldersChanged(list)) = self.browser.add_folder(folder) {
                self.settings.music_folders = list;
                self.settings.show_browser = true;
            }
        }
        if let Some(path) = files.into_iter().find(|p| is_audio(p)) {
            let target = hover
                .and_then(|pos| {
                    self.decks[..self.settings.deck_count]
                        .iter()
                        .position(|d| d.contains(pos))
                })
                .unwrap_or(0);
            self.open_file(target, path);
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn execute(&mut self, action: Action, trigger: Trigger, state: &EngineState) {
        if trigger == Trigger::Press
            && let Some(controls) = &mut self.controls
        {
            controls.fired(action);
        }
        let trigger = match trigger {
            Trigger::Reset => reset_position(action).map_or(Trigger::Reset, Trigger::Set),
            t => t,
        };
        let Some((action, trigger)) = self.pad_layer_action(action, trigger, state) else {
            return;
        };
        if trigger == Trigger::Press {
            self.count(|u| count_action(u, action));
        }
        if self.mixer.apply(action, trigger, &mut self.outbox)
            || self.fx.apply(action, trigger, &mut self.outbox)
            || self.cue_action(action, trigger, state)
            || self.sample_action(action, trigger, state)
        {
            return;
        }
        let pressed = trigger == Trigger::Press;
        let deck = |d: usize, c: DeckCommand| Command::Deck(d, c);
        let platter_deck: [bool; DECKS] = std::array::from_fn(|d| self.platter_deck(d));
        let platter_deck = |d: usize| d < DECKS && platter_deck[d];
        let spt = self.platter_seconds_per_turn();
        let out = &mut self.outbox;
        match action {
            Action::MasterTap if pressed => self.master_tap(),
            Action::RecordStop if pressed => self.toggle_recording(),
            Action::RecordCut if pressed => self.recorder.cut(),
            Action::LoopRecorder { part } => {
                let external = self.settings.external_mixer.on;
                if let Some(c) =
                    super::loop_rec::command(part, trigger, &state.loop_recorder, external)
                {
                    out.push(Command::LoopRecorder(c));
                }
            }
            Action::RecordGain => {
                let current = self.settings.recording_gain;
                match trigger {
                    Trigger::Set(v) => self.set_recording_gain(v),
                    Trigger::Delta(d) => self.set_recording_gain(current + d / 64.0),
                    _ => {}
                }
            }
            Action::LoadLastRecording { deck: d } if pressed && d < DECKS => {
                self.load_last_recording(d);
            }
            Action::LoadNext { deck: d } if pressed && d < DECKS => {
                if let Some(track) = self.browser.step_selection(1) {
                    self.load_from_library(d, track);
                }
            }
            Action::LoadSelectedAs { deck: d, mode }
                if pressed && d < DECKS && self.browse_mode == BrowseMode::List =>
            {
                if let Some(track) = self.browser.selected() {
                    self.load_from_library_as(d, track, mode.into());
                }
            }
            Action::Stem {
                deck: d,
                stem,
                part,
            } if d < DECKS => {
                self.decks[d].apply_stem(stem, part, trigger, &mut self.outbox);
            }
            Action::LoadPrevious { deck: d } if pressed && d < DECKS => {
                if let Some(track) = self.browser.step_selection(-1) {
                    self.load_from_library(d, track);
                }
            }
            Action::MasterTick if pressed => out.push(Command::MasterTick(!state.master_tick)),
            Action::ClockSend if pressed => {
                let on = !self.midi_clock.send;
                self.midi_clock.set_send(on, out);
            }
            Action::ClockResync if pressed => crate::midi_clock::MidiClock::resync(out),
            Action::ClockExternal if pressed => {
                out.push(Command::Sync(SyncCommand::SetExternal(
                    !state.sync.external,
                )));
                if !state.sync.external {
                    self.set_link(false);
                }
            }
            Action::ClockLink if pressed => self.set_link(!self.link.enabled()),
            Action::LiveInput { deck: d } if pressed => self.toggle_live_input(d, state),
            Action::LinkDownbeat if pressed && self.link.enabled() => {
                self.link
                    .force_beat_at_time(0.0, backspn_link::host_micros(), 4.0);
            }
            Action::DeckTick { deck: d } if pressed && d < DECKS => {
                out.push(Command::DeckTick(d, !state.ticks[d]));
            }
            Action::BeatTap { deck: d } if pressed && d < DECKS => {
                self.beat_tap(d, &state.decks[d]);
            }
            Action::PlayPause { deck: d } if pressed => out.push(deck(d, DeckCommand::TogglePlay)),
            Action::Cue { deck: d } => match trigger {
                Trigger::Press => out.push(deck(d, DeckCommand::CueDown)),
                Trigger::Release => out.push(deck(d, DeckCommand::CueUp)),
                _ => {}
            },
            Action::Cup { deck: d } => match trigger {
                Trigger::Press => out.push(deck(d, DeckCommand::CupDown)),
                Trigger::Release => out.push(deck(d, DeckCommand::CupUp)),
                _ => {}
            },
            Action::Sync { deck: d }
            | Action::TempoSync { deck: d }
            | Action::PhaseSync { deck: d }
                if platter_deck(d) => {}
            Action::Sync { deck: d } if pressed && d < DECKS => {
                out.push(Command::Sync(SyncCommand::SetSync(
                    d,
                    !state.sync.synced[d],
                )));
            }
            Action::TempoSync { deck: d } if pressed && d < DECKS => {
                out.push(Command::Sync(SyncCommand::TempoSync(d)));
            }
            Action::PhaseSync { deck: d } if pressed && d < DECKS => {
                out.push(Command::Sync(SyncCommand::PhaseSync(d)));
            }
            Action::Master { deck: d } if pressed => {
                let master = (state.sync.master != Some(d)).then_some(d);
                out.push(Command::Sync(SyncCommand::SetMaster(master)));
            }
            Action::Keylock { deck: d } if pressed && d < DECKS => {
                out.push(deck(d, DeckCommand::SetKeylock(!state.decks[d].keylock)));
            }
            Action::Hotcue { deck: d, slot } => match trigger {
                Trigger::Press => out.push(deck(d, DeckCommand::Hotcue(slot))),
                Trigger::Release => out.push(deck(d, DeckCommand::HotcueRelease(slot))),
                _ => {}
            },
            Action::Slip { deck: d } if pressed && d < DECKS => {
                out.push(deck(d, DeckCommand::SetSlip(!state.decks[d].slip)));
            }
            Action::Reverse { deck: d } => match trigger {
                Trigger::Press => out.push(deck(d, DeckCommand::Reverse(true))),
                Trigger::Release => out.push(deck(d, DeckCommand::Reverse(false))),
                _ => {}
            },
            Action::DeckSize { deck: d } if pressed && d < DECKS => {
                let next = self.decks[d].size().next();
                self.decks[d].set_size(next);
            }
            Action::Freeze { deck: d } if pressed && d < DECKS => {
                out.push(deck(
                    d,
                    DeckCommand::SetFreeze {
                        on: state.decks[d].freeze.is_none(),
                        loop_beats: self.decks[d].loop_beats(),
                    },
                ));
            }
            Action::Slice { deck: d, slot } => match trigger {
                Trigger::Press => out.push(deck(
                    d,
                    DeckCommand::FreezeSlice {
                        slice: slot,
                        pressed: true,
                    },
                )),
                Trigger::Release => out.push(deck(
                    d,
                    DeckCommand::FreezeSlice {
                        slice: slot,
                        pressed: false,
                    },
                )),
                _ => {}
            },
            Action::DeleteHotcue { deck: d, slot } if pressed => {
                out.push(deck(d, DeckCommand::DeleteHotcue(slot)));
            }
            Action::LoopIn { deck: d } if pressed => out.push(deck(d, DeckCommand::LoopIn)),
            Action::LoopOut { deck: d } if pressed => out.push(deck(d, DeckCommand::LoopOut)),
            Action::LoopActive { deck: d } if pressed && d < DECKS => {
                let command = if state.decks[d].loop_end_s.is_some() {
                    DeckCommand::LoopToggle
                } else {
                    DeckCommand::AutoLoop {
                        beats: self.decks[d].loop_beats(),
                    }
                };
                out.push(deck(d, command));
            }
            Action::FastLoop { deck: d } if pressed && d < DECKS => {
                let beats = self.decks[d].loop_beats();
                out.push(deck(d, DeckCommand::FastLoop { beats }));
            }
            Action::SlipRoll { deck: d, beats } if d < DECKS => {
                let on = match trigger {
                    Trigger::Press => true,
                    Trigger::Release => false,
                    _ => return,
                };
                let s = &state.decks[d];
                let now = (s.slip, s.loop_active, self.decks[d].loop_beats());
                let commands = self.slip_rolls[d].commands(on, beats, now);
                if on {
                    self.decks[d].set_loop_beats(beats);
                }
                out.extend(commands.into_iter().map(|c| deck(d, c)));
            }
            Action::MicToggle if pressed => {
                let at_zero_db = (self.settings.microphone_gain - 1.0).abs() < f32::EPSILON;
                self.settings.microphone_gain = if at_zero_db { 0.0 } else { 1.0 };
                self.send_inputs();
            }
            Action::LoopSize { deck: d, beats } if pressed => {
                if let Some(view) = self.decks.get_mut(d) {
                    view.set_loop_beats(beats);
                }
                out.push(deck(d, DeckCommand::AutoLoop { beats }));
            }
            Action::BeatJump { deck: d, beats } => {
                let stay_in_loop = crate::moves::is_fine_jump(beats);
                let beats = match trigger {
                    Trigger::Press => {
                        out.push(deck(d, DeckCommand::SlipJump(true)));
                        beats
                    }
                    Trigger::Delta(steps) => beats.abs() * f64::from(steps),
                    Trigger::Release => return out.push(deck(d, DeckCommand::SlipJump(false))),
                    Trigger::Set(_) | Trigger::Reset => return,
                };
                out.push(deck(
                    d,
                    DeckCommand::BeatJump {
                        beats,
                        stay_in_loop,
                    },
                ));
            }
            Action::AutoFade { direction } if pressed => {
                self.auto_fade = match self.auto_fade {
                    Some(_) => None,
                    None => Some(if direction < 0.0 { 0.0 } else { 1.0 }),
                };
            }
            Action::DuplicateDeck { deck: d, from } if pressed && d < DECKS => {
                self.duplicate_deck(d, from);
            }
            Action::CopyPhase { deck: d } if pressed && d < DECKS => {
                let master_beat = master_beat(state);
                self.edit_grid(d, crate::grid_edit::GridEdit::CopyPhase(master_beat));
            }
            Action::MoveMode { deck: d, mode } if pressed && d < DECKS => {
                self.decks[d].set_move_mode(mode);
            }
            Action::MoveSize { deck: d, size } if pressed && d < DECKS => {
                self.decks[d].set_move_size(size);
            }
            Action::Move { deck: d, direction } if d < DECKS => {
                let view = &self.decks[d];
                let steps = match trigger {
                    Trigger::Press => {
                        if view.move_mode() == MoveMode::BeatJump {
                            out.push(deck(d, DeckCommand::SlipJump(true)));
                        }
                        if direction < 0.0 { -1 } else { 1 }
                    }
                    Trigger::Delta(steps) => steps.round() as i32,
                    Trigger::Release => return out.push(deck(d, DeckCommand::SlipJump(false))),
                    Trigger::Set(_) | Trigger::Reset => return,
                };
                out.extend(view.move_command(steps).map(|c| deck(d, c)));
            }
            Action::Tempo { deck: d } | Action::TempoRelative { deck: d } if platter_deck(d) => {
                self.decks[d].apply_platter_tempo(trigger, out);
            }
            Action::Tempo { deck: d } if d < DECKS => self.decks[d].apply_tempo(trigger, out),
            Action::TempoRelative { deck: d } if d < DECKS => {
                if let Trigger::Delta(steps) = trigger {
                    self.decks[d].apply_relative_tempo(steps, out);
                }
            }
            Action::KeyAdjust { deck: d } if d < DECKS => {
                self.decks[d].apply_key_adjust(&state.decks[d], trigger, out);
            }
            Action::KeyFine { deck: d } if d < DECKS => {
                if let Trigger::Delta(steps) = trigger {
                    let st = crate::key_shift::fine_step(state.decks[d].key_adjust, steps);
                    out.push(deck(d, DeckCommand::SetKeyAdjust(st)));
                }
            }
            Action::KeySemitone { deck: d, direction } if pressed && d < DECKS => {
                let st =
                    crate::key_shift::semitone_step(state.decks[d].key_adjust, direction > 0.0);
                out.push(deck(d, DeckCommand::SetKeyAdjust(st)));
            }
            Action::KeyReset { deck: d } if d < DECKS => {
                self.key_reset_held[d] = pressed.then(Instant::now);
            }
            Action::TempoBend { deck: d, direction } if d < DECKS => match trigger {
                Trigger::Press => {
                    let bend = direction * self.bend_amount(Duration::ZERO);
                    self.bend_held[d] = Some((direction, Instant::now(), bend));
                    self.send_bend(d, bend, state);
                }
                Trigger::Release => {
                    self.bend_held[d] = None;
                    self.send_bend(d, 0.0, state);
                }
                _ => {}
            },
            Action::BrowserUpDown => {
                let steps = match trigger {
                    Trigger::Delta(steps) => steps,
                    Trigger::Press => 1.0,
                    _ => return,
                };
                match self.browse_mode {
                    BrowseMode::List => self.browser.move_selection(steps.round() as i32),
                    BrowseMode::Favorites => self.browser.step_favorite(steps.round() as i32),
                    BrowseMode::Preview(_) => {
                        if let Some(action) = self.browser.seek_preview(steps) {
                            self.handle_browser_actions(vec![action]);
                        }
                    }
                }
            }
            Action::BrowserPreview | Action::BrowserFavorites
                if matches!(trigger, Trigger::Press | Trigger::Release) =>
            {
                let favorites = action == Action::BrowserFavorites;
                let before = self.browse_mode;
                self.browse_mode = before.after(favorites, pressed, Instant::now());
                let action = match (before, self.browse_mode) {
                    (BrowseMode::List, BrowseMode::Preview(_)) => self.browser.preview_selected(),
                    (BrowseMode::Preview(_), BrowseMode::List) => {
                        Some(self.browser.unload_preview())
                    }
                    _ => None,
                };
                self.handle_browser_actions(action.into_iter().collect());
            }
            Action::LoadSelected { deck: d }
                if pressed && d < DECKS && self.browse_mode == BrowseMode::List =>
            {
                if let Some(track) = self.browser.selected() {
                    self.load_from_library(d, track);
                }
            }
            Action::JogMode { deck: d, turntable } if pressed && d < DECKS => {
                self.jog[d].turntable = turntable;
                if !turntable && let Some(c) = self.platters[d].leave() {
                    out.push(deck(d, c));
                }
            }
            Action::MotorOff { deck: d } if pressed && platter_deck(d) => {
                self.platters[d].motor_off();
            }
            Action::JogTouch { deck: d } if platter_deck(d) => {
                let down = match trigger {
                    Trigger::Press => true,
                    Trigger::Release => false,
                    _ => return,
                };
                let now = Instant::now();
                let jog = self.jog[d].touch(down);
                let platter = &mut self.platters[d];
                platter.touch(down);
                platter.update(state.decks[d].playing, spt, now);
                out.extend(platter.sync_scratch().map(|c| deck(d, c)));
                if platter.route() == crate::platter::Route::Paused {
                    out.push(deck(d, jog));
                }
            }
            Action::JogTurn { deck: d } if platter_deck(d) => {
                let Trigger::Delta(turns) = trigger else {
                    return;
                };
                let now = Instant::now();

                let seconds = f64::from(turns) * spt;
                let s = &state.decks[d];
                let platter = &mut self.platters[d];
                platter.turn(f64::from(turns));
                platter.update(s.playing, spt, now);
                out.extend(platter.sync_scratch().map(|c| deck(d, c)));
                match platter.route() {
                    crate::platter::Route::Ignore => {}
                    crate::platter::Route::Playing => {
                        out.push(deck(d, DeckCommand::ScratchMove(seconds)));
                    }
                    crate::platter::Route::Paused => {
                        let bend = crate::jog::Bend {
                            spin: 1.0,
                            nudge: 1.0,
                        };
                        let commands = self.jog[d].turn(seconds, bend, false, s.scratching, now);
                        out.extend(commands.into_iter().map(|c| deck(d, c)));
                    }
                }
            }
            Action::JogTouch { deck: d } if d < DECKS => {
                let down = match trigger {
                    Trigger::Press => true,
                    Trigger::Release => false,
                    _ => return,
                };
                out.push(deck(d, self.jog[d].touch(down)));
            }
            Action::JogTurn { deck: d } if d < DECKS => {
                let Trigger::Delta(turns) = trigger else {
                    return;
                };
                let seconds = f64::from(turns) * self.settings.jog_seconds_per_turn;
                let s = &state.decks[d];
                for c in self.jog[d].turn(
                    seconds,
                    crate::jog::Bend {
                        spin: crate::jog::bend_factor(self.settings.jog_sensitivity),
                        nudge: crate::jog::bend_factor(self.settings.jog_nudge_sensitivity),
                    },
                    s.playing,
                    s.scratching,
                    Instant::now(),
                ) {
                    if matches!(c, DeckCommand::Nudge(_)) && state.sync.synced[d] {
                        out.push(Command::Sync(SyncCommand::ReleasePhase(d)));
                    }
                    out.push(deck(d, c));
                }
            }
            Action::LoopMove { deck: d } if d < DECKS => {
                let steps = match trigger {
                    Trigger::Press => 1,
                    Trigger::Delta(steps) => steps.round() as i32,
                    _ => return,
                };
                let mode = if state.decks[d].loop_active {
                    MoveMode::Loop
                } else {
                    MoveMode::BeatJump
                };
                let size = backspn_control::MoveSize::LOOP;
                out.extend(
                    crate::moves::command(mode, size, steps, self.decks[d].loop_beats())
                        .map(|c| deck(d, c)),
                );
            }
            Action::TempoRange { deck: d } if d < DECKS => self.decks[d].apply_tempo_range(trigger),
            Action::LoopSizeStep { deck: d } if d < DECKS => {
                let Trigger::Delta(steps) = trigger else {
                    return;
                };
                let old = self.decks[d].loop_beats();
                let new = (old * 2f64.powf(f64::from(steps).round())).clamp(1.0 / 32.0, 32.0);
                self.decks[d].set_loop_beats(new);
                if state.decks[d].loop_active && (new - old).abs() > f64::EPSILON {
                    out.push(deck(d, DeckCommand::LoopScale(new / old)));
                }
            }
            Action::ReturnToStart { deck: d } if pressed => {
                out.push(deck(d, DeckCommand::Seek { seconds: 0.0 }));
            }
            Action::TempoReset { deck: d } if pressed && d < DECKS && !state.sync.synced[d] => {
                out.push(deck(d, DeckCommand::SetTempo(1.0)));
            }
            Action::TempoLock { deck: d }
                if pressed && d < DECKS && !state.sync.synced[d] && !platter_deck(d) =>
            {
                let absolute = !self.settings.tempo_fader_relative;
                self.decks[d].toggle_tempo_lock(absolute, out);
            }
            Action::BrowserTree => match trigger {
                Trigger::Delta(steps) => self.browser.move_source(steps.round() as i32),
                Trigger::Press => self.browser.move_source(1),
                _ => {}
            },
            Action::BrowserFavoriteStep => match trigger {
                Trigger::Delta(steps) => self.browser.step_favorite(steps.round() as i32),
                Trigger::Press => self.browser.step_favorite(1),
                _ => {}
            },
            Action::PreviewLoad if pressed => {
                let action = self.browser.preview_selected();
                self.handle_browser_actions(action.into_iter().collect());
            }
            Action::PreviewSeek => {
                if let Trigger::Delta(steps) = trigger
                    && let Some(action) = self.browser.seek_preview(steps)
                {
                    self.handle_browser_actions(vec![action]);
                }
            }
            Action::JogSearch { deck: d } if d < DECKS => {
                if let Trigger::Delta(turns) = trigger {
                    let s = &state.decks[d];
                    let seconds = s.position_s + f64::from(turns) * SEARCH_SECONDS_PER_TURN;
                    out.push(deck(d, DeckCommand::Seek { seconds }));
                }
            }
            Action::GridAdjust { deck: d } if d < DECKS => {
                if let (Trigger::Delta(turns), Some(grid)) = (trigger, state.decks[d].grid) {
                    let moved = Grid {
                        bpm: grid.bpm,
                        anchor_s: grid.anchor_s + f64::from(turns) * GRID_SECONDS_PER_TURN,
                        ..Grid::FIXED
                    };
                    out.push(deck(
                        d,
                        DeckCommand::SetGrid(Some(self.decks[d].flex(moved))),
                    ));
                    self.store_grid(d, moved);
                }
            }
            Action::PrepListToggle if pressed => self.browser.toggle_preparation(),
            Action::PrepListJump if pressed => {
                self.settings.show_browser = true;
                self.browser.show_preparation();
            }
            Action::BrowserView if pressed => {
                self.browser_maximized = !self.browser_maximized;
                self.settings.show_browser |= self.browser_maximized;
            }
            Action::DeckFocus { deck: d } if pressed && d < DECKS => self.focus.select(d),
            Action::FocusToggle if pressed => self.focus.toggle(),
            Action::Snap if pressed => {
                self.settings.snap = !self.settings.snap;
                self.send_deck_settings();
            }
            Action::Quantize if pressed => {
                self.settings.quantize = !self.settings.quantize;
                self.send_deck_settings();
            }
            _ => {}
        }
    }

    fn bend_amount(&self, held: Duration) -> f64 {
        crate::tempo_bend::amount(
            self.settings.bend_sensitivity,
            self.settings.bend_progressive,
            held,
        )
    }

    fn send_bend(&mut self, d: usize, bend: f64, state: &EngineState) {
        if state.sync.synced[d] {
            self.outbox
                .push(Command::Sync(SyncCommand::Bend(d, 1.0 + bend)));
            return;
        }
        let command = if bend == 0.0 {
            self.bend_base[d].take().map(DeckCommand::SetTempo)
        } else {
            let base = *self.bend_base[d].get_or_insert(state.decks[d].tempo);
            Some(DeckCommand::SetTempo(base + bend))
        };
        self.outbox.extend(command.map(|c| Command::Deck(d, c)));
    }

    pub(crate) fn update_auto_fade(&mut self, ctx: &egui::Context, dt_s: f64) {
        let Some(target) = self.auto_fade else {
            return;
        };
        let (position, done) = crate::auto_fade::step(
            self.mixer.crossfader(),
            target,
            self.settings.auto_fade_s,
            dt_s,
        );
        self.mixer
            .apply(Action::Crossfader, Trigger::Set(position), &mut self.outbox);
        if done {
            self.auto_fade = None;
        } else {
            ctx.request_repaint();
        }
    }

    pub(crate) fn update_auto_gain(&mut self) {
        for d in 0..DECKS {
            let db = self.decks[d]
                .lufs()
                .filter(|_| self.settings.auto_gain_on())
                .map_or(0.0, crate::auto_gain::auto_gain_db);
            if self.auto_gain_sent[d] != Some(db) {
                self.auto_gain_sent[d] = Some(db);
                self.outbox
                    .push(Command::Mixer(MixerCommand::AutoGain(d, db)));
            }
        }
    }

    pub(crate) fn update_key_resets(&mut self) {
        for d in 0..DECKS {
            if self.key_reset_held[d]
                .is_some_and(|since| since.elapsed() >= crate::key_shift::RESET_HOLD)
            {
                self.key_reset_held[d] = None;
                self.outbox
                    .push(Command::Deck(d, DeckCommand::SetKeyAdjust(0.0)));
            }
        }
    }

    pub(crate) fn update_bends(&mut self, state: &EngineState) {
        for d in 0..DECKS {
            let Some((direction, since, sent)) = self.bend_held[d] else {
                continue;
            };
            let bend = direction * self.bend_amount(since.elapsed());
            if (bend - sent).abs() > f64::EPSILON {
                self.bend_held[d] = Some((direction, since, bend));
                self.send_bend(d, bend, state);
            }
        }
    }

    pub(crate) fn handle_controls(&mut self, ctx: &egui::Context, state: &EngineState) {
        let clock = self.midi_clock.handlers();
        let controls = self
            .controls
            .get_or_insert_with(|| Controls::open(ctx, clock));
        let events = controls.events(ctx);
        let mut triggers = Vec::new();
        let mut inputs = crate::insights::Tally::default();
        if let Some(wizard) = self.wizard.as_mut().filter(|w| w.testing()) {
            for (source, event) in &events {
                wizard.record(source, event);
            }
        } else {
            for (source, event) in &events {
                if !controls.learn_from(source, event) {
                    let mapped = controls.map(source, event);
                    count_presses(&mut inputs, source, &mapped);
                    triggers.extend(mapped);
                }
            }
        }
        triggers.extend(controls.repeats(ctx));
        self.motor_decks = controls.motor_decks();
        self.count(|u| u.inputs.merge(&inputs));
        for (action, trigger) in triggers {
            self.execute(action, trigger, state);
        }
        let now = Instant::now();
        for d in 0..DECKS {
            let (command, pending) = self.jog[d].tick(now);
            if let Some(c) = command {
                self.outbox.push(Command::Deck(d, c));
            }
            if pending {
                ctx.request_repaint_after(jog::IDLE);
            }
        }
        self.tick_platters(ctx, state, now);
        self.tick_link(ctx, state);
        let turntable = self.jog.map(|j| j.turntable);
        let platter_rpm = self.platter_rpm();
        let platters = std::array::from_fn(|d| backspn_control::feedback::PlatterFeedback {
            motor_on: self.platters[d].motor_on(),
            rpm: platter_rpm
                * crate::platter::tempo_ratio(self.decks[d].tempo_fader_position()) as f32,
            motor_off: self.platters[d].motor_offs(),
        });
        for (view, deck) in self.decks.iter_mut().zip(&state.decks) {
            view.watch_tempo_lock(deck.tempo);
        }
        if let Some(controls) = &mut self.controls {
            controls.show(
                state,
                self.settings.quantize,
                turntable,
                &crate::controls::AppFeedback {
                    jog_tension: self.settings.jog_tension,
                    mixer_fx: self.mixer.mixer_fx(),
                    browser: backspn_control::feedback::BrowserFeedback {
                        maximized: self.settings.show_browser && self.browser_maximized,
                        favorites: self.browse_mode == BrowseMode::Favorites,
                        preview: self.browse_mode.preview_lit(now),
                    },
                    knobs: self.mixer.knobs(),
                    focus: self.focus.deck,
                    platters,
                    tempo_ranges: std::array::from_fn(|d| self.decks[d].tempo_range_index()),
                    microphone: (self.settings.microphone_gain - 1.0).abs() < f32::EPSILON,
                    pattern_steps: std::array::from_fn(|u| self.fx.mapped_steps(u)),
                    loop_sizes: std::array::from_fn(|d| {
                        backspn_control::feedback::loop_size_index(self.decks[d].loop_beats())
                    }),
                    stems: std::array::from_fn(|d| self.decks[d].stem_feedback()),
                    sample_deck: std::array::from_fn(|d| {
                        self.decks[d]
                            .sample_deck
                            .as_ref()
                            .zip(state.sample_deck[d].as_ref())
                            .map(|(view, s)| view.feedback(s, &self.decks[d].stem_controls))
                    }),
                    end_warnings: std::array::from_fn(|d| {
                        crate::track_end::warning(&state.decks[d], self.settings.end_warning_s)
                    }),
                    tempo_locked: std::array::from_fn(|d| self.decks[d].tempo_locked()),
                    live: std::array::from_fn(|d| self.decks[d].live),
                    pad_fx: std::array::from_fn(|d| self.pad_fx[d].lit()),
                    tempo_fader_relative: self.settings.tempo_fader_relative,
                },
            );
            if let Some(due) = controls.output_due() {
                ctx.request_repaint_after(due);
            }
        }
        if matches!(self.browse_mode, BrowseMode::Preview(_)) {
            ctx.request_repaint_after(PREVIEW_BLINK / 2);
        }
        self.update_screens(ctx, state);
    }

    fn platter_deck(&self, d: usize) -> bool {
        self.jog[d].turntable && self.motor_decks[d]
    }

    fn platter_rpm(&self) -> f32 {
        if self.settings.platter_45 {
            backspn_control::s4mk3::motor::RPM_45
        } else {
            backspn_control::s4mk3::motor::RPM_33
        }
    }

    fn platter_seconds_per_turn(&self) -> f64 {
        crate::platter::seconds_per_turn(self.platter_rpm())
    }

    fn tick_platters(&mut self, ctx: &egui::Context, state: &EngineState, now: Instant) {
        let spt = self.platter_seconds_per_turn();
        for d in 0..DECKS {
            if !self.platter_deck(d) {
                continue;
            }
            let platter = &mut self.platters[d];
            platter.update(state.decks[d].playing, spt, now);
            if let Some(c) = platter.sync_scratch() {
                self.outbox.push(Command::Deck(d, c));
            }
            if platter.pending() {
                ctx.request_repaint_after(crate::platter::CHECK);
            }
            if state.sync.synced[d] {
                self.outbox
                    .push(Command::Sync(SyncCommand::SetSync(d, false)));
            }
        }
    }

    fn update_screens(&mut self, ctx: &egui::Context, state: &EngineState) {
        let preview = std::env::var_os("BACKSPN_SCREEN_PREVIEW").map(PathBuf::from);
        let Some(controls) = &self.controls else {
            return;
        };
        let decks = match controls.screens() {
            Some((size, decks)) if size == (screen::WIDTH, screen::HEIGHT) => decks,
            None if preview.is_some() => vec![0, 1],
            Some(_) | None => return,
        };
        ctx.request_repaint_after(SCREEN_PERIOD);
        let now = Instant::now();
        if now < self.next_screen {
            return;
        }
        self.next_screen = now + SCREEN_PERIOD;
        let style = waveform::WaveStyle::get(ctx);
        let theme = crate::theme::get(ctx);
        let frames: Vec<_> = decks
            .iter()
            .map(|&d| {
                let info = self.decks[d].screen_info();
                let key = self.decks[d].key_label(&state.decks[d]);
                let deck = screen::ScreenDeck {
                    index: d,
                    state: &state.decks[d],
                    name: info.map_or("", |i| i.0),
                    peaks: info.map(|i| i.1),
                    loop_beats: self.decks[d].loop_beats(),
                    end_warning: crate::track_end::warning(
                        &state.decks[d],
                        self.settings.end_warning_s,
                    ),
                    key: key.as_deref(),
                    key_color: self.decks[d]
                        .key_text()
                        .and_then(backspn_formats::nml::MusicalKey::parse)
                        .map(crate::key_view::key_colour),
                    synced: state.sync.synced[d],
                    master: state.sync.master == Some(d),
                    zoom_s: self.decks[d].zoom_s,
                    stem: controls
                        .held_stem(d)
                        .and_then(|s| self.decks[d].stem_overlay(s)),
                    sample_deck: self.decks[d]
                        .sample_deck
                        .as_ref()
                        .zip(state.sample_deck[d].as_ref())
                        .map(|(view, sample_deck)| {
                            view.screen(
                                sample_deck,
                                &self.decks[d].stem_controls,
                                controls.held_slot(d),
                                self.decks[d].loop_beats(),
                                screen::deck_color(d, &theme),
                            )
                        }),
                    clock_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0)),
                };
                screen::render(&deck, &style, &theme, self.settings.screen_layout)
            })
            .collect();
        if let Some(dir) = preview {
            for (n, frame) in frames.iter().enumerate() {
                if let Err(e) = screen::write_ppm(&dir.join(format!("screen-{n}.ppm")), frame) {
                    log::warn!("screen preview: {e}");
                }
            }
        }
        controls.send_screens(frames);
    }
}

fn count_presses(inputs: &mut crate::insights::Tally, source: &str, mapped: &[(Action, Trigger)]) {
    let presses = mapped.iter().filter(|(_, t)| *t == Trigger::Press).count();
    let kind = if source == crate::controls::KEYBOARD {
        "keyboard"
    } else {
        "controller"
    };
    inputs.add_n(kind, presses as u64);
}

fn count_action(u: &mut crate::insights::Usage, action: Action) {
    match action {
        Action::Hotcue { .. } => u.hotcues += 1,
        Action::BeatJump { .. } => u.beat_jumps += 1,
        Action::SampleSlot { .. } | Action::SampleCell { .. } => u.samples += 1,
        Action::AutoFade { .. } => u.auto_fades += 1,
        _ => {}
    }
}

fn reset_position(action: Action) -> Option<f32> {
    if matches!(action, Action::TempoRange { .. }) {
        return Some(3.0 / 13.0);
    }
    if matches!(
        action,
        Action::Tempo { .. }
            | Action::KeyAdjust { .. }
            | Action::Balance { .. }
            | Action::MasterBalance
    ) {
        return Some(0.5);
    }
    let defaults = backspn_control::feedback::Feedback {
        knobs: crate::mixer_view::MixerView::default_knobs(),
        ..Default::default()
    };
    backspn_control::output::position(action, &defaults)
}

fn master_beat(state: &EngineState) -> f64 {
    state
        .sync
        .master
        .and_then(|m| {
            let deck = state.decks.get(m)?;
            Some(deck.grid?.beat_at(deck.position_s))
        })
        .unwrap_or(state.sync.clock_beat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_and_favorites_take_the_encoder_while_held() {
        let t = Instant::now();
        let list = BrowseMode::List;
        let preview = list.after(false, true, t);
        assert_eq!(preview, BrowseMode::Preview(t));
        assert_eq!(preview.after(true, true, t), preview, "FAVORITE waits");
        assert_eq!(preview.after(true, false, t), preview);
        assert_eq!(preview.after(false, false, t), list);
        let favorites = list.after(true, true, t);
        assert_eq!(favorites, BrowseMode::Favorites);
        assert_eq!(favorites.after(false, true, t), favorites, "PREVIEW waits");
        assert_eq!(favorites.after(true, false, t), list);
    }

    #[test]
    fn preview_light_blinks_while_previewing() {
        let t = Instant::now();
        let mode = BrowseMode::Preview(t);
        assert!(mode.preview_lit(t));
        assert!(!mode.preview_lit(t + Duration::from_millis(160)));
        assert!(mode.preview_lit(t + Duration::from_millis(310)));
        assert!(!BrowseMode::List.preview_lit(t));
    }
}
