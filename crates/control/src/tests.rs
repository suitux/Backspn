use super::*;

#[test]
fn knobs_with_a_centre_snap_to_it_near_it() {
    assert!((centre_zone(0.5) - 0.5).abs() < 1e-6);
    assert!((centre_zone(0.485) - 0.5).abs() < 1e-6);
    assert!((centre_zone(0.519) - 0.5).abs() < 1e-6);
    assert!(centre_zone(0.0).abs() < 1e-6);
    assert!((centre_zone(1.0) - 1.0).abs() < 1e-6);
    assert!((centre_zone(0.75) - 0.7396).abs() < 1e-3);
    assert!((centre_zone(0.5201) - 0.5).abs() < 1e-3);

    let knob = |action, input: Input| {
        let m = Mapping::new(input.clone(), action, Interaction::Direct);
        let e = InputEvent {
            input,
            value: 0.49,
            raw: 0,
            pressed: None,
        };
        match trigger(&m, &e) {
            Some(Trigger::Set(v)) => v,
            other => panic!("{other:?}"),
        }
    };
    let hid = || Input::Hid("Mixer.Ch1.Filter".to_owned());
    assert!((knob(Action::Filter { channel: 0 }, hid()) - 0.5).abs() < 1e-6);
    assert!(
        (knob(Action::Volume { channel: 0 }, hid()) - 0.49).abs() < 1e-6,
        "no centre"
    );
    assert!(
        (knob(Action::Tempo { deck: 0 }, hid()) - 0.49).abs() < 1e-6,
        "a native tempo fader keeps its own handling"
    );
    let midi = Input::Midi {
        channel: 0,
        kind: MidiKind::Cc,
        number: 1,
    };
    assert!((knob(Action::Tempo { deck: 0 }, midi) - 0.5).abs() < 1e-6);
}

fn key(k: &str, down: bool) -> InputEvent {
    InputEvent {
        input: Input::Key(k.into()),
        value: if down { 1.0 } else { 0.0 },
        raw: 0,
        pressed: Some(down),
    }
}

fn cc(number: u8, raw: u8) -> InputEvent {
    InputEvent {
        input: Input::Midi {
            channel: 0,
            kind: MidiKind::Cc,
            number,
        },
        value: f32::from(raw) / 127.0,
        raw,
        pressed: None,
    }
}

fn set(mappings: Vec<Mapping>) -> MappingSet {
    MappingSet {
        mappings,
        midi_inputs: Vec::new(),
        ..MappingSet::default()
    }
}

#[test]
fn hold_emits_press_and_release() {
    let s = set(MappingSet::default_keyboard());
    assert_eq!(
        s.map(&key("Q", true)),
        [(Action::Cue { deck: 0 }, Trigger::Press)]
    );
    assert_eq!(
        s.map(&key("Q", false)),
        [(Action::Cue { deck: 0 }, Trigger::Release)]
    );
    assert_eq!(s.map(&key("W", false)), []);
    assert_eq!(s.map(&key("Z", true)), []);
}

#[test]
fn a_switch_sets_the_value_of_each_position() {
    let s = set(vec![Mapping {
        positions: vec![(0, 0.5), (1, 1.0), (2, 0.0)],
        ..Mapping::new(
            cc(30, 0).input,
            Action::XfaderAssign { channel: 0 },
            Interaction::Direct,
        )
    }]);
    let assign = Action::XfaderAssign { channel: 0 };
    assert_eq!(s.map(&cc(30, 1)), [(assign, Trigger::Set(1.0))]);
    assert_eq!(s.map(&cc(30, 0)), [(assign, Trigger::Set(0.5))]);
    assert_eq!(s.map(&cc(30, 2)), [(assign, Trigger::Set(0.0))]);
    assert_eq!(s.map(&cc(30, 3)), Vec::new());
}

