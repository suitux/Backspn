use super::*;

const RATE: u32 = 1_000;

fn ramp_at(rate: u32, seconds: u32) -> Arc<Track> {
    let frames = (rate * seconds) as usize;
    let samples = (0..frames)
        .flat_map(|i| [i as f32 / frames as f32, 1.0])
        .collect();
    Arc::new(Track::from_stereo(rate, samples))
}

fn ramp(seconds: u32) -> Arc<Track> {
    ramp_at(RATE, seconds)
}

fn loaded() -> Deck {
    let mut deck = Deck::new(RATE);
    deck.handle(Command::Load(ramp(10)));
    deck
}

fn run(deck: &mut Deck, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    deck.process(&mut out);
    out
}

#[track_caller]
fn assert_near(a: f64, b: f64) {
    assert!((a - b).abs() < 0.02, "{a} != {b}");
}

#[test]
fn empty_deck_is_silent_and_ignores_transport() {
    let mut deck = Deck::new(RATE);
    deck.handle(Command::Play);
    assert!(run(&mut deck, 16).iter().all(|&s| s == 0.0));
    assert!(!deck.state().loaded && !deck.state().playing);
}

#[test]
fn play_advances_and_fades_in() {
    let mut deck = loaded();
    deck.handle(Command::Play);
    let out = run(&mut deck, 1_000);
    assert!(out[1] < 0.5, "first frame must be ramped, got {}", out[1]);
    assert!((out[2 * 999 + 1] - 1.0).abs() < 1e-6);
    assert_near(deck.state().position_s, 1.0);
}

#[test]
fn pause_fades_out_then_stops() {
    let mut deck = loaded();
    deck.handle(Command::Play);
    run(&mut deck, 500);
    deck.handle(Command::Pause);
    assert!(!deck.state().playing);
    let out = run(&mut deck, 100);
    assert!(out[1] > 0.0 && out[1] < 1.0, "fade-out expected");
    assert_eq!(out[2 * 99 + 1], 0.0);
    let stopped_at = deck.state().position_s;
    run(&mut deck, 100);
    assert!((deck.state().position_s - stopped_at).abs() < f64::EPSILON);
}

#[test]
fn resamples_to_output_rate() {
    let mut deck = Deck::new(2 * RATE);
    deck.handle(Command::Load(ramp(10)));
    deck.handle(Command::Play);
    run(&mut deck, 2_000);
    assert_near(deck.state().position_s, 1.0);
}

#[test]
fn interpolation_is_exact_on_lines() {
    let track = ramp(1);
    let [l, r] = interpolate(&track, 100.25);
    assert!((l - 100.25 / 1_000.0).abs() < 1e-6);
    assert!((r - 1.0).abs() < 1e-6);
}

#[test]
fn seek_while_stopped_is_immediate() {
    let mut deck = loaded();
    deck.handle(Command::Seek { seconds: 3.0 });
    assert_near(deck.state().position_s, 3.0);
    deck.handle(Command::Seek { seconds: 99.0 });
    assert_near(deck.state().position_s, 10.0);
}

#[test]
fn seek_while_playing_jumps_after_fade_and_keeps_playing() {
    let mut deck = loaded();
    deck.handle(Command::Play);
    run(&mut deck, 100);
    deck.handle(Command::Seek { seconds: 5.0 });
    run(&mut deck, 100);
    assert!(deck.state().playing);
    assert!(deck.state().position_s > 5.0 && deck.state().position_s < 5.2);
}

#[test]
fn cue_while_paused_sets_point_and_previews_while_held() {
    let mut deck = loaded();
    deck.handle(Command::Seek { seconds: 2.0 });
    deck.handle(Command::CueDown);
    assert_near(deck.state().cue_s, 2.0);
    assert!(deck.state().playing);
    run(&mut deck, 500);
    assert_near(deck.state().position_s, 2.5);
    deck.handle(Command::CueUp);
    run(&mut deck, 50);
    let s = deck.state();
    assert!(!s.playing);
    assert_near(s.position_s, 2.0);
}

#[test]
fn cue_while_playing_returns_to_cue_and_pauses() {
    let mut deck = loaded();
    deck.handle(Command::Seek { seconds: 1.0 });
    deck.handle(Command::CueDown);
    deck.handle(Command::CueUp);
    run(&mut deck, 10);
    deck.handle(Command::Play);
    run(&mut deck, 2_000);
    deck.handle(Command::CueDown);
    run(&mut deck, 50);
    let s = deck.state();
    assert!(!s.playing);
    assert_near(s.position_s, 1.0);
    deck.handle(Command::CueUp);
    assert!(!deck.state().playing);
}

#[test]
fn play_during_cue_preview_keeps_playing_on_release() {
    let mut deck = loaded();
    deck.handle(Command::CueDown);
    run(&mut deck, 100);
    deck.handle(Command::Play);
    deck.handle(Command::CueUp);
    run(&mut deck, 100);
    assert!(deck.state().playing);
    assert_near(deck.state().position_s, 0.2);
}

