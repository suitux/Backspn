use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use backspn_control::{Action, Trigger};
use backspn_engine::Renderer;
use eframe::egui;

use crate::App;

struct Harness {
    app: App,
    renderer: Renderer,
    ctx: egui::Context,
    dir: PathBuf,
}

impl Harness {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("backspn-app-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (mut app, renderer) = App::offline();
        app.settings.recordings_dir = dir.join("Recordings");
        app.settings.stems_dir = dir.join("Stems");
        app.settings.samples_dir = dir.join("Samples");
        app.settings.foreign_stems_dir = None;
        Self {
            app,
            renderer,
            ctx: egui::Context::default(),
            dir,
        }
    }

    fn frame(&mut self) {
        let mut out = vec![0.0; 2 * 512];
        self.renderer.render(&mut out, 2, None);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1500.0, 860.0),
            )),
            ..egui::RawInput::default()
        };
        let app = &mut self.app;
        let mut output = self.ctx.run_ui(input, |ui| app.frame(ui));
        output.textures_delta.clear();
    }

    fn until(&mut self, what: &str, done: impl Fn(&App) -> bool) {
        let start = Instant::now();
        while !done(&self.app) {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "timed out: {what}"
            );
            self.frame();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn until_engine(&mut self, what: &str, done: impl Fn(&backspn_engine::EngineState) -> bool) {
        let start = Instant::now();
        while !done(&self.app.engine_state()) {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "timed out: {what}"
            );
            self.frame();
        }
    }

    fn press(&mut self, action: Action) {
        let state = self.app.engine_state();
        self.app.execute(action, Trigger::Press, &state);
        self.app.flush();
    }

    fn tracks(&mut self, names: &[&str]) -> Vec<i64> {
        let files: Vec<PathBuf> = names.iter().map(|n| silent_wav(&self.dir, n)).collect();
        self.app.browser.add_files(&files)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn silent_wav(dir: &Path, name: &str) -> PathBuf {
    wav_from(dir, name, None)
}

fn wav_from(dir: &Path, name: &str, sound: Option<u32>) -> PathBuf {
    let path = dir.join(format!("{name}.wav"));
    let (rate, data) = (44_100u32, 44_100u32 * 4);
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    let start = wav.len();
    wav.resize(start + data as usize, 0);
    if let Some(from) = sound {
        for frame in wav[start + 4 * from as usize..].chunks_mut(2) {
            frame.copy_from_slice(&8000i16.to_le_bytes());
        }
    }
    std::fs::write(&path, wav).unwrap();
    path
}

#[test]
fn load_next_loads_the_row_after_the_selection_into_the_deck() {
    let mut h = Harness::new("load-next");
    h.tracks(&["A", "B", "C"]);
    h.frame();
    let first = h.app.browser.step_selection(1).unwrap();
    h.press(Action::LoadNext { deck: 0 });
    let next = h.app.browser.selected().unwrap();
    assert_ne!(next, first, "the selection moved on");
    h.until("deck A loads it", |app| {
        app.decks[0].track_id() == Some(next)
    });
    h.press(Action::LoadPrevious { deck: 1 });
    assert_eq!(h.app.browser.selected(), Some(first));
    h.until("deck B loads the first", |app| {
        app.decks[1].track_id() == Some(first)
    });
}

#[test]
fn a_hotcue_set_on_a_deck_shows_in_the_preview_column() {
    let mut h = Harness::new("hotcue");
    let id = h.tracks(&["A"])[0];
    h.app.load_from_library(0, id);
    h.until("deck A loads", |app| app.decks[0].track_id() == Some(id));
    h.until("the length is known", |app| {
        app.browser
            .library_track(id)
            .is_some_and(|t| t.duration_s.is_some())
    });
    h.until_engine("the engine has the track", |state| state.decks[0].loaded);
    h.frame();
    assert!(h.app.browser.cue_marks(id).is_empty());
    h.press(Action::Hotcue { deck: 0, slot: 0 });
    h.until("the hotcue is stored", |app| {
        app.browser
            .library_cues(id)
            .iter()
            .any(|c| c.hotcue == Some(0))
    });
    let marks = h.app.browser.cue_marks(id);
    assert_eq!(marks.len(), 1, "the browser read the new cue");
    assert_eq!(marks[0].hotcue, Some(1));
}

#[test]
fn a_recording_joins_audio_recordings() {
    let mut h = Harness::new("record");
    h.press(Action::RecordStop);
    assert!(h.app.recorder.recording());
    for _ in 0..20 {
        h.frame();
    }
    h.press(Action::RecordStop);
    h.until("the file reaches the browser", |app| {
        !app.browser.recorded_tracks().is_empty()
    });
    let tracks = h.app.browser.recorded_tracks();
    let row = h.app.browser.library_track(tracks[0]).unwrap();
    assert!(row.title.starts_with("Recording_"), "{}", row.title);
    assert!(Path::new(&row.path).starts_with(h.dir.join("Recordings")));
}

#[test]
fn a_saved_loop_joins_loops_with_its_tempo() {
    let mut h = Harness::new("save-loop");
    h.app.loop_capture = Some(crate::app::LoopCopyFor::Save);
    let copy = backspn_engine::loop_recorder::LoopCopy {
        frames: vec![[0.25, -0.25]; 4_000],
        bpm: 125.0,
        rate: 8_000,
    };
    h.app.finish_loop_capture(&copy);
    assert_eq!(h.app.loop_capture, None);
    let tracks = h.app.browser.loop_tracks();
    assert_eq!(tracks.len(), 1);
    let row = h.app.browser.library_track(tracks[0]).unwrap();
    assert!(row.title.starts_with("Loop Recorder "), "{}", row.title);
    assert!(Path::new(&row.path).starts_with(h.dir.join("Samples")));
    assert_eq!((row.bpm, row.grid_anchor_s), (Some(125.0), Some(0.0)));
    let back = backspn_decode::decode_file(Path::new(&row.path)).unwrap();
    assert_eq!(back.frames(), 4_000, "exactly the loop");
}

#[test]
fn a_deck_loop_is_saved_as_a_loop() {
    use crate::sample_view::LoopDrag;
    use backspn_engine::{Command, DeckCommand};

    let mut h = Harness::new("save-deck-loop");
    let id = h.tracks(&["Bass"])[0];
    h.app.load_from_library(0, id);
    h.until_engine("deck A has the track", |s| s.decks[0].loaded);
    h.app.save_loop_from(LoopDrag::Deck(0));
    assert_eq!(h.app.browser.loop_tracks(), Vec::<i64>::new());
    h.app
        .outbox
        .push(Command::Deck(0, DeckCommand::AutoLoop { beats: 1.0 }));
    h.app.flush();
    h.until_engine("the loop is on", |s| s.decks[0].loop_active);
    h.app.save_loop_from(LoopDrag::Deck(0));
    let tracks = h.app.browser.loop_tracks();
    assert_eq!(tracks.len(), 1);
    let row = h.app.browser.library_track(tracks[0]).unwrap();
    assert!(row.title.starts_with("Bass 20"), "{}", row.title);
    assert!(Path::new(&row.path).starts_with(h.dir.join("Samples")));
    assert!(
        (row.duration_s.unwrap() - 0.5).abs() < 1e-2,
        "1 beat at 120"
    );
}

fn factory_stem_copy(dir: &Path) -> Option<PathBuf> {
    let source = std::fs::read_dir(std::env::var_os("BACKSPN_STEM_FILES")?)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| crate::stems::is_stem_file(p))?;
    let copy = dir.join("Factory.stem.mp4");
    std::fs::copy(source, &copy).ok()?;
    Some(copy)
}

