use std::sync::LazyLock;

use super::{analog, button, encoder, jog, linear_meter, map};
use crate::feedback::Feedback;
use crate::modifier::Condition;
use crate::poti::StoredCalibration;
use crate::relative::Encoder as EncoderMode;
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Jog, Layout, Leds, Shifted};
use crate::{Action, Interaction, Mapping, MoveMode};

pub const NAME: &str = "Kontrol S2 MK2";
pub const SOURCE: &str = "S2 MK2";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1320;

const LED_REPORTS: &[(u8, usize)] = &[(0x80, 38), (0x81, 33)];
const R80: usize = 0;
const R81: usize = 38;
const SINGLE: Option<(u8, u8)> = Some((0x7F, 0x01));
const METER_ON: u8 = 0x7F;
const JOG_STEPS: f64 = 1024.0;

const fn at(base: usize, offset: usize) -> usize {
    base + offset - 1
}

fn b(name: impl Into<String>, byte: usize, mask: u8, led: usize) -> Button {
    button(name, byte, mask, Some(led), SINGLE)
}

fn plain(name: impl Into<String>, byte: usize, mask: u8) -> Button {
    button(name, byte, mask, None, None)
}

fn pad(name: impl Into<String>, byte: usize, mask: u8, rgb: usize) -> Button {
    Button {
        rgb: Some(at(R81, rgb)),
        ..button(name, byte, mask, None, None)
    }
}

fn led_only(name: &str, bit: u8, led: usize) -> Button {
    Button {
        name: name.to_owned(),
        byte: 0,
        bit,
        led: Some(led),
        single: SINGLE,
        rgb: None,
    }
}

#[allow(clippy::too_many_lines)]
fn buttons() -> Vec<Button> {
    let mut v = vec![
        b("Left.Shift", 0x0B, 0x08, at(R81, 0x19)),
        b("Left.Sync", 0x0B, 0x04, at(R81, 0x1A)),
        b("Left.Cue", 0x0B, 0x02, at(R81, 0x1B)),
        b("Left.Play", 0x0B, 0x01, at(R81, 0x1C)),
        b("Left.Loop.In", 0x0C, 0x40, at(R80, 0x21)),
        b("Left.Loop.Out", 0x0C, 0x80, at(R80, 0x22)),
        b("Left.Pad Mode.Samples", 0x0C, 0x02, at(R80, 0x17)),
        b("Left.Transport Mode.Flux", 0x0C, 0x20, at(R80, 0x20)),
        plain("Left.Browse.Load", 0x0C, 0x08),
        b("Mixer.A.Cue", 0x0C, 0x10, at(R80, 0x1B)),
        plain("Left.Loop.Push", 0x0F, 0x01),
        plain("Left.Move.Push", 0x0F, 0x02),
        plain("Mixer.A.Gain Push", 0x0D, 0x40),
        plain("Left.Jog.Touch", 0x0A, 0x01),
        b("Right.Shift", 0x09, 0x08, at(R81, 0x1D)),
        b("Right.Sync", 0x09, 0x04, at(R81, 0x1E)),
        b("Right.Cue", 0x09, 0x02, at(R81, 0x1F)),
        b("Right.Play", 0x09, 0x01, at(R81, 0x20)),
        b("Right.Loop.In", 0x0A, 0x40, at(R80, 0x23)),
        b("Right.Loop.Out", 0x0A, 0x80, at(R80, 0x24)),
        b("Right.Pad Mode.Samples", 0x0C, 0x01, at(R80, 0x18)),
        b("Right.Transport Mode.Flux", 0x0A, 0x20, at(R80, 0x25)),
        plain("Right.Browse.Load", 0x0C, 0x04),
        b("Mixer.B.Cue", 0x0A, 0x10, at(R80, 0x1F)),
        plain("Right.Loop.Push", 0x0F, 0x08),
        plain("Right.Move.Push", 0x0F, 0x10),
        plain("Mixer.B.Gain Push", 0x0D, 0x80),
        plain("Right.Jog.Touch", 0x0A, 0x02),
        b("Left.FX.Button 1", 0x0E, 0x10, at(R80, 0x0B)),
        b("Left.FX.Button 2", 0x0E, 0x80, at(R80, 0x0C)),
        b("Left.FX.Button 3", 0x0E, 0x40, at(R80, 0x0D)),
        b("Left.FX.Button 4", 0x0E, 0x20, at(R80, 0x0E)),
        b("Mixer.A.FX Assign 1", 0x0E, 0x08, at(R80, 0x0F)),
        b("Mixer.B.FX Assign 1", 0x0E, 0x02, at(R80, 0x11)),
        b("Right.FX.Button 1", 0x0D, 0x04, at(R80, 0x13)),
        b("Right.FX.Button 2", 0x0D, 0x20, at(R80, 0x14)),
        b("Right.FX.Button 3", 0x0D, 0x10, at(R80, 0x15)),
        b("Right.FX.Button 4", 0x0D, 0x08, at(R80, 0x16)),
        b("Mixer.A.FX Assign 2", 0x0E, 0x04, at(R80, 0x10)),
        b("Mixer.B.FX Assign 2", 0x0E, 0x01, at(R80, 0x12)),
        plain("Mixer.Browse.View", 0x0F, 0x04),
        plain("Mixer.Mic", 0x0A, 0x08),
        led_only("Mixer.A.Loaded", 1, at(R80, 0x19)),
        led_only("Mixer.B.Loaded", 2, at(R80, 0x1A)),
    ];
    for (side, byte, rgb) in [("Left", 0x0B, 0x01), ("Right", 0x09, 0x0D)] {
        for (i, mask) in [0x80u8, 0x40, 0x20, 0x10].into_iter().enumerate() {
            v.push(pad(
                format!("{side}.Pads.{}", i + 1),
                byte,
                mask,
                rgb + 3 * i,
            ));
        }
    }
    v
}

