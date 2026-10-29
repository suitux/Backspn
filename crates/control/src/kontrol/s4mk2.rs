use std::sync::LazyLock;

use super::{analog, button, encoder, jog, linear_meter, loop_display, map};
use crate::feedback::Feedback;
use crate::modifier::Condition;
use crate::poti::StoredCalibration;
use crate::relative::Encoder as EncoderMode;
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Jog, Layout, Leds, Shifted};
use crate::{Action, Interaction, Mapping, MoveMode, SampleSlotPart};

pub const NAME: &str = "Kontrol S4 MK2";
pub const SOURCE: &str = "S4 MK2";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1310;

const LED_REPORTS: &[(u8, usize)] = &[(0x80, 52), (0x81, 62), (0x82, 60)];
const R80: usize = 0;
const R81: usize = 52;
const R82: usize = 114;
const SINGLE: Option<(u8, u8)> = Some((0x7F, 0x09));
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

fn pad(name: impl Into<String>, byte: usize, mask: u8, rgb: usize) -> Button {
    Button {
        rgb: Some(at(R81, rgb)),
        ..button(name, byte, mask, None, None)
    }
}

#[allow(clippy::too_many_lines)]
fn buttons() -> Vec<Button> {
    let mut v = vec![
        b("Left.Shift", 0x0D, 0x08, at(R81, 0x1D)),
        b("Left.Sync", 0x0D, 0x04, at(R81, 0x1E)),
        b("Left.Cue", 0x0D, 0x02, at(R81, 0x1F)),
        b("Left.Play", 0x0D, 0x01, at(R81, 0x20)),
        b("Left.Loop.Out", 0x0E, 0x08, at(R81, 0x2A)),
        b("Left.Loop.In", 0x0E, 0x04, at(R81, 0x29)),
        b("Left.Transport Mode.Flux", 0x0E, 0x02, at(R81, 0x39)),
        b("Left.Tempo Reset", 0x0E, 0x01, at(R81, 0x2F)),
        plain("Left.Loop.Push", 0x13, 0x02),
        plain("Left.Loop.Active", 0x13, 0x01),
        plain("Left.Jog.Touch", 0x11, 0x01),
        b("Left.Deck Select", 0x0F, 0x20, at(R81, 0x2E)),
        led_only("Left.Display.Deck C/D", 1, at(R81, 0x31)),
        b("Left.Load", 0x0F, 0x10, at(R82, 0x14)),
        b("Left.FX.Button 1", 0x12, 0x80, at(R82, 0x01)),
        b("Left.FX.Button 2", 0x12, 0x40, at(R82, 0x02)),
        b("Left.FX.Button 3", 0x12, 0x20, at(R82, 0x03)),
        b("Left.FX.Button 4", 0x12, 0x10, at(R82, 0x04)),
        b("Left.FX.Mode", 0x11, 0x08, at(R82, 0x11)),
        b("Right.Shift", 0x0C, 0x08, at(R81, 0x25)),
        b("Right.Sync", 0x0C, 0x04, at(R81, 0x26)),
        b("Right.Cue", 0x0C, 0x02, at(R81, 0x27)),
        b("Right.Play", 0x0C, 0x01, at(R81, 0x28)),
        b("Right.Loop.Out", 0x0B, 0x08, at(R81, 0x2C)),
        b("Right.Loop.In", 0x0B, 0x04, at(R81, 0x2B)),
        b("Right.Transport Mode.Flux", 0x0B, 0x02, at(R81, 0x3B)),
        b("Right.Tempo Reset", 0x0B, 0x01, at(R81, 0x35)),
        plain("Right.Loop.Push", 0x13, 0x10),
        plain("Right.Loop.Active", 0x13, 0x08),
        plain("Right.Jog.Touch", 0x11, 0x02),
        b("Right.Deck Select", 0x0A, 0x20, at(R81, 0x34)),
        led_only("Right.Display.Deck C/D", 2, at(R81, 0x37)),
        b("Right.Load", 0x0A, 0x10, at(R82, 0x15)),
        b("Right.FX.Button 1", 0x10, 0x08, at(R82, 0x0D)),
        b("Right.FX.Button 2", 0x10, 0x04, at(R82, 0x0E)),
        b("Right.FX.Button 3", 0x10, 0x02, at(R82, 0x0F)),
        b("Right.FX.Button 4", 0x10, 0x01, at(R82, 0x10)),
        b("Right.FX.Mode", 0x11, 0x04, at(R82, 0x12)),
        b("Mixer.A.Cue", 0x0F, 0x40, at(R82, 0x18)),
        b("Mixer.B.Cue", 0x0A, 0x40, at(R82, 0x19)),
        b("Mixer.C.Cue", 0x0F, 0x80, at(R82, 0x17)),
        b("Mixer.D.Cue", 0x0A, 0x80, at(R82, 0x1A)),
        plain("Mixer.A.Gain Push", 0x11, 0x20),
        plain("Mixer.B.Gain Push", 0x11, 0x40),
        plain("Mixer.C.Gain Push", 0x11, 0x10),
        plain("Mixer.D.Gain Push", 0x11, 0x80),
        b("Mixer.A.FX Assign 1", 0x12, 0x02, at(R82, 0x07)),
        b("Mixer.B.FX Assign 1", 0x10, 0x80, at(R82, 0x09)),
        b("Mixer.C.FX Assign 1", 0x12, 0x08, at(R82, 0x05)),
        b("Mixer.D.FX Assign 1", 0x10, 0x20, at(R82, 0x0B)),
        b("Mixer.A.FX Assign 2", 0x12, 0x01, at(R82, 0x08)),
        b("Mixer.B.FX Assign 2", 0x10, 0x40, at(R82, 0x0A)),
        b("Mixer.C.FX Assign 2", 0x12, 0x04, at(R82, 0x06)),
        b("Mixer.D.FX Assign 2", 0x10, 0x10, at(R82, 0x0C)),
        plain("Mixer.Browse.Push", 0x13, 0x04),
        b("Mixer.Preview", 0x0F, 0x01, at(R81, 0x3D)),
        b("Mixer.LoopRec.Record", 0x0F, 0x04, at(R80, 0x34)),
        b("Mixer.Quant", 0x0A, 0x08, at(R80, 0x31)),
        plain("Mixer.Snap", 0x0A, 0x02),
        led_only("Mixer.A.Loaded", 3, at(R80, 0x10)),
        led_only("Mixer.B.Loaded", 4, at(R80, 0x18)),
        led_only("Mixer.C.Loaded", 5, at(R80, 0x08)),
        led_only("Mixer.D.Loaded", 6, at(R80, 0x20)),
    ];
    for (side, byte, masks, rgb, remix_byte, remix_led) in [
        ("Left", 0x0D, [0x80u8, 0x40, 0x20, 0x10], 0x01, 0x0E, 0x19),
        ("Right", 0x0C, [0x80, 0x40, 0x20, 0x10], 0x0D, 0x0B, 0x21),
    ] {
        for (i, mask) in masks.into_iter().enumerate() {
            v.push(pad(
                format!("{side}.Pads.{}", i + 1),
                byte,
                mask,
                rgb + 3 * i,
            ));
            v.push(b(
                format!("{side}.Remix.{}", i + 1),
                remix_byte,
                mask,
                at(R81, remix_led + i),
            ));
        }
    }
    v
}

