use std::sync::LazyLock;

use super::map;
use super::{analog, button, encoder, jog, linear_meter};
use crate::feedback::Feedback;
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Jog, Layout, Leds, Shifted, colour};
use crate::{Action, Interaction, Mapping};

pub const NAME: &str = "Kontrol MX2";
pub const SOURCE: &str = "MX2";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x2420;

const LED_LEN: usize = 0x69;
const METER_ON: u8 = 0x7E;
const JOG_STEPS: f64 = 1024.0;

fn b(name: impl Into<String>, byte: usize, mask: u8, led: usize) -> Button {
    button(name, byte, mask, Some(led), None)
}

fn plain(name: impl Into<String>, byte: usize, mask: u8) -> Button {
    button(name, byte, mask, None, None)
}

#[allow(clippy::too_many_lines)]
fn buttons() -> Vec<Button> {
    let mut v = vec![
        b("Left.FX.Button 1", 0x01, 0x01, 0x00),
        b("Left.FX.Button 2", 0x01, 0x02, 0x01),
        b("Left.FX.Button 3", 0x01, 0x04, 0x02),
        b("Left.FX.Button 4", 0x01, 0x08, 0x03),
        b("Left.Browse.Favorite", 0x01, 0x10, 0x04),
        b("Left.Browse.Add To Playlist", 0x01, 0x20, 0x05),
        b("Left.Browse.Preview", 0x01, 0x40, 0x06),
        b("Left.Browse.View", 0x01, 0x80, 0x07),
        b("Left.Transport Mode.Flux", 0x02, 0x01, 0x08),
        b("Left.Transport Mode.Reverse", 0x02, 0x02, 0x09),
        b("Left.Jog Mode.TT", 0x02, 0x04, 0x0A),
        b("Left.Jog Mode.Jog", 0x02, 0x08, 0x0B),
        b("Left.Shift", 0x02, 0x10, 0x0C),
        b("Left.Sync", 0x02, 0x20, 0x0D),
        b("Left.Master", 0x02, 0x40, 0x0E),
        b("Left.Keylock", 0x02, 0x80, 0x0F),
        b("Left.Pad Mode.Hotcues", 0x03, 0x01, 0x10),
        b("Left.Pad Mode.Stems", 0x03, 0x02, 0x11),
        b("Left.Pad Mode.Pattern", 0x03, 0x04, 0x12),
        b("Left.Pad Mode.Loops", 0x03, 0x08, 0x13),
        b("Left.Cue", 0x04, 0x10, 0x1C),
        b("Left.Play", 0x04, 0x20, 0x1D),
        b("Right.FX.Button 1", 0x04, 0x40, 0x1E),
        b("Right.FX.Button 2", 0x04, 0x80, 0x1F),
        b("Right.FX.Button 3", 0x05, 0x01, 0x20),
        b("Right.FX.Button 4", 0x05, 0x02, 0x21),
        b("Right.Browse.Favorite", 0x05, 0x04, 0x22),
        b("Right.Browse.Add To Playlist", 0x05, 0x08, 0x23),
        b("Right.Browse.Preview", 0x05, 0x10, 0x24),
        b("Right.Browse.View", 0x05, 0x20, 0x25),
        b("Right.Transport Mode.Flux", 0x05, 0x40, 0x26),
        b("Right.Transport Mode.Reverse", 0x05, 0x80, 0x27),
        b("Right.Jog Mode.TT", 0x06, 0x01, 0x28),
        b("Right.Jog Mode.Jog", 0x06, 0x02, 0x29),
        b("Right.Shift", 0x06, 0x04, 0x2A),
        b("Right.Sync", 0x06, 0x08, 0x2B),
        b("Right.Master", 0x06, 0x10, 0x2C),
        b("Right.Keylock", 0x06, 0x20, 0x2D),
        b("Right.Pad Mode.Hotcues", 0x06, 0x40, 0x2E),
        b("Right.Pad Mode.Stems", 0x06, 0x80, 0x2F),
        b("Right.Pad Mode.Pattern", 0x07, 0x01, 0x30),
        b("Right.Pad Mode.Loops", 0x07, 0x02, 0x31),
        b("Right.Cue", 0x08, 0x04, 0x3A),
        b("Right.Play", 0x08, 0x08, 0x3B),
        b("Mixer.A.FX Assign 1", 0x08, 0x10, 0x3C),
        b("Mixer.A.FX Assign 2", 0x08, 0x20, 0x3D),
        b("Mixer.A.Mixer FX.On", 0x08, 0x40, 0x3E),
        b("Mixer.A.Cue", 0x08, 0x80, 0x3F),
        b("Mixer.B.FX Assign 1", 0x09, 0x01, 0x40),
        b("Mixer.B.FX Assign 2", 0x09, 0x02, 0x41),
        b("Mixer.B.Mixer FX.On", 0x09, 0x04, 0x42),
        b("Mixer.B.Cue", 0x09, 0x08, 0x43),
        b("Mixer.Mixer FX Select.1", 0x09, 0x10, 0x44),
        b("Mixer.Mixer FX Select.2", 0x09, 0x20, 0x45),
        b("Mixer.Mixer FX Select.3", 0x09, 0x40, 0x46),
        b("Mixer.Mixer FX Select.4", 0x09, 0x80, 0x47),
        b("Mixer.Mixer FX Select.Filter", 0x0A, 0x01, 0x48),
        b("Mixer.Mic", 0x0A, 0x02, 0x49),
        plain("Left.Browse.Encoder.Push", 0x0A, 0x04),
        plain("Left.Move.Push", 0x0A, 0x08),
        plain("Left.Loop.Push", 0x0A, 0x10),
        plain("Right.Browse.Encoder.Push", 0x0A, 0x20),
        plain("Right.Move.Push", 0x0A, 0x40),
        plain("Right.Loop.Push", 0x0A, 0x80),
        plain("Left.Jog.Touch", 0x0B, 0x01),
        plain("Right.Jog.Touch", 0x0B, 0x02),
    ];
    let left: [(usize, u8); 8] = [
        (0x03, 0x10),
        (0x03, 0x20),
        (0x03, 0x40),
        (0x03, 0x80),
        (0x04, 0x01),
        (0x04, 0x02),
        (0x04, 0x04),
        (0x04, 0x08),
    ];
    let right: [(usize, u8); 8] = [
        (0x07, 0x04),
        (0x07, 0x08),
        (0x07, 0x10),
        (0x07, 0x20),
        (0x07, 0x40),
        (0x07, 0x80),
        (0x08, 0x01),
        (0x08, 0x02),
    ];
    for (i, (byte, mask)) in left.into_iter().enumerate() {
        v.push(b(format!("Left.Pads.{}", i + 1), byte, mask, 0x14 + i));
    }
    for (i, (byte, mask)) in right.into_iter().enumerate() {
        v.push(b(format!("Right.Pads.{}", i + 1), byte, mask, 0x32 + i));
    }
    v
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
    analog("Mixer.A.Gain", 0x11),
    analog("Mixer.A.EQ.High", 0x13),
    analog("Mixer.A.EQ.Mid", 0x15),
    analog("Mixer.A.EQ.Low", 0x17),
    analog("Mixer.A.Mixer FX.Amount", 0x19),
    analog("Mixer.B.Gain", 0x1B),
    analog("Mixer.B.EQ.High", 0x1D),
    analog("Mixer.B.EQ.Mid", 0x1F),
    analog("Mixer.B.EQ.Low", 0x21),
    analog("Mixer.B.Mixer FX.Amount", 0x23),
    analog("Mixer.Master", 0x25),
    analog("Mixer.Cue Mix", 0x27),
    analog("Mixer.Cue Volume", 0x29),
    analog("Mixer.A.Volume", 0x2B),
    analog("Mixer.B.Volume", 0x2D),
    analog("Mixer.Xfader", 0x2F),
    analog("Left.Tempo", 0x31),
    analog("Right.Tempo", 0x33),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Browse.Encoder.Turn", 0x0C, false),
    encoder("Left.Move.Turn", 0x0C, true),
    encoder("Left.Loop.Turn", 0x0D, false),
    encoder("Right.Browse.Encoder.Turn", 0x0D, true),
    encoder("Right.Move.Turn", 0x0E, false),
    encoder("Right.Loop.Turn", 0x0E, true),
];

