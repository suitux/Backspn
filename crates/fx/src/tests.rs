use super::*;

const RATE: u32 = 48_000;

fn ctx() -> Context {
    Context {
        bpm: 120.0,
        beat: 0.0,
        beat_end: 0.0,
    }
}

fn tone(frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let v = (i as f32 * 0.05).sin() * 0.5;
            [v, v]
        })
        .collect()
}

fn effects(kind: EffectKind) -> [Box<dyn Effect>; CHANNELS] {
    std::array::from_fn(|_| create(kind, RATE))
}

fn run(rack: &mut Rack, channel: usize, input: &[f32], blocks: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for _ in 0..blocks {
        let mut buf = input.to_vec();
        rack.process(channel, &mut buf, &ctx());
        out = buf;
    }
    out
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

#[test]
fn unassigned_or_off_units_are_transparent() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 0,
        slot: 0,
        effects: effects(EffectKind::LoFi),
    });
    rack.handle(Command::SetSlotOn(0, 0, true));
    rack.handle(Command::SetAmount(0, 0, 1.0));
    let input = tone(512);
    assert_eq!(run(&mut rack, 0, &input, 5), input, "not assigned");
    rack.handle(Command::Assign {
        channel: 0,
        unit: 0,
        on: true,
    });
    assert_eq!(run(&mut rack, 0, &input, 5), input, "unit off");
}

#[test]
fn group_slot_applies_its_effect_with_a_fade() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 1,
        slot: 2,
        effects: effects(EffectKind::LoFi),
    });
    rack.handle(Command::SetAmount(1, 2, 1.0));
    rack.handle(Command::SetSlotOn(1, 2, true));
    rack.handle(Command::SetOn(1, true));
    rack.handle(Command::Assign {
        channel: 3,
        unit: 1,
        on: true,
    });
    let input = tone(512);
    let mut first = input.clone();
    rack.process(3, &mut first, &ctx());
    assert!(
        (first[0] - input[0]).abs() < 0.05,
        "starts nearly dry (fade-in)"
    );
    let out = run(&mut rack, 3, &input, 10);
    let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
    assert!(rms(&diff) > 0.01, "effect audible after the fade");
    assert_eq!(run(&mut rack, 0, &input, 2), input);
}

#[test]
fn dry_wet_zero_is_dry() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 0,
        slot: 0,
        effects: effects(EffectKind::Ringmod),
    });
    rack.handle(Command::SetMode(0, Mode::Single));
    rack.handle(Command::SetParam(0, 1, 1.0));
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::SetDryWet(0, 0.0));
    rack.handle(Command::Assign {
        channel: 0,
        unit: 0,
        on: true,
    });
    let input = tone(512);
    let out = run(&mut rack, 0, &input, 20);
    let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
    assert!(rms(&diff) < 1e-4);
    rack.handle(Command::SetDryWet(0, 1.0));
    let out = run(&mut rack, 0, &input, 20);
    let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
    assert!(rms(&diff) > 0.05);
}

#[test]
fn single_effects_with_their_own_law_take_the_dry_wet() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 0,
        slot: 0,
        effects: effects(EffectKind::DelayT3),
    });
    rack.handle(Command::SetMode(0, Mode::Single));
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::Assign {
        channel: 0,
        unit: 0,
        on: true,
    });
    rack.handle(Command::SetDryWet(0, 0.5));
    let input = tone(256);
    let out = run(&mut rack, 0, &input, 4);
    let diff: Vec<f32> = out.iter().zip(&input).map(|(a, b)| a - b).collect();
    assert!(rms(&diff) < 1e-3, "dry whole, no echo yet");
    rack.handle(Command::SetDryWet(0, 1.0));
    let out = run(&mut rack, 0, &input, 20);
    assert!(rms(&out) < 1e-3);
}

#[test]
fn listening_effects_record_while_their_slot_is_off() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 0,
        slot: 0,
        effects: effects(EffectKind::BackSpin),
    });
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::Assign {
        channel: 0,
        unit: 0,
        on: true,
    });
    let input = tone(512);
    let out = run(&mut rack, 0, &input, 100);
    assert_eq!(out, input);
    rack.handle(Command::SetSlotOn(0, 0, true));
    let out = run(&mut rack, 0, &vec![0.0; 2 * 512], 10);
    assert!(rms(&out) > 0.05, "the past, not silence");
}