#[test]
fn cup_jumps_to_cue_and_plays_on_release() {
    let mut deck = loaded();
    deck.handle(Command::Seek { seconds: 4.0 });
    deck.handle(Command::CueDown);
    deck.handle(Command::CueUp);
    deck.handle(Command::Seek { seconds: 7.0 });
    deck.handle(Command::Play);
    run(&mut deck, 100);
    deck.handle(Command::CupDown);
    run(&mut deck, 50);
    assert!(!deck.state().playing);
    assert_near(deck.state().position_s, 4.0);
    deck.handle(Command::CupUp);
    run(&mut deck, 100);
    assert!(deck.state().playing);
    assert_near(deck.state().position_s, 4.1);
}

#[test]
fn stops_at_end_and_play_restarts_from_cue() {
    let mut deck = loaded();
    deck.handle(Command::Seek { seconds: 9.9 });
    deck.handle(Command::Play);
    run(&mut deck, 200);
    let s = deck.state();
    assert!(!s.playing);
    assert_near(s.position_s, 10.0);
    deck.handle(Command::Play);
    run(&mut deck, 100);
    assert!(deck.state().playing);
    assert_near(deck.state().position_s, 0.1);
}

#[test]
fn load_returns_previous_track_and_resets() {
    let mut deck = loaded();
    deck.handle(Command::SetGrid(Some(Grid {
        bpm: 120.0,
        anchor_s: 0.0,
        ..Grid::FIXED
    })));
    deck.handle(Command::Hotcue(0));
    let generation = deck.state().generation;
    let old = deck.handle(Command::Load(ramp(1)));
    assert!(old.is_some());
    let s = deck.state();
    assert_eq!(s.generation, generation + 1);
    assert_near(s.duration_s, 1.0);
    assert_eq!((s.grid, s.hotcues[0]), (None, None));
    assert!(deck.handle(Command::Unload).is_some());
    assert!(!deck.state().loaded);
}

#[test]
fn tempo_scales_playback_speed() {
    let mut deck = loaded();
    deck.handle(Command::SetTempo(1.5));
    deck.handle(Command::Play);
    run(&mut deck, 1_000);
    assert_near(deck.state().position_s, 1.5);
    deck.handle(Command::SetTempo(9.0));
    assert!((deck.state().tempo - MAX_TEMPO).abs() < f64::EPSILON);
}

#[test]
fn effective_bpm_follows_tempo() {
    let mut deck = loaded();
    deck.handle(Command::SetGrid(Some(Grid {
        bpm: 128.0,
        anchor_s: 0.0,
        ..Grid::FIXED
    })));
    deck.handle(Command::SetTempo(1.05));
    assert!((deck.state().effective_bpm().unwrap() - 134.4).abs() < 1e-9);
}

#[test]
fn keylock_keeps_position_in_step_with_tempo() {
    let rate = 44_100;
    let mut deck = Deck::new(rate);
    deck.handle(Command::Load(ramp_at(rate, 20)));
    deck.handle(Command::SetKeylock(true));
    deck.handle(Command::SetTempo(1.2));
    deck.handle(Command::Seek { seconds: 2.0 });
    deck.handle(Command::Play);
    let out = run(&mut deck, rate as usize);
    let s = deck.state();
    assert!(s.keylock && s.playing);
    assert!((s.position_s - 3.2).abs() < 0.01, "{}", s.position_s);
    let tail = &out[out.len() - 2_000..];
    let level = tail.iter().skip(1).step_by(2).sum::<f32>() / 1_000.0;
    assert!((level - 1.0).abs() < 0.1, "level {level}");
}

#[test]
fn keylock_switches_off_outside_its_range() {
    let rate = 44_100;
    let mut deck = Deck::new(rate);
    deck.handle(Command::Load(ramp_at(rate, 20)));
    deck.handle(Command::SetKeylock(true));
    deck.handle(Command::Play);
    run(&mut deck, rate as usize / 2);
    deck.handle(Command::SetTempo(0.3));
    run(&mut deck, rate as usize / 2);
    deck.handle(Command::SetTempo(1.1));
    let out = run(&mut deck, rate as usize);
    let s = deck.state();
    assert!(
        (s.position_s - (0.5 + 0.15 + 1.1)).abs() < 0.02,
        "{}",
        s.position_s
    );
    let tail = &out[out.len() - 2_000..];
    let level = tail.iter().skip(1).step_by(2).sum::<f32>() / 1_000.0;
    assert!((level - 1.0).abs() < 0.1, "level {level}");
}

#[test]
fn key_adjust_is_clamped_and_kept_across_loads() {
    let mut deck = loaded();
    deck.handle(Command::SetKeyAdjust(3.0));
    assert!((deck.state().key_adjust - 3.0).abs() < f32::EPSILON);
    deck.handle(Command::SetKeyAdjust(20.0));
    assert!((deck.state().key_adjust - MAX_KEY_ADJUST).abs() < f32::EPSILON);
    deck.handle(Command::SetKeyAdjust(f32::NAN));
    deck.handle(Command::Load(ramp(5)));
    assert!((deck.state().key_adjust - MAX_KEY_ADJUST).abs() < f32::EPSILON);
}

