//! The Kontrol D2: one deck section of the S8, with its screen, strip, FX
//! section, faders and endless knobs, driving the left deck pair.

use std::sync::LazyLock;

use super::s8::{self, Bits, Deck, SideLeds, Stems};
use super::{analog, encoder, map};
use crate::feedback::Feedback;
use crate::s4mk3::screen::{Format, Panel};
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Erp, Layout, Leds, TouchStrip};
use crate::{Action, Interaction, Mapping};

pub const NAME: &str = "Kontrol D2";
pub const SOURCE: &str = "D2";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1400;

const BITS: Bits = Bits { first: 0x02 };

const DECK: Deck = Deck {
    side: "Left",
    play: (52, 59),
    cue: (53, 58),
    sync: (54, 56),
    shift: (55, 55),
    flux: (44, 52),
    hotcue: (51, 44),
    freeze: (49, 48),
    remix: (48, 50),
    select: (42, 53),
    back: (23, 37),
    browse: (3, 61),
    encoder: (36, 56),
    fx_buttons: [(7, 25), (10, 26), (9, 27), (8, 28)],
    fx_touch: [62, 63, 68, 69],
    display: [
        (5, 29),
        (4, 30),
        (0, 31),
        (22, 32),
        (11, 33),
        (12, 34),
        (13, 35),
        (14, 36),
    ],
    pads: [
        (43, 0),
        (40, 3),
        (32, 6),
        (33, 9),
        (45, 12),
        (46, 15),
        (47, 18),
        (41, 21),
    ],
};

const STEMS: Stems = Stems {
    side: "Left",
    buttons: [(20, 40), (19, 41), (18, 42), (17, 43)],
    fader_touch: [64, 65, 66, 67],
    knob_touch: [57, 58, 59, 60],
};

/// FX assign of the deck to units 1 to 4.
const FX_ASSIGN: [(usize, usize); 4] = [(24, 118), (25, 119), (26, 120), (27, 121)];

const SIDE_LEDS: [SideLeds; 1] = [SideLeds {
    warm: 93,
    cool: 68,
    ring: (64, 4),
    deck: 53,
}];

fn buttons() -> Vec<Button> {
    let mut v = s8::deck_buttons(BITS, &DECK);
    v.extend(s8::stem_buttons(BITS, &STEMS));
    v.extend([
        BITS.lit("Left.Pad Mode.Loops", 50, 46),
        BITS.lit("Left.Edit", 16, 39),
        BITS.lit("Left.Capture", 21, 38),
        BITS.lit("Left.FX.Select", 6, 24),
    ]);
    for (k, (index, led)) in FX_ASSIGN.into_iter().enumerate() {
        v.push(BITS.lit(&format!("Left.FX Assign {}", k + 1), index, led));
    }
    v
}

const ENCODERS: &[Encoder] = &[
    encoder("Left.Loop.Turn", 0x01, false),
    encoder("Left.Browse.Encoder.Turn", 0x01, true),
];

const ERPS: &[Erp] = &[
    Erp {
        name: "Left.Knobs.1",
        byte: 0x01,
    },
    Erp {
        name: "Left.Knobs.2",
        byte: 0x03,
    },
    Erp {
        name: "Left.Knobs.3",
        byte: 0x05,
    },
    Erp {
        name: "Left.Knobs.4",
        byte: 0x07,
    },
];

/// Knob or fader number *k* of the unit (from 4, after the endless knobs) is
/// the u16 at `9 + 2 (k − 4)` of the second report.
const fn p(name: &'static str, k: usize) -> Analog {
    analog(name, 9 + 2 * (k - 4))
}

const ANALOGS: &[Analog] = &[
    p("Left.Faders.1", 4),
    p("Left.Faders.2", 5),
    p("Left.Faders.3", 6),
    p("Left.Faders.4", 7),
    p("Left.FX.Knob 1", 8),
    p("Left.FX.Knob 2", 9),
    p("Left.FX.Knob 3", 10),
    p("Left.FX.Knob 4", 11),
];

const STRIPS: [TouchStrip; 1] = [s8::side_strip(0x0D, 0)];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 64), (0x02, 64)];

static PANEL: Panel = Panel {
    vendor_id: VENDOR_ID,
    product_id: PRODUCT_ID,
    interface: None,
    width: 480,
    height: 272,
    count: 1,
    format: Format::Rgb565Runs,
};

/// The deck's channel goes to FX units 1 to 4.
fn fx_assign() -> Vec<Mapping> {
    (0..4)
        .map(|unit| {
            map(
                &format!("Left.FX Assign {}", unit + 1),
                Action::FxAssign { channel: 0, unit },
                Interaction::Toggle,
            )
        })
        .collect()
}

fn default_mappings() -> Vec<Mapping> {
    let mut extra = s8::side_mappings(&LAYOUT);
    extra.extend(fx_assign());
    super::default_mappings(&LAYOUT, extra)
}

fn deck_leds(feedback: &Feedback, _decoder: &Decoder, layer: [usize; 2], leds: &mut Leds) {
    s8::side_leds(&SIDE_LEDS, feedback, layer, leds);
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
    erps: ERPS,
    jogs: &[],
    jog_steps_per_turn: s8::STRIP_STEPS_PER_TURN,
    analog_report: 0x02,
    jog_report: 0x03,
    encoder_report: 0x01,
    led_reports: &[(0x80, 122)],
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
    meter_leds: None,
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
    use crate::{Input, StemPart};

    #[test]
    fn the_layout_is_consistent() {
        crate::s4mk3::tests::check_layout(&LAYOUT);
        assert_eq!(LAYOUT.led_len(), 122);
    }

    #[test]
    fn one_deck_section_with_its_stems_and_fx() {
        let defaults = default_mappings();
        let find = |name: &str| {
            defaults
                .iter()
                .find(|m| m.input == Input::Hid(name.into()))
                .map(|m| m.action)
        };
        assert_eq!(find("Left.Play"), Some(Action::PlayPause { deck: 0 }));
        assert_eq!(find("Left.FX.Knob 1"), Some(Action::FxDryWet { unit: 0 }));
        assert_eq!(
            find("Left.FX Assign 3"),
            Some(Action::FxAssign {
                channel: 0,
                unit: 2
            })
        );
        assert_eq!(
            find("Left.Buttons.2"),
            Some(Action::Stem {
                deck: 0,
                stem: 1,
                part: StemPart::Mute
            })
        );
        assert_eq!(find("Right.Play"), None, "one side only");
    }

    #[test]
    fn buttons_start_at_the_second_byte() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        d.decode(&r);
        r[0x02 + 52 / 8] |= 1 << (52 % 8);
        let events = d.decode(&r);
        assert!(
            events
                .iter()
                .any(|e| e.input == Input::Hid("Left.Play".into()) && e.pressed == Some(true))
        );
    }
}
