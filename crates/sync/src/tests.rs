use std::sync::Arc;

use backspn_deck::{Command as DeckCommand, Grid, Track};

use super::*;

const RATE: u32 = 1_000;
const BLOCK: usize = 64;

fn deck(bpm: f64, anchor_s: f64) -> Deck {
    let mut d = Deck::new(RATE);
    let samples = vec![0.25f32; 2 * RATE as usize * 600];
    d.handle(DeckCommand::Load(Arc::new(Track::from_stereo(
        RATE, samples,
    ))));
    d.handle(DeckCommand::SetGrid(Some(Grid {
        bpm,
        anchor_s,
        ..Grid::FIXED
    })));
    d
}

fn run(sync: &mut Sync, decks: &mut [Deck], blocks: usize) {
    let mut out = vec![0.0f32; 2 * BLOCK];
    for _ in 0..blocks {
        sync.update(decks, &[true; MAX_DECKS], BLOCK, RATE);
        for d in decks.iter_mut() {
            d.process(&mut out);
        }
    }
}

fn phase_gap(a: &Deck, b: &Deck) -> f64 {
    wrap_phase(a.beat_position().unwrap() - b.beat_position().unwrap()).abs()
}

#[test]
fn follower_matches_tempo_and_phase_on_start() {
    let mut decks = [deck(120.0, 0.0), deck(125.0, 0.13)];
    let mut sync = Sync::default();
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 30);
    sync.handle(Command::SetSync(1, true));
    decks[1].handle(DeckCommand::Seek { seconds: 10.37 });
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 2);
    assert_eq!(sync.state().master, Some(0));
    assert!((decks[1].state().tempo - 120.0 / 125.0).abs() < 0.011);
    assert!(
        phase_gap(&decks[0], &decks[1]) < 0.01,
        "gap {}",
        phase_gap(&decks[0], &decks[1])
    );
    run(&mut sync, &mut decks, 2_000);
    assert!(phase_gap(&decks[0], &decks[1]) < 0.01);
    assert!((decks[1].state().effective_bpm().unwrap() - 120.0).abs() < 0.5);
}

#[test]
fn stopped_follower_gets_tempo_only() {
    let mut decks = [deck(130.0, 0.0), deck(126.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetSync(1, true));
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 3);
    assert!((decks[1].state().tempo - 130.0 / 126.0).abs() < 1e-9);
    assert!(!decks[1].is_playing());
}

#[test]
fn half_time_grid_is_matched_two_to_one() {
    let mut decks = [deck(140.0, 0.0), deck(70.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetSync(1, true));
    decks[0].handle(DeckCommand::Play);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 50);
    assert!((decks[1].state().tempo - 1.0).abs() < 0.011);
}

#[test]
fn corrects_drift_smoothly() {
    let mut decks = [deck(120.0, 0.0), deck(120.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetSync(1, true));
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 1);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 20);
    decks[1].shift_beats(0.1);
    run(&mut sync, &mut decks, 2);
    assert!(
        phase_gap(&decks[0], &decks[1]) > 0.05,
        "no jump for small errors"
    );
    run(&mut sync, &mut decks, 300);
    assert!(phase_gap(&decks[0], &decks[1]) < 0.01);
}

#[test]
fn auto_master_moves_when_master_stops() {
    let mut decks = [deck(120.0, 0.0), deck(124.0, 0.0)];
    let mut sync = Sync::default();
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 5);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 5);
    assert_eq!(sync.state().master, Some(0));
    decks[0].handle(DeckCommand::Pause);
    run(&mut sync, &mut decks, 5);
    assert_eq!(sync.state().master, Some(1));
    assert!((sync.state().clock_bpm - 124.0).abs() < 1e-9);
}

#[test]
fn a_lone_auto_master_that_stops_keeps_leading() {
    let mut decks = [deck(120.0, 0.0), deck(124.0, 0.0)];
    let mut sync = Sync::default();
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 5);
    decks[0].handle(DeckCommand::Pause);
    run(&mut sync, &mut decks, 5);
    assert_eq!(sync.state().master, Some(0));
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 5);
    assert_eq!(sync.state().master, Some(0));
    decks[0].handle(DeckCommand::Pause);
    run(&mut sync, &mut decks, 5);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 5);
    assert_eq!(sync.state().master, Some(1));
}

