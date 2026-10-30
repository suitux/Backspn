use std::sync::LazyLock;

use super::{analog, button, encoder, map};
use crate::s4mk3::{Analog, Button, Encoder, Layout, Shifted, TouchStrip};
use crate::strip::Bar;
use crate::{Action, Interaction, Mapping};

pub const NAME: &str = "Kontrol X1 MK2";
pub const SOURCE: &str = "X1 MK2";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1220;

const LED_REPORTS: &[(u8, usize)] = &[(0x80, 51), (0x81, 90)];
const R80: usize = 0;
const R81: usize = 51;
/// LEDs numbered across both reports: the first 51 go in 0x80, the rest in 0x81.
const FIRST_IN_R81: usize = 51;
const SINGLE: Option<(u8, u8)> = Some((0x7F, 0x0A));
/// The buttons are a bit field: button `i` is bit `i % 8` of byte `BUTTONS + i / 8`.
const BUTTONS: usize = 0x13;

/// The bar under the strip: 21 positions with two LEDs each, orange then blue.
const BAR_FIRST: usize = 99;
const BAR_POSITIONS: usize = 21;
/// Two fingers, positions about 15 to 1009 from left to right.
const STRIP: TouchStrip = TouchStrip {
    report: 0x01,
    byte: 0x1B,
    fingers: 2,
    centre: 512,
    gap: 50,
    first: 15,
    last: 1009,
    bar: Some(Bar {
        first: led(BAR_FIRST),
        positions: BAR_POSITIONS,
    }),
    side: None,
};
/// A slide across one half moves the deck about half a platter turn.
const STRIP_STEPS_PER_TURN: f64 = 1000.0;

const fn at(base: usize, offset: usize) -> usize {
    base + offset - 1
}

const fn led(n: usize) -> usize {
    if n < FIRST_IN_R81 {
        at(R80, n + 1)
    } else {
        at(R81, n - FIRST_IN_R81 + 1)
    }
}

fn bit(index: usize) -> (usize, u8) {
    (BUTTONS + index / 8, 1 << (index % 8))
}

fn b(name: &str, index: usize, n: usize) -> Button {
    let (byte, mask) = bit(index);
    button(name, byte, mask, Some(led(n)), SINGLE)
}

fn plain(name: &str, index: usize) -> Button {
    let (byte, mask) = bit(index);
    button(name, byte, mask, None, None)
}

fn pad(name: &str, index: usize, n: usize) -> Button {
    Button {
        rgb: Some(led(n)),
        ..plain(name, index)
    }
}

fn buttons() -> Vec<Button> {
    vec![
        b("Right.FX.Button 4", 0, 7),
        b("Right.FX.Button 3", 1, 6),
        b("Right.FX.Button 2", 2, 5),
        b("Right.FX.Mode", 3, 4),
        b("Left.FX.Button 4", 4, 3),
        b("Left.FX.Button 3", 5, 2),
        b("Left.FX.Button 2", 6, 1),
        b("Left.FX.Mode", 7, 0),
        plain("Right.Loop.Push", 8),
        b("Right.Load", 9, 18),
        b("Left.Shift", 10, 17),
        b("Left.Load", 11, 16),
        b("Mixer.B.FX Assign 2", 12, 15),
        b("Mixer.B.FX Assign 1", 13, 14),
        b("Mixer.A.FX Assign 2", 14, 9),
        b("Mixer.A.FX Assign 1", 15, 8),
        b("Right.Play", 16, 50),
        b("Right.Cue", 17, 49),
        b("Right.Sync", 18, 46),
        b("Right.Transport Mode.Flux", 19, 45),
        pad("Right.Pads.4", 20, 40),
        pad("Right.Pads.3", 21, 37),
        pad("Right.Pads.2", 22, 28),
        pad("Right.Pads.1", 23, 25),
        b("Left.Play", 24, 48),
        b("Left.Cue", 25, 47),
        b("Left.Sync", 26, 44),
        b("Left.Transport Mode.Flux", 27, 43),
        pad("Left.Pads.4", 28, 34),
        pad("Left.Pads.3", 29, 31),
        pad("Left.Pads.2", 30, 22),
        pad("Left.Pads.1", 31, 19),
        plain("Left.Loop.Push", 32),
        plain("Mixer.Browse.Push", 33),
        plain("Left.Loop.Touch", 34),
        plain("Mixer.Browse.Touch", 35),
        plain("Right.Loop.Touch", 36),
    ]
}