const ANALOGS: &[Analog] = &[
    analog("Left.Tempo", 0x09),
    analog("Right.Tempo", 0x0B),
    analog("Mixer.Xfader", 0x07),
    analog("Mixer.Cue Mix", 0x0D),
    analog("Mixer.Master", 0x11),
    analog("Mixer.A.Volume", 0x37),
    analog("Mixer.B.Volume", 0x39),
    analog("Mixer.C.Volume", 0x3B),
    analog("Mixer.D.Volume", 0x3D),
    analog("Mixer.A.Mixer FX.Amount", 0x1D),
    analog("Mixer.B.Mixer FX.Amount", 0x25),
    analog("Mixer.C.Mixer FX.Amount", 0x2D),
    analog("Mixer.D.Mixer FX.Amount", 0x35),
    analog("Mixer.A.EQ.Low", 0x1B),
    analog("Mixer.A.EQ.Mid", 0x19),
    analog("Mixer.A.EQ.High", 0x17),
    analog("Mixer.B.EQ.Low", 0x23),
    analog("Mixer.B.EQ.Mid", 0x21),
    analog("Mixer.B.EQ.High", 0x1F),
    analog("Mixer.C.EQ.Low", 0x2B),
    analog("Mixer.C.EQ.Mid", 0x29),
    analog("Mixer.C.EQ.High", 0x27),
    analog("Mixer.D.EQ.Low", 0x33),
    analog("Mixer.D.EQ.Mid", 0x31),
    analog("Mixer.D.EQ.High", 0x2F),
    analog("Left.FX.Knob 1", 0x3F),
    analog("Left.FX.Knob 2", 0x41),
    analog("Left.FX.Knob 3", 0x43),
    analog("Left.FX.Knob 4", 0x45),
    analog("Right.FX.Knob 1", 0x47),
    analog("Right.FX.Knob 2", 0x49),
    analog("Right.FX.Knob 3", 0x4B),
    analog("Right.FX.Knob 4", 0x4D),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Move.Turn", 0x01, false),
    encoder("Left.Loop.Turn", 0x01, true),
    encoder("Mixer.Browse.Turn", 0x02, false),
    encoder("Right.Move.Turn", 0x02, true),
    encoder("Right.Loop.Turn", 0x03, false),
    encoder("Mixer.A.Gain", 0x03, true),
    encoder("Mixer.B.Gain", 0x04, false),
    encoder("Mixer.C.Gain", 0x04, true),
    encoder("Mixer.D.Gain", 0x05, false),
];