#[test]
fn explicit_master_and_free_clock() {
    let mut decks = [deck(120.0, 0.0), deck(128.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetAutoMaster(false));
    sync.handle(Command::SetMaster(Some(1)));
    decks[0].handle(DeckCommand::Play);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 3);
    assert_eq!(sync.state().master, Some(1));
    assert!((sync.state().clock_bpm - 128.0).abs() < 1e-9);

    sync.handle(Command::SetMaster(None));
    sync.handle(Command::SetClockBpm(150.0));
    sync.handle(Command::SetSync(0, true));
    let before = sync.state().clock_beat;
    run(&mut sync, &mut decks, 1_000);
    assert_eq!(sync.state().master, None);
    assert!((sync.state().clock_beat - before - 160.0).abs() < 0.01);
    assert!((decks[0].state().effective_bpm().unwrap() - 150.0).abs() < 1.6);
}

#[test]
fn clock_tempo_is_clamped_to_its_range() {
    let mut sync = Sync::default();
    assert!((sync.state().clock_bpm - DEFAULT_CLOCK_BPM).abs() < 1e-9);
    sync.handle(Command::SetClockBpm(20.0));
    assert!((sync.state().clock_bpm - MIN_CLOCK_BPM).abs() < 1e-9);
    sync.handle(Command::SetClockBpm(500.0));
    assert!((sync.state().clock_bpm - MAX_CLOCK_BPM).abs() < 1e-9);
}

#[test]
fn pausing_the_explicit_master_hands_over_to_the_clock() {
    let mut decks = [deck(128.0, 0.0), deck(126.0, 0.05)];
    let mut sync = Sync::default();
    sync.handle(Command::SetAutoMaster(false));
    sync.handle(Command::SetMaster(Some(0)));
    sync.handle(Command::SetSync(1, true));
    decks[0].handle(DeckCommand::Play);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 200);
    decks[0].handle(DeckCommand::Pause);
    assert!(!decks[0].is_playing());
    let before = decks[1].state().position_s;
    let tempo = decks[1].state().tempo;
    run(&mut sync, &mut decks, 300);
    let moved = decks[1].state().position_s - before;
    let expected = 300.0 * BLOCK as f64 / f64::from(RATE) * tempo;
    assert!(
        (moved - expected).abs() < 0.05,
        "moved {moved}, expected {expected}"
    );
    assert!((decks[1].state().effective_bpm().unwrap() - 128.0).abs() < 0.5);
    assert_eq!(sync.state().master, None);
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 10);
    assert_eq!(sync.state().master, None);
}

#[test]
fn phase_to_the_master_is_published() {
    let mut decks = [deck(120.0, 0.0), deck(120.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetMaster(Some(0)));
    decks[1].handle(DeckCommand::Seek { seconds: 10.125 });
    decks[0].handle(DeckCommand::Seek { seconds: 10.0 });
    decks[0].handle(DeckCommand::Play);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 5);
    let s = sync.state();
    assert_eq!(s.phase[0], 0.0, "the master has no offset");
    assert!((s.phase[1] - 0.25).abs() < 0.02, "{}", s.phase[1]);
    decks[1].handle(DeckCommand::Pause);
    run(&mut sync, &mut decks, 2);
    assert_eq!(sync.state().phase[1], 0.0);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 2);
    assert!(sync.state().phase[1] != 0.0);
    decks[0].handle(DeckCommand::Pause);
    run(&mut sync, &mut decks, 2);
    assert_eq!(sync.state().master, Some(1));
    assert_eq!(sync.state().phase[1], 0.0);
}

#[test]
fn bending_a_synced_deck_keeps_tempo_sync_and_drops_phase_lock() {
    let mut decks = [deck(128.0, 0.0), deck(126.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetMaster(Some(0)));
    sync.handle(Command::SetSync(1, true));
    decks[0].handle(DeckCommand::Play);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 100);
    assert!(sync.state().phase_locked[1]);
    assert!(phase_gap(&decks[0], &decks[1]) < 0.01);
    sync.handle(Command::Bend(1, 1.04));
    run(&mut sync, &mut decks, 20);
    assert!((decks[1].state().tempo - 128.0 / 126.0 * 1.04).abs() < 1e-9);
    sync.handle(Command::Bend(1, 1.0));
    run(&mut sync, &mut decks, 300);
    let s = sync.state();
    assert!(s.synced[1] && !s.phase_locked[1], "tempo sync only");
    assert!(s.phase[1] > 0.01, "ahead after the bend: {}", s.phase[1]);
    assert!((decks[1].state().effective_bpm().unwrap() - 128.0).abs() < 0.5);
    sync.handle(Command::SetSync(1, true));
    run(&mut sync, &mut decks, 50);
    assert!(sync.state().phase_locked[1]);
    assert!(phase_gap(&decks[0], &decks[1]) < 0.01);
}