#[test]
fn first_bank_effects_show_their_controls_mirrored() {
    let info = EffectKind::Delay.info();
    assert_eq!(info.params, ["FILTER", "FEEDB", "RATE"]);
    assert_eq!(info.buttons, ["RST", "FRZ", "SPR"]);
    assert!((info.defaults[2] - 0.499_75).abs() < 1e-6);
    assert_eq!(EffectKind::Echo.info().params, ["BEAT", "FEEDB", "FILTER"]);
    assert!(!EffectKind::Echo.mirrored());
    assert_eq!(EffectKind::Turntable.info().buttons, ["RST", "RCK", "REW"]);
}

fn retired_kind(old: Option<Retired>) -> EffectKind {
    match old {
        Some(Retired::Effects(e)) => e[0].kind(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn replaced_effects_are_handed_back() {
    let mut rack = Rack::new(RATE);
    let old = rack.handle(Command::SetEffect {
        unit: 0,
        slot: 1,
        effects: effects(EffectKind::Delay),
    });
    assert_eq!(retired_kind(old), EffectKind::None);
    let old = rack.handle(Command::SetEffect {
        unit: 0,
        slot: 1,
        effects: effects(EffectKind::Gater),
    });
    assert_eq!(retired_kind(old), EffectKind::Delay);
    let rejected = rack.handle(Command::SetEffect {
        unit: 9,
        slot: 0,
        effects: effects(EffectKind::Gater),
    });
    assert!(
        rejected.is_some(),
        "out-of-range commands still return their instances"
    );
    assert_eq!(rack.state().units[0].slots[1].kind, EffectKind::Gater);
}

#[test]
fn single_mode_uses_the_effect_defaults_and_params() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetMode(2, Mode::Single));
    rack.handle(Command::SetEffect {
        unit: 2,
        slot: 0,
        effects: effects(EffectKind::Delay),
    });
    assert_eq!(
        rack.state().units[2].params,
        EffectKind::Delay.info().defaults
    );
    rack.handle(Command::SetParam(2, 1, 0.9));
    rack.handle(Command::SetButton(2, 0, true));
    let s = rack.state().units[2];
    assert!((s.params[1] - 0.9).abs() < 1e-6 && s.buttons[0]);
}

#[test]
fn every_effect_is_finite_and_quiet_at_zero() {
    for kind in EffectKind::ALL {
        let mut e = create(kind, RATE);
        e.set_amount(kind.info().neutral_amount);
        let input = tone(4_800);
        let mut buf = input.clone();
        for block in buf.chunks_mut(512) {
            e.process(block, &ctx());
        }
        assert!(buf.iter().all(|v| v.is_finite()), "{kind:?}");
        if !kind.info().group_mix {
            continue;
        }
        let diff: Vec<f32> = buf[2 * 2_400..]
            .iter()
            .zip(&input[2 * 2_400..])
            .map(|(a, b)| a - b)
            .collect();
        assert!(
            rms(&diff) < 0.01,
            "{kind:?} not transparent at its neutral amount: {}",
            rms(&diff)
        );
    }
}

fn delay_rack(amount: f32) -> Rack {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 0,
        slot: 0,
        effects: effects(EffectKind::Delay),
    });
    rack.handle(Command::SetAmount(0, 0, amount));
    rack.handle(Command::SetSlotOn(0, 0, true));
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::Assign {
        channel: 0,
        unit: 0,
        on: true,
    });
    rack
}

fn tail_after(off: impl FnOnce(&mut Rack), seconds: f32) -> Vec<f32> {
    let mut rack = delay_rack(1.0);
    let mut burst = tone(4_800);
    rack.process(0, &mut burst, &ctx());
    off(&mut rack);
    let mut out = Vec::new();
    for _ in 0..(seconds * RATE as f32 / 512.0) as usize {
        let mut block = vec![0.0; 1_024];
        rack.process(0, &mut block, &ctx());
        out.extend(block);
    }
    out
}