#[test]
fn direct_and_relative_controls() {
    let s = set(vec![
        Mapping::new(
            cc(7, 0).input,
            Action::Volume { channel: 1 },
            Interaction::Direct,
        ),
        Mapping {
            invert: true,
            ..Mapping::new(cc(8, 0).input, Action::BrowserUpDown, Interaction::Relative)
        },
        Mapping::new(
            cc(9, 0).input,
            Action::Sync { deck: 0 },
            Interaction::Toggle,
        ),
    ]);
    assert_eq!(
        s.map(&cc(7, 127)),
        [(Action::Volume { channel: 1 }, Trigger::Set(1.0))]
    );
    assert_eq!(
        s.map(&cc(8, 1)),
        [(Action::BrowserUpDown, Trigger::Delta(-1.0))]
    );
    assert_eq!(
        s.map(&cc(8, 126)),
        [(Action::BrowserUpDown, Trigger::Delta(2.0))]
    );
    assert_eq!(
        s.map(&cc(9, 127)),
        [(Action::Sync { deck: 0 }, Trigger::Press)]
    );
    assert_eq!(s.map(&cc(9, 0)), []);
}

#[test]
fn learn_binds_the_next_suitable_input() {
    let mut learn = Learn::new(Action::Volume { channel: 0 });
    let m = learn.accept(&cc(70, 30)).unwrap();
    assert_eq!(
        (m.interaction, m.input.clone()),
        (Interaction::Direct, cc(70, 0).input)
    );
    let mut learn = Learn::new(Action::Cue { deck: 1 });
    assert!(
        learn.accept(&key("K", false)).is_none(),
        "releases are ignored"
    );
    assert_eq!(
        learn.accept(&key("K", true)).unwrap().interaction,
        Interaction::Hold
    );
}

#[test]
fn learn_tells_14_bit_pairs_by_their_numbers() {
    let id = |number| MidiId {
        channel: 0,
        kind: MidiKind::Cc,
        number,
    };
    for order in [[7, 39, 7], [39, 7, 39]] {
        let mut learn = Learn::new(Action::Volume { channel: 0 });
        assert!(learn.accept(&cc(order[0], 1)).is_none());
        assert!(learn.accept(&cc(order[1], 1)).is_none());
        let m = learn.accept(&cc(order[2], 1)).unwrap();
        assert_eq!(
            m.input,
            Input::MidiPair {
                lsb: id(39),
                msb: id(7)
            }
        );
    }
    let mut learn = Learn::new(Action::Volume { channel: 0 });
    assert!(learn.accept(&cc(10, 1)).is_none());
    assert!(learn.accept(&cc(10, 2)).is_none());
    assert_eq!(learn.accept(&cc(10, 3)).unwrap().input, cc(10, 0).input);
    let mut learn = Learn::new(Action::PlayPause { deck: 0 });
    assert!(learn.accept(&cc(10, 127)).is_some());
}

