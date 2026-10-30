use std::sync::LazyLock;

use super::{analog, button, db_meter, encoder, jog, linear_meter, map};
use crate::feedback::Feedback;
use crate::s4mk3::{Analog, Button, Encoder, Jog, Layout, Meters, Shifted};
use crate::{Action, Interaction, Mapping};

pub const NAME: &str = "Kontrol S3";
pub const SOURCE: &str = "S3";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1900;

const LED_LEN: usize = 267;
const METER_REPORT: u8 = 0x81;
const METER_LEN: usize = 251;
const DECK_SINGLE: Option<(u8, u8)> = Some((0x77, 0x20));
const MIXER_SINGLE: Option<(u8, u8)> = Some((0xFF, 0x04));
const SEGMENT_ON: u8 = 0x7F;
const CLIP_ON: u8 = 0x7E;
const JOG_STEPS: f64 = 768.0;

fn b(name: impl Into<String>, byte: usize, mask: u8, led: usize) -> Button {
    button(name, byte, mask, Some(led), DECK_SINGLE)
}

fn m(name: impl Into<String>, byte: usize, mask: u8, led: usize) -> Button {
    button(name, byte, mask, Some(led), MIXER_SINGLE)
}

fn plain(name: impl Into<String>, byte: usize, mask: u8) -> Button {
    button(name, byte, mask, None, None)
}

fn rgb(name: impl Into<String>, byte: usize, mask: u8, led: usize) -> Button {
    button(name, byte, mask, Some(led), None)
}

#[allow(clippy::too_many_lines)]
fn buttons() -> Vec<Button> {
    let mut v = vec![
        b("Left.Play", 0x03, 0x01, 0x10),
        b("Left.Cue", 0x02, 0x80, 0x0F),
        b("Left.Shift", 0x01, 0x01, 0x00),
        b("Left.Sync", 0x02, 0x08, 0x0B),
        b("Left.Keylock", 0x02, 0x10, 0x0C),
        b("Left.Pad Mode.Hotcues", 0x02, 0x20, 0x0D),
        b("Left.Pad Mode.Samples", 0x02, 0x40, 0x0E),
        plain("Left.Browse.Encoder.Push", 0x09, 0x01),
        b("Left.Browse.Preview", 0x01, 0x08, 0x03),
        b("Left.Browse.Favorite", 0x01, 0x10, 0x04),
        b("Left.Browse.Add To Playlist", 0x01, 0x20, 0x05),
        b("Left.Browse.View", 0x01, 0x40, 0x06),
        plain("Left.Loop.Push", 0x09, 0x04),
        plain("Left.Move.Push", 0x09, 0x02),
        b("Left.Transport Mode.Reverse", 0x01, 0x04, 0x02),
        b("Left.Transport Mode.Flux", 0x01, 0x02, 0x01),
        b("Left.Quant", 0x01, 0x80, 0x07),
        b("Left.Jog Mode.Jog", 0x02, 0x01, 0x08),
        plain("Left.Jog.Touch", 0x0A, 0x10),
        rgb("Left.Deck.A", 0x02, 0x02, 0x09),
        rgb("Left.Deck.C", 0x02, 0x04, 0x0A),
        b("Right.Play", 0x06, 0x02, 0x29),
        b("Right.Cue", 0x06, 0x01, 0x28),
        b("Right.Shift", 0x04, 0x02, 0x19),
        b("Right.Sync", 0x05, 0x10, 0x24),
        b("Right.Keylock", 0x05, 0x20, 0x25),
        b("Right.Pad Mode.Hotcues", 0x05, 0x40, 0x26),
        b("Right.Pad Mode.Samples", 0x05, 0x80, 0x27),
        plain("Right.Browse.Encoder.Push", 0x09, 0x08),
        b("Right.Browse.Preview", 0x04, 0x10, 0x1C),
        b("Right.Browse.Favorite", 0x04, 0x20, 0x1D),
        b("Right.Browse.Add To Playlist", 0x04, 0x40, 0x1E),
        b("Right.Browse.View", 0x04, 0x80, 0x1F),
        plain("Right.Loop.Push", 0x09, 0x20),
        plain("Right.Move.Push", 0x09, 0x10),
        b("Right.Transport Mode.Reverse", 0x04, 0x08, 0x1B),
        b("Right.Transport Mode.Flux", 0x04, 0x04, 0x1A),
        b("Right.Quant", 0x05, 0x01, 0x20),
        b("Right.Jog Mode.Jog", 0x05, 0x02, 0x21),
        plain("Right.Jog.Touch", 0x0A, 0x20),
        rgb("Right.Deck.B", 0x05, 0x04, 0x22),
        rgb("Right.Deck.D", 0x05, 0x08, 0x23),
        m("Mixer.A.Cue", 0x08, 0x01, 0x38),
        m("Mixer.B.Cue", 0x08, 0x02, 0x39),
        m("Mixer.C.Cue", 0x07, 0x80, 0x37),
        m("Mixer.D.Cue", 0x08, 0x04, 0x3A),
        m("Mixer.D.Ext", 0x07, 0x04, 0x32),
        rgb("Mixer.Mixer FX Select.Filter", 0x08, 0x80, 0x3F),
        rgb("Mixer.Mixer FX Select.1", 0x08, 0x08, 0x3B),
        rgb("Mixer.Mixer FX Select.2", 0x08, 0x10, 0x3C),
        rgb("Mixer.Mixer FX Select.3", 0x08, 0x20, 0x3D),
        rgb("Mixer.Mixer FX Select.4", 0x08, 0x40, 0x3E),
        rgb("Mixer.A.Mixer FX.On", 0x07, 0x10, 0x34),
        rgb("Mixer.B.Mixer FX.On", 0x07, 0x20, 0x35),
        rgb("Mixer.C.Mixer FX.On", 0x07, 0x08, 0x33),
        rgb("Mixer.D.Mixer FX.On", 0x07, 0x40, 0x36),
    ];
    let left: [(usize, u8); 8] = [
        (0x03, 0x02),
        (0x03, 0x04),
        (0x03, 0x08),
        (0x03, 0x10),
        (0x03, 0x20),
        (0x03, 0x40),
        (0x03, 0x80),
        (0x04, 0x01),
    ];
    let right: [(usize, u8); 8] = [
        (0x06, 0x04),
        (0x06, 0x08),
        (0x06, 0x10),
        (0x06, 0x20),
        (0x06, 0x40),
        (0x06, 0x80),
        (0x07, 0x01),
        (0x07, 0x02),
    ];
    for (i, (byte, mask)) in left.into_iter().enumerate() {
        v.push(rgb(format!("Left.Pads.{}", i + 1), byte, mask, 0x11 + i));
    }
    for (i, (byte, mask)) in right.into_iter().enumerate() {
        v.push(rgb(format!("Right.Pads.{}", i + 1), byte, mask, 0x2A + i));
    }
    v
}

