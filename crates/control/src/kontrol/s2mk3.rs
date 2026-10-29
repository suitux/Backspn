use std::sync::LazyLock;

use super::{analog, button, db_meter, encoder, jog, map};
use crate::feedback::Feedback;
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Jog, Layout, Leds, Shifted};
use crate::{Action, Interaction, Mapping};

pub const NAME: &str = "Kontrol S2 MK3";
pub const SOURCE: &str = "S2 MK3";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1710;

const LED_LEN: usize = 0x3E;
const SINGLE: Option<(u8, u8)> = Some((0x7E, 0x7C));
const METER_ON: u8 = 0x7E;
const JOG_STEPS: f64 = 600.0;

fn b(name: impl Into<String>, byte: usize, mask: u8, led: usize) -> Button {
    button(name, byte, mask, Some(led), SINGLE)
}

fn plain(name: impl Into<String>, byte: usize, mask: u8) -> Button {
    button(name, byte, mask, None, None)
}

fn pad(name: impl Into<String>, byte: usize, mask: u8, led: usize) -> Button {
    button(name, byte, mask, Some(led), None)
}

fn buttons() -> Vec<Button> {
    let mut v = vec![
        b("Left.Play", 0x02, 0x08, 0x0C),
        b("Left.Cue", 0x02, 0x04, 0x0B),
        b("Left.Shift", 0x01, 0x20, 0x06),
        b("Left.Sync", 0x02, 0x01, 0x09),
        b("Left.Keylock", 0x02, 0x02, 0x0A),
        b("Left.Pad Mode.Hotcues", 0x01, 0x40, 0x07),
        b("Left.Pad Mode.Samples", 0x01, 0x80, 0x08),
        plain("Left.Browse.Load", 0x07, 0x01),
        b("Left.Browse.View", 0x01, 0x08, 0x04),
        b("Left.Browse.Add To Playlist", 0x01, 0x04, 0x03),
        plain("Left.Loop.Push", 0x07, 0x04),
        plain("Left.Move.Push", 0x07, 0x02),
        plain("Left.Jog.Touch", 0x08, 0x40),
        b("Left.Transport Mode.Reverse", 0x01, 0x01, 0x01),
        b("Left.Transport Mode.Flux", 0x01, 0x02, 0x02),
        b("Left.Jog Mode.Grid", 0x01, 0x10, 0x05),
        b("Right.Play", 0x05, 0x20, 0x33),
        b("Right.Cue", 0x05, 0x10, 0x32),
        b("Right.Shift", 0x04, 0x80, 0x2D),
        b("Right.Sync", 0x05, 0x04, 0x30),
        b("Right.Keylock", 0x05, 0x08, 0x31),
        b("Right.Pad Mode.Hotcues", 0x05, 0x01, 0x2E),
        b("Right.Pad Mode.Samples", 0x05, 0x02, 0x2F),
        plain("Right.Browse.Load", 0x07, 0x08),
        b("Right.Browse.View", 0x04, 0x20, 0x2B),
        b("Right.Browse.Add To Playlist", 0x04, 0x10, 0x2A),
        plain("Right.Loop.Push", 0x07, 0x20),
        plain("Right.Move.Push", 0x07, 0x10),
        plain("Right.Jog.Touch", 0x08, 0x80),
        b("Right.Transport Mode.Reverse", 0x04, 0x04, 0x28),
        b("Right.Transport Mode.Flux", 0x04, 0x08, 0x29),
        b("Right.Jog Mode.Grid", 0x04, 0x40, 0x2C),
        b("Mixer.A.Cue", 0x04, 0x01, 0x1A),
        b("Mixer.B.Cue", 0x04, 0x02, 0x1B),
        b("Mixer.Quant", 0x06, 0x40, 0x3C),
        b("Mixer.Mic", 0x06, 0x80, 0x3D),
    ];
    let left: [(usize, u8); 8] = [
        (0x02, 0x10),
        (0x02, 0x20),
        (0x02, 0x40),
        (0x02, 0x80),
        (0x03, 0x01),
        (0x03, 0x02),
        (0x03, 0x04),
        (0x03, 0x08),
    ];
    let right: [(usize, u8); 8] = [
        (0x05, 0x40),
        (0x05, 0x80),
        (0x06, 0x01),
        (0x06, 0x02),
        (0x06, 0x04),
        (0x06, 0x08),
        (0x06, 0x10),
        (0x06, 0x20),
    ];
    for (i, (byte, mask)) in left.into_iter().enumerate() {
        v.push(pad(format!("Left.Pads.{}", i + 1), byte, mask, 0x0D + i));
    }
    for (i, (byte, mask)) in right.into_iter().enumerate() {
        v.push(pad(format!("Right.Pads.{}", i + 1), byte, mask, 0x34 + i));
    }
    for (i, mask) in [0x10u8, 0x20, 0x40, 0x80].into_iter().enumerate() {
        v.push(pad(
            format!("Mixer.Mixer FX Select.{}", i + 1),
            0x03,
            mask,
            0x16 + i,
        ));
    }
    v
}