#[test]
fn persistence_round_trip() {
    let dir = std::env::temp_dir().join(format!("backspn-control-{}", std::process::id()));
    let path = dir.join("mappings.json");
    let mut s = set(MappingSet::default_keyboard());
    s.midi_inputs.push("My Controller".into());
    s.upsert(Mapping::new(
        Input::Key("Q".into()),
        Action::Cue { deck: 0 },
        Interaction::Trigger,
    ));
    assert_eq!(
        s.mappings.len(),
        4,
        "upsert replaces the same input + action"
    );
    s.save(&path).unwrap();
    assert_eq!(MappingSet::load(&path).unwrap(), (s, 0));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn labels_read_well() {
    assert_eq!(
        Action::BeatJump {
            deck: 1,
            beats: -0.5
        }
        .label(),
        "Deck B Jump -1/2"
    );
    assert_eq!(
        Action::LoopSize {
            deck: 0,
            beats: 4.0
        }
        .label(),
        "Deck A Loop 4"
    );
    assert_eq!(
        Input::Midi {
            channel: 0,
            kind: MidiKind::Note,
            number: 61
        }
        .to_string(),
        "Ch01 Note C#3"
    );
}

fn hid(name: &str, value: f32, pressed: Option<bool>) -> InputEvent {
    InputEvent {
        input: Input::Hid(name.into()),
        value,
        raw: 0,
        pressed,
    }
}

#[test]
fn hid_relative_controls_use_their_step_count() {
    let s = set(vec![Mapping::new(
        Input::Hid("Left.Jog.Turn".into()),
        Action::JogTurn { deck: 0 },
        Interaction::Relative,
    )]);
    assert_eq!(
        s.map(&hid("Left.Jog.Turn", -300.0, None)),
        [(Action::JogTurn { deck: 0 }, Trigger::Delta(-300.0))]
    );
    assert_eq!(s.map(&hid("Left.Jog.Turn", 0.0, None)), []);
}

#[test]
fn learn_from_hid_controls() {
    let learn = Learn::new;
    let fader = learn(Action::Volume { channel: 2 })
        .accept(&hid("Mixer.C.Volume", 0.3, None))
        .unwrap();
    assert_eq!(fader.interaction, Interaction::Direct);
    let encoder = learn(Action::BrowserUpDown)
        .accept(&hid("Left.Browse.Encoder.Turn", 1.0, None))
        .unwrap();
    assert_eq!(encoder.interaction, Interaction::Relative);
    let mut button = learn(Action::Sync { deck: 0 });
    assert!(button.accept(&hid("Left.Sync", 0.0, Some(false))).is_none());
    assert!(
        button.accept(&hid("Mixer.A.Volume", 0.5, None)).is_none(),
        "faders do not bind to buttons"
    );
    assert_eq!(
        button
            .accept(&hid("Left.Sync", 1.0, Some(true)))
            .unwrap()
            .interaction,
        Interaction::Toggle
    );
}

#[test]
fn deck_actions_expose_their_deck() {
    let mut deck_actions = [
        Action::PlayPause { deck: 1 },
        Action::Hotcue { deck: 1, slot: 3 },
        Action::BeatJump {
            deck: 1,
            beats: 4.0,
        },
        Action::JogTurn { deck: 1 },
        Action::LoadSelected { deck: 1 },
    ];
    for a in &mut deck_actions {
        assert_eq!(a.deck_mut().copied(), Some(1), "{a:?}");
    }
    let mut others = [
        Action::Volume { channel: 1 },
        Action::FxOn { unit: 1 },
        Action::FxAssign {
            channel: 1,
            unit: 1,
        },
        Action::Crossfader,
    ];
    for a in &mut others {
        let label = format!("{a:?}");
        assert_eq!(a.deck_mut(), None, "{label}");
    }
}

#[test]
fn new_actions_have_sensible_defaults() {
    assert_eq!(
        Action::JogTouch { deck: 0 }.default_interaction(),
        Interaction::Hold
    );
    assert_eq!(
        Action::JogTurn { deck: 0 }.default_interaction(),
        Interaction::Relative
    );
    assert_eq!(
        Action::FxAssign {
            channel: 0,
            unit: 1
        }
        .default_interaction(),
        Interaction::Toggle
    );
    assert!(Action::XfaderCurve.is_continuous());
    assert_eq!(
        Action::FxAssign {
            channel: 2,
            unit: 1
        }
        .label(),
        "Ch C → FX2"
    );
    assert_eq!(Input::Hid("Left.Play".into()).to_string(), "HID Left.Play");
}

#[test]
fn unreadable_mappings_are_skipped_not_fatal() {
    let set = MappingSet {
        mappings: MappingSet::default_keyboard(),
        midi_inputs: vec!["S4".into()],
        ..MappingSet::default()
    };
    let mut value = serde_json::to_value(&set).unwrap();
    value["mappings"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"input": {"Key": "Z"}, "action": "NoSuchAction"}));
    let (loaded, skipped) = MappingSet::from_json(&value.to_string()).unwrap();
    assert_eq!(skipped, 1);
    assert_eq!(loaded, set);
    assert!(
        MappingSet::from_json("[1, 2]").is_err(),
        "not a mapping file"
    );
}

#[test]
fn override_factory_map_is_saved_only_when_set() {
    let mut set = MappingSet::default();
    let input = Input::Hid("Left.Cue".into());
    let mut m = Mapping::new(
        input.clone(),
        Action::Keylock { deck: 0 },
        Interaction::Trigger,
    );
    set.mappings.push(m.clone());
    assert!(!set.overrides(&input));
    let json = serde_json::to_string(&set).unwrap();
    assert!(!json.contains("override_factory"), "{json}");
    m.override_factory = true;
    set.mappings.push(m);
    assert!(set.overrides(&input));
    assert!(!set.overrides(&Input::Hid("Left.Play".into())));
    let json = serde_json::to_string(&set).unwrap();
    let (loaded, _) = MappingSet::from_json(&json).unwrap();
    assert_eq!(loaded, set);
}