#[test]
fn toggling_keylock_while_playing_fades_and_keeps_position() {
    let rate = 44_100;
    let mut deck = Deck::new(rate);
    deck.handle(Command::Load(ramp_at(rate, 20)));
    deck.handle(Command::Play);
    run(&mut deck, rate as usize);
    deck.handle(Command::SetKeylock(true));
    assert!(deck.state().keylock);
    run(&mut deck, rate as usize);
    assert!((deck.state().position_s - 2.0).abs() < 0.02);
}

fn with_grid(bpm: f64, anchor_s: f64) -> Deck {
    let mut deck = loaded();
    deck.handle(Command::SetGrid(Some(Grid {
        bpm,
        anchor_s,
        ..Grid::FIXED
    })));
    deck
}

#[test]
fn snap_moves_cue_to_nearest_beat() {
    let mut deck = with_grid(120.0, 0.1);
    deck.handle(Command::SetSnap(true));
    deck.handle(Command::Seek { seconds: 1.25 });
    deck.handle(Command::CueDown);
    assert_near(deck.state().cue_s, 1.1);
    deck.handle(Command::CueUp);
    run(&mut deck, 20);
    assert_near(deck.state().position_s, 1.1);
}

#[test]
fn quantized_hotcue_jump_keeps_phase() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::SetSnap(true));
    deck.handle(Command::SetQuantize(true));
    deck.handle(Command::Seek { seconds: 4.0 });
    deck.handle(Command::Hotcue(2));
    assert_eq!(deck.state().hotcues[2].map(|h| h.start_s), Some(4.0));
    deck.handle(Command::Seek { seconds: 1.0 });
    deck.handle(Command::Play);
    run(&mut deck, 120);
    deck.handle(Command::Hotcue(2));
    run(&mut deck, 20);
    let pos = deck.state().position_s;
    let grid = deck.state().grid.unwrap();
    assert!(pos > 4.1 && pos < 4.2, "{pos}");
    assert!(
        (grid.offset_from_beat(pos) - 0.14).abs() < 0.02,
        "phase lost: {pos}"
    );
}

#[test]
fn hotcues_store_jump_and_delete() {
    let mut deck = loaded();
    deck.handle(Command::Seek { seconds: 3.0 });
    deck.handle(Command::Hotcue(0));
    deck.handle(Command::Seek { seconds: 6.0 });
    deck.handle(Command::Hotcue(0));
    assert_near(deck.state().position_s, 3.0);
    assert_near(deck.state().cue_s, 3.0);
    deck.handle(Command::DeleteHotcue(0));
    assert_eq!(deck.state().hotcues[0], None);
    deck.handle(Command::Hotcue(99));
}

#[test]
fn manual_loop_repeats() {
    let mut deck = loaded();
    deck.handle(Command::Seek { seconds: 1.0 });
    deck.handle(Command::LoopIn);
    deck.handle(Command::Seek { seconds: 1.5 });
    deck.handle(Command::LoopOut);
    deck.handle(Command::Play);
    run(&mut deck, 700);
    let s = deck.state();
    assert!(s.loop_active);
    assert!(
        s.position_s >= 1.0 && s.position_s < 1.5,
        "{}",
        s.position_s
    );
    assert_near(s.position_s, 1.2);
}

#[test]
fn auto_loop_uses_beats_and_toggles() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::SetSnap(true));
    deck.handle(Command::Seek { seconds: 2.1 });
    deck.handle(Command::AutoLoop { beats: 2.0 });
    let s = deck.state();
    assert_eq!((s.loop_start_s, s.loop_end_s), (Some(2.0), Some(3.0)));
    deck.handle(Command::LoopScale(0.5));
    assert_eq!(deck.state().loop_end_s, Some(2.5));
    deck.handle(Command::AutoLoop { beats: 1.0 });
    assert!(!deck.state().loop_active, "same size toggles off");
    deck.handle(Command::Seek { seconds: 6.0 });
    deck.handle(Command::LoopToggle);
    assert!(deck.state().loop_active);
    assert_near(deck.state().position_s, 2.0);
}

#[test]
fn snapped_loops_start_on_the_nearest_beat() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::SetSnap(true));
    deck.handle(Command::Seek { seconds: 2.4 });
    deck.handle(Command::AutoLoop { beats: 2.0 });
    assert_eq!(deck.state().loop_start_s, Some(2.5));
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::SetQuantize(true));
    deck.handle(Command::Seek { seconds: 2.4 });
    deck.handle(Command::AutoLoop { beats: 2.0 });
    assert_near(deck.state().loop_start_s.unwrap(), 2.4);
}