#[test]
fn a_stem_file_loads_as_stems_or_as_a_track() {
    let mut h = Harness::new("stems");
    let Some(file) = factory_stem_copy(&h.dir) else {
        return;
    };
    let id = h.app.browser.add_files(&[file])[0];
    h.app.load_from_library(0, id);
    h.until_engine("deck A plays stems", |s| s.decks[0].stems);
    let stems = h.app.decks[0].stem_feedback().expect("a stem deck");
    assert!(stems.iter().all(|s| !s.muted && s.colour != [0; 3]));
    h.press(Action::Stem {
        deck: 0,
        stem: 2,
        part: backspn_control::StemPart::Mute,
    });
    assert!(h.app.decks[0].stem_feedback().unwrap()[2].muted);
    h.app
        .load_from_library_as(1, id, crate::stems::LoadAs::Track);
    h.until_engine("deck B plays the track", |s| s.decks[1].loaded);
    assert!(!h.app.engine_state().decks[1].stems);
    assert_eq!(h.app.decks[1].stem_feedback(), None);
}

#[test]
fn a_track_with_generated_stems_loads_them() {
    let mut h = Harness::new("generated");
    let Some(stems) = factory_stem_copy(&h.dir) else {
        return;
    };
    let wav = silent_wav(&h.dir, "plain");
    let id = h.app.browser.add_files(std::slice::from_ref(&wav))[0];
    let hash = backspn_library::storage::track_hash(None, &wav).unwrap();
    h.app
        .browser
        .library()
        .unwrap()
        .set_stem_hash(id, Some(hash))
        .unwrap();
    let target = backspn_library::storage::stem_file(&h.dir.join("Stems"), hash);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::rename(&stems, &target).unwrap();
    h.frame();
    assert_eq!(h.app.browser.generated_stems(id), Some(target));
    h.app.load_from_library(0, id);
    h.until_engine("deck A plays the generated stems", |s| s.decks[0].stems);
    h.app
        .load_from_library_as(1, id, crate::stems::LoadAs::Track);
    h.until_engine("deck B plays the track", |s| s.decks[1].loaded);
    assert!(!h.app.engine_state().decks[1].stems);
}

