use std::sync::LazyLock;

use super::{analog, button, linear_meter};
use crate::Mapping;
use crate::feedback::Feedback;
use crate::poti::StoredCalibration;
use crate::s4mk3::{Analog, Button, Decoder, Layout, Leds, Shifted};

pub const NAME: &str = "Kontrol Z1";
pub const SOURCE: &str = "Z1";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1210;

const LED_LEN: usize = 0x15;
const SINGLE: Option<(u8, u8)> = Some((0x7F, 0x0A));
const METER_ON: u8 = 0x7F;

fn b(name: &str, mask: u8, led: usize) -> Button {
    button(name, 0x1D, mask, Some(led), SINGLE)
}

fn buttons() -> Vec<Button> {
    vec![
        b("Mixer.Mode", 0x02, 0x12),
        b("Mixer.A.Cue", 0x10, 0x0E),
        b("Mixer.B.Cue", 0x01, 0x0F),
        b("Mixer.A.Mixer FX.On", 0x04, 0x11),
        b("Mixer.B.Mixer FX.On", 0x08, 0x14),
    ]
}

const ANALOGS: &[Analog] = &[
    analog("Mixer.A.Gain", 0x01),
    analog("Mixer.A.EQ.High", 0x03),
    analog("Mixer.A.EQ.Mid", 0x05),
    analog("Mixer.A.EQ.Low", 0x07),
    analog("Mixer.A.Mixer FX.Amount", 0x09),
    analog("Mixer.B.Gain", 0x0B),
    analog("Mixer.B.EQ.High", 0x0D),
    analog("Mixer.B.EQ.Mid", 0x0F),
    analog("Mixer.B.EQ.Low", 0x11),
    analog("Mixer.B.Mixer FX.Amount", 0x13),
    analog("Mixer.Cue Mix", 0x15),
    analog("Mixer.A.Volume", 0x17),
    analog("Mixer.B.Volume", 0x19),
    analog("Mixer.Xfader", 0x1B),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 64)];

static CALIBRATION: StoredCalibration = StoredCalibration {
    reports: &[0xD0, 0xD1, 0xD2],
    product_id: PRODUCT_ID,
    first_entry: 0x12,
    with_centre: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    faders: &[11, 12, 13],
    no_centre: &[0, 5, 11, 12, 13],
    first_analog: 0x01,
};

const METER_FIRST: [usize; 2] = [0x00, 0x07];
const METER_SEGMENTS: usize = 7;

fn meter_leds(feedback: &Feedback, _decoder: &Decoder, leds: &mut Leds) {
    for (ch, &first) in METER_FIRST.iter().enumerate() {
        let mut bytes = [0u8; 8];
        linear_meter(
            &mut bytes,
            0,
            METER_SEGMENTS,
            feedback.channels[ch].level,
            METER_ON,
        );
        leds.0[first..first + METER_SEGMENTS].copy_from_slice(&bytes[..METER_SEGMENTS]);
    }
}

fn default_mappings() -> Vec<Mapping> {
    super::default_mappings(&LAYOUT, Vec::new())
}

fn shifted(_name: &str) -> Shifted {
    None
}

pub static LAYOUT: LazyLock<Layout> = LazyLock::new(|| Layout {
    name: NAME,
    source: SOURCE,
    vendor_id: VENDOR_ID,
    product_id: PRODUCT_ID,
    buttons: buttons(),
    switches: &[],
    analogs: ANALOGS,
    encoders: &[],
    erps: &[],
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
    use crate::{Action, Input};

    #[test]
    fn the_mixer_maps_its_strip_and_the_filter_buttons() {
        let defaults = default_mappings();
        let find = |name: &str| {
            defaults
                .iter()
                .find(|m| m.input == Input::Hid(name.into()))
                .map(|m| m.action)
        };
        assert_eq!(find("Mixer.A.Gain"), Some(Action::Gain { channel: 0 }));
        assert_eq!(
            find("Mixer.B.Mixer FX.Amount"),
            Some(Action::Filter { channel: 1 })
        );
        assert_eq!(
            find("Mixer.B.Mixer FX.On"),
            Some(Action::MixerFxOn { channel: 1 })
        );
        assert_eq!(find("Mixer.Cue Mix"), Some(Action::CueMix));
        assert_eq!(find("Mixer.Xfader"), Some(Action::Crossfader));
        assert_eq!(find("Mixer.Mode"), None, "the driver's");
    }

    #[test]
    fn faders_come_in_report_one() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        r[0x17] = 0xFF;
        r[0x18] = 0x0F;
        let events = d.decode(&r);
        let v = events
            .iter()
            .find(|e| e.input == Input::Hid("Mixer.A.Volume".into()))
            .expect("volume at once");
        assert!((v.value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn stored_calibration_maps_the_crossfader() {
        assert_eq!(CALIBRATION.analog_byte(13), 0x1B, "crossfader");
        assert_eq!(CALIBRATION.analog_byte(0), 0x01, "gain A");
        let mut data = vec![0u8; 96];
        data[1..4].copy_from_slice(&[0x00, 0x10, 0x12]);
        data[0x5C..0x5E].copy_from_slice(&300u16.to_le_bytes());
        data[0x5E..0x60].copy_from_slice(&3800u16.to_le_bytes());
        let potis = CALIBRATION.parse(&data).expect("valid");
        let mut d = Decoder::new(&LAYOUT);
        d.set_potis(&potis);
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        r[0x1B..0x1D].copy_from_slice(&3784u16.to_le_bytes());
        let events = d.decode(&r);
        let x = events
            .iter()
            .find(|e| e.input == Input::Hid("Mixer.Xfader".into()))
            .expect("crossfader");
        assert!((x.value - 1.0).abs() < 1e-6, "{}", x.value);
    }

    #[test]
    fn meters_fill_from_the_bottom() {
        let mut f = Feedback::default();
        f.channels[1].level = 3.0 / 7.0 + 0.01;
        let mut leds = Leds::for_layout(&LAYOUT);
        meter_leds(&f, &Decoder::new(&LAYOUT), &mut leds);
        assert_eq!(
            &leds.0[0x07..0x0E],
            &[METER_ON, METER_ON, METER_ON, METER_ON, 0, 0, 0]
        );
        assert!(leds.0[0x00..0x07].iter().all(|&b| b == 0));
    }
}