const JOGS: &[Jog] = &[
    jog("Left.Jog.Turn", 0x08, 32),
    jog("Right.Jog.Turn", 0x10, 32),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 80), (0x02, 118)];

const STRIPS: [[usize; 6]; 2] = [
    [0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F],
    [0x50, 0x51, 0x52, 0x53, 0x54, 0x55],
];

const METER_FIRST: [usize; 2] = [0x57, 0x60];
const METER_SEGMENTS: usize = 8;
const MASTER_CLIP: usize = 0x56;

fn meter_leds(feedback: &Feedback, _decoder: &Decoder, leds: &mut Leds) {
    for (ch, &first) in METER_FIRST.iter().enumerate() {
        linear_meter(
            &mut leds.0,
            first,
            METER_SEGMENTS,
            feedback.channels[ch].level,
            METER_ON,
        );
    }
    let clip = feedback.master_level.iter().any(|&l| l > 1.0);
    leds.0[MASTER_CLIP] = if clip {
        colour::RED + colour::BRIGHT
    } else {
        colour::OFF
    };
}

fn default_mappings() -> Vec<Mapping> {
    let mut extra = super::side_extras(&LAYOUT);
    extra.push(map("Mixer.Mic", Action::MicToggle, Interaction::Trigger));
    super::default_mappings(&LAYOUT, extra)
}