const ANALOGS: &[Analog] = &[
    analog("Left.FX.Knob 1", 0x01),
    analog("Left.FX.Knob 2", 0x03),
    analog("Left.FX.Knob 3", 0x05),
    analog("Left.FX.Knob 4", 0x07),
    analog("Right.FX.Knob 1", 0x09),
    analog("Right.FX.Knob 2", 0x0B),
    analog("Right.FX.Knob 3", 0x0D),
    analog("Right.FX.Knob 4", 0x0F),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Loop.Turn", 0x11, false),
    encoder("Mixer.Browse.Turn", 0x11, true),
    encoder("Right.Loop.Turn", 0x12, false),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 31)];

fn default_mappings() -> Vec<Mapping> {
    let mut extra = vec![
        map(
            "Mixer.Browse.Turn",
            Action::BrowserUpDown,
            Interaction::Relative,
        ),
        map(
            "Mixer.Browse.Push",
            Action::BrowserView,
            Interaction::Trigger,
        ),
    ];
    for (deck, side) in [(0usize, "Left"), (1, "Right")] {
        extra.push(map(
            &format!("{side}.Load"),
            Action::LoadSelected { deck },
            Interaction::Trigger,
        ));
        extra.push(map(
            &format!("{side}.FX.Mode"),
            Action::FxMode { unit: deck },
            Interaction::Relative,
        ));
    }
    super::default_mappings(&LAYOUT, extra)
}