const JOGS: &[Jog] = &[
    jog("Left.Jog.Turn", 0x01, 8),
    jog("Right.Jog.Turn", 0x05, 8),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 32), (0x02, 80)];

static CALIBRATION: StoredCalibration = StoredCalibration {
    reports: &[0xD0, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5],
    product_id: PRODUCT_ID,
    first_entry: 0x0E,
    with_centre: &[
        28, 29, 30, 31, 32, 33, 34, 35, 11, 15, 19, 23, 10, 14, 18, 22, 9, 13, 17, 21, 8, 12, 16,
        20, 7,
    ],
    faders: &[24, 25, 26, 27, 1, 2, 0],
    no_centre: &[24, 25, 26, 27, 1, 2, 0],
    first_analog: 0x07,
};

const METER_FIRST: [usize; 4] = [0x09, 0x11, 0x01, 0x19];
const METER_SEGMENTS: usize = 6;
const MASTER_PEAK: [usize; 2] = [0x3B, 0x3C];

const LOOP_DISPLAY: [usize; 2] = [0x1B, 0x2B];
const SEGMENT_ON: u8 = 0x7F;

fn deck_leds(feedback: &Feedback, decoder: &Decoder, layer: [usize; 2], leds: &mut Leds) {
    let phase = phase_ms();
    for (i, side) in ["Left", "Right"].into_iter().enumerate() {
        let d = &feedback.decks[layer[i]];
        let digits = loop_display::loop_display(d.loop_size, d.loop_active, d.live, phase);
        loop_display::write_digits(&mut leds.0, at(R82, LOOP_DISPLAY[i]), digits, SEGMENT_ON);
        let slots = (layer[i] >= 2)
            .then(|| feedback.decks[i + 2].sample_deck)
            .flatten();
        for slot in 0..4 {
            if let Some(led) = decoder.led(&format!("{side}.Remix.{}", slot + 1)) {
                let state = slots.map_or(0, |s| s.slots[slot].state.min(4));
                leds.0[led] = remix_led(state);
            }
        }
    }
}

fn remix_led(state: u8) -> u8 {
    u8::try_from(u32::from(state.min(4)) * u32::from(SEGMENT_ON) / 4).unwrap_or(SEGMENT_ON)
}

fn phase_ms() -> u128 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
        % loop_display::BLINK_PERIOD_MS
}

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
    for (side, &offset) in MASTER_PEAK.iter().enumerate() {
        leds.0[at(R82, offset)] = if feedback.master_level[side] > 1.0 {
            METER_ON
        } else {
            0
        };
    }
}

const PREVIEW_MODIFIER: usize = 7;

fn when_preview(mut mapping: Mapping, value: u8) -> Mapping {
    mapping.gate.conditions = vec![Condition {
        modifier: PREVIEW_MODIFIER,
        value,
    }];
    mapping
}

fn browse_mappings() -> Vec<Mapping> {
    let mut v = vec![
        when_preview(
            map(
                "Mixer.Browse.Turn",
                Action::BrowserUpDown,
                Interaction::Relative,
            ),
            0,
        ),
        when_preview(
            map(
                "Mixer.Browse.Turn",
                Action::PreviewSeek,
                Interaction::Relative,
            ),
            1,
        ),
        map(
            "Mixer.Browse.Push",
            Action::PreviewLoad,
            Interaction::Trigger,
        ),
    ];
    for (from, to) in [(0, 1), (1, 0)] {
        let mut toggle = map(
            "Mixer.Browse.Push",
            Action::Modifier {
                index: PREVIEW_MODIFIER,
            },
            Interaction::Direct,
        );
        toggle.gate.value = Some(to);
        v.push(when_preview(toggle, from));
    }
    v
}