#[test]
fn fast_loop_leaves_the_loop_then_sets_a_new_one_where_the_playhead_is() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::SetSnap(true));
    deck.handle(Command::Seek { seconds: 2.0 });
    deck.handle(Command::FastLoop { beats: 1.0 });
    assert_eq!(deck.state().loop_start_s, Some(2.0));
    deck.handle(Command::FastLoop { beats: 1.0 });
    assert!(!deck.state().loop_active, "inside the loop: leaves it");
    deck.handle(Command::Seek { seconds: 6.0 });
    deck.handle(Command::FastLoop { beats: 1.0 });
    let s = deck.state();
    assert!(s.loop_active);
    assert_eq!(s.loop_start_s, Some(6.0), "a new loop, not the old one");
    assert_near(s.position_s, 6.0);
}

#[test]
fn scratching_back_and_reverse_stay_inside_the_loop() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::SetSnap(true));
    deck.handle(Command::Seek { seconds: 2.0 });
    deck.handle(Command::AutoLoop { beats: 2.0 });
    deck.handle(Command::Play);
    run(&mut deck, 100);
    deck.handle(Command::ScratchStart);
    deck.handle(Command::ScratchMove(-3.0));
    run(&mut deck, 3_000);
    let pos = deck.state().position_s;
    assert!((2.0..3.0).contains(&pos), "backspin left the loop: {pos}");
    deck.handle(Command::ScratchMove(4.0));
    run(&mut deck, 3_000);
    let pos = deck.state().position_s;
    assert!(
        (2.0..3.0).contains(&pos),
        "forward spin left the loop: {pos}"
    );
    deck.handle(Command::ScratchEnd);
    deck.handle(Command::Reverse(true));
    run(&mut deck, 2_500);
    let pos = deck.state().position_s;
    assert!((2.0..3.0).contains(&pos), "reverse left the loop: {pos}");
}

#[test]
fn keylocked_loop_repeats() {
    let rate = 44_100;
    let mut deck = Deck::new(rate);
    deck.handle(Command::Load(ramp_at(rate, 20)));
    deck.handle(Command::SetGrid(Some(Grid {
        bpm: 120.0,
        anchor_s: 0.0,
        ..Grid::FIXED
    })));
    deck.handle(Command::SetKeylock(true));
    deck.handle(Command::Seek { seconds: 4.0 });
    deck.handle(Command::AutoLoop { beats: 1.0 });
    deck.handle(Command::Play);
    run(&mut deck, 2 * rate as usize);
    let pos = deck.state().position_s;
    assert!((4.0..4.5).contains(&pos), "{pos}");
}

#[test]
fn beat_jump_moves_the_playhead_but_not_the_loop() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 2.0 });
    deck.handle(Command::BeatJump {
        beats: 4.0,
        stay_in_loop: false,
    });
    assert_near(deck.state().position_s, 4.0);
    deck.handle(Command::AutoLoop { beats: 1.0 });
    deck.handle(Command::BeatJump {
        beats: -2.0,
        stay_in_loop: false,
    });
    let s = deck.state();
    assert_near(s.position_s, 3.0);
    assert_eq!(s.loop_start_s, Some(4.0));
}

#[test]
fn fine_beat_jump_stays_inside_the_active_loop() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 4.25 });
    deck.handle(Command::AutoLoop { beats: 1.0 });
    deck.handle(Command::Seek { seconds: 4.5 });
    deck.handle(Command::BeatJump {
        beats: 0.75,
        stay_in_loop: true,
    });
    assert_near(deck.state().position_s, 4.375);
}

#[test]
fn moving_the_loop_folds_the_playhead_into_it() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 4.0 });
    deck.handle(Command::AutoLoop { beats: 4.0 });
    deck.handle(Command::Seek { seconds: 4.25 });
    deck.handle(Command::MoveLoop {
        edge: LoopEdge::Both,
        beats: Some(1.0),
        steps: 1,
    });
    let s = deck.state();
    assert_eq!((s.loop_start_s, s.loop_end_s), (Some(4.5), Some(6.5)));
    assert_near(s.position_s, 6.25);
    deck.handle(Command::MoveLoop {
        edge: LoopEdge::Out,
        beats: None,
        steps: -1,
    });
    assert_eq!(deck.state().loop_end_s, Some(5.5));
    deck.handle(Command::MoveLoop {
        edge: LoopEdge::In,
        beats: Some(4.0),
        steps: 1,
    });
    assert_eq!(deck.state().loop_start_s, Some(4.5));
}

#[test]
fn shrinking_the_loop_folds_the_playhead_into_it() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 4.0 });
    deck.handle(Command::AutoLoop { beats: 4.0 });
    deck.handle(Command::Seek { seconds: 5.25 });
    deck.handle(Command::LoopScale(0.5));
    let s = deck.state();
    assert_eq!((s.loop_start_s, s.loop_end_s), (Some(4.0), Some(5.0)));
    assert_near(s.position_s, 4.25);
    deck.handle(Command::LoopScale(4.0));
    assert_near(deck.state().position_s, 4.25);
}

#[test]
fn shift_beats_aligns_phase() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 2.0 });
    deck.shift_beats(0.25);
    assert_near(deck.state().position_s, 2.125);
    assert!((deck.beat_position().unwrap() - 4.25).abs() < 1e-6);
}