#[test]
fn switching_a_delay_off_lets_its_echoes_ring_out() {
    for (label, out) in [
        (
            "slot off",
            tail_after(
                |r| {
                    r.handle(Command::SetSlotOn(0, 0, false));
                },
                3.0,
            ),
        ),
        (
            "unit off",
            tail_after(
                |r| {
                    r.handle(Command::SetOn(0, false));
                },
                3.0,
            ),
        ),
        (
            "deck unassigned",
            tail_after(
                |r| {
                    r.handle(Command::Assign {
                        channel: 0,
                        unit: 0,
                        on: false,
                    });
                },
                3.0,
            ),
        ),
    ] {
        let second = &out[2 * 24_000..2 * 48_000];
        assert!(rms(second) > 1e-3, "{label}: tail cut");
        let late = &out[out.len() - 2 * 24_000..];
        assert!(rms(late) < rms(second), "{label}: tail does not decay");
    }
}

#[test]
fn new_input_is_not_echoed_after_switch_off() {
    let mut rack = delay_rack(1.0);
    rack.handle(Command::SetSlotOn(0, 0, false));
    let mut silence = vec![0.0; 1_024];
    for _ in 0..10 {
        rack.process(0, &mut silence, &ctx());
    }
    let mut burst = tone(4_800);
    let input = burst.clone();
    rack.process(0, &mut burst, &ctx());
    assert_eq!(burst, input, "dry while off");
    let mut after = vec![0.0; 2 * 24_000];
    rack.process(0, &mut after[..8_192], &ctx());
    for chunk in after.chunks_mut(1_024) {
        rack.process(0, chunk, &ctx());
    }
    assert!(after.iter().all(|v| v.abs() < 1e-6), "echo of new input");
}

#[test]
fn tails_eventually_stop_being_processed() {
    let out = tail_after(
        |r| {
            r.handle(Command::SetOn(0, false));
        },
        80.0,
    );
    assert!(
        out[out.len() - 2_048..].iter().all(|&v| v == 0.0),
        "tail ends in exact silence"
    );
}

#[test]
fn effects_without_tails_stop_quickly() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 0,
        slot: 0,
        effects: effects(EffectKind::BeatChopper),
    });
    rack.handle(Command::SetAmount(0, 0, 0.5));
    rack.handle(Command::SetSlotOn(0, 0, true));
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::Assign {
        channel: 0,
        unit: 0,
        on: true,
    });
    let input = tone(512);
    let _ = run(&mut rack, 0, &input, 50);
    rack.handle(Command::SetSlotOn(0, 0, false));
    let out = run(&mut rack, 0, &input, 20);
    assert_eq!(out, input, "repeats stop after the short fade");
}

fn lofi_unit(channel: usize) -> Rack {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 0,
        slot: 0,
        effects: effects(EffectKind::LoFi),
    });
    rack.handle(Command::SetAmount(0, 0, 1.0));
    rack.handle(Command::SetSlotOn(0, 0, true));
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::Assign {
        channel,
        unit: 0,
        on: true,
    });
    rack
}

#[test]
fn each_unit_runs_only_in_its_routing_stage() {
    let input = tone(512);
    let mut rack = lofi_unit(1);
    assert_ne!(run(&mut rack, 1, &input, 5), input, "insert by default");

    rack.handle(Command::SetRouting(0, Routing::PostFader));
    assert_eq!(rack.routing(0), Routing::PostFader);
    assert_eq!(
        run(&mut rack, 1, &input, 5),
        input,
        "not in the insert stage"
    );
    let mut post = input.clone();
    for _ in 0..5 {
        post.copy_from_slice(&input);
        rack.process_post_fader(1, &mut post, &ctx());
    }
    assert_ne!(post, input, "in the post-fader stage");
}

#[test]
fn a_send_unit_processes_its_bus_whatever_decks_feed_it() {
    let input = tone(512);
    let mut rack = lofi_unit(2);
    rack.handle(Command::SetRouting(0, Routing::Send));
    assert_eq!(
        run(&mut rack, 2, &input, 5),
        input,
        "not inserted on the deck"
    );
    let mut bus = input.clone();
    for _ in 0..5 {
        bus.copy_from_slice(&input);
        rack.process_send(0, &mut bus, &ctx());
    }
    assert_ne!(bus, input, "the bus goes through the unit");

    rack.handle(Command::SetDryWet(0, 0.0));
    for _ in 0..20 {
        bus.copy_from_slice(&input);
        rack.process_send(0, &mut bus, &ctx());
    }
    let diff: Vec<f32> = bus.iter().zip(&input).map(|(a, b)| a - b).collect();
    assert!(rms(&diff) < 1e-4, "{}", rms(&diff));
}