fn default_mappings() -> Vec<Mapping> {
    let mut v: Vec<Mapping> = super::default_mappings(&LAYOUT, Vec::new())
        .into_iter()
        .filter(|m| !matches!(m.action, Action::Gain { .. }))
        .collect();
    for (channel, ch) in ["A", "B", "C", "D"].into_iter().enumerate() {
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
            &format!("{side}.Loop.Active"),
            Action::LoopActive { deck },
            Interaction::Toggle,
        ));
        v.push(map(
            &format!("{side}.Tempo Reset"),
            Action::TempoReset { deck },
            Interaction::Trigger,
        ));
        v.push(map(
            &format!("{side}.Load"),
            Action::LoadSelected { deck },
            Interaction::Trigger,
        ));
        v.push(map(
            &format!("{side}.FX.Mode"),
            Action::FxMode { unit: deck },
            Interaction::Relative,
        ));
        for slot in 0..4 {
            v.push(map(
                &format!("{side}.Remix.{}", slot + 1),
                Action::SampleSlot {
                    deck: deck + 2,
                    slot,
                    part: SampleSlotPart::Primary,
                },
                Interaction::Hold,
            ));
        }
    }
    v.extend(browse_mappings());
    v.push(map(
        "Mixer.Preview",
        Action::BrowserPreview,
        Interaction::Hold,
    ));
    v.push(map("Mixer.Snap", Action::Snap, Interaction::Toggle));
    v.push(map(
        "Mixer.LoopRec.Record",
        Action::LoopRecorder {
            part: crate::LoopRecorderPart::Record,
        },
        Interaction::Toggle,
    ));
    v
}