#[test]
fn reverse_on_a_synced_deck_plays_back_without_phase_jumps() {
    let mut decks = [deck(128.0, 0.0), deck(128.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetMaster(Some(0)));
    sync.handle(Command::SetSync(1, true));
    decks[0].handle(DeckCommand::Play);
    decks[1].handle(DeckCommand::Seek { seconds: 20.0 });
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 100);
    decks[1].handle(DeckCommand::Reverse(true));
    let mut last = decks[1].state().position_s;
    for _ in 0..200 {
        run(&mut sync, &mut decks, 1);
        let now = decks[1].state().position_s;
        assert!(now < last, "backwards every block: {last} -> {now}");
        last = now;
    }
    decks[1].handle(DeckCommand::Reverse(false));
    run(&mut sync, &mut decks, 50);
    assert!(phase_gap(&decks[0], &decks[1]) < 0.01);
}

#[test]
fn reversing_the_master_leaves_the_follower_alone() {
    let mut decks = [deck(128.0, 0.0), deck(128.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetMaster(Some(0)));
    sync.handle(Command::SetSync(1, true));
    decks[0].handle(DeckCommand::Seek { seconds: 20.0 });
    decks[0].handle(DeckCommand::Play);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 100);
    decks[0].handle(DeckCommand::Reverse(true));
    let mut last = decks[1].state().position_s;
    for _ in 0..200 {
        run(&mut sync, &mut decks, 1);
        let now = decks[1].state().position_s;
        assert!(now > last, "follower plays on: {last} -> {now}");
        last = now;
    }
    decks[0].handle(DeckCommand::Reverse(false));
    run(&mut sync, &mut decks, 50);
    assert!(phase_gap(&decks[0], &decks[1]) < 0.01);
}

#[test]
fn nudging_the_master_leaves_the_follower_in_place() {
    let mut decks = [deck(120.0, 0.0), deck(120.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetSync(1, true));
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 1);
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 50);
    assert_eq!(sync.state().master, Some(0));
    decks[0].handle(DeckCommand::Nudge(0.3));
    let mut last = decks[1].state().position_s;
    let step = BLOCK as f64 / f64::from(RATE);
    for _ in 0..200 {
        run(&mut sync, &mut decks, 1);
        let now = decks[1].state().position_s;
        assert!(
            (now - last - step).abs() < 0.01,
            "the follower jumped by {}",
            now - last
        );
        last = now;
    }
}

#[test]
fn a_restarted_master_takes_the_clock_with_it() {
    let mut decks = [deck(120.0, 0.0), deck(120.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetMaster(Some(0)));
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 20);
    decks[0].handle(DeckCommand::Pause);
    decks[0].handle(DeckCommand::Seek { seconds: 7.31 });
    run(&mut sync, &mut decks, 2);
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 3);
    let block_beats = BLOCK as f64 / f64::from(RATE) * 2.0;
    let gap = wrap_phase(decks[0].beat_position().unwrap() - block_beats - sync.state().clock_beat);
    assert!(gap.abs() < 0.01, "gap {gap}");
}

#[test]
fn auto_mode_picks_the_deck_closest_in_tempo_then_in_phase() {
    let mut decks = [deck(126.0, 0.0), deck(121.0, 0.0), deck(121.0, 0.2)];
    let mut sync = Sync::default();
    for d in &mut decks {
        d.handle(DeckCommand::Play);
    }
    run(&mut sync, &mut decks, 1);
    assert_eq!(sync.state().master, Some(1));
}

#[test]
fn only_decks_on_air_lead_when_asked() {
    let mut decks = [deck(120.0, 0.0), deck(120.0, 0.0)];
    let mut sync = Sync::default();
    sync.handle(Command::SetOnAirOnly(true));
    decks[0].handle(DeckCommand::Play);
    decks[1].handle(DeckCommand::Play);
    sync.update(&mut decks, &[false, true], BLOCK, RATE);
    assert_eq!(sync.state().master, Some(1));
    sync.update(&mut decks, &[true, false], BLOCK, RATE);
    assert_eq!(sync.state().master, Some(0));
    sync.update(&mut decks, &[false, false], BLOCK, RATE);
    assert_eq!(sync.state().master, None, "no deck heard: the clock leads");
}

#[test]
fn a_deck_out_of_range_does_not_lead() {
    let mut decks = [deck(320.0, 0.0)];
    let mut sync = Sync::default();
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 2);
    assert_eq!(sync.state().master, None);
}

