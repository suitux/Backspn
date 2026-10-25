use super::*;

const RATE: u32 = 1_000;
const BLOCK: usize = 100;

fn sample(seconds: f64) -> Arc<Track> {
    let frames = (seconds * f64::from(RATE)) as usize;
    Arc::new(Track::from_stereo(
        RATE,
        (0..frames).flat_map(|_| [0.5, 1.0]).collect(),
    ))
}

fn audio(seconds: f64, params: CellParams) -> CellAudio {
    CellAudio {
        track: sample(seconds),
        params,
    }
}

fn deck() -> SampleDeck {
    let mut d = SampleDeck::new(RATE);
    d.handle(Command::SetBehaviour(Behaviour {
        auto_play: false,
        one_shot_ignores_quantize: false,
        one_shot_ignores_punch: false,
    }));
    d
}

fn run(deck: &mut SampleDeck, blocks: usize, slot: usize) -> f32 {
    let mut bufs: [Vec<f32>; SLOTS] = std::array::from_fn(|_| vec![0.0; 2 * BLOCK]);
    let mut peak = 0.0f32;
    for _ in 0..blocks {
        let [s0, s1, s2, s3] = &mut bufs;
        let mut outs: [&mut [f32]; SLOTS] = [s0, s1, s2, s3];
        deck.process(&mut outs);
        peak = bufs[slot].iter().fold(0.0, |m, x| m.max(x.abs()));
    }
    peak
}

#[test]
fn quantize_rounds_up_with_a_small_tolerance() {
    assert!((quantize(1.2, 1.0) - 2.0).abs() < 1e-9);
    assert!((quantize(1.03, 1.0) - 1.0).abs() < 1e-9, "within 1/16 past");
    assert!((quantize(1.07, 1.0) - 2.0).abs() < 1e-9);
    assert!((quantize(3.0, 4.0) - 4.0).abs() < 1e-9);
    assert!((quantize(0.3, 0.25) - 0.25).abs() < 1e-9);
    assert!((quantize(0.33, 0.25) - 0.5).abs() < 1e-9);
    assert!((quantize(-0.5, 1.0) - 0.0).abs() < 1e-9);
    assert!(
        (quantize(2.5, 0.0) - 2.5).abs() < 1e-9,
        "q ≤ 0: unquantized"
    );
}

#[test]
fn gain_control_spans_plus_minus_twelve_db() {
    let p = CellParams::default();
    assert!((p.linear_gain() - 1.0).abs() < 1e-6);
    let loud = CellParams {
        gain: 1.0,
        ..CellParams::default()
    };
    assert!((loud.linear_gain() - 4.0).abs() < 1e-6);
    let quiet = CellParams {
        gain: 0.0,
        ..CellParams::default()
    };
    assert!((quiet.linear_gain() - 0.25).abs() < 1e-6);
}

#[test]
fn markers_are_beats_from_start_and_from_the_end() {
    let p = CellParams {
        bpm: 120.0,
        start_beats: 1.0,
        end_beats: -2.0,
        ..CellParams::default()
    };
    let (start, end) = p.range_s(4.0);
    assert!((start - 0.5).abs() < 1e-9 && (end - 3.0).abs() < 1e-9);
}

#[test]
fn slot_size_sets_the_range_as_a_multiple_of_the_length() {
    let mut p = CellParams {
        bpm: 120.0,
        ..CellParams::default()
    };
    assert!(p.set_size(size_factor(10), 4.0));
    assert!((p.end_beats + 4.0).abs() < 1e-5);
    assert!(p.alter_range(true, true, 4.0));
    assert!(p.end_beats.abs() < 1e-5);
    assert!(p.alter_range(false, false, 4.0));
    assert!((p.start_beats - 4.0).abs() < 1e-5);
    let mut tiny = p;
    assert!(!tiny.set_size(size_factor(0), 4.0));
    assert!((tiny.start_beats - p.start_beats).abs() < 1e-9);
}

#[test]
fn trigger_waits_for_the_quantize_point_then_plays() {
    let mut d = deck();
    d.handle(Command::SetBaseBpm(120.0));
    d.handle(Command::LoadCell {
        slot: 0,
        cell: 0,
        audio: audio(4.0, CellParams::default()),
    });
    d.handle(Command::Play(true));
    run(&mut d, 2, 0);
    d.handle(Command::Trigger {
        slot: 0,
        cell: 0,
        pressed: true,
    });
    assert_eq!(d.state().slots[0].cells[0], CellState::Queued);
    assert_eq!(run(&mut d, 1, 0), 0.0, "silent until the beat");
    assert_eq!(
        d.state().slots[0].player,
        PlayerState::Empty,
        "loaded at the beat"
    );
    run(&mut d, 3, 0);
    assert!(d.state().beat >= 1.0);
    assert_eq!(d.state().slots[0].player, PlayerState::Playing);
    assert_eq!(d.state().slots[0].cells[0], CellState::Playing);
    assert!(run(&mut d, 1, 0) > 0.9);
}