const ANALOGS: &[Analog] = &[
    analog("Left.Tempo", 0x01),
    analog("Mixer.A.Volume", 0x03),
    analog("Mixer.Xfader", 0x05),
    analog("Mixer.B.Volume", 0x07),
    analog("Right.Tempo", 0x09),
    analog("Mixer.A.Gain", 0x0B),
    analog("Mixer.A.EQ.High", 0x0D),
    analog("Mixer.A.EQ.Mid", 0x0F),
    analog("Mixer.A.EQ.Low", 0x11),
    analog("Mixer.A.Mixer FX.Amount", 0x13),
    analog("Mixer.Master", 0x15),
    analog("Mixer.Samples", 0x17),
    analog("Mixer.Cue Mix", 0x19),
    analog("Mixer.Cue Volume", 0x1B),
    analog("Mixer.B.Gain", 0x1D),
    analog("Mixer.B.EQ.High", 0x1F),
    analog("Mixer.B.EQ.Mid", 0x21),
    analog("Mixer.B.EQ.Low", 0x23),
    analog("Mixer.B.Mixer FX.Amount", 0x25),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Browse.Encoder.Turn", 0x09, false),
    encoder("Left.Move.Turn", 0x09, true),
    encoder("Left.Loop.Turn", 0x0A, false),
    encoder("Right.Browse.Encoder.Turn", 0x0A, true),
    encoder("Right.Move.Turn", 0x0B, false),
    encoder("Right.Loop.Turn", 0x0B, true),
];

const JOGS: &[Jog] = &[
    jog("Left.Jog.Turn", 0x0C, 10),
    jog("Right.Jog.Turn", 0x10, 10),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 64), (0x02, 64)];

const METER_DB: [f32; 5] = [-30.0, -24.0, -18.0, -12.0, -6.0];
const METER_FIRST: [usize; 2] = [0x1C, 0x22];

fn meter_leds(feedback: &Feedback, _decoder: &Decoder, leds: &mut Leds) {
    for (ch, &first) in METER_FIRST.iter().enumerate() {
        db_meter(
            &mut leds.0,
            first,
            &METER_DB,
            feedback.channels[ch].level,
            METER_ON,
        );
    }
}

fn default_mappings() -> Vec<Mapping> {
    let mut extra = super::side_extras(&LAYOUT);
    extra.push(map("Mixer.Mic", Action::MicToggle, Interaction::Trigger));
    for channel in [2, 3] {
        extra.push(map(
            "Mixer.Samples",
            Action::Volume { channel },
            Interaction::Direct,
        ));
    }
    super::default_mappings(&LAYOUT, extra)
}

fn shifted(name: &str) -> Shifted {
    if name.ends_with(".Browse.Encoder.Turn") {
        return Some(Some((Action::BrowserFavoriteStep, Interaction::Relative)));
    }
    super::common_shifted(name)
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
    meter_leds: Some(meter_leds),
    deck_leds: None,
    default_mappings,
    shifted,
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: true,
    samples_button_toggles: false,
    fx_select_toggles: true,
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
    fn the_samples_knob_drives_decks_c_and_d() {
        let decks: Vec<usize> = default_mappings()
            .into_iter()
            .filter(|m| m.input == Input::Hid("Mixer.Samples".into()))
            .filter_map(|m| match m.action {
                Action::Volume { channel } => Some(channel),
                _ => None,
            })
            .collect();
        assert_eq!(decks, [2, 3]);
        assert!(default_mappings().iter().any(|m| matches!(
            (&m.input, m.action),
            (Input::Hid(n), Action::Keylock { deck: 1 }) if n == "Right.Keylock"
        )));
    }

    #[test]
    fn shift_and_the_browse_encoder_walk_the_favourites() {
        for side in ["Left", "Right"] {
            assert_eq!(
                shifted(&format!("{side}.Browse.Encoder.Turn")),
                Some(Some((Action::BrowserFavoriteStep, Interaction::Relative)))
            );
        }
        assert_eq!(
            shifted("Left.Cue"),
            Some(Some((
                Action::ReturnToStart { deck: 0 },
                Interaction::Trigger
            )))
        );
    }

    #[test]
    fn meters_light_in_the_led_report() {
        let mut f = Feedback::default();
        f.channels[0].level = 0.6;
        let mut leds = Leds::for_layout(&LAYOUT);
        meter_leds(&f, &Decoder::new(&LAYOUT), &mut leds);
        assert_eq!(&leds.0[0x1C..0x21], &[METER_ON; 5]);
        assert_eq!(leds.0[0x21], 0);
        assert!(leds.0[0x22..0x28].iter().all(|&b| b == 0));
    }
}