fn shifted(name: &str) -> Shifted {
    let (side, control) = name.split_once('.')?;
    let deck = match side {
        "Left" => 0,
        "Right" => 1,
        "Mixer" => {
            return match control {
                "Browse.Turn" => Some(Some((Action::BrowserFavoriteStep, Interaction::Relative))),
                "Quant" => Some(Some((Action::Snap, Interaction::Toggle))),
                _ => {
                    let (ch, rest) = control.split_once('.')?;
                    let channel = usize::from(ch.as_bytes().first()?.checked_sub(b'A')?);
                    Some(Some(match rest {
                        "FX Assign 1" => (
                            Action::XfaderSide {
                                channel,
                                right: false,
                            },
                            Interaction::Trigger,
                        ),
                        "FX Assign 2" => (
                            Action::XfaderSide {
                                channel,
                                right: true,
                            },
                            Interaction::Trigger,
                        ),
                        _ => return None,
                    }))
                }
            };
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
        "FX.Button 2" => (
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
        "FX.Mode" => (Action::FxSnapshotStore { unit: deck }, Interaction::Trigger),
        "FX.Button 1" => return Some(None),
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
    screens: false,
    tempo_leds: None,
    wheel_leds: None,
    strip_leds: None,
    meters: None,
    meter_leds: Some(meter_leds),
    deck_leds: Some(deck_leds),
    default_mappings,
    shifted,
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: false,
    samples_button_toggles: false,
    fx_select_toggles: false,
    mx2_transport: false,
    shared_shift: false,
    connect_reports: &[],
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
    fn leds_span_the_three_reports() {
        let leds = Leds::for_layout(&LAYOUT);
        assert_eq!(leds.0.len(), 52 + 62 + 60);
        let reports: Vec<(u8, usize)> =
            leds.reports(&LAYOUT).map(|(id, b)| (id, b.len())).collect();
        assert_eq!(reports, [(0x80, 52), (0x81, 62), (0x82, 60)]);
        let mut leds = Leds::for_layout(&LAYOUT);
        let d = Decoder::new(&LAYOUT);
        leds.set(&d, "Left.Pads.1", crate::s4mk3::colour::RED, true);
        assert_eq!(&leds.0[R81..R81 + 3], &[0x7F, 0, 0], "red triplet");
        leds.set(&d, "Mixer.A.Cue", crate::s4mk3::colour::BLUE, false);
        assert_eq!(leds.0[at(R82, 0x18)], 0x09, "dim one-colour LED");
    }

    #[test]
    fn defaults_follow_the_factory_mapping() {
        let defaults = default_mappings();
        let find = |name: &str| {
            defaults
                .iter()
                .find(|m| m.input == Input::Hid(name.into()))
                .cloned()
        };
        let gain = find("Mixer.C.Gain").expect("gain");
        assert_eq!(
            (gain.action, gain.interaction),
            (Action::Gain { channel: 2 }, Interaction::Relative)
        );
        assert_eq!(
            find("Mixer.C.Gain Push").map(|m| m.interaction),
            Some(Interaction::Reset)
        );
        assert_eq!(
            find("Right.Remix.3").map(|m| m.action),
            Some(Action::SampleSlot {
                deck: 3,
                slot: 2,
                part: SampleSlotPart::Primary
            })
        );
        assert_eq!(
            find("Left.Loop.In").map(|m| m.action),
            Some(Action::LoopIn { deck: 0 })
        );
        assert_eq!(shifted("Mixer.B.Cue"), None, "SHIFT + PFL is PFL");
        assert_eq!(
            shifted("Left.Cue"),
            Some(Some((
                Action::ReturnToStart { deck: 0 },
                Interaction::Trigger
            )))
        );
        assert_eq!(
            shifted("Right.FX.Mode"),
            Some(Some((
                Action::FxSnapshotStore { unit: 1 },
                Interaction::Trigger
            )))
        );
        assert_eq!(
            shifted("Mixer.Browse.Turn"),
            Some(Some((Action::BrowserFavoriteStep, Interaction::Relative)))
        );
        assert_eq!(
            find("Left.Load").map(|m| m.action),
            Some(Action::LoadSelected { deck: 0 })
        );
        assert_eq!(
            find("Right.FX.Mode").map(|m| (m.action, m.interaction)),
            Some((Action::FxMode { unit: 1 }, Interaction::Relative))
        );
        assert_eq!(
            shifted("Left.Play"),
            Some(Some((Action::Keylock { deck: 0 }, Interaction::Toggle)))
        );
    }

    #[test]
    fn the_browse_push_switches_the_encoder_to_the_preview() {
        let defaults = default_mappings();
        let on = |name: &str| -> Vec<Mapping> {
            defaults
                .iter()
                .filter(|m| m.input == Input::Hid(name.into()))
                .cloned()
                .collect()
        };
        let turn = on("Mixer.Browse.Turn");
        assert_eq!(turn.len(), 2);
        assert_eq!(turn[0].action, Action::BrowserUpDown);
        assert_eq!(turn[0].gate.conditions[0].value, 0);
        assert_eq!(turn[1].action, Action::PreviewSeek);
        assert_eq!(turn[1].gate.conditions[0].value, 1);
        let push = on("Mixer.Browse.Push");
        assert!(push.iter().any(|m| m.action == Action::PreviewLoad));
        let toggles: Vec<(u8, Option<u8>)> = push
            .iter()
            .filter(|m| matches!(m.action, Action::Modifier { index: 7 }))
            .map(|m| (m.gate.conditions[0].value, m.gate.value))
            .collect();
        assert_eq!(toggles, [(0, Some(1)), (1, Some(0))]);
    }

    #[test]
    fn displays_show_each_side_s_loop_size_and_remix_slots() {
        let mut f = Feedback::default();
        f.decks[0].loop_size = 7;
        f.decks[3].loop_size = 0;
        let mut slots = crate::feedback::SampleFeedback::default();
        slots.slots[1].state = 2;
        slots.slots[2].state = 1;
        f.decks[2].sample_deck = Some(slots);
        let d = Decoder::new(&LAYOUT);
        let mut leds = Leds::for_layout(&LAYOUT);
        deck_leds(&f, &d, [0, 3], &mut leds);
        let mut want = [0u8; 16];
        loop_display::write_digits(&mut want, 0, loop_display::loop_size_digits(7), 0x7F);
        assert_eq!(&leds.0[at(R82, 0x1B)..at(R82, 0x2B)], &want);
        loop_display::write_digits(&mut want, 0, loop_display::loop_size_digits(0), 0x7F);
        assert_eq!(&leds.0[at(R82, 0x2B)..at(R82, 0x3B)], &want);
        assert_eq!(leds.0[d.led("Left.Remix.2").expect("led")], 0);
        deck_leds(&f, &d, [2, 3], &mut leds);
        assert_eq!(leds.0[d.led("Left.Remix.1").expect("led")], 0);
        assert_eq!(leds.0[d.led("Left.Remix.2").expect("led")], 0x3F);
        assert_eq!(leds.0[d.led("Left.Remix.3").expect("led")], 0x1F);
    }

    #[test]
    fn encoders_come_in_report_two() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 80];
        r[0] = 0x02;
        d.decode(&r);
        r[0x03] = 0x10;
        let events = d.decode(&r);
        assert!(
            events
                .iter()
                .any(|e| e.input == Input::Hid("Mixer.A.Gain".into())
                    && (e.value - 1.0).abs() < 1e-6),
            "{events:?}"
        );
    }
}