#[test]
fn encoder_modes_and_faders_mapped_as_relative() {
    let relative = |number, action, encoder| Mapping {
        encoder,
        ..Mapping::new(cc(number, 0).input, action, Interaction::Relative)
    };
    let s = set(vec![
        relative(1, Action::JogTurn { deck: 0 }, relative::Encoder::Offset),
        relative(2, Action::FxDryWet { unit: 0 }, relative::Encoder::Absolute),
    ]);
    let mut source = relative::Source::default();
    let mut delta = |e| match s.map_source(
        &e,
        &mut source,
        &crate::feedback::Feedback::default(),
        &|_| None,
    )[..]
    {
        [(_, Trigger::Delta(d))] => d,
        ref other => panic!("{other:?}"),
    };
    assert!((delta(cc(1, 66)) - 2.0).abs() < 1e-5);
    assert!((delta(cc(1, 61)) + 3.0).abs() < 1e-5);
    assert!((delta(cc(2, 10)) - 10.0).abs() < 1e-5);
    assert!((delta(cc(2, 7)) + 3.0).abs() < 1e-5);
}

#[test]
fn stepped_relative_actions_keep_fractions() {
    let s = set(vec![Mapping {
        sensitivity: 50.0,
        ..Mapping::new(cc(3, 0).input, Action::BrowserUpDown, Interaction::Relative)
    }]);
    let mut source = relative::Source::default();
    let moves: usize = (0..8)
        .map(|_| {
            s.map_source(
                &cc(3, 1),
                &mut source,
                &crate::feedback::Feedback::default(),
                &|_| None,
            )
            .len()
        })
        .sum();
    assert_eq!(moves, 2, "a step every fourth click at 50 %");
}

#[test]
fn inverted_buttons_trigger_on_release() {
    let s = set(vec![Mapping {
        invert: true,
        ..Mapping::new(
            Input::Key("Q".into()),
            Action::Cue { deck: 0 },
            Interaction::Hold,
        )
    }]);
    assert_eq!(
        s.map(&key("Q", true)),
        [(Action::Cue { deck: 0 }, Trigger::Release)]
    );
    assert_eq!(
        s.map(&key("Q", false)),
        [(Action::Cue { deck: 0 }, Trigger::Press)]
    );
}

#[test]
fn a_cc_button_is_down_at_any_value_but_its_lowest() {
    let play = Action::PlayPause { deck: 0 };
    let s = set(vec![Mapping::new(
        cc(9, 0).input,
        play,
        Interaction::Trigger,
    )]);
    assert_eq!(s.map(&cc(9, 10)), [(play, Trigger::Press)]);
    assert_eq!(s.map(&cc(9, 127)), [(play, Trigger::Press)]);
    assert_eq!(s.map(&cc(9, 0)), Vec::new());
    let mut m = Mapping::new(cc(9, 0).input, play, Interaction::Trigger);
    m.invert = true;
    let s = set(vec![m]);
    assert_eq!(s.map(&cc(9, 0)), [(play, Trigger::Press)]);
    assert_eq!(s.map(&cc(9, 100)), [(play, Trigger::Press)]);
    assert_eq!(s.map(&cc(9, 127)), Vec::new());
}

#[test]
fn inc_dec_on_a_cc_button_steps_once_per_press() {
    let mut m = Mapping::new(cc(8, 0).input, Action::BrowserUpDown, Interaction::Relative);
    m.encoder = relative::Encoder::Button;
    m.invert = true;
    let s = set(vec![m.clone()]);
    let up = [(Action::BrowserUpDown, Trigger::Delta(-1.0))];
    assert_eq!(s.map(&cc(8, 127)), up);
    assert_eq!(s.map(&cc(8, 0)), Vec::new());
    m.on_release = true;
    let s = set(vec![m]);
    assert_eq!(s.map(&cc(8, 127)), Vec::new());
    assert_eq!(s.map(&cc(8, 0)), up);
}