#[test]
fn restores_stored_cues() {
    let mut deck = loaded();
    let point = |start_s, kind, loop_s| {
        Some(HotcuePoint {
            start_s,
            kind,
            loop_s,
        })
    };
    let mut hotcues = [None; HOTCUES];
    hotcues[1] = point(3.0, CueKind::FadeIn, None);
    hotcues[4] = point(5.0, CueKind::Loop, Some(0.5));
    hotcues[5] = point(6.0, CueKind::Load, Some(0.5));
    deck.handle(Command::RestoreCues {
        cue_s: Some(2.0),
        hotcues,
    });
    let s = deck.state();
    assert_near(s.cue_s, 2.0);
    assert_near(s.position_s, 2.0);
    assert_eq!(s.hotcues[1], hotcues[1]);
    assert_eq!(s.hotcues[4], hotcues[4]);
    assert_eq!(s.hotcues[5], point(6.0, CueKind::Load, None));
    deck.handle(Command::Hotcue(5));
    assert!(!deck.state().loop_active);
    assert_near(deck.state().position_s, 6.0);
    deck.handle(Command::Hotcue(4));
    assert!(deck.state().loop_active);
}

#[test]
fn hotcues_can_be_replaced_and_cues_jumped_to() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 1.0 });
    deck.handle(Command::CueDown);
    deck.handle(Command::CueUp);
    let mut hotcues = [None; HOTCUES];
    hotcues[3] = Some(HotcuePoint {
        start_s: 4.0,
        kind: CueKind::Load,
        loop_s: None,
    });
    deck.handle(Command::SetHotcues(hotcues));
    let s = deck.state();
    assert_eq!(s.hotcues, hotcues);
    assert_near(s.cue_s, 1.0);
    assert_near(s.position_s, 1.0);
    deck.handle(Command::JumpToCue(HotcuePoint {
        start_s: 2.0,
        kind: CueKind::Loop,
        loop_s: Some(1.0),
    }));
    let s = deck.state();
    assert_near(s.position_s, 2.0);
    assert_near(s.cue_s, 2.0);
    assert!(s.loop_active);
    assert_eq!((s.loop_start_s, s.loop_end_s), (Some(2.0), Some(3.0)));
}

#[test]
fn stored_hotcues_are_cues_or_loops() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 1.0 });
    deck.handle(Command::Hotcue(0));
    deck.handle(Command::AutoLoop { beats: 4.0 });
    deck.handle(Command::Hotcue(1));
    let s = deck.state();
    assert_eq!(
        s.hotcues[0].map(|h| (h.kind, h.loop_s)),
        Some((CueKind::Cue, None))
    );
    assert_eq!(
        s.hotcues[1].map(|h| (h.kind, h.loop_s)),
        Some((CueKind::Loop, Some(2.0)))
    );
}

#[test]
fn scratch_follows_the_platter_even_while_paused() {
    let mut deck = loaded();
    deck.handle(Command::ScratchStart);
    assert!(deck.state().scratching);
    deck.handle(Command::ScratchMove(0.5));
    let out = run(&mut deck, 1_000);
    assert_near(deck.state().position_s, 0.5);
    assert!(out.iter().skip(1).step_by(2).any(|&r| r > 0.5), "audible");
    deck.handle(Command::ScratchMove(-0.3));
    run(&mut deck, 1_000);
    assert_near(deck.state().position_s, 0.2);
    deck.handle(Command::ScratchEnd);
    assert!(!deck.state().scratching && !deck.state().playing);
}

#[test]
fn scratch_then_release_keeps_playing() {
    let mut deck = loaded();
    deck.handle(Command::Play);
    run(&mut deck, 1_000);
    deck.handle(Command::ScratchStart);
    run(&mut deck, 500);
    assert_near(deck.state().position_s, 1.0);
    deck.handle(Command::ScratchEnd);
    run(&mut deck, 1_000);
    assert_near(deck.state().position_s, 2.0);
}

#[test]
fn nudge_moves_ahead_of_the_transport() {
    let mut deck = loaded();
    deck.handle(Command::Play);
    deck.handle(Command::Nudge(0.1));
    run(&mut deck, 1_000);
    assert_near(deck.state().position_s, 1.1);
    let mut paused = loaded();
    paused.handle(Command::Nudge(0.1));
    run(&mut paused, 100);
    assert_near(paused.state().position_s, 0.0);
}

#[test]
fn a_spin_runs_the_track_on_fast() {
    let mut deck = loaded();
    deck.handle(Command::Play);
    deck.handle(Command::Nudge(3.0));
    run(&mut deck, 500);
    assert_near(deck.state().position_s, 3.5);
}

