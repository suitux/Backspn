use std::sync::LazyLock;

use super::{analog, button, encoder, map};
use crate::feedback::Feedback;
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Layout, Leds, Shifted, colour};
use crate::{Action, Interaction, Mapping};

pub const NAME: &str = "Kontrol X1 MK3";
pub const SOURCE: &str = "X1 MK3";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x2200;

const LED_LEN: usize = 49;
const SINGLE: Option<(u8, u8)> = Some((0x7E, 0x7C));
const LOOP_LED: [usize; 2] = [1, 2];
const BOTTOM_LEDS: [[usize; 6]; 2] = [[43, 44, 45, 46, 47, 48], [41, 40, 39, 38, 37, 36]];
const NUDGE_BEATS: f64 = 32.0;
/// Switches the device from MIDI to HID control on connect.
const APP_MODE: &[u8] = &[0xA0, 0x00, 0xFF];

/// Buttons are numbered as bits of the report from its first data byte;
/// most have their own colour LED two places further on.
fn at(index: usize) -> (usize, u8) {
    (1 + index / 8, 1 << (index % 8))
}

fn b(name: &str, index: usize) -> Button {
    let (byte, mask) = at(index);
    button(name, byte, mask, Some(index + 2), None)
}

fn plain(name: &str, index: usize) -> Button {
    let (byte, mask) = at(index);
    button(name, byte, mask, None, None)
}

fn buttons() -> Vec<Button> {
    let (byte, mask) = at(0);
    let mut v = vec![
        button("Left.Shift", byte, mask, Some(0), SINGLE),
        plain("Mode", 31),
    ];
    for (side, first) in [("Left", 0usize), ("Right", 1)] {
        let n = |c: &str| format!("{side}.{c}");
        let pair = 2 * first;
        v.extend([
            b(&n("Play"), 1 + pair),
            b(&n("Sync"), 2 + pair),
            b(&n("Cue"), 5 + pair),
            b(&n("Transport Mode.Reverse"), 6 + pair),
            b(&n("Nudge.Slow"), 9 + pair),
            b(&n("Nudge.Fast"), 10 + pair),
            b(&n("Pads.1"), 17 + pair),
            b(&n("Pads.2"), 18 + pair),
            b(&n("Pads.3"), 13 + pair),
            b(&n("Pads.4"), 14 + pair),
            b(&n("FX.Button 1"), 27 + first),
            b(&n("FX.Button 2"), 25 + first),
            b(&n("FX.Button 3"), 23 + first),
            b(&n("FX.Button 4"), 21 + first),
            b(&n("Assign.Left"), 29 + 3 * first),
            b(&n("Assign.Right"), 30 + 3 * first),
            plain(&n("Loop.Push"), 34 + first),
            plain(&n("Browse.Encoder.Push"), 36 + first),
        ]);
    }
    v
}

const ANALOGS: &[Analog] = &[
    analog("Left.FX.Knob 1", 0x15),
    analog("Left.FX.Knob 2", 0x11),
    analog("Left.FX.Knob 3", 0x0D),
    analog("Left.FX.Knob 4", 0x09),
    analog("Right.FX.Knob 1", 0x17),
    analog("Right.FX.Knob 2", 0x13),
    analog("Right.FX.Knob 3", 0x0F),
    analog("Right.FX.Knob 4", 0x0B),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Loop.Turn", 0x07, false),
    encoder("Right.Loop.Turn", 0x07, true),
    encoder("Left.Browse.Encoder.Turn", 0x08, false),
    encoder("Right.Browse.Encoder.Turn", 0x08, true),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 64)];

fn deck_leds(feedback: &Feedback, decoder: &Decoder, layer: [usize; 2], leds: &mut Leds) {
    for (i, side) in ["Left", "Right"].into_iter().enumerate() {
        let deck = &feedback.decks[layer[i]];
        let channel = &feedback.channels[layer[i]];
        let n = |c: &str| format!("{side}.{c}");
        for (k, button) in ["Assign.Left", "Assign.Right"].into_iter().enumerate() {
            leds.set(decoder, &n(button), colour::ORANGE, channel.fx_assign[k]);
        }
        for button in ["Nudge.Slow", "Nudge.Fast"] {
            leds.set(decoder, &n(button), colour::WHITE, false);
        }
        leds.0[LOOP_LED[i]] = if deck.loop_active { 0x7E } else { 0x7C };
        let strip = if deck.loop_active {
            colour::GREEN + colour::BRIGHT
        } else if deck.playing && deck.end_warning {
            colour::RED + colour::BRIGHT
        } else {
            colour::OFF
        };
        for &led in &BOTTOM_LEDS[i] {
            leds.0[led] = strip;
        }
    }
}