#[test]
fn unquantized_trigger_plays_at_once_and_auto_play_starts_the_deck() {
    let mut d = SampleDeck::new(RATE);
    d.handle(Command::SetQuantize {
        on: false,
        beats: 1.0,
    });
    d.handle(Command::LoadCell {
        slot: 1,
        cell: 3,
        audio: audio(2.0, CellParams::default()),
    });
    assert!(!d.is_playing());
    d.handle(Command::Trigger {
        slot: 1,
        cell: 3,
        pressed: true,
    });
    assert!(d.is_playing(), "AutoEnableDeckPlayOnSampleTrigger");
    run(&mut d, 1, 1);
    assert_eq!(d.state().slots[1].player, PlayerState::Playing);
    assert_eq!(d.state().slots[1].active, 3);
}

#[test]
fn gate_stops_on_release_and_latch_does_not() {
    let mut d = deck();
    d.handle(Command::SetQuantize {
        on: false,
        beats: 1.0,
    });
    let gate = CellParams {
        trigger_type: TriggerType::Gate,
        ..CellParams::default()
    };
    d.handle(Command::LoadCell {
        slot: 0,
        cell: 0,
        audio: audio(4.0, gate),
    });
    d.handle(Command::LoadCell {
        slot: 1,
        cell: 0,
        audio: audio(4.0, CellParams::default()),
    });
    d.handle(Command::Play(true));
    for slot in 0..2 {
        d.handle(Command::Trigger {
            slot,
            cell: 0,
            pressed: true,
        });
    }
    run(&mut d, 2, 0);
    for slot in 0..2 {
        d.handle(Command::Trigger {
            slot,
            cell: 0,
            pressed: false,
        });
    }
    run(&mut d, 2, 0);
    assert_eq!(d.state().slots[0].player, PlayerState::Stopped, "gate");
    assert_eq!(d.state().slots[1].player, PlayerState::Playing, "latch");
}

#[test]
fn empty_cell_trigger_selects_it_or_stops_a_playing_slot() {
    let mut d = deck();
    d.handle(Command::SetQuantize {
        on: false,
        beats: 1.0,
    });
    d.handle(Command::LoadCell {
        slot: 0,
        cell: 0,
        audio: audio(4.0, CellParams::default()),
    });
    d.handle(Command::Trigger {
        slot: 0,
        cell: 5,
        pressed: true,
    });
    assert_eq!(d.state().slots[0].active, 5);
    d.handle(Command::Play(true));
    d.handle(Command::Trigger {
        slot: 0,
        cell: 0,
        pressed: true,
    });
    run(&mut d, 2, 0);
    assert_eq!(d.state().slots[0].player, PlayerState::Playing);
    d.handle(Command::Trigger {
        slot: 0,
        cell: 7,
        pressed: true,
    });
    run(&mut d, 2, 0);
    assert_eq!(d.state().slots[0].player, PlayerState::Empty);
    assert_eq!(d.state().slots[0].cells[0], CellState::Loaded);
}

#[test]
fn synced_loop_follows_the_deck_tempo() {
    let mut d = deck();
    d.handle(Command::SetQuantize {
        on: false,
        beats: 1.0,
    });
    d.handle(Command::SetBaseBpm(120.0));
    let cell = CellParams {
        bpm: 60.0,
        ..CellParams::default()
    };
    d.handle(Command::LoadCell {
        slot: 0,
        cell: 0,
        audio: audio(8.0, cell),
    });
    d.handle(Command::Play(true));
    d.handle(Command::Trigger {
        slot: 0,
        cell: 0,
        pressed: true,
    });
    run(&mut d, 10, 0);
    let pos = d.state().slots[0].position_s;
    assert!((pos - 2.0).abs() < 0.15, "position {pos}");
    let free = CellParams {
        synced: false,
        ..cell
    };
    d.handle(Command::SetCellParams {
        slot: 0,
        cell: 0,
        params: free,
    });
    run(&mut d, 1, 0);
    let before = d.state().slots[0].position_s;
    run(&mut d, 10, 0);
    let moved = d.state().slots[0].position_s - before;
    assert!((moved - 1.0).abs() < 0.15, "moved {moved}");
}

#[test]
fn first_loop_sets_the_deck_tempo_once() {
    let mut d = deck();
    assert!((d.state().base_bpm - DEFAULT_BPM).abs() < 1e-9);
    let cell = |bpm| CellParams {
        bpm,
        ..CellParams::default()
    };
    d.handle(Command::LoadCell {
        slot: 0,
        cell: 0,
        audio: audio(1.0, cell(100.0)),
    });
    d.handle(Command::LoadCell {
        slot: 1,
        cell: 0,
        audio: audio(1.0, cell(90.0)),
    });
    assert!((d.state().base_bpm - 100.0).abs() < 1e-9);
}