#[test]
fn nudge_and_scratch_work_with_keylock() {
    let mut deck = loaded();
    deck.handle(Command::SetKeylock(true));
    deck.handle(Command::Play);
    run(&mut deck, 500);
    deck.handle(Command::Nudge(0.1));
    run(&mut deck, 500);
    assert_near(deck.state().position_s, 1.1);
    deck.handle(Command::ScratchStart);
    deck.handle(Command::ScratchMove(-0.5));
    run(&mut deck, 1_000);
    assert_near(deck.state().position_s, 0.6);
    deck.handle(Command::ScratchEnd);
    let out = run(&mut deck, 1_000);
    assert_near(deck.state().position_s, 1.6);
    assert!(out.iter().all(|s| s.is_finite()));
}

#[test]
fn a_backspin_with_keylock_sounds_as_without() {
    let rate = 44_100;
    let decks = [true, false].map(|keylock| {
        let mut deck = Deck::new(rate);
        deck.handle(Command::Load(ramp_at(rate, 20)));
        deck.handle(Command::SetKeylock(keylock));
        deck.handle(Command::Play);
        deck
    });
    let [mut locked, mut plain] = decks;
    for deck in [&mut locked, &mut plain] {
        run(deck, rate as usize);
        deck.handle(Command::Nudge(-2.0));
    }
    for _ in 0..10 {
        let a = run(&mut locked, 256);
        let b = run(&mut plain, 256);
        for (x, y) in a.iter().zip(&b).step_by(2) {
            assert!((x - y).abs() < 0.01, "{x} != {y}");
        }
    }
}

#[test]
fn scratch_goes_before_the_start_but_not_past_the_end_and_load_clears_it() {
    let mut deck = loaded();
    deck.handle(Command::ScratchStart);
    deck.handle(Command::ScratchMove(-5.0));
    run(&mut deck, 1_000);
    assert_near(deck.state().position_s, -5.0);
    let mut out = vec![0.0f32; 2 * 64];
    deck.process(&mut out);
    assert!(out.iter().all(|v| v.abs() < 1e-6));
    deck.handle(Command::ScratchMove(55.0));
    run(&mut deck, 2_000);
    assert_near(deck.state().position_s, 10.0);
    deck.handle(Command::Load(ramp(10)));
    assert!(!deck.state().scratching);
}

fn playing_at(at_s: f64) -> Deck {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: at_s });
    deck.handle(Command::Play);
    run(&mut deck, 100);
    deck
}

#[test]
fn slip_loop_exit_continues_where_the_track_would_be() {
    for slip in [false, true] {
        let mut deck = playing_at(2.0);
        deck.handle(Command::SetSlip(slip));
        let start = deck.state().position_s;
        deck.handle(Command::AutoLoop { beats: 1.0 });
        assert_eq!(deck.state().slip_position_s.is_some(), slip);
        run(&mut deck, 2_000);
        deck.handle(Command::LoopToggle);
        run(&mut deck, 100);
        let s = deck.state();
        assert_eq!(s.slip_position_s, None);
        if slip {
            assert_near(s.position_s, start + 2.1);
        } else {
            assert!(s.position_s < 3.0, "stays where the loop left it");
        }
    }
}

#[test]
fn reverse_plays_backwards_then_continues_like_censor() {
    let mut deck = playing_at(5.0);
    let start = deck.state().position_s;
    deck.handle(Command::Reverse(true));
    run(&mut deck, 1_000);
    let s = deck.state();
    assert!(s.reverse);
    assert_near(s.position_s, start - 1.0);
    assert_near(s.slip_position_s.unwrap(), start + 1.0);
    deck.handle(Command::Reverse(false));
    run(&mut deck, 100);
    let s = deck.state();
    assert!(!s.reverse && s.playing);
    assert_near(s.position_s, start + 1.1);
}

#[test]
fn reverse_needs_a_playing_deck() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 3.0 });
    deck.handle(Command::Reverse(true));
    assert!(!deck.state().reverse);
    run(&mut deck, 500);
    assert_near(deck.state().position_s, 3.0);
}

#[test]
fn slip_hotcue_returns_when_released() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 1.0 });
    deck.handle(Command::Hotcue(0));
    deck.handle(Command::Seek { seconds: 5.0 });
    deck.handle(Command::Play);
    run(&mut deck, 100);
    deck.handle(Command::SetSlip(true));
    let start = deck.state().position_s;
    deck.handle(Command::Hotcue(0));
    run(&mut deck, 1_000);
    assert!(deck.state().position_s < 2.5, "playing from the hotcue");
    deck.handle(Command::HotcueRelease(0));
    run(&mut deck, 100);
    assert_near(deck.state().position_s, start + 1.1);
}

#[test]
fn slip_beat_jump_returns_when_released() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: 5.0 });
    deck.handle(Command::Play);
    run(&mut deck, 100);
    deck.handle(Command::SetSlip(true));
    let start = deck.state().position_s;
    deck.handle(Command::SlipJump(true));
    deck.handle(Command::BeatJump {
        beats: -4.0,
        stay_in_loop: false,
    });
    run(&mut deck, 1_000);
    assert!(deck.state().position_s < start, "playing from the jump");
    deck.handle(Command::SlipJump(false));
    run(&mut deck, 100);
    assert_near(deck.state().position_s, start + 1.1);
}