fn default_mappings() -> Vec<Mapping> {
    let mut extra = Vec::new();
    for (deck, side) in [(0usize, "Left"), (1, "Right")] {
        for (button, direction) in [("Slow", -1.0), ("Fast", 1.0)] {
            extra.push(map(
                &format!("{side}.Nudge.{button}"),
                Action::TempoBend { deck, direction },
                Interaction::Hold,
            ));
        }
        for (unit, button) in ["Left", "Right"].into_iter().enumerate() {
            extra.push(map(
                &format!("{side}.Assign.{button}"),
                Action::FxAssign {
                    channel: deck,
                    unit,
                },
                Interaction::Toggle,
            ));
        }
    }
    super::default_mappings(&LAYOUT, extra)
}

fn shifted(name: &str) -> Shifted {
    let (side, control) = name.split_once('.')?;
    let deck = match side {
        "Left" => 0,
        "Right" => 1,
        _ => return None,
    };
    let fx_slot = |slot| (Action::FxSelect { unit: deck, slot }, Interaction::Relative);
    Some(Some(match control {
        "Play" => (Action::BeatTap { deck }, Interaction::Trigger),
        "Sync" => (Action::Master { deck }, Interaction::Trigger),
        "Transport Mode.Reverse" => (Action::Slip { deck }, Interaction::Toggle),
        "Nudge.Slow" => (
            Action::BeatJump {
                deck,
                beats: -NUDGE_BEATS,
            },
            Interaction::Trigger,
        ),
        "Nudge.Fast" => (
            Action::BeatJump {
                deck,
                beats: NUDGE_BEATS,
            },
            Interaction::Trigger,
        ),
        "Loop.Turn" => (Action::LoopMove { deck }, Interaction::Relative),
        "FX.Button 1" => (Action::FxMode { unit: deck }, Interaction::Relative),
        "FX.Button 2" => fx_slot(0),
        "FX.Button 3" => fx_slot(1),
        "FX.Button 4" => fx_slot(2),
        "Assign.Left" | "Assign.Right" => {
            let unit = if control == "Assign.Left" { 2 } else { 3 };
            (
                Action::FxAssign {
                    channel: deck,
                    unit,
                },
                Interaction::Toggle,
            )
        }
        _ => return super::common_shifted(name),
    }))
}

pub static LAYOUT: LazyLock<Layout> = LazyLock::new(|| Layout {
    name: NAME,
    source: SOURCE,
    vendor_id: VENDOR_ID,
    product_id: PRODUCT_ID,
    buttons: buttons(),
    switches: &[],
    analogs: ANALOGS,
    encoders: ENCODERS,
    jogs: &[],
    jog_steps_per_turn: 1.0,
    analog_report: 0x01,
    jog_report: 0x03,
    encoder_report: 0x01,
    led_reports: &[(0x80, LED_LEN)],
    meter_report: None,
    initial_reports: INITIAL_REPORTS,
    motors: false,
    sound_card: None,
    calibration: &[],
    stored_calibration: None,
    rings: false,
    screens: false,
    tempo_leds: None,
    wheel_leds: None,
    strip_leds: None,
    meters: None,
    meter_leds: None,
    deck_leds: Some(deck_leds),
    default_mappings,
    shifted,
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: false,
    samples_button_toggles: false,
    fx_select_toggles: false,
    mx2_transport: false,
    shared_shift: true,
    connect_reports: &[APP_MODE],
});