#[test]
fn inc_dec_keys_can_step_on_release() {
    let mut m = Mapping::new(
        Input::Key("Q".into()),
        Action::BrowserUpDown,
        Interaction::Relative,
    );
    m.on_release = true;
    let s = set(vec![m]);
    assert_eq!(s.map(&key("Q", true)), Vec::new());
    assert_eq!(
        s.map(&key("Q", false)),
        [(Action::BrowserUpDown, Trigger::Delta(1.0))]
    );
}

#[test]
fn mappings_tied_to_a_port_ignore_the_others() {
    let play = |deck| Action::PlayPause { deck };
    let mut a = Mapping::new(cc(9, 0).input, play(0), Interaction::Trigger);
    a.port = Some("DDJ-FLX4".into());
    let any = Mapping::new(cc(9, 0).input, play(1), Interaction::Trigger);
    let mut s = set(vec![a.clone(), any]);
    let shown = crate::feedback::Feedback::default();
    let on = |s: &MappingSet, port: &str| {
        let mut source = relative::Source {
            port: Some(port.into()),
            ..relative::Source::default()
        };
        s.map_source(&cc(9, 127), &mut source, &shown, &|_| None)
    };
    assert_eq!(
        on(&s, "DDJ-FLX4"),
        [(play(0), Trigger::Press), (play(1), Trigger::Press)]
    );
    assert_eq!(on(&s, "Other"), [(play(1), Trigger::Press)]);
    let mut b = a.clone();
    b.port = Some("Other".into());
    s.upsert(b);
    s.upsert(a);
    assert_eq!(s.mappings.len(), 3);
}

#[test]
fn soft_takeover_waits_for_the_fader_to_reach_the_value() {
    let s = set(vec![Mapping {
        soft_takeover: true,
        ..Mapping::new(
            cc(7, 0).input,
            Action::Volume { channel: 0 },
            Interaction::Direct,
        )
    }]);
    let mut source = relative::Source::default();
    let at = |v: f32| move |_: Action| Some(v);
    assert_eq!(
        s.map_source(
            &cc(7, 20),
            &mut source,
            &crate::feedback::Feedback::default(),
            &at(0.8)
        ),
        []
    );
    assert_eq!(
        s.map_source(
            &cc(7, 60),
            &mut source,
            &crate::feedback::Feedback::default(),
            &at(0.8)
        ),
        []
    );
    let caught = s.map_source(
        &cc(7, 105),
        &mut source,
        &crate::feedback::Feedback::default(),
        &at(0.8),
    );
    assert!(matches!(caught[..], [(_, Trigger::Set(v))] if (v - 105.0 / 127.0).abs() < 1e-6));
    assert_eq!(
        s.map_source(
            &cc(7, 10),
            &mut source,
            &crate::feedback::Feedback::default(),
            &|_| None
        )
        .len(),
        1
    );
}

#[test]
fn held_inc_buttons_repeat_from_the_fourth_tick() {
    let s = set(vec![Mapping {
        auto_repeat: true,
        ..Mapping::new(
            Input::Key("Down".into()),
            Action::BrowserUpDown,
            Interaction::Relative,
        )
    }]);
    let mut source = relative::Source::default();
    let step = [(Action::BrowserUpDown, Trigger::Delta(1.0))];
    assert_eq!(
        s.map_source(
            &key("Down", true),
            &mut source,
            &crate::feedback::Feedback::default(),
            &|_| None
        ),
        step
    );
    let ticks: Vec<usize> = (0..6)
        .map(|_| {
            s.repeat_tick(&mut source, &crate::feedback::Feedback::default())
                .len()
        })
        .collect();
    assert_eq!(ticks, [0, 0, 0, 1, 1, 1]);
    assert!(source.repeating());
    s.map_source(
        &key("Down", false),
        &mut source,
        &crate::feedback::Feedback::default(),
        &|_| None,
    );
    assert_eq!(
        s.repeat_tick(&mut source, &crate::feedback::Feedback::default()),
        []
    );
    assert!(!source.repeating());
}