fn shifted(name: &str) -> Shifted {
    if name == "Mixer.Browse.Turn" {
        return Some(Some((Action::BrowserFavoriteStep, Interaction::Relative)));
    }
    let (side, control) = name.split_once('.')?;
    let deck = match side {
        "Left" => 0,
        "Right" => 1,
        _ => return None,
    };
    if let Some(n) = control.strip_prefix("FX.Button ") {
        let slot = n.parse::<usize>().ok()?.checked_sub(2)?;
        return Some(Some((
            Action::FxSelect { unit: deck, slot },
            Interaction::Relative,
        )));
    }
    Some(Some(match control {
        "Sync" => (Action::Master { deck }, Interaction::Trigger),
        "Loop.Turn" => (Action::LoopMove { deck }, Interaction::Relative),
        "Loop.Push" => return Some(None),
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
    jogs: &[],
    jog_steps_per_turn: STRIP_STEPS_PER_TURN,
    analog_report: 0x01,
    jog_report: 0x03,
    encoder_report: 0x01,
    led_reports: LED_REPORTS,
    meter_report: None,
    initial_reports: INITIAL_REPORTS,
    motors: false,
    sound_card: None,
    calibration: &[],
    stored_calibration: None,
    rings: false,
    screens: None,
    tempo_leds: None,
    wheel_leds: None,
    strip_leds: None,
    meters: None,
    meter_leds: None,
    deck_leds: None,
    default_mappings,
    shifted,
    modifier_hooks: None,
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: false,
    samples_button_toggles: false,
    fx_select_toggles: false,
    mx2_transport: false,
    shared_shift: true,
    connect_reports: &[],
    touch_strips: &[STRIP],
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
    use crate::s4mk3::{Decoder, Leds};

    fn report(buttons: &[usize]) -> Vec<u8> {
        let mut r = vec![0u8; 31];
        r[0] = 0x01;
        for &i in buttons {
            let (byte, mask) = bit(i);
            r[byte] |= mask;
        }
        r
    }

    fn find(name: &str) -> Option<(Action, Interaction)> {
        default_mappings()
            .into_iter()
            .find(|m| m.input == Input::Hid(name.into()))
            .map(|m| (m.action, m.interaction))
    }

    #[test]
    fn the_layout_is_consistent() {
        crate::s4mk3::tests::check_layout(&LAYOUT);
        assert_eq!(LAYOUT.buttons.len(), 37);
        assert!(LAYOUT.buttons.iter().all(|b| b.byte < 0x18));
    }

    #[test]
    fn button_indices_count_bits_from_the_first_button_byte() {
        let mut d = Decoder::new(&LAYOUT);
        assert!(d.decode(&report(&[])).iter().all(|e| e.pressed.is_none()));
        let events = d.decode(&report(&[24]));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].input, Input::Hid("Left.Play".into()));
        assert_eq!(events[0].pressed, Some(true));
        let events = d.decode(&report(&[24, 36]));
        assert_eq!(events[0].input, Input::Hid("Right.Loop.Touch".into()));
    }

    #[test]
    fn knobs_and_encoders_share_the_input_report() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = report(&[]);
        d.decode(&r);
        r[0x0F] = 0xFF;
        r[0x10] = 0x0F;
        r[0x11] = 0x10;
        let events = d.decode(&r);
        let knob = events
            .iter()
            .find(|e| e.input == Input::Hid("Right.FX.Knob 4".into()))
            .expect("knob");
        assert!((knob.value - 1.0).abs() < 1e-6);
        let turn = events
            .iter()
            .find(|e| e.input == Input::Hid("Mixer.Browse.Turn".into()))
            .expect("browse");
        assert!((turn.value - 1.0).abs() < 1e-6);
        assert!(
            !events
                .iter()
                .any(|e| e.input == Input::Hid("Left.Loop.Turn".into()))
        );
    }

    #[test]
    fn leds_span_both_reports() {
        let leds = Leds::for_layout(&LAYOUT);
        let reports: Vec<(u8, usize)> =
            leds.reports(&LAYOUT).map(|(id, b)| (id, b.len())).collect();
        assert_eq!(reports, [(0x80, 51), (0x81, 90)]);
        assert_eq!(led(50), 50);
        assert_eq!(led(51), R81, "the first LED of the second report");
        let mut leds = Leds::for_layout(&LAYOUT);
        let d = Decoder::new(&LAYOUT);
        leds.set(&d, "Left.Pads.1", crate::s4mk3::colour::RED, true);
        assert_eq!(&leds.0[19..22], &[0x7F, 0, 0], "red triplet");
        leds.set(&d, "Right.Play", crate::s4mk3::colour::BLUE, false);
        assert_eq!(leds.0[50], 0x0A, "dim one-colour LED");
    }

    #[test]
    fn each_side_drives_its_deck_and_fx_unit() {
        assert_eq!(
            find("Right.Play"),
            Some((Action::PlayPause { deck: 1 }, Interaction::Trigger))
        );
        assert_eq!(
            find("Left.Pads.4").map(|m| m.0),
            Some(Action::Hotcue { deck: 0, slot: 3 })
        );
        assert_eq!(
            find("Left.Transport Mode.Flux").map(|m| m.0),
            Some(Action::Slip { deck: 0 })
        );
        assert_eq!(
            find("Right.FX.Knob 1").map(|m| m.0),
            Some(Action::FxDryWet { unit: 1 })
        );
        assert_eq!(
            find("Right.FX.Button 2").map(|m| m.0),
            Some(Action::FxButton { unit: 1, button: 0 })
        );
        assert_eq!(
            find("Left.FX.Mode"),
            Some((Action::FxMode { unit: 0 }, Interaction::Relative))
        );
        assert_eq!(
            find("Mixer.B.FX Assign 1").map(|m| m.0),
            Some(Action::FxAssign {
                channel: 1,
                unit: 0
            })
        );
        assert_eq!(
            find("Left.Load").map(|m| m.0),
            Some(Action::LoadSelected { deck: 0 })
        );
        assert_eq!(
            find("Left.Loop.Turn").map(|m| m.0),
            Some(Action::LoopSizeStep { deck: 0 })
        );
        assert_eq!(
            find("Mixer.Browse.Turn").map(|m| m.0),
            Some(Action::BrowserUpDown)
        );
        assert_eq!(find("Left.Loop.Touch"), None, "touch only");
    }

    fn strip(r: &mut [u8], fingers: [u16; 2]) {
        for (f, at) in fingers.into_iter().enumerate() {
            let byte = STRIP.byte + 2 * f;
            r[byte..byte + 2].copy_from_slice(&at.to_le_bytes());
        }
    }

    fn named(events: &[crate::InputEvent]) -> Vec<(String, f32, Option<bool>)> {
        events
            .iter()
            .filter_map(|e| match &e.input {
                Input::Hid(n) if n.contains(".Jog.") => Some((n.clone(), e.value, e.pressed)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_finger_drives_the_half_it_lands_on() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = report(&[]);
        d.decode(&r);
        strip(&mut r, [300, 0]);
        assert_eq!(
            named(&d.decode(&r)),
            [("Left.Jog.Touch".into(), 1.0, Some(true))]
        );
        strip(&mut r, [340, 0]);
        assert_eq!(named(&d.decode(&r)), [("Left.Jog.Turn".into(), 40.0, None)]);
        strip(&mut r, [700, 0]);
        assert_eq!(
            named(&d.decode(&r)),
            [("Left.Jog.Turn".into(), 360.0, None)],
            "still the left deck across the middle"
        );
        strip(&mut r, [700, 900]);
        assert_eq!(
            named(&d.decode(&r)),
            [("Right.Jog.Touch".into(), 1.0, Some(true))]
        );
        strip(&mut r, [0, 880]);
        assert_eq!(
            named(&d.decode(&r)),
            [
                ("Left.Jog.Touch".into(), 0.0, Some(false)),
                ("Right.Jog.Turn".into(), -20.0, None),
            ]
        );
        strip(&mut r, [0, 0]);
        assert_eq!(
            named(&d.decode(&r)),
            [("Right.Jog.Touch".into(), 0.0, Some(false))]
        );
    }

    #[test]
    fn the_middle_and_missing_readings_are_ignored() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = report(&[]);
        d.decode(&r);
        strip(&mut r, [530, 0]);
        assert_eq!(named(&d.decode(&r)), [], "landing in the gap");
        strip(&mut r, [600, 0]);
        assert_eq!(named(&d.decode(&r)), [], "it never landed on a half");
        strip(&mut r, [0xFFFF, 0xFFFF]);
        assert_eq!(named(&d.decode(&r)), []);
    }

    #[test]
    fn the_strip_works_like_a_jog() {
        assert_eq!(
            find("Left.Jog.Touch").map(|m| m.0),
            Some(Action::JogTouch { deck: 0 })
        );
        assert_eq!(
            find("Right.Jog.Turn"),
            Some((Action::JogTurn { deck: 1 }, Interaction::Relative))
        );
        assert_eq!(
            shifted("Right.Jog.Turn"),
            Some(Some((Action::JogSearch { deck: 1 }, Interaction::Relative)))
        );
    }

    #[test]
    fn the_bar_fits_the_second_report() {
        let bar = STRIP.bar.expect("a bar");
        assert_eq!(bar.first, R81 + BAR_FIRST - FIRST_IN_R81);
        assert!(bar.first + 2 * bar.positions <= LAYOUT.led_len());
    }

    #[test]
    fn the_strip_reports_where_the_finger_is() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = report(&[]);
        d.decode(&r);
        let along = |events: &[crate::InputEvent], name: &str| {
            events
                .iter()
                .find(|e| e.input == Input::Hid(name.into()))
                .map(|e| e.value)
        };
        strip(&mut r, [1009, 0]);
        let events = d.decode(&r);
        assert_eq!(
            events[0].input,
            Input::Hid(crate::s4mk3::STRIP_ANY_TOUCH.into())
        );
        assert_eq!(along(&events, crate::s4mk3::STRIP_ANY_POSITION), Some(1.0));
        assert_eq!(along(&events, "Right.Strip.Position"), Some(1.0));
        strip(&mut r, [15, 0]);
        let events = d.decode(&r);
        assert_eq!(along(&events, crate::s4mk3::STRIP_ANY_POSITION), Some(0.0));
        assert_eq!(
            along(&events, "Right.Strip.Position"),
            Some(0.0),
            "still the right half"
        );
        strip(&mut r, [520, 0]);
        d.decode(&r);
        strip(&mut r, [0, 0]);
        let events = d.decode(&r);
        assert_eq!(
            events.last().map(|e| (&e.input, e.pressed)),
            Some((
                &Input::Hid(crate::s4mk3::STRIP_ANY_TOUCH.into()),
                Some(false)
            ))
        );
        strip(&mut r, [512, 0]);
        let events = d.decode(&r);
        assert!(
            along(&events, crate::s4mk3::STRIP_ANY_POSITION).is_some(),
            "the gap counts for the whole strip"
        );
        assert_eq!(along(&events, "Left.Strip.Position"), None);
    }

    #[test]
    fn shift_reaches_the_second_layer() {
        assert_eq!(
            shifted("Right.Sync"),
            Some(Some((Action::Master { deck: 1 }, Interaction::Trigger)))
        );
        assert_eq!(
            shifted("Left.Loop.Turn"),
            Some(Some((Action::LoopMove { deck: 0 }, Interaction::Relative)))
        );
        assert_eq!(
            shifted("Left.FX.Button 3"),
            Some(Some((
                Action::FxSelect { unit: 0, slot: 1 },
                Interaction::Relative
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
            shifted("Mixer.Browse.Turn"),
            Some(Some((Action::BrowserFavoriteStep, Interaction::Relative)))
        );
    }
}