const ANALOGS: &[Analog] = &[
    analog("Left.Tempo", 0x01),
    analog("Right.Tempo", 0x0D),
    analog("Mixer.A.Volume", 0x05),
    analog("Mixer.B.Volume", 0x07),
    analog("Mixer.C.Volume", 0x03),
    analog("Mixer.D.Volume", 0x09),
    analog("Mixer.A.Gain", 0x11),
    analog("Mixer.B.Gain", 0x13),
    analog("Mixer.C.Gain", 0x0F),
    analog("Mixer.D.Gain", 0x15),
    analog("Mixer.A.EQ.High", 0x25),
    analog("Mixer.A.EQ.Mid", 0x27),
    analog("Mixer.A.EQ.Low", 0x29),
    analog("Mixer.B.EQ.High", 0x2B),
    analog("Mixer.B.EQ.Mid", 0x2D),
    analog("Mixer.B.EQ.Low", 0x2F),
    analog("Mixer.C.EQ.High", 0x1F),
    analog("Mixer.C.EQ.Mid", 0x21),
    analog("Mixer.C.EQ.Low", 0x23),
    analog("Mixer.D.EQ.High", 0x31),
    analog("Mixer.D.EQ.Mid", 0x33),
    analog("Mixer.D.EQ.Low", 0x35),
    analog("Mixer.Xfader", 0x0B),
    analog("Mixer.Master", 0x17),
    analog("Mixer.Cue Mix", 0x1D),
    analog("Mixer.Cue Volume", 0x1B),
    analog("Mixer.A.Mixer FX.Amount", 0x39),
    analog("Mixer.B.Mixer FX.Amount", 0x3B),
    analog("Mixer.C.Mixer FX.Amount", 0x37),
    analog("Mixer.D.Mixer FX.Amount", 0x3D),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Browse.Encoder.Turn", 0x0B, false),
    encoder("Left.Move.Turn", 0x0B, true),
    encoder("Left.Loop.Turn", 0x0C, false),
    encoder("Right.Browse.Encoder.Turn", 0x0C, true),
    encoder("Right.Move.Turn", 0x0D, false),
    encoder("Right.Loop.Turn", 0x0D, true),
];

const JOGS: &[Jog] = &[
    jog("Left.Jog.Turn", 0x0E, 8),
    jog("Right.Jog.Turn", 0x12, 8),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 64), (0x02, 80)];

const WHEELS: [[usize; 8]; 2] = [
    [0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49],
    [0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x50, 0x51],
];

