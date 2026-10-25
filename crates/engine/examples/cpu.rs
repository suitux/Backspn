use std::sync::Arc;
use std::time::Instant;

use backspn_engine::{
    Band, Command, DeckCommand, EffectKind, FxCommand, Grid, MixerCommand, SyncCommand, Track,
    channel, create_effect,
};

fn main() {
    let rate = 44_100u32;
    let samples: Vec<f32> = (0..rate * 60)
        .flat_map(|i| {
            [
                (i as f32 * 0.021).sin() * 0.3,
                (i as f32 * 0.017).sin() * 0.3,
            ]
        })
        .collect();
    let track = Arc::new(Track::from_stereo(rate, samples));
    let (mut renderer, mut controller) = channel(rate);
    for deck in 0..4 {
        for command in [
            Command::Deck(deck, DeckCommand::Load(Arc::clone(&track))),
            Command::Deck(
                deck,
                DeckCommand::SetGrid(Some(Grid {
                    bpm: 124.0 + deck as f64,
                    anchor_s: 0.0,
                    ..Grid::FIXED
                })),
            ),
            Command::Deck(deck, DeckCommand::SetKeylock(true)),
            Command::Deck(deck, DeckCommand::Play),
            Command::Sync(SyncCommand::SetSync(deck, deck > 0)),
            Command::Mixer(MixerCommand::Eq(deck, Band::Low, 0.3)),
            Command::Mixer(MixerCommand::Filter(deck, -0.4)),
        ] {
            controller.send(command).unwrap();
        }
    }
    for (unit, kinds) in [
        [EffectKind::Delay, EffectKind::Reverb, EffectKind::Flanger],
        [
            EffectKind::Phaser,
            EffectKind::Gater,
            EffectKind::BeatChopper,
        ],
    ]
    .into_iter()
    .enumerate()
    {
        for (slot, kind) in kinds.into_iter().enumerate() {
            let effects = std::array::from_fn(|_| create_effect(kind, rate));
            controller
                .send(Command::Fx(FxCommand::SetEffect {
                    unit,
                    slot,
                    effects,
                }))
                .unwrap();
            controller
                .send(Command::Fx(FxCommand::SetAmount(unit, slot, 0.6)))
                .unwrap();
            controller
                .send(Command::Fx(FxCommand::SetSlotOn(unit, slot, true)))
                .unwrap();
        }
        controller
            .send(Command::Fx(FxCommand::SetOn(unit, true)))
            .unwrap();
        for deck in 0..4 {
            controller
                .send(Command::Fx(FxCommand::Assign {
                    channel: deck,
                    unit,
                    on: true,
                }))
                .unwrap();
        }
    }
    let block = 256;
    let mut out = vec![0.0f32; 4 * block];
    let seconds = 30;
    let blocks = rate as usize * seconds / block;
    let started = Instant::now();
    let mut worst = 0.0f64;
    for _ in 0..blocks {
        let t = Instant::now();
        renderer.render(&mut out, 4, Some((2, 3)));
        worst = worst.max(t.elapsed().as_secs_f64());
    }
    let elapsed = started.elapsed().as_secs_f64();
    let budget = block as f64 / f64::from(rate);
    println!(
        "{seconds} s of audio in {elapsed:.2} s: average load {:.1} %, worst block {:.1} % of its {:.1} ms budget",
        100.0 * elapsed / seconds as f64,
        100.0 * worst / budget,
        budget * 1e3
    );
}