fn shifted(name: &str) -> Shifted {
    let (side, control) = name.split_once('.')?;
    let unit = match side {
        "Left" => 0,
        "Right" => 1,
        _ => return super::common_shifted(name),
    };
    match control {
        "Pad Mode.Pattern" => Some(Some((Action::PatternClear { unit }, Interaction::Trigger))),
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
    jogs: JOGS,
    jog_steps_per_turn: JOG_STEPS,
    analog_report: 0x02,
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
    strip_leds: Some(STRIPS),
    meters: None,
    meter_leds: Some(meter_leds),
    deck_leds: None,
    default_mappings,
    shifted,
    modifier_hooks: None,
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: false,
    samples_button_toggles: false,
    fx_select_toggles: false,
    mx2_transport: true,
    shared_shift: false,
    connect_reports: &[],
    touch_strip: None,
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
    fn defaults_cover_the_fx_units_and_the_jog_modes() {
        let defaults = default_mappings();
        let has = |name: &str| defaults.iter().any(|m| m.input == Input::Hid(name.into()));
        assert!(has("Left.FX.Knob 1") && has("Right.FX.Button 4"));
        assert!(has("Left.Jog Mode.TT") && has("Right.Keylock"));
        assert!(has("Mixer.B.FX Assign 2") && has("Mixer.Mixer FX Select.Filter"));
        assert!(!has("Left.Pad Mode.Pattern"), "the driver's");
        assert!(
            defaults
                .iter()
                .any(|m| m.input == Input::Hid("Mixer.Mic".into()) && m.action == Action::MicToggle)
        );
    }

    #[test]
    fn shift_clears_the_pattern() {
        assert_eq!(
            shifted("Left.Pad Mode.Pattern"),
            Some(Some((
                Action::PatternClear { unit: 0 },
                Interaction::Trigger
            )))
        );
    }

    #[test]
    fn meters_and_the_master_clip() {
        let mut f = Feedback::default();
        f.channels[1].level = 1.0;
        f.master_level = [1.5, 0.0];
        let mut leds = Leds::for_layout(&LAYOUT);
        meter_leds(&f, &Decoder::new(&LAYOUT), &mut leds);
        assert_eq!(&leds.0[0x60..0x68], &[METER_ON; 8]);
        assert_eq!(leds.0[0x68], 0);
        assert_eq!(leds.0[MASTER_CLIP], colour::RED + colour::BRIGHT);
    }
}