#[test]
fn tempo_sync_and_phase_sync_act_once_without_turning_sync_on() {
    let mut decks = [deck(120.0, 0.0), deck(125.0, 0.0)];
    let mut sync = Sync::default();
    decks[0].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 1);
    decks[1].handle(DeckCommand::Seek { seconds: 10.12 });
    decks[1].handle(DeckCommand::Play);
    run(&mut sync, &mut decks, 5);
    assert_eq!(sync.state().master, Some(0));
    let gap = wrap_phase(decks[1].beat_position().unwrap() - decks[0].beat_position().unwrap());
    decks[1].shift_beats(0.25 - gap);
    run(&mut sync, &mut decks, 1);
    assert!((phase_gap(&decks[0], &decks[1]) - 0.25).abs() < 0.02);

    sync.handle(Command::TempoSync(1));
    run(&mut sync, &mut decks, 1);
    assert!((decks[1].state().effective_bpm().unwrap() - 120.0).abs() < 1e-6);
    assert!(!sync.state().synced[1]);
    assert!(
        phase_gap(&decks[0], &decks[1]) > 0.15,
        "the phase is left alone"
    );

    sync.handle(Command::PhaseSync(1));
    run(&mut sync, &mut decks, 1);
    assert!(phase_gap(&decks[0], &decks[1]) < 0.02);
    assert!(!sync.state().synced[1]);
    decks[1].set_tempo(1.0);
    run(&mut sync, &mut decks, 20);
    assert!((decks[1].state().effective_bpm().unwrap() - 125.0).abs() < 1e-6);
}

#[test]
fn in_ext_the_clock_leads_and_follows_the_received_tempo() {
    let mut sync = Sync::default();
    let mut decks = [deck(128.0, 0.0)];
    decks[0].handle(DeckCommand::Play);
    sync.handle(Command::SetExternal(true));
    sync.handle(Command::SetClockBpm(90.0));
    assert!(
        (sync.state().clock_bpm - DEFAULT_CLOCK_BPM).abs() < 1e-9,
        "tempo held"
    );
    sync.follow_external(Some(124.0), None);
    run(&mut sync, &mut decks, 10);
    assert_eq!(sync.state().master, None);
    assert!((sync.state().clock_bpm - DEFAULT_CLOCK_BPM).abs() < 1e-9);
    let mut ext = sync.state().clock_beat;
    let block_s = BLOCK as f64 / f64::from(RATE);
    for _ in 0..4_000 {
        sync.follow_external(Some(124.0), Some(ext));
        run(&mut sync, &mut decks, 1);
        ext += 124.0 / 60.0 * block_s;
    }
    assert!(sync.state().external_playing);
    let bpm = sync.state().clock_bpm;
    assert!((bpm - 124.0).abs() < 0.1, "{bpm}");
}

#[test]
fn with_link_the_clock_leads_and_moves_forward_to_the_session_phase() {
    let mut s = Sync::default();
    s.handle(Command::SetLink(true));
    assert_eq!(s.state().master, None);
    s.handle(Command::SetLink(false));
    s.follow_link(0.5);
    assert!(s.state().clock_beat.abs() < 1e-9);
    s.handle(Command::SetLink(true));
    s.follow_link(0.25);
    assert!((s.state().clock_beat - 0.25).abs() < 1e-9);
    s.follow_link(0.2);
    assert!((s.state().clock_beat - 0.25).abs() < 1e-9);
}

#[test]
fn a_playing_sample_deck_can_lead_unless_the_preference_says_no() {
    let mut decks = [Deck::new(RATE), deck(124.0, 0.0)];
    let lead = Lead {
        loaded: true,
        playing: true,
        bpm: 128.0,
        beat: 3.5,
    };
    let leads = [Some(lead), None];
    let mut sync = Sync::default();
    sync.update_with(&mut decks, &leads, &[true; MAX_DECKS], BLOCK, RATE);
    assert_eq!(sync.state().master, Some(0), "auto picks the sample deck");
    assert!((sync.state().clock_bpm - 128.0).abs() < 1e-9);
    assert!(
        (sync.state().clock_beat - 3.5).abs() < 1e-9,
        "and its phase"
    );
    sync.handle(Command::SetSampleDecksLead(false));
    decks[1].handle(DeckCommand::Play);
    sync.update_with(&mut decks, &leads, &[true; MAX_DECKS], BLOCK, RATE);
    assert_eq!(sync.state().master, Some(1), "only the track deck may lead");
    let empty = [
        Some(Lead {
            loaded: false,
            ..lead
        }),
        None,
    ];
    sync.handle(Command::SetSampleDecksLead(true));
    sync.update_with(&mut decks, &empty, &[true; MAX_DECKS], BLOCK, RATE);
    assert_eq!(
        sync.state().master,
        Some(1),
        "a sample deck with no audio cannot"
    );
}