fn gridded() -> Deck {
    let mut deck = loaded();
    deck.handle(Command::SetGrid(Some(Grid {
        bpm: 120.0,
        anchor_s: 0.0,
        ..Grid::FIXED
    })));
    deck
}

fn slice(deck: &mut Deck, slice: usize, pressed: bool) {
    deck.handle(Command::FreezeSlice { slice, pressed });
}

#[test]
fn freeze_starts_at_the_nearest_beat_and_slices_by_size() {
    let mut deck = gridded();
    deck.handle(Command::Seek { seconds: 2.2 });
    deck.handle(Command::SetFreeze {
        on: true,
        loop_beats: 4.0,
    });
    let view = deck.state().freeze.unwrap();
    assert_near(view.bounds_s[0], 2.0);
    assert_near(view.bounds_s[1], 2.5);
    assert_near(view.bounds_s[SLICES], 6.0);
    assert_eq!(view.current, Some(0));
}

#[test]
fn a_slice_jumps_there_and_slip_returns_on_release() {
    let mut deck = gridded();
    deck.handle(Command::SetSlip(true));
    deck.handle(Command::Seek { seconds: 2.0 });
    deck.handle(Command::Play);
    deck.handle(Command::SetFreeze {
        on: true,
        loop_beats: 4.0,
    });
    run(&mut deck, 100);
    slice(&mut deck, 4, true);
    run(&mut deck, 100);
    assert_near(deck.state().position_s, 4.1);
    run(&mut deck, 400);
    slice(&mut deck, 4, false);
    run(&mut deck, 10);
    assert_near(deck.state().position_s, 2.61);
}

#[test]
fn slices_need_freeze_mode() {
    let mut deck = gridded();
    slice(&mut deck, 3, true);
    assert_eq!(deck.state().position_s, 0.0);
    assert!(deck.state().freeze.is_none());
}

#[test]
fn with_a_loop_slices_loop_and_release_restores_it() {
    let mut deck = gridded();
    deck.handle(Command::Seek { seconds: 1.0 });
    deck.handle(Command::AutoLoop { beats: 4.0 });
    deck.handle(Command::SetFreeze {
        on: true,
        loop_beats: 4.0,
    });
    slice(&mut deck, 2, true);
    slice(&mut deck, 3, true);
    let s = deck.state();
    assert_near(s.loop_start_s.unwrap(), 1.5);
    assert_near(s.loop_end_s.unwrap(), 2.0);
    slice(&mut deck, 2, false);
    slice(&mut deck, 3, false);
    let s = deck.state();
    assert_near(s.loop_start_s.unwrap(), 1.0);
    assert_near(s.loop_end_s.unwrap(), 3.0);
}

#[test]
fn a_beatjump_out_of_the_region_ends_freeze() {
    let mut deck = gridded();
    deck.handle(Command::SetFreeze {
        on: true,
        loop_beats: 4.0,
    });
    deck.handle(Command::BeatJump {
        beats: 8.0,
        stay_in_loop: false,
    });
    assert!(deck.state().freeze.is_some(), "inside region + 2 beats");
    deck.handle(Command::BeatJump {
        beats: 8.0,
        stay_in_loop: false,
    });
    assert!(deck.state().freeze.is_none());
}

#[test]
fn slip_pause_resumes_where_the_track_would_be() {
    let mut deck = loaded();
    deck.handle(Command::SetSlip(true));
    deck.handle(Command::SetSlipPause(true));
    deck.handle(Command::Play);
    run(&mut deck, 1_000);
    deck.handle(Command::Pause);
    run(&mut deck, 2_000);
    assert!(!deck.state().playing);
    assert_near(deck.state().slip_position_s.unwrap(), 3.0);
    deck.handle(Command::Play);
    run(&mut deck, 10);
    assert_near(deck.state().position_s, 3.01);
}

#[test]
fn without_slip_pause_play_resumes_where_it_stopped() {
    let mut deck = loaded();
    deck.handle(Command::SetSlip(true));
    deck.handle(Command::Play);
    run(&mut deck, 1_000);
    deck.handle(Command::Pause);
    run(&mut deck, 2_000);
    deck.handle(Command::Play);
    run(&mut deck, 10);
    assert_near(deck.state().position_s, 1.01);
}

fn stem_tracks() -> [Track; STEMS] {
    std::array::from_fn(|k| {
        let frames = (RATE * 10) as usize;
        let samples = (0..frames)
            .flat_map(|i| {
                let x = (i as f32 * (k + 1) as f32 / frames as f32).fract();
                [x, 0.25 * (k + 1) as f32]
            })
            .collect();
        Track::from_stereo(RATE, samples)
    })
}