#[test]
fn jogging_a_synced_deck_leaves_its_phase_lock() {
    use backspn_engine::{Command, SyncCommand};
    let mut h = Harness::new("jogsync");
    let mut state = h.app.engine_state();
    state.decks[1].playing = true;
    state.sync.synced[1] = true;
    h.app.outbox.clear();
    h.app
        .execute(Action::JogTurn { deck: 1 }, Trigger::Delta(0.2), &state);
    let released = |app: &App| {
        app.outbox
            .iter()
            .any(|c| matches!(c, Command::Sync(SyncCommand::ReleasePhase(1))))
    };
    assert!(released(&h.app), "the nudge is not pulled back");
    state.sync.synced[1] = false;
    h.app.outbox.clear();
    h.app
        .execute(Action::JogTurn { deck: 1 }, Trigger::Delta(0.2), &state);
    assert!(!released(&h.app));
}

#[test]
fn a_new_session_starts_as_the_last_one_ended() {
    use backspn_engine::{Command, DeckCommand, SyncCommand};

    let mut h = Harness::new("session");
    let state = h.app.engine_state();
    h.app
        .execute(Action::Volume { channel: 1 }, Trigger::Set(0.3), &state);
    h.app.execute(Action::Crossfader, Trigger::Set(0.8), &state);
    h.app.outbox.extend([
        Command::Deck(0, DeckCommand::SetTempo(1.05)),
        Command::Deck(1, DeckCommand::SetSlip(true)),
        Command::Sync(SyncCommand::SetSync(1, true)),
        Command::Sync(SyncCommand::SetClockBpm(128.0)),
    ]);
    h.frame();
    h.frame();
    let state = h.app.engine_state();
    let status = h.app.current_status(&state);
    assert!((status.decks[0].tempo - 1.05).abs() < 1e-9);
    assert!(status.decks[1].slip && status.decks[1].sync);
    assert!((status.clock_bpm - 128.0).abs() < 1e-9);

    let mut next = Harness::new("session-next");
    next.app.settings.status = status.clone();
    next.app.restore_status();
    next.frame();
    next.frame();
    let state = next.app.engine_state();
    assert_eq!(next.app.current_status(&state), status);
    assert!((state.mixer.channels[1].fader - 0.3).abs() < 1e-6);
    for d in &state.decks {
        assert!(!d.loaded, "no track comes back");
    }
}

#[test]
fn the_auto_master_setting_reaches_the_engine() {
    let mut h = Harness::new("auto-master");
    h.frame();
    assert!(h.app.engine_state().sync.auto, "on by default");
    h.app.settings.auto_master = false;
    h.app.send_deck_settings();
    h.frame();
    h.frame();
    assert!(!h.app.engine_state().sync.auto);
}