pub fn start(
    events: crate::controller::EventSender,
    wake: crate::controller::Waker,
) -> Result<Box<dyn crate::controller::Controller>, String> {
    crate::s4mk3::driver::start_with(&LAYOUT, events, wake)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Input;

    fn find(name: &str) -> Option<Action> {
        default_mappings()
            .into_iter()
            .find(|m| m.input == Input::Hid(name.into()))
            .map(|m| m.action)
    }

    #[test]
    fn layout_is_consistent() {
        crate::s4mk3::tests::check_layout(&LAYOUT);
        assert_eq!(LAYOUT.buttons.len(), 38);
        let leds: Vec<usize> = LAYOUT.buttons.iter().filter_map(|b| b.led).collect();
        for led in LOOP_LED.iter().chain(BOTTOM_LEDS.iter().flatten()) {
            assert!(*led < LED_LEN);
            assert!(!leds.contains(led), "LED {led} shared");
        }
    }

    #[test]
    fn buttons_follow_their_bit() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 25];
        r[0] = 0x01;
        assert!(d.decode(&r).iter().all(|e| e.pressed.is_none()));
        r[3] = 0x02;
        let events: Vec<_> = d
            .decode(&r)
            .into_iter()
            .filter(|e| e.pressed.is_some())
            .collect();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].input, Input::Hid("Left.Pads.1".into()));
        assert_eq!(d.led("Left.Pads.1"), Some(19));
        assert_eq!(d.led("Right.Assign.Right"), Some(35));
    }

    #[test]
    fn knobs_read_twelve_bits() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 25];
        r[0] = 0x01;
        r[0x15] = 0xFF;
        r[0x16] = 0x0F;
        let events = d.decode(&r);
        let v = events
            .iter()
            .find(|e| e.input == Input::Hid("Left.FX.Knob 1".into()))
            .expect("dry/wet");
        assert!((v.value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn each_side_drives_its_deck_and_fx_unit() {
        assert_eq!(find("Right.Play"), Some(Action::PlayPause { deck: 1 }));
        assert_eq!(
            find("Left.Pads.3"),
            Some(Action::Hotcue { deck: 0, slot: 2 })
        );
        assert_eq!(find("Left.FX.Knob 1"), Some(Action::FxDryWet { unit: 0 }));
        assert_eq!(
            find("Right.FX.Button 4"),
            Some(Action::FxButton { unit: 1, button: 2 })
        );
        assert_eq!(
            find("Left.Nudge.Slow"),
            Some(Action::TempoBend {
                deck: 0,
                direction: -1.0
            })
        );
        assert_eq!(
            find("Right.Assign.Left"),
            Some(Action::FxAssign {
                channel: 1,
                unit: 0
            })
        );
        assert_eq!(
            find("Left.Loop.Turn"),
            Some(Action::LoopSizeStep { deck: 0 })
        );
        assert_eq!(find("Mode"), None);
    }

    #[test]
    fn shift_reaches_the_second_layer() {
        assert_eq!(
            shifted("Right.Nudge.Fast"),
            Some(Some((
                Action::BeatJump {
                    deck: 1,
                    beats: NUDGE_BEATS
                },
                Interaction::Trigger
            )))
        );
        assert_eq!(
            shifted("Left.Pads.2"),
            Some(Some((
                Action::DeleteHotcue { deck: 0, slot: 1 },
                Interaction::Trigger
            )))
        );
        assert_eq!(
            shifted("Left.Loop.Turn"),
            Some(Some((Action::LoopMove { deck: 0 }, Interaction::Relative)))
        );
    }

    #[test]
    fn bottom_strip_shows_the_loop() {
        let mut f = Feedback::default();
        f.decks[1].loop_active = true;
        f.channels[0].fx_assign[1] = true;
        let d = Decoder::new(&LAYOUT);
        let mut leds = Leds::for_layout(&LAYOUT);
        deck_leds(&f, &d, [0, 1], &mut leds);
        assert!(
            BOTTOM_LEDS[1]
                .iter()
                .all(|&i| leds.0[i] == colour::GREEN + colour::BRIGHT)
        );
        assert!(BOTTOM_LEDS[0].iter().all(|&i| leds.0[i] == colour::OFF));
        assert_eq!(leds.0[LOOP_LED[1]], 0x7E);
        let assign = d.led("Left.Assign.Right").expect("led");
        assert_eq!(leds.0[assign], colour::ORANGE + colour::BRIGHT);
    }
}