#[test]
fn every_stem_plays_exactly_like_its_own_deck() {
    let tracks = stem_tracks();
    let mut stem_deck = Deck::with_stems(RATE);
    stem_deck.handle(Command::LoadStems(Arc::new(Stems(tracks.clone()))));
    let mut decks: Vec<Deck> = tracks
        .iter()
        .map(|t| {
            let mut d = Deck::new(RATE);
            d.handle(Command::Load(Arc::new(t.clone())));
            d
        })
        .collect();
    let script = [
        Command::Play,
        Command::SetTempo(1.3),
        Command::AutoLoop { beats: 2.0 },
        Command::SetKeylock(true),
        Command::LoopToggle,
        Command::ScratchStart,
        Command::ScratchMove(0.3),
        Command::ScratchEnd,
        Command::Reverse(true),
        Command::Reverse(false),
        Command::Seek { seconds: 4.0 },
    ];
    for command in script {
        stem_deck.handle(command.clone());
        for d in &mut decks {
            d.handle(command.clone());
        }
        let mut layers = vec![vec![0.0; 2 * 300]; STEMS];
        let played = {
            let mut outs: Vec<&mut [f32]> = layers.iter_mut().map(Vec::as_mut_slice).collect();
            stem_deck.process_layers(&mut outs)
        };
        assert_eq!(played, STEMS);
        for (k, d) in decks.iter_mut().enumerate() {
            assert_eq!(run(d, 300), layers[k], "stem {k}");
        }
        assert_eq!(stem_deck.state().position_s, decks[0].state().position_s);
    }
}

#[test]
fn a_stem_deck_mixes_its_stems_and_plain_decks_refuse_them() {
    let tracks = stem_tracks();
    let stems = Arc::new(Stems(tracks));
    let mut plain = Deck::new(RATE);
    assert!(matches!(
        plain.handle(Command::LoadStems(Arc::clone(&stems))),
        Some(Material::Stems(_))
    ));
    assert!(!plain.state().loaded, "no room for stems");

    let mut a = Deck::with_stems(RATE);
    let mut b = Deck::with_stems(RATE);
    for d in [&mut a, &mut b] {
        d.handle(Command::LoadStems(Arc::clone(&stems)));
        d.handle(Command::Play);
    }
    let mixed = run(&mut a, 500);
    let mut layers = vec![vec![0.0; 1_000]; STEMS];
    let mut outs: Vec<&mut [f32]> = layers.iter_mut().map(Vec::as_mut_slice).collect();
    b.process_layers(&mut outs);
    for (i, &m) in mixed.iter().enumerate() {
        let sum: f32 = layers.iter().map(|l| l[i]).sum();
        assert!((m - sum).abs() < 1e-6, "{i}: {m} vs {sum}");
    }
    assert!((a.state().duration_s - 10.0).abs() < 1e-9);
}

#[test]
fn a_plain_track_on_a_stem_deck_fills_the_first_layer_only() {
    let mut deck = Deck::with_stems(RATE);
    deck.handle(Command::Load(ramp(10)));
    deck.handle(Command::Play);
    let mut layers = vec![vec![1.0; 200]; STEMS];
    let mut outs: Vec<&mut [f32]> = layers.iter_mut().map(Vec::as_mut_slice).collect();
    assert_eq!(deck.process_layers(&mut outs), 1);
    assert!(layers[0].iter().any(|&s| s != 0.0));
    assert!(layers[1..].iter().all(|l| l.iter().all(|&s| s == 0.0)));
}

#[test]
fn seeking_and_jumping_back_go_before_the_start() {
    let mut deck = with_grid(120.0, 0.0);
    deck.handle(Command::Seek { seconds: -2.0 });
    assert_near(deck.state().position_s, -2.0);
    deck.handle(Command::Seek { seconds: 0.5 });
    deck.handle(Command::BeatJump {
        beats: -4.0,
        stay_in_loop: false,
    });
    assert_near(deck.state().position_s, -1.5);
    deck.handle(Command::Play);
    run(&mut deck, 2_000);
    assert!(deck.state().position_s > 0.0);
}

#[test]
fn a_keylocked_jump_while_playing_has_no_gap() {
    let rate = 44_100;
    let frames = rate as usize * 20;
    let samples = (0..frames)
        .flat_map(|i| {
            let v = (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5;
            [v, v]
        })
        .collect();
    let mut deck = Deck::new(rate);
    deck.handle(Command::Load(Arc::new(Track::from_stereo(rate, samples))));
    deck.handle(Command::SetTempo(1.05));
    deck.handle(Command::SetKeylock(true));
    deck.handle(Command::Play);
    run(&mut deck, rate as usize);
    deck.handle(Command::Seek {
        seconds: deck.state().position_s + 0.5,
    });
    let left: Vec<f32> = run(&mut deck, rate as usize / 4)
        .chunks(2)
        .map(|c| c[0])
        .collect();
    let quiet = left
        .windows(100)
        .map(|w| w.iter().fold(0.0f32, |a, b| a.max(b.abs())))
        .fold((0, 0), |(run, longest), peak| {
            let run = if peak < 0.25 { run + 1 } else { 0 };
            (run, longest.max(run))
        })
        .1;
    assert!(quiet < rate as usize / 100, "{quiet} quiet frames");
}