#[test]
fn reset_puts_mixer_controls_back_to_their_default() {
    let mut h = Harness::new("reset");
    let state = h.app.engine_state();
    let start = h.app.mixer.knobs();
    h.app
        .execute(Action::Volume { channel: 1 }, Trigger::Set(0.3), &state);
    h.app.execute(Action::Crossfader, Trigger::Set(0.9), &state);
    h.app
        .execute(Action::Volume { channel: 1 }, Trigger::Reset, &state);
    h.app.execute(Action::Crossfader, Trigger::Reset, &state);
    let k = h.app.mixer.knobs();
    assert!((k.fader[1] - start.fader[1]).abs() < 1e-6);
    assert!((k.crossfader - start.crossfader).abs() < 1e-6);
    h.app
        .execute(Action::PlayPause { deck: 0 }, Trigger::Reset, &state);
    h.app
        .execute(Action::Tempo { deck: 0 }, Trigger::Set(0.9), &state);
    h.app
        .execute(Action::Tempo { deck: 0 }, Trigger::Reset, &state);
    assert_eq!(h.app.decks[0].tempo_fader_position(), 0.0);
}

#[test]
fn focus_actions_move_the_focus_and_the_deck_marks_follow() {
    let mut h = Harness::new("focus");
    let state = h.app.engine_state();
    assert_eq!(h.app.focus.deck, 0, "deck A at start-up");
    h.app
        .execute(Action::DeckFocus { deck: 2 }, Trigger::Press, &state);
    h.app.execute(Action::FocusToggle, Trigger::Press, &state);
    assert_eq!(h.app.focus.deck, 0);
    h.app.execute(Action::FocusToggle, Trigger::Press, &state);
    assert_eq!(h.app.focus.deck, 2);
    h.frame();
    assert!(h.app.decks[2].focused && !h.app.decks[0].focused);
}

#[test]
fn changing_the_audio_device_keeps_the_decks() {
    let mut h = Harness::new("audio-change");
    h.app.decks[0].pending_open = Some(PathBuf::from("kept.wav"));
    h.app.change_audio();
    assert_eq!(
        h.app.decks[0].pending_open,
        Some(PathBuf::from("kept.wav")),
        "same rate: the stream moves, the decks stay"
    );
}

#[test]
fn pad_pages_fader_start_and_tempo_range() {
    use backspn_engine::{Command, DeckCommand};
    let mut h = Harness::new("rbpads");
    let mut state = h.app.engine_state();
    state.decks[0].duration_s = 100.0;
    let key = |app: &App, want: f32| {
        app.outbox.iter().any(
            |c| matches!(c, Command::Deck(0, DeckCommand::SetKeyAdjust(k)) if (k - want).abs() < 1e-6),
        )
    };
    let toggled = |app: &App| {
        app.outbox
            .iter()
            .any(|c| matches!(c, Command::Deck(0, DeckCommand::TogglePlay)))
    };
    h.app.outbox.clear();
    h.app.execute(
        Action::KeyShiftPad { deck: 0, pad: 0 },
        Trigger::Press,
        &state,
    );
    assert!(key(&h.app, 4.0));
    h.app.execute(
        Action::KeyShiftPage {
            deck: 0,
            direction: -1.0,
        },
        Trigger::Press,
        &state,
    );
    h.app.outbox.clear();
    h.app.execute(
        Action::KeyShiftPad { deck: 0, pad: 0 },
        Trigger::Press,
        &state,
    );
    assert!(key(&h.app, 12.0));
    h.app.outbox.clear();
    let start = Action::FaderStart {
        deck: 0,
        play: true,
    };
    h.app.execute(start, Trigger::Press, &state);
    assert!(toggled(&h.app));
    state.decks[0].playing = true;
    h.app.outbox.clear();
    h.app.execute(start, Trigger::Press, &state);
    assert!(!toggled(&h.app));
    state.decks[0].tempo = 1.0;
    h.app
        .execute(Action::TempoRangeCycle { deck: 0 }, Trigger::Press, &state);
    assert!((h.app.decks[0].tempo_range() - 0.10).abs() < 1e-6);
}

#[test]
fn keyboard_pads_pick_a_hot_cue_then_play_it_at_a_key() {
    use backspn_engine::{Command, CueKind, DeckCommand, HotcuePoint};
    let mut h = Harness::new("rbkeys");
    let mut state = h.app.engine_state();
    state.decks[0].duration_s = 100.0;
    state.decks[0].hotcues[2] = Some(HotcuePoint {
        start_s: 10.0,
        kind: CueKind::Cue,
        loop_s: None,
    });
    let pad = |pad| Action::KeyboardPad { deck: 0, pad };
    assert_eq!(
        h.app.pad_layer_action(pad(0), Trigger::Press, &state),
        Some((Action::Hotcue { deck: 0, slot: 0 }, Trigger::Press))
    );
    assert_eq!(h.app.pad_layer_action(pad(2), Trigger::Press, &state), None);
    h.app.outbox.clear();
    assert_eq!(
        h.app.pad_layer_action(pad(0), Trigger::Press, &state),
        Some((Action::Hotcue { deck: 0, slot: 2 }, Trigger::Press))
    );
    assert!(h.app.outbox.iter().any(
        |c| matches!(c, Command::Deck(0, DeckCommand::SetKeyAdjust(k)) if (k - 4.0).abs() < 1e-6)
    ));
    assert_eq!(
        h.app
            .pad_layer_action(Action::KeyboardDelete { deck: 0 }, Trigger::Press, &state),
        Some((Action::DeleteHotcue { deck: 0, slot: 2 }, Trigger::Press))
    );
    assert_eq!(h.app.pad_pages[0].key_cue, None);
}

