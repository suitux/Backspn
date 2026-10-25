use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use backspn_engine::{
    Assign, Band, Command, DeckCommand, DrumMachine, EffectKind, FxCommand, FxMode, Grid,
    MixerCommand, StemCommand, StemControls, StemMaster, StemMasterSettings, Stems, SyncCommand,
    Track, channel, create_effect, fx_pattern,
};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

#[test]
#[allow(clippy::too_many_lines)]
fn audio_callback_does_not_allocate() {
    let rate = 44_100u32;
    let samples: Vec<f32> = (0..rate * 30)
        .flat_map(|i| [(i as f32 * 0.02).sin(), (i as f32 * 0.03).sin()])
        .collect();
    let stems = Arc::new(Stems(std::array::from_fn(|_| {
        Track::from_stereo(rate, samples.clone())
    })));
    let track = Arc::new(Track::from_stereo(rate, samples));
    let mut mastering = StemMasterSettings::default();
    mastering.compressor.enabled = true;
    mastering.limiter.enabled = true;
    let (mut renderer, mut controller) = channel(rate);
    let mut out = vec![0.0f32; 4 * 256];

    let sound = fx_pattern::Sound {
        name: "kick".into(),
        samples: vec![0.5; 4000].into(),
        rate: 44_100.0,
        level: 1.0,
        patterns: [fx_pattern::Steps {
            bits: 0b0101_0101_0101_0101,
            per_beat: 4,
        }; fx_pattern::PATTERNS],
    };
    let (tap, _feed) = backspn_engine::record_tap(rate, 2.0);
    let build = |with_loads: bool| {
        let mut commands = Vec::new();
        for deck in 0..2 {
            if with_loads {
                commands.push(Command::Deck(deck, DeckCommand::Load(Arc::clone(&track))));
            }
            let grid = Grid {
                bpm: 120.0 + deck as f64 * 4.0,
                anchor_s: 0.0,
                ..Grid::FIXED
            };
            commands.push(Command::Deck(deck, DeckCommand::SetGrid(Some(grid))));
            commands.push(Command::Deck(deck, DeckCommand::Play));
        }
        commands.extend([
            Command::Sync(SyncCommand::SetSync(1, true)),
            Command::Deck(1, DeckCommand::SetKeylock(true)),
            Command::Mixer(MixerCommand::Eq(0, Band::Low, 0.0)),
            Command::Mixer(MixerCommand::Filter(1, -0.6)),
            Command::Mixer(MixerCommand::Assign(0, Assign::A)),
            Command::Mixer(MixerCommand::Crossfader(0.3)),
            Command::Mixer(MixerCommand::Cue(1, true)),
            Command::DeckTick(0, true),
            Command::MasterTick(true),
            Command::Deck(0, DeckCommand::AutoLoop { beats: 4.0 }),
            Command::Sync(SyncCommand::SetMaster(Some(0))),
            Command::Deck(
                1,
                DeckCommand::BeatJump {
                    beats: 8.0,
                    stay_in_loop: true,
                },
            ),
            Command::Mixer(MixerCommand::Filter(1, 0.7)),
        ]);
        for (slot, kind) in [
            EffectKind::Delay,
            EffectKind::Reverb,
            EffectKind::BeatChopper,
        ]
        .into_iter()
        .enumerate()
        {
            commands.push(Command::Fx(FxCommand::SetEffect {
                unit: 0,
                slot,
                effects: std::array::from_fn(|_| create_effect(kind, rate)),
            }));
            commands.push(Command::Fx(FxCommand::SetAmount(0, slot, 0.7)));
            commands.push(Command::Fx(FxCommand::SetSlotOn(0, slot, true)));
        }
        commands.extend([
            Command::Fx(FxCommand::SetOn(0, true)),
            Command::Fx(FxCommand::Assign {
                channel: 0,
                unit: 0,
                on: true,
            }),
            Command::Fx(FxCommand::Assign {
                channel: 1,
                unit: 0,
                on: true,
            }),
            Command::Fx(FxCommand::SetMode(1, FxMode::Single)),
            Command::Fx(FxCommand::SetEffect {
                unit: 1,
                slot: 0,
                effects: std::array::from_fn(|_| create_effect(EffectKind::Phaser, rate)),
            }),
            Command::Fx(FxCommand::SetOn(1, true)),
            Command::Fx(FxCommand::Assign {
                channel: 1,
                unit: 1,
                on: true,
            }),
            Command::Fx(FxCommand::SetParam(1, 2, 0.3)),
            Command::Fx(FxCommand::SetMode(2, FxMode::DrumMachine)),
            Command::Fx(FxCommand::SetKit {
                unit: 2,
                players: std::array::from_fn(|_| {
                    Box::new(DrumMachine::new(std::slice::from_ref(&sound), rate))
                }),
            }),
            Command::Fx(FxCommand::SetOn(2, true)),
            Command::Fx(FxCommand::Assign {
                channel: 0,
                unit: 2,
                on: true,
            }),
            Command::Fx(FxCommand::PatternDuck(2, true)),
            Command::Fx(FxCommand::PatternStep(2, 3, true)),
            Command::Fx(FxCommand::PatternKnob(2, 2, 0.5)),
            Command::Fx(FxCommand::PatternReset(2)),
            Command::Fx(FxCommand::PatternRoll(2, true)),
        ]);
        if with_loads {
            commands.push(Command::Deck(2, DeckCommand::LoadStems(Arc::clone(&stems))));
        }
        commands.extend([
            Command::Stem(
                2,
                StemCommand::Mastering(Some(Box::new(StemMaster::new(
                    &mastering,
                    f64::from(rate),
                )))),
            ),
            Command::Deck(2, DeckCommand::Play),
            Command::Fx(FxCommand::Assign {
                channel: 2,
                unit: 0,
                on: true,
            }),
            Command::Stem(
                2,
                StemCommand::Controls(
                    1,
                    StemControls {
                        fx_on: true,
                        fx_send: 0.5,
                        filter_on: true,
                        filter: 0.8,
                        ..StemControls::default()
                    },
                ),
            ),
            Command::Deck(2, DeckCommand::SetKeylock(true)),
        ]);
        commands
    };
    for command in build(true) {
        controller.send(command).unwrap();
        for _ in 0..10 {
            renderer.render(&mut out, 4, Some((2, 3)));
        }
    }
    controller.collect_garbage();
    let commands = build(false);

    let before = allocations();
    controller.send(Command::RecordTap(Some(tap))).unwrap();
    for command in commands {
        controller.send(command).unwrap();
        for _ in 0..20 {
            renderer.render(&mut out, 4, Some((2, 3)));
        }
    }
    assert_eq!(allocations() - before, 0, "the audio callback allocated");
}
