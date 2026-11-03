//! The Kontrol S5: the S8's deck sections and four-channel mixer, without
//! the faders and endless knobs under the screens.

use std::sync::LazyLock;

use super::s8::{self, Bits, Deck, SideLeds};
use super::{analog, encoder, map};
use crate::feedback::Feedback;
use crate::s4mk3::screen::{Format, Panel};
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Layout, Leds, TouchStrip};
use crate::{Action, Interaction, Mapping};

pub const NAME: &str = "Kontrol S5";
pub const SOURCE: &str = "S5";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1420;

const BITS: Bits = Bits { first: 0x04 };

const LEFT: Deck = Deck {
    side: "Left",
    play: (42, 53),
    cue: (45, 52),
    sync: (43, 50),
    shift: (44, 49),
    flux: (48, 48),
    hotcue: (54, 42),
    freeze: (49, 44),
    remix: (55, 46),
    select: (31, 38),
    back: (24, 36),
    browse: (25, 84),
    encoder: (30, 85),
    fx_buttons: [(18, 24), (21, 25), (19, 26), (20, 27)],
    fx_touch: [3, 2, 1, 0],
    display: [
        (38, 28),
        (33, 29),
        (39, 30),
        (32, 31),
        (28, 32),
        (27, 33),
        (29, 34),
        (26, 35),
    ],
    pads: [
        (52, 0),
        (51, 3),
        (53, 6),
        (50, 9),
        (46, 12),
        (41, 15),
        (47, 18),
        (40, 21),
    ],
};

const RIGHT: Deck = Deck {
    side: "Right",
    play: (69, 157),
    cue: (66, 156),
    sync: (65, 154),
    shift: (68, 153),
    flux: (96, 152),
    hotcue: (104, 146),
    freeze: (111, 148),
    remix: (103, 150),
    select: (90, 142),
    back: (89, 140),
    browse: (88, 86),
    encoder: (91, 87),
    fx_buttons: [(80, 128), (81, 129), (82, 130), (83, 131)],
    fx_touch: [7, 6, 5, 4],
    display: [
        (76, 132),
        (77, 133),
        (78, 134),
        (79, 135),
        (109, 136),
        (106, 137),
        (110, 138),
        (105, 139),
    ],
    pads: [
        (99, 104),
        (101, 107),
        (97, 110),
        (108, 113),
        (100, 116),
        (98, 119),
        (102, 122),
        (107, 125),
    ],
};

/// Per channel, A to D: FX assign 1 and 2, filter on and cue.
const CHANNEL_BUTTONS: [[(usize, usize); 4]; 4] = [
    [(9, 210), (14, 211), (17, 219), (16, 223)],
    [(72, 214), (73, 215), (58, 220), (61, 225)],
    [(11, 208), (10, 209), (13, 218), (12, 222)],
    [(74, 216), (75, 217), (62, 221), (60, 226)],
];

const METERS: [usize; 4] = [238, 249, 227, 260];
const CLIP: [usize; 2] = [271, 272];

const SIDE_LEDS: [SideLeds; 2] = [
    SideLeds {
        warm: 79,
        cool: 54,
        ring: (40, 1),
        deck: 38,
    },
    SideLeds {
        warm: 183,
        cool: 158,
        ring: (144, 1),
        deck: 142,
    },
];

fn buttons() -> Vec<Button> {
    let mut v = s8::deck_buttons(BITS, &LEFT);
    v.extend(s8::deck_buttons(BITS, &RIGHT));
    for (ch, row) in s8::CHANNELS.iter().zip(CHANNEL_BUTTONS) {
        for (name, (index, led)) in ["FX Assign 1", "FX Assign 2", "Mixer FX.On", "Cue"]
            .into_iter()
            .zip(row)
        {
            v.push(BITS.lit(&format!("Mixer.{ch}.{name}"), index, led));
        }
    }
    v.extend([
        BITS.lit("Mixer.Snap", 8, 212),
        BITS.lit("Mixer.Quant", 15, 213),
        BITS.plain("Mixer.Tempo.Push", 63),
    ]);
    v
}

/// Encoders: browse and loop of each side, and the tempo encoder.
const ENCODERS: &[Encoder] = &[
    encoder("Left.Browse.Encoder.Turn", 0x01, false),
    encoder("Left.Loop.Turn", 0x01, true),
    encoder("Right.Browse.Encoder.Turn", 0x02, false),
    encoder("Right.Loop.Turn", 0x02, true),
    encoder("Mixer.Tempo.Turn", 0x03, false),
];

/// Knob or fader number *k* of the unit is the u16 at `1 + 2k` of the second
/// report.
const fn p(name: &'static str, k: usize) -> Analog {
    analog(name, 1 + 2 * k)
}