#[test]
fn a_sample_deck_loads_a_cell_plays_it_and_saves_its_set() {
    use crate::sample_view::DeckKind;
    use backspn_engine::{CellState, PlayerState};

    let mut h = Harness::new("sample_deck");
    let id = h.tracks(&["Kick"])[0];
    h.app.set_deck_kind(0, DeckKind::SampleDeck);
    h.app.flush();
    h.until_engine("deck A is a sample deck", |s| s.sample_deck[0].is_some());
    assert_eq!(h.app.deck_kind(0), DeckKind::SampleDeck);
    h.app.load_cell(0, 1, 2, id);
    h.until_engine("the cell holds the sample", |s| {
        s.sample_deck[0].is_some_and(|r| r.slots[1].cells[2] == CellState::Loaded)
    });
    let view = h.app.decks[0].sample_deck.as_ref().unwrap();
    assert_eq!(view.cell(1, 2).unwrap().name, "Kick");
    assert_eq!(
        view.cell(1, 2).unwrap().play_type,
        1,
        "no sample type: one-shot"
    );
    assert!(view.modified);
    let out = &mut h.app.outbox;
    h.app.decks[0].sample_deck.as_mut().unwrap().set.quant_on = false;
    h.app.decks[0]
        .sample_deck
        .as_mut()
        .unwrap()
        .trigger(1, 2, true, out);
    let _ = out;
    h.app.flush();
    h.until_engine("the slot plays", |s| {
        s.sample_deck[0].is_some_and(|r| r.slots[1].player == PlayerState::Playing)
    });
    h.app.save_sample_set(0, false);
    let sets = h.app.browser.library_sets();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].set.title, "New Sample Set");
    assert_eq!(sets[0].track_of(1, 2), Some(id));
    let view = h.app.decks[0].sample_deck.as_ref().unwrap();
    assert_eq!(view.set_id, Some(sets[0].id));
    assert!(!view.modified);
    h.app.set_deck_kind(0, DeckKind::Track);
    h.app.flush();
    h.until_engine("deck A is a track deck again", |s| {
        s.sample_deck[0].is_none()
    });
    assert!(h.app.decks[0].sample_deck.is_none());
}

#[test]
fn a_saved_set_loads_back_with_its_cells() {
    use crate::sample_view::DeckKind;
    use backspn_engine::CellState;

    let mut h = Harness::new("sample_deck-set");
    let ids = h.tracks(&["A", "B"]);
    h.app.set_deck_kind(1, DeckKind::SampleDeck);
    h.app.load_cell(1, 0, 0, ids[0]);
    h.app.load_cell(1, 3, 5, ids[1]);
    h.until_engine("both cells load", |s| {
        s.sample_deck[1].is_some_and(|r| {
            r.slots[0].cells[0] == CellState::Loaded && r.slots[3].cells[5] == CellState::Loaded
        })
    });
    h.app.decks[1].sample_deck.as_mut().unwrap().set.title = "Drums".to_owned();
    h.app.save_sample_set(1, false);
    let set = h.app.browser.library_sets()[0].id;
    h.app.set_deck_kind(2, DeckKind::SampleDeck);
    h.app.load_sample_set(2, set);
    h.until_engine("deck C has the set's cells", |s| {
        s.sample_deck[2].is_some_and(|r| {
            r.slots[0].cells[0] == CellState::Loaded && r.slots[3].cells[5] == CellState::Loaded
        })
    });
    let view = h.app.decks[2].sample_deck.as_ref().unwrap();
    assert_eq!(view.set.title, "Drums");
    assert_eq!(view.set_id, Some(set));
    assert_eq!(
        view.tracks[3 * backspn_engine::SAMPLE_CELLS + 5],
        Some(ids[1])
    );
    h.app.copy_from_slot(1, 0, 2 * 4 + 3);
    let c = h.app.decks[2].sample_deck.as_mut().unwrap();
    c.set.slots[3].active_cell = 5;
    c.set.slots[3]
        .cells
        .iter_mut()
        .find(|c| c.index == 5)
        .unwrap()
        .reverse = true;
    h.app.copy_from_slot(1, 0, 2 * 4 + 3);
    let b = h.app.decks[1].sample_deck.as_ref().unwrap();
    let copied = b.cell(0, 0).unwrap();
    assert!(copied.reverse, "the source's attributes");
    assert_eq!(b.tracks[0], Some(ids[1]));
    h.app.copy_from_slot(1, 0, 4);
    assert!(
        crate::toast::showing(crate::toast::Kind::Warning, |t| t
            .contains("copied onto itself")),
        "the same slot warns"
    );
    h.app.browser.open_set(set);
    h.press(Action::SampleDeck {
        deck: 3,
        part: backspn_control::SamplePart::LoadSet,
    });
    h.until_engine("deck D has the set's cells", |s| {
        s.sample_deck[3].is_some_and(|r| r.slots[3].cells[5] == CellState::Loaded)
    });
}