fn kit_players(rate: u32) -> [Box<DrumMachine>; CHANNELS] {
    let sound = pattern::Sound {
        name: "click".into(),
        samples: vec![1.0; 2000].into(),
        rate: rate as f32,
        level: 1.0,
        patterns: [pattern::Steps {
            bits: 0b0001_0001_0001_0001,
            per_beat: 4,
        }; pattern::PATTERNS],
    };
    std::array::from_fn(|_| Box::new(DrumMachine::new(std::slice::from_ref(&sound), rate)))
}

#[test]
fn drum_machine_mode_adds_its_sounds_to_assigned_decks_only() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetMode(0, Mode::DrumMachine));
    assert!(matches!(
        rack.handle(Command::SetKit {
            unit: 0,
            players: kit_players(RATE),
        }),
        Some(Retired::Patterns(_))
    ));
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::Assign {
        channel: 1,
        unit: 0,
        on: true,
    });
    let mut beat = 0.0;
    let mut heard = [0.0f32; 2];
    for _ in 0..60 {
        let ctx = Context {
            bpm: 120.0,
            beat,
            beat_end: beat + 0.02,
        };
        beat += 0.02;
        for (ch, h) in heard.iter_mut().enumerate() {
            let mut buf = vec![0.0; 960];
            rack.process(ch, &mut buf, &ctx);
            *h = h.max(buf.iter().copied().fold(0.0, f32::max));
        }
    }
    assert_eq!(heard[0], 0.0, "deck A not assigned");
    assert!(heard[1] > 0.4, "deck B hears the kit at D/W {}", heard[1]);
    assert!(rack.state().units[0].pattern.step.is_some());
}

#[test]
fn pattern_controls_are_kept_for_a_new_kit() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::PatternKnob(2, 1, 0.75));
    rack.handle(Command::PatternDuck(2, true));
    rack.handle(Command::SetKit {
        unit: 2,
        players: kit_players(RATE),
    });
    let p = rack.state().units[2].pattern;
    assert_eq!(p.knobs, [1.0, 0.75, 0.0]);
    assert!(p.duck);
    rack.handle(Command::PatternSound(2, 5));
    assert_eq!(
        rack.state().units[2].pattern.sound,
        0,
        "no sound 6 in the kit"
    );
}

fn modified_rack(effect: EffectKind, kind: EffectKind, amount: f32) -> Rack {
    let mut rack = Rack::new(RATE);
    for (slot, k, a) in [(0, effect, 1.0), (1, kind, amount)] {
        rack.handle(Command::SetEffect {
            unit: 0,
            slot,
            effects: effects(k),
        });
        rack.handle(Command::SetAmount(0, slot, a));
        rack.handle(Command::SetSlotOn(0, slot, true));
    }
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::Assign {
        channel: 0,
        unit: 0,
        on: true,
    });
    rack
}

fn sine(freq: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let v = (std::f32::consts::TAU * freq * i as f32 / RATE as f32).sin() * 0.5;
            [v, v]
        })
        .collect()
}

fn change(rack: &mut Rack, freq: f32) -> f32 {
    const BLOCKS: usize = 40;
    let signal = sine(freq, 512 * BLOCKS);
    let mut last = Vec::new();
    for block in signal.chunks(1_024) {
        last = block.to_vec();
        rack.process(0, &mut last, &ctx());
    }
    let reference = &signal[signal.len() - 1_024..];
    let diff: Vec<f32> = last.iter().zip(reference).map(|(a, b)| a - b).collect();
    rms(&diff) / rms(reference)
}