const CHANNEL_FIRST: [usize; 4] = [0x0F, 0x1E, 0x00, 0x2D];
const CHANNEL_SEGMENTS: usize = 14;
const MASTER_FIRST: [usize; 2] = [0x3C, 0x45];
const MASTER_DB: [f32; 8] = [-15.0, -10.0, -7.0, -5.0, -3.0, -2.0, -1.0, 0.0];

fn meters(feedback: &Feedback) -> Meters {
    let mut bytes = vec![0u8; METER_LEN];
    for (ch, &first) in CHANNEL_FIRST.iter().enumerate() {
        linear_meter(
            &mut bytes,
            first,
            CHANNEL_SEGMENTS,
            feedback.channels[ch].level,
            SEGMENT_ON,
        );
        bytes[first + CHANNEL_SEGMENTS] = if feedback.channels[ch].level > 1.0 {
            CLIP_ON
        } else {
            0
        };
    }
    for (side, &first) in MASTER_FIRST.iter().enumerate() {
        db_meter(
            &mut bytes,
            first,
            &MASTER_DB,
            feedback.master_level[side],
            SEGMENT_ON,
        );
        bytes[first + MASTER_DB.len()] = if feedback.master_level[side] > 1.0 {
            CLIP_ON
        } else {
            0
        };
    }
    Meters(bytes)
}

fn default_mappings() -> Vec<Mapping> {
    let mut extra = super::side_extras(&LAYOUT);
    extra.push(map(
        "Mixer.D.Ext",
        Action::LiveInput { deck: 3 },
        Interaction::Trigger,
    ));
    super::default_mappings(&LAYOUT, extra)
        .into_iter()
        .filter(|m| !matches!(m.action, Action::JogMode { .. }))
        .collect()
}

fn shifted(name: &str) -> Shifted {
    let (side, control) = name.split_once('.')?;
    let deck = match side {
        "Left" => 0,
        "Right" => 1,
        "Mixer" if control == "D.Ext" => return Some(None),
        _ => return super::common_shifted(name),
    };
    match control {
        "Sync" => Some(Some((Action::Master { deck }, Interaction::Trigger))),
        "Keylock" => Some(Some((Action::TempoReset { deck }, Interaction::Trigger))),
        _ => super::common_shifted(name),
    }
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
    encoder_report: 0x01,
    led_reports: &[(0x80, LED_LEN)],
    meter_report: Some((METER_REPORT, METER_LEN)),
    initial_reports: INITIAL_REPORTS,
    motors: false,
    sound_card: None,
    calibration: &[],
    stored_calibration: None,
    rings: false,
    screens: None,
    tempo_leds: None,
    wheel_leds: Some(WHEELS),
    strip_leds: None,
    meters: Some(meters),
    meter_leds: None,
    deck_leds: None,
    default_mappings,
    shifted,
    modifier_hooks: None,
    upgrade: None,
    jog_button_toggles_touch: true,
    samples_button_stems: true,
    samples_button_toggles: false,
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
    fn shift_sync_is_master_and_keylock_resets_the_tempo() {
        assert_eq!(
            shifted("Right.Sync"),
            Some(Some((Action::Master { deck: 1 }, Interaction::Trigger)))
        );
        assert_eq!(
            shifted("Left.Keylock"),
            Some(Some((Action::TempoReset { deck: 0 }, Interaction::Trigger)))
        );
        assert_eq!(
            shifted("Left.Cue"),
            Some(Some((
                Action::ReturnToStart { deck: 0 },
                Interaction::Trigger
            )))
        );
        assert_eq!(shifted("Mixer.D.Ext"), Some(None));
        assert_eq!(shifted("Left.Loop.Turn"), None);
    }

    #[test]
    fn defaults_cover_the_four_channels_and_the_deck_switches() {
        let defaults = default_mappings();
        let has = |name: &str| defaults.iter().any(|m| m.input == Input::Hid(name.into()));
        assert!(has("Mixer.D.Volume") && has("Mixer.C.EQ.Low"));
        assert!(has("Left.Quant") && has("Mixer.D.Ext"));
        assert!(!has("Left.Jog Mode.Jog"), "handled by the driver");
        assert!(LAYOUT.has("Left.Deck.C") && LAYOUT.has("Right.Deck.D"));
    }

    #[test]
    fn meters_fill_their_rows() {
        let mut f = Feedback::default();
        f.channels[2].level = 1.0;
        f.master_level = [1.2, 0.0];
        let m = meters(&f).0;
        assert!(m[0x00..0x0E].iter().all(|&b| b == SEGMENT_ON), "channel C");
        assert_eq!(m[0x0E], 0, "no clip at full scale");
        assert!(
            m[0x3C..0x44].iter().all(|&b| b == SEGMENT_ON),
            "master left"
        );
        assert_eq!(m[0x44], CLIP_ON);
        assert!(m[0x45..0x4E].iter().all(|&b| b == 0), "master right silent");
    }
}