const ANALOGS: &[Analog] = &[
    p("Mixer.A.Volume", 0),
    p("Mixer.B.Volume", 1),
    p("Mixer.C.Volume", 2),
    p("Mixer.D.Volume", 3),
    p("Mixer.Xfader", 4),
    p("Mixer.Cue Mix", 7),
    p("Mixer.Cue Volume", 8),
    p("Mixer.A.Gain", 9),
    p("Mixer.A.EQ.High", 10),
    p("Mixer.A.EQ.Mid", 11),
    p("Mixer.A.EQ.Low", 12),
    p("Mixer.A.Mixer FX.Amount", 13),
    p("Mixer.B.Gain", 14),
    p("Mixer.B.EQ.High", 15),
    p("Mixer.B.EQ.Mid", 16),
    p("Mixer.B.EQ.Low", 17),
    p("Mixer.B.Mixer FX.Amount", 18),
    p("Mixer.C.Gain", 19),
    p("Mixer.C.EQ.High", 20),
    p("Mixer.C.EQ.Mid", 21),
    p("Mixer.C.EQ.Low", 22),
    p("Mixer.C.Mixer FX.Amount", 23),
    p("Mixer.D.Gain", 24),
    p("Mixer.D.EQ.High", 25),
    p("Mixer.D.EQ.Mid", 26),
    p("Mixer.D.EQ.Low", 27),
    p("Mixer.D.Mixer FX.Amount", 28),
    p("Left.FX.Knob 1", 29),
    p("Left.FX.Knob 2", 30),
    p("Left.FX.Knob 3", 31),
    p("Left.FX.Knob 4", 32),
    p("Right.FX.Knob 1", 33),
    p("Right.FX.Knob 2", 34),
    p("Right.FX.Knob 3", 35),
    p("Right.FX.Knob 4", 36),
];

const STRIPS: [TouchStrip; 2] = [s8::side_strip(0x14, 0), s8::side_strip(0x1A, 1)];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 64), (0x02, 128)];

static PANEL: Panel = Panel {
    vendor_id: VENDOR_ID,
    product_id: PRODUCT_ID,
    interface: None,
    width: 480,
    height: 272,
    count: 2,
    format: Format::Rgb565Runs,
};

fn default_mappings() -> Vec<Mapping> {
    let mut extra = s8::side_mappings(&LAYOUT);
    extra.extend([
        map("Mixer.Snap", Action::Snap, Interaction::Toggle),
        map("Mixer.Tempo.Push", Action::MasterTap, Interaction::Trigger),
    ]);
    super::default_mappings(&LAYOUT, extra)
}

fn deck_leds(feedback: &Feedback, _decoder: &Decoder, layer: [usize; 2], leds: &mut Leds) {
    s8::side_leds(&SIDE_LEDS, feedback, layer, leds);
}

fn meter_leds(feedback: &Feedback, _decoder: &Decoder, leds: &mut Leds) {
    for (channel, &first) in METERS.iter().enumerate() {
        s8::meter(&mut leds.0, first, feedback.channels[channel].level);
    }
    for (side, &at) in CLIP.iter().enumerate() {
        leds.0[at] = if feedback.master_level[side] > 1.0 {
            s8::LIT
        } else {
            0
        };
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
    jogs: &[],
    jog_steps_per_turn: s8::STRIP_STEPS_PER_TURN,
    analog_report: 0x02,
    jog_report: 0x03,
    encoder_report: 0x01,
    led_reports: &[(0x80, 104), (0x81, 104), (0x82, 73)],
    meter_report: None,
    initial_reports: INITIAL_REPORTS,
    motors: false,
    sound_card: None,
    calibration: &[],
    stored_calibration: None,
    rings: false,
    screens: Some(&PANEL),
    tempo_leds: None,
    wheel_leds: None,
    strip_leds: None,
    meters: None,
    meter_leds: Some(meter_leds),
    deck_leds: Some(deck_leds),
    default_mappings,
    shifted: s8::shifted,
    modifier_hooks: None,
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: false,
    samples_button_toggles: false,
    fx_select_toggles: false,
    mx2_transport: false,
    shared_shift: false,
    connect_reports: &[],
    touch_strips: &STRIPS,
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
    fn the_layout_is_consistent() {
        crate::s4mk3::tests::check_layout(&LAYOUT);
        assert_eq!(LAYOUT.led_len(), 281);
    }

    #[test]
    fn the_mixer_buttons_and_faders_map_to_the_channels() {
        let defaults = default_mappings();
        let find = |name: &str| {
            defaults
                .iter()
                .find(|m| m.input == Input::Hid(name.into()))
                .map(|m| m.action)
        };
        assert_eq!(find("Mixer.C.Cue"), Some(Action::MonitorCue { channel: 2 }));
        assert_eq!(
            find("Mixer.D.Mixer FX.On"),
            Some(Action::MixerFxOn { channel: 3 })
        );
        assert_eq!(find("Mixer.B.Volume"), Some(Action::Volume { channel: 1 }));
        assert_eq!(find("Mixer.Cue Mix"), Some(Action::CueMix));
        assert_eq!(
            find("Left.Loop.Turn"),
            Some(Action::LoopSizeStep { deck: 0 })
        );
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 128];
        r[0] = 0x02;
        d.decode(&r);
        r[1 + 2 * 29] = 0xFF;
        r[2 + 2 * 29] = 0x0F;
        let events = d.decode(&r);
        assert!(
            events
                .iter()
                .any(|e| e.input == Input::Hid("Left.FX.Knob 1".into()))
        );
    }

    #[test]
    fn the_master_clips_light_over_full_scale() {
        let f = Feedback {
            master_level: [1.2, 0.5],
            ..Feedback::default()
        };
        let d = Decoder::new(&LAYOUT);
        let mut leds = Leds::for_layout(&LAYOUT);
        meter_leds(&f, &d, &mut leds);
        assert_eq!(leds.0[CLIP[0]], s8::LIT);
        assert_eq!(leds.0[CLIP[1]], 0);
    }
}