#[test]
fn effect_modifier_filter_keeps_the_effects_to_one_band() {
    use EffectKind::{ModFilter, Ringmod};
    let mut rack = modified_rack(Ringmod, ModFilter, 0.5);
    assert!(change(&mut rack, 8_000.0) > 0.3 && change(&mut rack, 60.0) > 0.3);
    let mut rack = modified_rack(Ringmod, ModFilter, 1.0);
    assert!(change(&mut rack, 8_000.0) < 0.05, "highs clean");
    assert!(change(&mut rack, 60.0) > 0.3, "lows affected");
    let mut rack = modified_rack(Ringmod, ModFilter, 0.0);
    assert!(change(&mut rack, 60.0) < 0.05, "lows clean");
    assert!(change(&mut rack, 8_000.0) > 0.3, "highs affected");
}

#[test]
fn effect_modifier_decay_lengthens_or_shortens_the_other_effects() {
    use EffectKind::{Delay, ModDecay};
    let tail = |decay: f32| {
        let mut rack = modified_rack(Delay, ModDecay, decay);
        rack.handle(Command::SetAmount(0, 0, 0.75));
        let mut burst = tone(4_800);
        rack.process(0, &mut burst, &ctx());
        let mut out = Vec::new();
        for _ in 0..(3 * RATE as usize / 512) {
            let mut block = vec![0.0; 1_024];
            rack.process(0, &mut block, &ctx());
            out.extend(block);
        }
        rms(&out[2 * 2 * RATE as usize..])
    };
    let (short, centre, long) = (tail(0.0), tail(0.5), tail(1.0));
    assert!(short < 0.1 * centre, "short {short} centre {centre}");
    assert!(long > 2.0 * centre, "long {long} centre {centre}");
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetEffect {
        unit: 0,
        slot: 0,
        effects: effects(ModDecay),
    });
    rack.handle(Command::SetSlotOn(0, 0, true));
    rack.handle(Command::SetOn(0, true));
    rack.handle(Command::Assign {
        channel: 0,
        unit: 0,
        on: true,
    });
    let input = tone(512);
    assert_eq!(run(&mut rack, 0, &input, 3), input);
}

#[test]
fn modifiers_show_a_short_name_on_their_slot() {
    assert_eq!(EffectKind::ModFilter.short_name(), "Mod: Filter");
    assert_eq!(EffectKind::ModDecay.short_name(), "Mod: Decay");
    assert_eq!(
        EffectKind::Delay.short_name(),
        EffectKind::Delay.info().name
    );
    for kind in EffectKind::ALL {
        assert!(
            kind.short_name().len() <= 12,
            "{} too long",
            kind.short_name()
        );
    }
}

fn hits(buf: &[f32], offset: usize, out: &mut Vec<usize>) {
    for i in 1..buf.len() / 2 {
        if buf[2 * i] - buf[2 * i - 2] > 0.2 {
            out.push(offset + i);
        }
    }
}

#[test]
fn a_drum_machine_assigned_again_plays_in_step_with_the_other_deck() {
    let mut rack = Rack::new(RATE);
    rack.handle(Command::SetMode(0, Mode::DrumMachine));
    rack.handle(Command::SetKit {
        unit: 0,
        players: kit_players(RATE),
    });
    rack.handle(Command::SetOn(0, true));
    let assign = |rack: &mut Rack, channel, on| {
        rack.handle(Command::Assign {
            channel,
            unit: 0,
            on,
        });
    };
    assign(&mut rack, 0, true);
    let frames = 480;
    let per_block = 120.0 / 60.0 * frames as f64 / f64::from(RATE);
    let mut beat = 0.37;
    let mut heard: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    for b in 0..400 {
        if b == 37 || b == 160 {
            assign(&mut rack, 1, true);
        }
        if b == 120 {
            assign(&mut rack, 1, false);
        }
        let ctx = Context {
            bpm: 120.0,
            beat,
            beat_end: beat + per_block,
        };
        beat += per_block;
        for (ch, h) in heard.iter_mut().enumerate() {
            let mut buf = vec![0.0; 2 * frames];
            rack.process(ch, &mut buf, &ctx);
            if b >= 200 {
                hits(&buf, b * frames, h);
            }
        }
    }
    assert!(!heard[0].is_empty(), "the kit plays");
    assert_eq!(heard[0], heard[1], "both decks hit together");
}