#[test]
fn a_sample_deck_captures_a_loop_from_a_track_deck() {
    use crate::sample_view::DeckKind;
    use backspn_engine::CellState;

    let mut h = Harness::new("sample_deck-capture");
    let id = h.tracks(&["Bass"])[0];
    h.app.load_from_library(0, id);
    h.until_engine("deck A has the track", |s| s.decks[0].loaded);
    h.app.set_deck_kind(1, DeckKind::SampleDeck);
    h.app.flush();
    h.until_engine("deck B is a sample deck", |s| s.sample_deck[1].is_some());
    h.app.decks[1].sample_deck.as_mut().unwrap().capture_source = 0;
    h.app.capture(1, 2, 0);
    h.app.flush();
    h.until_engine("the cell holds the loop", |s| {
        s.sample_deck[1].is_some_and(|r| r.slots[2].cells[0] == CellState::Loaded)
    });
    let view = h.app.decks[1].sample_deck.as_ref().unwrap();
    let cell = view.cell(2, 0).unwrap();
    assert!(cell.name.starts_with("Bass 20"), "{}", cell.name);
    assert_eq!(cell.play_type, 0, "a loop");
    assert!((cell.bpm - 120.0).abs() < 1e-9);
    let beats = h.app.decks[0].loop_beats();
    assert!((view.durations[2 * backspn_engine::SAMPLE_CELLS] - beats * 0.5).abs() < 1e-3);
    let sample = view.tracks[2 * backspn_engine::SAMPLE_CELLS].unwrap();
    let row = h.app.browser.library_track(sample).unwrap();
    assert!(
        row.path
            .starts_with(h.dir.join("Samples").to_str().unwrap())
    );
    assert!(
        std::path::Path::new(&row.path)
            .extension()
            .is_some_and(|e| e == "wav")
    );
    assert_eq!(row.sample_type, Some(0));
    assert!((row.bpm.unwrap() - 120.0).abs() < 1e-9);
    let lists = h
        .app
        .browser
        .with_lib_value(|lib| {
            let names: Vec<String> = lib
                .playlists_with(sample)?
                .into_iter()
                .filter_map(|id| lib.nodes().ok()?.into_iter().find(|n| n.id == id))
                .map(|n| n.name)
                .collect();
            Ok(names)
        })
        .unwrap();
    assert_eq!(lists, vec![backspn_library::playlists::LOOPS.to_owned()]);
    h.app.decks[1].sample_deck.as_mut().unwrap().capture_source = 1;
    h.app.capture(1, 2, 1);
    assert!(
        h.app.decks[1]
            .sample_deck
            .as_ref()
            .unwrap()
            .cell(2, 1)
            .is_none()
    );
    h.app.decks[1].sample_deck.as_mut().unwrap().capture_source = 3;
    h.app.capture(1, 2, 1);
    assert!(
        h.app.decks[1]
            .sample_deck
            .as_ref()
            .unwrap()
            .cell(2, 1)
            .is_none()
    );
}