const ANALOGS: &[Analog] = &[
    analog("Left.Tempo", 0x07),
    analog("Right.Tempo", 0x09),
    analog("Mixer.Xfader", 0x05),
    analog("Mixer.Cue Mix", 0x0B),
    analog("Mixer.Samples", 0x0D),
    analog("Mixer.Master", 0x11),
    analog("Mixer.A.Volume", 0x13),
    analog("Mixer.B.Volume", 0x15),
    analog("Left.FX.Knob 1", 0x17),
    analog("Left.FX.Knob 2", 0x19),
    analog("Left.FX.Knob 3", 0x1B),
    analog("Left.FX.Knob 4", 0x1D),
    analog("Right.FX.Knob 1", 0x1F),
    analog("Right.FX.Knob 2", 0x21),
    analog("Right.FX.Knob 3", 0x23),
    analog("Right.FX.Knob 4", 0x25),
    analog("Mixer.A.EQ.High", 0x27),
    analog("Mixer.A.EQ.Mid", 0x29),
    analog("Mixer.A.EQ.Low", 0x2B),
    analog("Mixer.B.EQ.High", 0x2D),
    analog("Mixer.B.EQ.Mid", 0x2F),
    analog("Mixer.B.EQ.Low", 0x31),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Loop.Turn", 0x01, false),
    encoder("Left.Move.Turn", 0x01, true),
    encoder("Mixer.Browse.Turn", 0x02, false),
    encoder("Right.Loop.Turn", 0x02, true),
    encoder("Right.Move.Turn", 0x03, false),
    encoder("Mixer.A.Gain", 0x03, true),
    encoder("Mixer.B.Gain", 0x04, false),
];

const JOGS: &[Jog] = &[
    jog("Left.Jog.Turn", 0x01, 8),
    jog("Right.Jog.Turn", 0x05, 8),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 32), (0x02, 64)];

static CALIBRATION: StoredCalibration = StoredCalibration {
    reports: &[0xD0, 0xD1, 0xD2, 0xD3],
    product_id: PRODUCT_ID,
    first_entry: 0x0C,
    with_centre: &[9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 4, 5],
    faders: &[1, 2, 7, 8, 0],
    no_centre: &[1, 2, 7, 8, 0, 5],
    first_analog: 0x05,
};