#[test]
fn reset_mappings_send_reset_on_press() {
    let s = set(vec![Mapping::new(
        Input::Key("R".into()),
        Action::Volume { channel: 0 },
        Interaction::Reset,
    )]);
    assert_eq!(
        s.map(&key("R", true)),
        [(Action::Volume { channel: 0 }, Trigger::Reset)]
    );
    assert_eq!(s.map(&key("R", false)), []);
    let mut m = modifier::Modifiers([5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    m.apply(0, Interaction::Reset, None, Trigger::Reset);
    assert_eq!(m.0[0], 0);
}

#[test]
fn focus_mappings_drive_the_focused_deck() {
    let mut play = Mapping::new(
        Input::Key("P".into()),
        Action::PlayPause { deck: 0 },
        Interaction::Trigger,
    );
    play.focus = true;
    let mut volume = Mapping::new(
        Input::Key("V".into()),
        Action::Volume { channel: 0 },
        Interaction::Direct,
    );
    volume.focus = true;
    let s = set(vec![play, volume]);
    let shown = feedback::Feedback {
        focus: 2,
        ..feedback::Feedback::default()
    };
    let mut source = relative::Source::default();
    let mut map = |e| s.map_source(&e, &mut source, &shown, &|_| None);
    assert_eq!(
        map(key("P", true)),
        [(Action::PlayPause { deck: 2 }, Trigger::Press)]
    );
    assert_eq!(
        map(key("V", true)),
        [(Action::Volume { channel: 2 }, Trigger::Set(1.0))]
    );
    assert!(!Action::Crossfader.has_deck());
    assert_eq!(Action::Crossfader.on_deck(3), Action::Crossfader);
}

#[test]
fn state_conditions_gate_mappings() {
    let mut pad = Mapping::new(
        Input::Key("H".into()),
        Action::Hotcue { deck: 0, slot: 0 },
        Interaction::Hold,
    );
    pad.gate.states = vec![state::StateCondition {
        control: state::Control::Playing,
        channel: Some(0),
        value: 1,
    }];
    let s = set(vec![pad]);
    let mut shown = feedback::Feedback::default();
    let mut source = relative::Source::default();
    assert_eq!(
        s.map_source(&key("H", true), &mut source, &shown, &|_| None),
        [],
        "deck A stopped"
    );
    shown.decks[0].playing = true;
    assert_eq!(
        s.map_source(&key("H", true), &mut source, &shown, &|_| None),
        [(Action::Hotcue { deck: 0, slot: 0 }, Trigger::Press)]
    );
}

#[test]
fn a_pair_with_a_10_bit_range_reaches_the_top_at_1023() {
    let id = |number| MidiId {
        channel: 0,
        kind: MidiKind::Cc,
        number,
    };
    let pair = Input::MidiPair {
        lsb: id(39),
        msb: id(7),
    };
    let mut s = set(vec![Mapping::new(
        pair,
        Action::Volume { channel: 0 },
        Interaction::Direct,
    )]);
    let range = PairRange {
        lsb: id(39),
        msb: id(7),
        min: 0.0,
        max: 1023.0,
    };
    s.set_pair_range(range);
    s.set_pair_range(range);
    assert_eq!(s.pair_ranges, [range]);
    let mut source = relative::Source::default();
    let shown = crate::feedback::Feedback::default();
    s.map_source(&cc(7, 7), &mut source, &shown, &|_| None);
    match s.map_source(&cc(39, 127), &mut source, &shown, &|_| None)[..] {
        [(_, Trigger::Set(v))] => assert!((v - 1.0).abs() < 1e-6, "{v}"),
        ref other => panic!("{other:?}"),
    }
    s.set_pair_range(PairRange {
        max: 16_383.0,
        ..range
    });
    assert_eq!(s.pair_ranges, Vec::new());
}

#[test]
fn a_14_bit_pair_sends_its_value_when_the_low_half_completes_it() {
    let id = |number| MidiId {
        channel: 0,
        kind: MidiKind::Cc,
        number,
    };
    let pair = Input::MidiPair {
        lsb: id(39),
        msb: id(7),
    };
    let s = set(vec![
        Mapping::new(pair, Action::Volume { channel: 0 }, Interaction::Direct),
        Mapping::new(
            cc(20, 0).input,
            Action::Volume { channel: 1 },
            Interaction::Direct,
        ),
    ]);
    let mut source = relative::Source::default();
    let mut map = |e: InputEvent| {
        s.map_source(
            &e,
            &mut source,
            &crate::feedback::Feedback::default(),
            &|_| None,
        )
    };
    assert_eq!(map(cc(7, 64)), Vec::new());
    match map(cc(39, 1))[..] {
        [(Action::Volume { channel: 0 }, Trigger::Set(v))] => {
            assert!((v - 8193.0 / 16_383.0).abs() < 1e-6, "{v}");
        }
        ref other => panic!("{other:?}"),
    }
    assert_eq!(map(cc(7, 127)), Vec::new());
    match map(cc(39, 127))[..] {
        [(_, Trigger::Set(v))] => assert!((v - 1.0).abs() < 1e-6, "{v}"),
        ref other => panic!("{other:?}"),
    }
    assert_eq!(map(cc(7, 10)), Vec::new());
    assert_eq!(map(cc(20, 127)).len(), 1);
    assert_eq!(map(cc(39, 5)), Vec::new());
}

#[test]
fn sample_deck_outputs_show_slot_and_cell_states() {
    use feedback::{SampleFeedback, SampleSlotFeedback};
    let mut f = feedback::Feedback::default();
    let value = |a, f: &feedback::Feedback| output::value(a, f, &modifier::Modifiers::default());
    let state = Action::SampleSlot {
        deck: 1,
        slot: 2,
        part: SampleSlotPart::State,
    };
    assert_eq!(value(state, &f), None, "a track deck shows nothing");
    let mut sample_deck = SampleFeedback::default();
    sample_deck.slots[2] = SampleSlotFeedback {
        state: 2,
        keylock: true,
        one_shot: true,
        ..SampleSlotFeedback::default()
    };
    sample_deck.slots[2].cells[5] = 3;
    sample_deck.quantize_index = 4;
    f.decks[1].sample_deck = Some(sample_deck);
    assert_eq!(
        value(state, &f),
        Some(output::OutValue::Number {
            value: 2.0,
            min: 0.0,
            max: 2.0
        })
    );
    assert_eq!(
        value(
            Action::SampleCell {
                deck: 1,
                slot: 2,
                cell: 5
            },
            &f
        ),
        Some(output::OutValue::Number {
            value: 3.0,
            min: 0.0,
            max: 2.0
        })
    );
    assert_eq!(
        value(
            Action::SampleSlot {
                deck: 1,
                slot: 2,
                part: SampleSlotPart::PlayMode,
            },
            &f
        ),
        Some(output::OutValue::OnOff(false)),
        "one-shot: Play Mode 0"
    );
    assert_eq!(
        value(
            Action::SampleDeck {
                deck: 1,
                part: SamplePart::QuantizeSelector,
            },
            &f
        ),
        Some(output::OutValue::Number {
            value: 4.0,
            min: 0.0,
            max: 7.0
        })
    );
    assert_eq!(
        state::StateCondition {
            control: state::Control::DeckType,
            channel: Some(1),
            value: 1,
        }
        .current(&f),
        Some(1)
    );
    assert!(output::supported(Action::DeckType { deck: 0 }));
}

#[test]
fn mappings_saved_with_the_old_action_names_still_load() {
    let old = r#"[{"action":"RemixCell","deck":2,"slot":0,"cell":1},
                  {"action":"Flux","deck":0},
                  {"action":"DeckFlavor","deck":3},
                  {"action":"Remix","deck":1,"part":"QuantizeOn"}]"#;
    let actions: Vec<Action> = serde_json::from_str(old).unwrap();
    assert_eq!(
        actions,
        [
            Action::SampleCell {
                deck: 2,
                slot: 0,
                cell: 1
            },
            Action::Slip { deck: 0 },
            Action::DeckType { deck: 3 },
            Action::SampleDeck {
                deck: 1,
                part: crate::SamplePart::QuantizeOn
            },
        ]
    );
}