#[test]
fn sync_matches_the_clock_tempo_and_phase() {
    let mut d = deck();
    d.handle(Command::SetBaseBpm(100.0));
    d.handle(Command::Play(true));
    d.follow_clock(true, 128.0, 7.25, 1.0);
    run(&mut d, 1, 0);
    let s = d.state();
    assert!((s.bpm - 128.0).abs() < 1e-9);
    let phase = wrap_phase(s.beat - (7.25 + 128.0 * 0.1 / 60.0));
    assert!(phase.abs() < 1e-9, "phase {phase}");
}

#[test]
fn unloading_the_played_cell_empties_the_player() {
    let mut d = deck();
    d.handle(Command::SetQuantize {
        on: false,
        beats: 1.0,
    });
    d.handle(Command::LoadCell {
        slot: 2,
        cell: 1,
        audio: audio(2.0, CellParams::default()),
    });
    d.handle(Command::Play(true));
    d.handle(Command::Trigger {
        slot: 2,
        cell: 1,
        pressed: true,
    });
    run(&mut d, 1, 2);
    let old = d.handle(Command::UnloadCell { slot: 2, cell: 1 });
    assert!(old.is_some(), "the cell's track comes back to be freed");
    assert_eq!(d.state().slots[2].player, PlayerState::Empty);
    assert_eq!(d.state().slots[2].cells[1], CellState::Empty);
    assert_eq!(run(&mut d, 1, 2), 0.0);
}

#[test]
fn the_sequencer_plays_its_steps_at_once_and_records_presses() {
    let mut d = deck();
    d.handle(Command::SetBaseBpm(120.0));
    d.handle(Command::LoadCell {
        slot: 0,
        cell: 5,
        audio: audio(4.0, CellParams::default()),
    });
    d.handle(Command::SetSelectedCell { slot: 0, cell: 5 });
    d.handle(Command::SequencerTrigger { slot: 0 });
    assert_eq!(
        d.state().slots[0].cells[5],
        CellState::Loaded,
        "with the sequencer off, nothing"
    );
    d.handle(Command::SequencerOn(true));
    d.handle(Command::SequencerRecord(true));
    d.handle(Command::Play(true));
    run(&mut d, 2, 0);
    d.handle(Command::SequencerTrigger { slot: 0 });
    run(&mut d, 1, 0);
    assert_eq!(
        d.state().slots[0].player,
        PlayerState::Playing,
        "no waiting for the quantize point"
    );
    assert_eq!(d.state().slots[0].steps, 0, "the next step takes it");
    run(&mut d, 1, 0);
    let state = d.state();
    assert_eq!(state.slots[0].steps, 1 << 2);
    assert_eq!(state.slots[0].current_step, 2);
    assert!(state.recording && state.sequencer_on);
    d.handle(Command::DeletePattern { slot: 0 });
    assert_eq!(d.state().slots[0].steps, 0);
}

#[test]
fn a_synced_loop_follows_the_clock_tempo() {
    let mut d = deck();
    d.handle(Command::SetQuantize {
        on: false,
        beats: 1.0,
    });
    d.handle(Command::LoadCell {
        slot: 0,
        cell: 0,
        audio: audio(
            8.0,
            CellParams {
                bpm: 100.0,
                play_type: PlayType::Loop,
                synced: true,
                ..CellParams::default()
            },
        ),
    });
    d.handle(Command::Play(true));
    d.handle(Command::Trigger {
        slot: 0,
        cell: 0,
        pressed: true,
    });
    let mut clock_beat = 0.0;
    for _ in 0..10 {
        d.follow_clock(true, 128.0, clock_beat, 1.0);
        run(&mut d, 1, 0);
        clock_beat += 128.0 * 0.1 / 60.0;
    }
    let s = d.state();
    eprintln!(
        "deck bpm {} base {} pos {}",
        s.bpm, s.base_bpm, s.slots[0].position_s
    );
    assert!((s.bpm - 128.0).abs() < 1e-6, "deck at the clock's tempo");
    assert!(
        (s.slots[0].position_s - 1.28).abs() < 0.05,
        "the loop plays 128/100 faster: {}",
        s.slots[0].position_s
    );
}

#[test]
fn stopping_the_deck_freezes_its_slots_until_it_plays_again() {
    let mut d = deck();
    d.handle(Command::SetBaseBpm(120.0));
    d.handle(Command::SetQuantize {
        on: false,
        beats: 1.0,
    });
    d.handle(Command::LoadCell {
        slot: 0,
        cell: 0,
        audio: audio(8.0, CellParams::default()),
    });
    d.handle(Command::Play(true));
    d.handle(Command::Trigger {
        slot: 0,
        cell: 0,
        pressed: true,
    });
    assert!(run(&mut d, 5, 0) > 0.9);
    let at = d.state().slots[0].position_s;
    d.handle(Command::Play(false));
    assert_eq!(run(&mut d, 5, 0), 0.0, "silent while the deck is stopped");
    let s = d.state().slots[0];
    assert_eq!(s.player, PlayerState::Playing, "still in its play state");
    assert!((s.position_s - at).abs() < 1e-9, "standing still");
    d.handle(Command::Play(true));
    assert!(run(&mut d, 5, 0) > 0.9, "goes on");
    assert!(d.state().slots[0].position_s > at);
}