#[test]
fn mapped_sample_actions_reach_the_deck() {
    use crate::sample_view::DeckKind;
    use backspn_control::{SamplePart, SampleSlotPart};
    use backspn_engine::CellState;

    let mut h = Harness::new("sample_deck-actions");
    let id = h.tracks(&["Hat"])[0];
    let state = h.app.engine_state();
    h.app
        .execute(Action::DeckType { deck: 1 }, Trigger::Delta(1.0), &state);
    h.app.flush();
    h.until_engine("deck B becomes a sample deck", |s| {
        s.sample_deck[1].is_some()
    });
    assert_eq!(h.app.deck_kind(1), DeckKind::SampleDeck);
    h.app.browser.step_selection(1);
    assert_eq!(h.app.browser.selected(), Some(id));
    h.press(Action::SampleSlot {
        deck: 1,
        slot: 3,
        part: SampleSlotPart::LoadFromList,
    });
    h.until_engine("the cell loads", |s| {
        s.sample_deck[1].is_some_and(|r| r.slots[3].cells[0] == CellState::Loaded)
    });
    h.press(Action::SampleDeck {
        deck: 1,
        part: SamplePart::QuantizeOn,
    });
    assert!(!h.app.decks[1].sample_deck.as_ref().unwrap().set.quant_on);
    h.press(Action::SampleCell {
        deck: 1,
        slot: 3,
        cell: 0,
    });
    h.until_engine("the cell plays", |s| {
        s.sample_deck[1].is_some_and(|r| r.slots[3].cells[0] == CellState::Playing)
    });
    let sample_state = h.app.engine_state().sample_deck[1].unwrap();
    let feedback = h.app.decks[1]
        .sample_deck
        .as_ref()
        .unwrap()
        .feedback(&sample_state, &h.app.decks[1].stem_controls);
    assert_eq!(feedback.slots[3].state, 2);
    assert_eq!(feedback.slots[3].cells[0], 2);
    let state = h.app.engine_state();
    h.app.execute(
        Action::SampleDeck {
            deck: 1,
            part: SamplePart::CellDeleteModifier,
        },
        Trigger::Press,
        &state,
    );
    h.press(Action::SampleCell {
        deck: 1,
        slot: 3,
        cell: 0,
    });
    h.until_engine("the cell is empty", |s| {
        s.sample_deck[1].is_some_and(|r| r.slots[3].cells[0] == CellState::Empty)
    });
}

#[test]
fn live_input_needs_a_stopped_deck_and_sends_its_channels() {
    use backspn_engine::Command;
    let mut h = Harness::new("live");
    let mut state = h.app.engine_state();
    h.app.settings.input_routing.decks[1] = Some((2, 3));
    state.decks[1].playing = true;
    h.app.set_live(1, true, &state);
    assert!(!h.app.decks[1].live, "playing: refused");
    state.decks[1].playing = false;
    h.app.outbox.clear();
    h.app.set_live(1, true, &state);
    assert!(h.app.decks[1].live);
    assert!(
        h.app
            .outbox
            .iter()
            .any(|c| matches!(c, Command::LiveInput(1, Some((2, 3)))))
    );
    h.app.toggle_live_input(1, &state);
    assert!(!h.app.decks[1].live);
}

#[test]
fn the_mix_recorder_source_reaches_the_engine_with_its_input_channels() {
    use crate::prefs::RecordingSource;
    use backspn_engine::{Command, RecordSource};
    let mut h = Harness::new("record_source");
    h.app.settings.input_routing.decks[2] = Some((4, 5));
    let sent = |h: &mut Harness| {
        h.app.outbox.clear();
        h.app.send_inputs();
        h.app.outbox.iter().find_map(|c| match c {
            Command::RecordSource(s) => Some(*s),
            _ => None,
        })
    };
    assert_eq!(sent(&mut h), Some(RecordSource::Master));
    h.app.settings.recording_source = RecordingSource::Deck(2);
    assert_eq!(sent(&mut h), Some(RecordSource::Input(Some((4, 5)))));
    h.app.settings.recording_source = RecordingSource::Send;
    assert_eq!(sent(&mut h), Some(RecordSource::Input(None)), "unassigned");
}