const METER_FIRST: [usize; 2] = [0x01, 0x06];
const METER_SEGMENTS: usize = 4;

fn meter_leds(feedback: &Feedback, _decoder: &Decoder, leds: &mut Leds) {
    for (ch, &first) in METER_FIRST.iter().enumerate() {
        linear_meter(
            &mut leds.0,
            at(R80, first),
            METER_SEGMENTS,
            feedback.channels[ch].level,
            METER_ON,
        );
    }
}

const LOOP_IN_HELD: usize = 5;
const LOOP_OUT_HELD: usize = 6;

fn default_mappings() -> Vec<Mapping> {
    let mut v: Vec<Mapping> = super::default_mappings(&LAYOUT, super::side_extras(&LAYOUT))
        .into_iter()
        .filter(|m| !matches!(m.action, Action::Gain { .. }))
        .map(|mut m| {
            if matches!(m.action, Action::JogTurn { .. } | Action::JogTouch { .. }) {
                m.gate.conditions = [LOOP_IN_HELD, LOOP_OUT_HELD]
                    .map(|modifier| Condition { modifier, value: 0 })
                    .to_vec();
            }
            m
        })
        .collect();
    for (channel, ch) in ["A", "B"].into_iter().enumerate() {
        let mut gain = map(
            &format!("Mixer.{ch}.Gain"),
            Action::Gain { channel },
            Interaction::Relative,
        );
        gain.encoder = EncoderMode::TwosComplement;
        v.push(gain);
        v.push(map(
            &format!("Mixer.{ch}.Gain Push"),
            Action::Gain { channel },
            Interaction::Reset,
        ));
    }
    for (deck, side) in [(0usize, "Left"), (1, "Right")] {
        v.push(map(
            &format!("{side}.Loop.In"),
            Action::LoopIn { deck },
            Interaction::Trigger,
        ));
        v.push(map(
            &format!("{side}.Loop.Out"),
            Action::LoopOut { deck },
            Interaction::Trigger,
        ));
        v.push(map(
            &format!("{side}.Move.Push"),
            Action::LoopActive { deck },
            Interaction::Toggle,
        ));
        for (button, index) in [("Loop.In", LOOP_IN_HELD), ("Loop.Out", LOOP_OUT_HELD)] {
            let mut held = map(
                &format!("{side}.{button}"),
                Action::Modifier { index },
                Interaction::Hold,
            );
            held.gate.value = Some(1);
            v.push(held);
        }
    }
    for channel in [2, 3] {
        v.push(map(
            "Mixer.Samples",
            Action::Volume { channel },
            Interaction::Direct,
        ));
    }
    v.push(map(
        "Mixer.Browse.Turn",
        Action::BrowserUpDown,
        Interaction::Relative,
    ));
    v.push(map(
        "Mixer.Browse.View",
        Action::BrowserView,
        Interaction::Trigger,
    ));
    v
}