#[test]
fn the_deck_type_setting_makes_a_deck_sample_live_input_or_track() {
    use crate::sample_view::DeckKind;
    use crate::status::DeckType;
    let mut h = Harness::new("deck_type");
    let state = h.app.engine_state();
    h.app.set_deck_type(1, DeckType::Sample, &state);
    assert_eq!(h.app.deck_kind(1), DeckKind::SampleDeck);
    h.app.set_deck_type(1, DeckType::LiveInput, &state);
    assert!(h.app.decks[1].live, "Live Input on");
    assert_eq!(
        h.app.deck_kind(1),
        DeckKind::Track,
        "a live deck is a track deck"
    );
    assert_eq!(
        DeckType::of(h.app.deck_kind(1), h.app.decks[1].live),
        DeckType::LiveInput
    );
    h.app.set_deck_type(1, DeckType::Track, &state);
    assert!(!h.app.decks[1].live);
    assert_eq!(h.app.deck_kind(1), DeckKind::Track);
    assert!(!h.app.current_status(&state).decks[1].live);
}

#[test]
fn a_saved_layout_comes_back_with_decks_and_fx() {
    use crate::deck_view::DeckSize;
    use crate::layouts::{DeckSetup, ScreenLayout, add};
    use crate::status::DeckType;
    use backspn_engine::{EffectKind, FxMode, FxRouting};
    let mut h = Harness::new("layout");
    h.frame();
    let mut club = ScreenLayout {
        deck_count: 4,
        deck_sizes: [DeckSize::Micro; backspn_engine::DECKS],
        show_fx: false,
        ..ScreenLayout::default()
    };
    club.fx_modes[1] = FxMode::Single;
    club.fx_routing[0] = FxRouting::Send;
    let mut setup = DeckSetup {
        effects: h.app.fx.effects(),
        ..DeckSetup::default()
    };
    setup.deck_types[2] = DeckType::Sample;
    setup.deck_types[3] = DeckType::LiveInput;
    setup.effects[1][0] = EffectKind::Reverb;
    add(
        &mut h.app.settings.layouts,
        "Club",
        club.clone(),
        setup.clone(),
    )
    .unwrap();
    assert_eq!(h.app.active_layout(), None);
    h.app.apply_layout(0);
    h.frame();
    h.frame();
    assert!(h.app.decks.iter().all(|d| d.size() == DeckSize::Micro));
    assert!(h.app.decks[2].sample_deck.is_some());
    assert!(h.app.decks[3].live);
    assert_eq!(h.app.fx.modes()[1], FxMode::Single);
    assert_eq!(h.app.fx.effects()[1][0], EffectKind::Reverb);
    assert_eq!(h.app.settings.fx_routing[0], FxRouting::Send);
    assert_eq!(ScreenLayout::of(&h.app.settings), club);
    assert_eq!(h.app.deck_setup(), setup);
    assert_eq!(h.app.active_layout(), Some(0));
}

#[test]
fn auto_cue_skips_the_silence_unless_turned_off() {
    let mut h = Harness::new("auto-cue");
    let files = [
        wav_from(&h.dir, "first", None),
        wav_from(&h.dir, "late", Some(22_100)),
    ];
    h.app.browser.add_files(&files);
    h.frame();
    h.app.browser.step_selection(1).unwrap();
    h.press(Action::LoadNext { deck: 0 });
    h.until_engine("deck A is cued at the sound", |s| {
        s.decks[0].loaded && (s.decks[0].cue_s - 0.5).abs() < 1e-6
    });
    assert!((h.app.engine_state().decks[0].position_s - 0.5).abs() < 0.01);
    h.app.settings.auto_cue[1] = false;
    h.frame();
    h.app.browser.step_selection(-1).unwrap();
    h.press(Action::LoadNext { deck: 1 });
    h.until("deck B loads", |app| app.decks[1].track_id().is_some());
    h.until_engine("deck B is loaded", |s| s.decks[1].loaded);
    assert!(h.app.engine_state().decks[1].cue_s.abs() < 1e-9);
}

#[test]
fn background_analysis_writes_the_activity_diary() {
    let mut h = Harness::new("activity");
    let ids = h.tracks(&["Quiet"]);
    h.frame();
    h.app.browser.reanalyse_grid(ids[0], true);
    h.until("the analysis is done", |app| {
        app.browser.activity.entries().count() >= 2
    });
    let texts: Vec<String> = h
        .app
        .browser
        .activity
        .entries()
        .map(|e| e.text.clone())
        .collect();
    assert_eq!(texts[0], "Analysis queued: 1 track");
    assert!(texts[1].starts_with("Analysed: "), "{texts:?}");
}