fn shifted(name: &str) -> Shifted {
    let (side, control) = name.split_once('.')?;
    let deck = match side {
        "Left" => 0,
        "Right" => 1,
        "Mixer" if control == "Browse.Turn" => {
            return Some(Some((Action::BrowserTree, Interaction::Relative)));
        }
        _ => return None,
    };
    Some(Some(match control {
        "Play" => (Action::Keylock { deck }, Interaction::Toggle),
        "Sync" => (Action::Master { deck }, Interaction::Trigger),
        "Loop.In" => (
            Action::MoveMode {
                deck,
                mode: MoveMode::LoopIn,
            },
            Interaction::Trigger,
        ),
        "Loop.Out" => (
            Action::MoveMode {
                deck,
                mode: MoveMode::LoopOut,
            },
            Interaction::Trigger,
        ),
        "FX.Button 1" | "FX.Button 2" => (
            Action::FxSelect {
                unit: deck,
                slot: 0,
            },
            Interaction::Relative,
        ),
        "FX.Button 3" => (
            Action::FxSelect {
                unit: deck,
                slot: 1,
            },
            Interaction::Relative,
        ),
        "FX.Button 4" => (
            Action::FxSelect {
                unit: deck,
                slot: 2,
            },
            Interaction::Relative,
        ),
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
    erps: &[],
    jogs: JOGS,
    jog_steps_per_turn: JOG_STEPS,
    analog_report: 0x02,
    jog_report: 0x01,
    encoder_report: 0x02,
    led_reports: LED_REPORTS,
    meter_report: None,
    initial_reports: INITIAL_REPORTS,
    motors: false,
    sound_card: None,
    calibration: &[],
    stored_calibration: Some(&CALIBRATION),
    rings: false,
    screens: None,
    tempo_leds: None,
    wheel_leds: None,
    strip_leds: None,
    meters: None,
    meter_leds: Some(meter_leds),
    deck_leds: None,
    default_mappings,
    shifted,
    modifier_hooks: None,
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: false,
    samples_button_toggles: true,
    fx_select_toggles: false,
    mx2_transport: false,
    shared_shift: false,
    connect_reports: &[],
    touch_strips: &[],
    displays: None,
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

    #[test]
    fn defaults_follow_the_factory_mapping() {
        let defaults = default_mappings();
        let find = |name: &str| {
            defaults
                .iter()
                .find(|m| m.input == Input::Hid(name.into()))
                .cloned()
        };
        assert_eq!(
            find("Mixer.B.Gain").map(|m| m.interaction),
            Some(Interaction::Relative)
        );
        assert_eq!(
            find("Left.Browse.Load").map(|m| m.action),
            Some(Action::LoadSelected { deck: 0 })
        );
        assert_eq!(
            find("Right.Move.Push").map(|m| m.action),
            Some(Action::LoopActive { deck: 1 })
        );
        assert_eq!(
            defaults
                .iter()
                .filter(|m| m.input == Input::Hid("Mixer.Samples".into()))
                .count(),
            2
        );
        assert_eq!(
            shifted("Right.Sync"),
            Some(Some((Action::Master { deck: 1 }, Interaction::Trigger)))
        );
        assert_eq!(
            shifted("Left.Cue"),
            Some(Some((
                Action::ReturnToStart { deck: 0 },
                Interaction::Trigger
            )))
        );
        assert_eq!(
            shifted("Right.FX.Button 3"),
            Some(Some((
                Action::FxSelect { unit: 1, slot: 1 },
                Interaction::Relative
            )))
        );
        assert_eq!(
            shifted("Left.FX.Button 1"),
            shifted("Left.FX.Button 2"),
            "FX On and button 1 both step the first effect"
        );
    }

    #[test]
    fn held_loop_buttons_stop_the_jogs() {
        let defaults = default_mappings();
        let jog: Vec<&Mapping> = defaults
            .iter()
            .filter(|m| matches!(m.action, Action::JogTurn { .. } | Action::JogTouch { .. }))
            .collect();
        assert_eq!(jog.len(), 4, "turn and touch, both sides");
        for m in jog {
            assert_eq!(
                m.gate.conditions,
                [
                    Condition {
                        modifier: LOOP_IN_HELD,
                        value: 0
                    },
                    Condition {
                        modifier: LOOP_OUT_HELD,
                        value: 0
                    }
                ]
            );
        }
        let held: Vec<(String, usize)> = defaults
            .iter()
            .filter_map(|m| match (&m.input, m.action) {
                (Input::Hid(n), Action::Modifier { index }) => {
                    assert_eq!((m.interaction, m.gate.value), (Interaction::Hold, Some(1)));
                    Some((n.clone(), index))
                }
                _ => None,
            })
            .collect();
        assert_eq!(held.len(), 4);
        assert!(held.contains(&("Right.Loop.Out".into(), LOOP_OUT_HELD)));
    }

    #[test]
    fn pads_light_as_rgb_triplets() {
        let mut leds = Leds::for_layout(&LAYOUT);
        let d = Decoder::new(&LAYOUT);
        leds.set(&d, "Right.Pads.2", crate::s4mk3::colour::GREEN, true);
        assert_eq!(&leds.0[at(R81, 0x10)..at(R81, 0x13)], &[0, 0x7F, 0]);
    }
}
