//! The Kontrol S8, and what it shares with the S5 and the D2: a deck section
//! per side with eight pads, a touch strip, an FX section, four faders and
//! four endless knobs under a colour screen, and single-colour LEDs that take
//! a brightness byte each.

use std::sync::LazyLock;

use super::{analog, button, encoder, map};
use crate::feedback::Feedback;
use crate::s4mk3::screen::{Format, Panel};
use crate::s4mk3::{
    Analog, Button, Decoder, Encoder, Erp, Layout, Leds, Shifted, Switch, TouchStrip,
};
use crate::{Action, Interaction, Mapping, StemPart};

pub const NAME: &str = "Kontrol S8";
pub const SOURCE: &str = "S8";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1370;

pub(super) const LIT: u8 = 0x7F;
pub(super) const DIM: u8 = 0x10;
const SINGLE: Option<(u8, u8)> = Some((LIT, DIM));
pub(super) const SIDES: [&str; 2] = ["Left", "Right"];
pub(super) const CHANNELS: [&str; 4] = ["A", "B", "C", "D"];

/// A slide along a strip moves the deck about half a platter turn.
pub(super) const STRIP_STEPS_PER_TURN: f64 = 1000.0;

/// A strip that drives one side; positions about 15 to 1009.
pub(super) const fn side_strip(byte: usize, side: usize) -> TouchStrip {
    TouchStrip {
        report: 0x01,
        byte,
        fingers: 2,
        centre: 512,
        gap: 0,
        first: 15,
        last: 1009,
        bar: None,
        side: Some(side),
    }
}

/// Buttons are numbered as bits of the first report from `first`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Bits {
    pub first: usize,
}

impl Bits {
    fn at(self, index: usize) -> (usize, u8) {
        (self.first + index / 8, 1 << (index % 8))
    }

    /// A button with an LED of its own.
    pub fn lit(self, name: &str, index: usize, led: usize) -> Button {
        let (byte, mask) = self.at(index);
        button(name, byte, mask, Some(led), SINGLE)
    }

    pub fn plain(self, name: &str, index: usize) -> Button {
        let (byte, mask) = self.at(index);
        button(name, byte, mask, None, None)
    }

    /// A pad with its red, green and blue LEDs from `rgb`.
    pub fn pad(self, name: &str, index: usize, rgb: usize) -> Button {
        let mut b = self.plain(name, index);
        b.rgb = Some(rgb);
        b
    }
}

/// The controls of one deck section.
pub(super) struct Deck {
    pub side: &'static str,
    pub play: (usize, usize),
    pub cue: (usize, usize),
    pub sync: (usize, usize),
    pub shift: (usize, usize),
    pub flux: (usize, usize),
    pub hotcue: (usize, usize),
    pub freeze: (usize, usize),
    pub remix: (usize, usize),
    pub select: (usize, usize),
    pub back: (usize, usize),
    /// Push and touch of the browse encoder, and of the loop encoder.
    pub browse: (usize, usize),
    pub encoder: (usize, usize),
    pub fx_buttons: [(usize, usize); 4],
    pub fx_touch: [usize; 4],
    pub display: [(usize, usize); 8],
    pub pads: [(usize, usize); 8],
}

pub(super) fn deck_buttons(bits: Bits, d: &Deck) -> Vec<Button> {
    let n = |c: &str| format!("{}.{c}", d.side);
    let mut v = vec![
        bits.lit(&n("Play"), d.play.0, d.play.1),
        bits.lit(&n("Cue"), d.cue.0, d.cue.1),
        bits.lit(&n("Sync"), d.sync.0, d.sync.1),
        bits.lit(&n("Shift"), d.shift.0, d.shift.1),
        bits.lit(&n("Transport Mode.Flux"), d.flux.0, d.flux.1),
        bits.lit(&n("Pad Mode.Hotcues"), d.hotcue.0, d.hotcue.1),
        bits.lit(&n("Pad Mode.Stems"), d.freeze.0, d.freeze.1),
        bits.lit(&n("Pad Mode.Samples"), d.remix.0, d.remix.1),
        bits.lit(&n("Deck Select"), d.select.0, d.select.1),
        bits.lit(&n("Browse.Back"), d.back.0, d.back.1),
        bits.plain(&n("Browse.Encoder.Push"), d.browse.0),
        bits.plain(&n("Browse.Encoder.Touch"), d.browse.1),
        bits.plain(&n("Loop.Push"), d.encoder.0),
        bits.plain(&n("Loop.Touch"), d.encoder.1),
    ];
    for (k, &(index, led)) in d.fx_buttons.iter().enumerate() {
        v.push(bits.lit(&n(&format!("FX.Button {}", k + 1)), index, led));
    }
    for (k, &index) in d.fx_touch.iter().enumerate() {
        v.push(bits.plain(&n(&format!("FX.Knob {}.Touch", k + 1)), index));
    }
    for (k, &(index, led)) in d.display.iter().enumerate() {
        v.push(bits.lit(&n(&format!("Display.Button {}", k + 1)), index, led));
    }
    for (k, &(index, rgb)) in d.pads.iter().enumerate() {
        v.push(bits.pad(&n(&format!("Pads.{}", k + 1)), index, rgb));
    }
    v
}

/// The four faders, knobs and buttons under a screen: one per stem.
pub(super) struct Stems {
    pub side: &'static str,
    pub buttons: [(usize, usize); 4],
    pub fader_touch: [usize; 4],
    pub knob_touch: [usize; 4],
}

pub(super) fn stem_buttons(bits: Bits, s: &Stems) -> Vec<Button> {
    let mut v = Vec::new();
    for k in 0..4 {
        let (index, led) = s.buttons[k];
        v.push(bits.lit(&format!("{}.Buttons.{}", s.side, k + 1), index, led));
        v.push(bits.plain(
            &format!("{}.Faders.{}.Touch", s.side, k + 1),
            s.fader_touch[k],
        ));
        v.push(bits.plain(
            &format!("{}.Knobs.{}.Touch", s.side, k + 1),
            s.knob_touch[k],
        ));
    }
    v
}

/// The stems of the side's deck: fader = volume, knob = filter (switching it
/// on), button = mute.
pub(super) fn stem_mappings(side: usize) -> Vec<Mapping> {
    let name = SIDES[side];
    let stem = |stem, part| Action::Stem {
        deck: side,
        stem,
        part,
    };
    let mut v = Vec::new();
    for k in 0..4 {
        v.push(map(
            &format!("{name}.Faders.{}", k + 1),
            stem(k, StemPart::Volume),
            Interaction::Direct,
        ));
        v.push(map(
            &format!("{name}.Knobs.{}", k + 1),
            stem(k, StemPart::FilterEngaged),
            Interaction::Relative,
        ));
        v.push(map(
            &format!("{name}.Buttons.{}", k + 1),
            stem(k, StemPart::Mute),
            Interaction::Toggle,
        ));
    }
    v
}

/// What each side does beyond the shared defaults.
pub(super) fn side_mappings(layout: &Layout) -> Vec<Mapping> {
    let mut v = super::side_extras(layout);
    for (side, name) in SIDES.into_iter().enumerate() {
        let n = |c: &str| format!("{name}.{c}");
        if layout.has(&n("FX.Select")) {
            v.push(map(
                &n("FX.Select"),
                Action::FxMode { unit: side },
                Interaction::Trigger,
            ));
        }
        if layout.has(&n("Browse.Back")) {
            v.push(map(
                &n("Browse.Back"),
                Action::BrowserView,
                Interaction::Trigger,
            ));
        }
        if layout.has(&n("Faders.1")) {
            v.extend(stem_mappings(side));
        }
    }
    v
}

pub(super) fn shifted(name: &str) -> Shifted {
    let (side, control) = name.split_once('.')?;
    let deck = SIDES.iter().position(|&s| s == side)?;
    match control {
        "Loop.Turn" => Some(Some((Action::LoopMove { deck }, Interaction::Relative))),
        _ => super::common_shifted(name),
    }
}

/// The level meter of a channel, bottom first, from these thresholds in dB.
pub(super) const METER_DB: [f32; 11] = [
    -30.0, -20.0, -10.0, -6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 8.0,
];

pub(super) fn meter(leds: &mut [u8], first: usize, level: f32) {
    let db = super::db(level);
    for (k, &at) in METER_DB.iter().enumerate() {
        if let Some(b) = leds.get_mut(first + k) {
            *b = if db >= at { LIT } else { 0 };
        }
    }
}

/// The LEDs along a strip: the warm row lights at the end-of-track warning,
/// the cool row fills with how much of the track has played.
pub(super) fn strip_bar(
    leds: &mut [u8],
    warm: usize,
    cool: usize,
    count: usize,
    feedback: &Feedback,
    deck: usize,
) {
    let d = &feedback.decks[deck.min(feedback.decks.len() - 1)];
    let filled = crate::strip::played(d, count);
    let warning = d.playing && d.end_warning;
    for k in 0..count {
        let on = k < filled;
        if let Some(b) = leds.get_mut(warm + k) {
            *b = if on && warning { LIT } else { 0 };
        }
        if let Some(b) = leds.get_mut(cool + k) {
            *b = if on && !warning { LIT } else { 0 };
        }
    }
}

const BITS: Bits = Bits { first: 0x04 };

const LEFT: Deck = Deck {
    side: "Left",
    play: (44, 59),
    cue: (45, 58),
    sync: (46, 56),
    shift: (47, 55),
    flux: (36, 52),
    hotcue: (43, 44),
    freeze: (41, 48),
    remix: (40, 50),
    select: (34, 53),
    back: (23, 37),
    browse: (3, 53),
    encoder: (28, 48),
    fx_buttons: [(7, 25), (10, 26), (9, 27), (8, 28)],
    fx_touch: [54, 55, 60, 61],
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
        (35, 0),
        (32, 3),
        (24, 6),
        (25, 9),
        (37, 12),
        (38, 15),
        (39, 18),
        (33, 21),
    ],
};

const RIGHT: Deck = Deck {
    side: "Right",
    play: (108, 177),
    cue: (109, 176),
    sync: (110, 174),
    shift: (111, 173),
    flux: (100, 170),
    hotcue: (107, 162),
    freeze: (105, 166),
    remix: (104, 168),
    select: (98, 171),
    back: (87, 155),
    browse: (67, 117),
    encoder: (92, 112),
    fx_buttons: [(71, 143), (74, 144), (73, 145), (72, 146)],
    fx_touch: [118, 119, 124, 125],
    display: [
        (69, 147),
        (68, 148),
        (64, 149),
        (86, 150),
        (75, 151),
        (76, 152),
        (77, 153),
        (78, 154),
    ],
    pads: [
        (99, 118),
        (96, 121),
        (88, 124),
        (89, 127),
        (101, 130),
        (102, 133),
        (103, 136),
        (97, 139),
    ],
};

/// The loop pad mode, edit and capture of each side.
const EXTRA: [[(&str, usize, usize); 4]; 2] = [
    [
        ("Pad Mode.Loops", 42, 46),
        ("Edit", 16, 39),
        ("Capture", 21, 38),
        ("FX.Select", 6, 24),
    ],
    [
        ("Pad Mode.Loops", 106, 164),
        ("Edit", 80, 157),
        ("Capture", 85, 156),
        ("FX.Select", 70, 142),
    ],
];

const STEMS: [Stems; 2] = [
    Stems {
        side: "Left",
        buttons: [(20, 40), (19, 41), (18, 42), (17, 43)],
        fader_touch: [56, 57, 58, 59],
        knob_touch: [49, 50, 51, 52],
    },
    Stems {
        side: "Right",
        buttons: [(84, 158), (83, 159), (82, 160), (81, 161)],
        fader_touch: [120, 121, 122, 123],
        knob_touch: [113, 114, 115, 116],
    },
];

/// FX assign 1 and 2 of each channel, A to D.
const FX_ASSIGN: [[(usize, usize); 2]; 4] = [
    [(149, 282), (150, 283)],
    [(172, 286), (173, 287)],
    [(147, 280), (148, 281)],
    [(174, 288), (175, 289)],
];

/// The first LED of each channel's meter, A to D.
const METERS: [usize; 4] = [247, 258, 236, 269];

const SIDE_LEDS: [SideLeds; 2] = [
    SideLeds {
        warm: 93,
        cool: 68,
        ring: (64, 4),
        deck: 53,
    },
    SideLeds {
        warm: 211,
        cool: 186,
        ring: (182, 4),
        deck: 171,
    },
];

fn buttons() -> Vec<Button> {
    let mut v = deck_buttons(BITS, &LEFT);
    v.extend(deck_buttons(BITS, &RIGHT));
    for (side, extra) in SIDES.iter().zip(EXTRA) {
        for (name, index, led) in extra {
            v.push(BITS.lit(&format!("{side}.{name}"), index, led));
        }
    }
    for stems in &STEMS {
        v.extend(stem_buttons(BITS, stems));
    }
    for (ch, assign) in CHANNELS.iter().zip(FX_ASSIGN) {
        for (k, (index, led)) in assign.into_iter().enumerate() {
            v.push(BITS.lit(&format!("Mixer.{ch}.FX Assign {}", k + 1), index, led));
        }
    }
    v.extend([
        BITS.lit("Mixer.Snap", 151, 284),
        BITS.lit("Mixer.Quant", 171, 285),
        BITS.lit("Mixer.Mic.Assign 1", 146, 298),
        BITS.lit("Mixer.Mic.Assign 2", 168, 299),
        BITS.plain("Mixer.Tempo.Push", 154),
    ]);
    v
}

/// The latched switches of the mixer: crossfader side, filter on and cue of
/// each channel, as the unit keeps them.
const SWITCHES: &[Switch] = &[
    Switch {
        name: "Mixer.A.Xfader Assign",
        byte: 0x18,
        bit: 4,
        bits: 2,
    },
    Switch {
        name: "Mixer.B.Xfader Assign",
        byte: 0x18,
        bit: 0,
        bits: 2,
    },
    Switch {
        name: "Mixer.C.Xfader Assign",
        byte: 0x18,
        bit: 6,
        bits: 2,
    },
    Switch {
        name: "Mixer.D.Xfader Assign",
        byte: 0x18,
        bit: 2,
        bits: 2,
    },
    Switch {
        name: "Mixer.A.Filter Switch",
        byte: 0x27,
        bit: 0,
        bits: 1,
    },
    Switch {
        name: "Mixer.B.Filter Switch",
        byte: 0x27,
        bit: 1,
        bits: 1,
    },
    Switch {
        name: "Mixer.C.Filter Switch",
        byte: 0x27,
        bit: 2,
        bits: 1,
    },
    Switch {
        name: "Mixer.D.Filter Switch",
        byte: 0x27,
        bit: 3,
        bits: 1,
    },
    Switch {
        name: "Mixer.A.Cue Switch",
        byte: 0x28,
        bit: 0,
        bits: 1,
    },
    Switch {
        name: "Mixer.B.Cue Switch",
        byte: 0x28,
        bit: 1,
        bits: 1,
    },
    Switch {
        name: "Mixer.C.Cue Switch",
        byte: 0x28,
        bit: 2,
        bits: 1,
    },
    Switch {
        name: "Mixer.D.Cue Switch",
        byte: 0x28,
        bit: 3,
        bits: 1,
    },
];

/// Encoders: loop and browse of each side, and the tempo encoder.
const ENCODERS: &[Encoder] = &[
    encoder("Left.Loop.Turn", 0x01, false),
    encoder("Left.Browse.Encoder.Turn", 0x01, true),
    encoder("Right.Loop.Turn", 0x02, false),
    encoder("Right.Browse.Encoder.Turn", 0x02, true),
    encoder("Mixer.Tempo.Turn", 0x03, false),
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
    Erp {
        name: "Right.Knobs.1",
        byte: 0x09,
    },
    Erp {
        name: "Right.Knobs.2",
        byte: 0x0B,
    },
    Erp {
        name: "Right.Knobs.3",
        byte: 0x0D,
    },
    Erp {
        name: "Right.Knobs.4",
        byte: 0x0F,
    },
];

/// Knobs and faders: number *k* of the unit (from 8, after the endless knobs)
/// is the u16 at `0x11 + 2 (k − 8)` of the second report.
const fn p(name: &'static str, k: usize) -> Analog {
    analog(name, 0x11 + 2 * (k - 8))
}

const ANALOGS: &[Analog] = &[
    p("Left.Faders.1", 8),
    p("Left.Faders.2", 9),
    p("Left.Faders.3", 10),
    p("Left.Faders.4", 11),
    p("Right.Faders.1", 12),
    p("Right.Faders.2", 13),
    p("Right.Faders.3", 14),
    p("Right.Faders.4", 15),
    p("Mixer.A.Volume", 16),
    p("Mixer.B.Volume", 17),
    p("Mixer.C.Volume", 18),
    p("Mixer.D.Volume", 19),
    p("Mixer.Xfader", 20),
    p("Left.FX.Knob 1", 21),
    p("Left.FX.Knob 2", 22),
    p("Left.FX.Knob 3", 23),
    p("Left.FX.Knob 4", 24),
    p("Right.FX.Knob 1", 25),
    p("Right.FX.Knob 2", 26),
    p("Right.FX.Knob 3", 27),
    p("Right.FX.Knob 4", 28),
    p("Mixer.Xfader Curve", 33),
    p("Mixer.A.Gain", 34),
    p("Mixer.A.EQ.High", 35),
    p("Mixer.A.EQ.Mid", 36),
    p("Mixer.A.EQ.Low", 37),
    p("Mixer.A.Mixer FX.Amount", 38),
    p("Mixer.B.Gain", 39),
    p("Mixer.B.EQ.High", 40),
    p("Mixer.B.EQ.Mid", 41),
    p("Mixer.B.EQ.Low", 42),
    p("Mixer.B.Mixer FX.Amount", 43),
    p("Mixer.C.Gain", 44),
    p("Mixer.C.EQ.High", 45),
    p("Mixer.C.EQ.Mid", 46),
    p("Mixer.C.EQ.Low", 47),
    p("Mixer.C.Mixer FX.Amount", 48),
    p("Mixer.D.Gain", 49),
    p("Mixer.D.EQ.High", 50),
    p("Mixer.D.EQ.Mid", 51),
    p("Mixer.D.EQ.Low", 52),
    p("Mixer.D.Mixer FX.Amount", 53),
];

const STRIPS: [TouchStrip; 2] = [side_strip(0x1C, 0), side_strip(0x22, 1)];

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

/// The latched mixer switches set the app's state; the tempo encoder's push
/// taps the master tempo.
pub(super) fn mixer_switches() -> Vec<Mapping> {
    let mut v = Vec::new();
    for (channel, ch) in CHANNELS.into_iter().enumerate() {
        for (name, action) in [
            ("Filter Switch", Action::MixerFxOn { channel }),
            ("Cue Switch", Action::MonitorCue { channel }),
        ] {
            let mut m = map(&format!("Mixer.{ch}.{name}"), action, Interaction::Direct);
            m.positions = vec![(0, 0.0), (1, 1.0)];
            v.push(m);
        }
    }
    v
}

fn default_mappings() -> Vec<Mapping> {
    let mut extra = side_mappings(&LAYOUT);
    extra.extend(mixer_switches());
    extra.extend([
        map("Mixer.Snap", Action::Snap, Interaction::Toggle),
        map("Mixer.Tempo.Push", Action::MasterTap, Interaction::Trigger),
    ]);
    super::default_mappings(&LAYOUT, extra)
}

/// The LEDs of a side that follow its deck: the rows under the strip, the
/// ring round the loop encoder (first LED and count) and the deck button.
pub(super) struct SideLeds {
    pub warm: usize,
    pub cool: usize,
    pub ring: (usize, usize),
    pub deck: usize,
}

pub(super) const STRIP_POSITIONS: usize = 25;

/// The strip shows how much of the track has played, the ring lights while a
/// loop plays and the deck button while the side drives its lower deck.
pub(super) fn side_leds(
    sides: &[SideLeds],
    feedback: &Feedback,
    layer: [usize; 2],
    leds: &mut Leds,
) {
    for (side, l) in sides.iter().enumerate() {
        let deck = layer[side];
        strip_bar(&mut leds.0, l.warm, l.cool, STRIP_POSITIONS, feedback, deck);
        let looping = feedback.decks[deck].loop_active;
        for k in 0..l.ring.1 {
            light(&mut leds.0, l.ring.0 + k, looping);
        }
        light(&mut leds.0, l.deck, deck >= 2);
    }
}

/// Lights an LED of `leds` if `on`, dim if not.
pub(super) fn light(leds: &mut [u8], at: usize, on: bool) {
    if let Some(b) = leds.get_mut(at) {
        *b = if on { LIT } else { DIM };
    }
}

fn deck_leds(feedback: &Feedback, _decoder: &Decoder, layer: [usize; 2], leds: &mut Leds) {
    side_leds(&SIDE_LEDS, feedback, layer, leds);
}

fn meter_leds(feedback: &Feedback, _decoder: &Decoder, leds: &mut Leds) {
    for (channel, &first) in METERS.iter().enumerate() {
        meter(&mut leds.0, first, feedback.channels[channel].level);
    }
}

pub static LAYOUT: LazyLock<Layout> = LazyLock::new(|| Layout {
    name: NAME,
    source: SOURCE,
    vendor_id: VENDOR_ID,
    product_id: PRODUCT_ID,
    buttons: buttons(),
    switches: SWITCHES,
    analogs: ANALOGS,
    encoders: ENCODERS,
    erps: ERPS,
    jogs: &[],
    jog_steps_per_turn: STRIP_STEPS_PER_TURN,
    analog_report: 0x02,
    jog_report: 0x03,
    encoder_report: 0x01,
    led_reports: &[(0x80, 118), (0x81, 118), (0x82, 73)],
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

    fn find(name: &str) -> Vec<Mapping> {
        default_mappings()
            .into_iter()
            .filter(|m| m.input == Input::Hid(name.into()))
            .collect()
    }

    #[test]
    fn the_layout_is_consistent() {
        crate::s4mk3::tests::check_layout(&LAYOUT);
        assert_eq!(LAYOUT.led_len(), 309);
    }

    #[test]
    fn buttons_and_knobs_decode_from_their_bits() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        d.decode(&r);
        r[0x04 + 44 / 8] |= 1 << (44 % 8);
        let events = d.decode(&r);
        assert!(
            events
                .iter()
                .any(|e| e.input == Input::Hid("Left.Play".into()) && e.pressed == Some(true))
        );
        let mut r = vec![0u8; 128];
        r[0] = 0x02;
        d.decode(&r);
        let at = 0x11 + 2 * (16 - 8);
        r[at] = 0xFF;
        r[at + 1] = 0x0F;
        let events = d.decode(&r);
        let v = events
            .iter()
            .find(|e| e.input == Input::Hid("Mixer.A.Volume".into()))
            .expect("volume");
        assert!((v.value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn endless_knobs_turn_the_stem_filters() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 128];
        r[0] = 0x02;
        r[1..3].copy_from_slice(&990u16.to_le_bytes());
        d.decode(&r);
        r[1..3].copy_from_slice(&60u16.to_le_bytes());
        let events = d.decode(&r);
        let knob = events
            .iter()
            .find(|e| e.input == Input::Hid("Left.Knobs.1".into()))
            .expect("turned across the top of the turn");
        assert!((knob.value - 2.0).abs() < 1e-6, "{}", knob.value);
        let m = &find("Left.Knobs.1")[0];
        assert_eq!(
            m.action,
            Action::Stem {
                deck: 0,
                stem: 0,
                part: StemPart::FilterEngaged
            }
        );
        assert_eq!(
            find("Right.Faders.3")[0].action,
            Action::Stem {
                deck: 1,
                stem: 2,
                part: StemPart::Volume
            }
        );
    }

    #[test]
    fn the_mixer_switches_set_cue_filter_and_crossfader_side() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        d.decode(&r);
        r[0x28] = 0b0010;
        r[0x18] = 0b10_0000;
        let events = d.decode(&r);
        let cue = events
            .iter()
            .find(|e| e.input == Input::Hid("Mixer.B.Cue Switch".into()))
            .expect("cue B");
        assert_eq!(cue.raw, 1);
        assert!(
            events
                .iter()
                .all(|e| e.input != Input::Hid("Mixer.A.Cue Switch".into()))
        );
        let side = events
            .iter()
            .find(|e| e.input == Input::Hid("Mixer.A.Xfader Assign".into()))
            .expect("crossfader side of A");
        assert_eq!(side.raw, 2);
        assert_eq!(
            find("Mixer.B.Cue Switch")[0].action,
            Action::MonitorCue { channel: 1 }
        );
        assert_eq!(
            find("Mixer.C.Filter Switch")[0].action,
            Action::MixerFxOn { channel: 2 }
        );
    }

    #[test]
    fn each_strip_drives_its_own_side() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        d.decode(&r);
        r[0x22..0x24].copy_from_slice(&100u16.to_le_bytes());
        let events = d.decode(&r);
        assert!(events.iter().any(|e| e.input == Input::Hid("Right.Jog.Touch".into()) && e.pressed == Some(true)));
        assert!(
            events
                .iter()
                .all(|e| e.input != Input::Hid("Strip.Touch".into())),
            "no whole-strip gestures"
        );
        r[0x22..0x24].copy_from_slice(&140u16.to_le_bytes());
        let events = d.decode(&r);
        let turn = events
            .iter()
            .find(|e| e.input == Input::Hid("Right.Jog.Turn".into()))
            .expect("slide");
        assert!((turn.value - 40.0).abs() < 1e-6);
    }

    #[test]
    fn strips_show_the_track_and_meters_the_level() {
        let mut f = Feedback::default();
        f.decks[1].loaded = true;
        f.decks[1].duration_s = 100.0;
        f.decks[1].position_s = 50.0;
        f.channels[0].level = 1.0;
        let d = Decoder::new(&LAYOUT);
        let mut leds = Leds::for_layout(&LAYOUT);
        deck_leds(&f, &d, [0, 1], &mut leds);
        meter_leds(&f, &d, &mut leds);
        let cool = SIDE_LEDS[1].cool;
        assert_eq!(leds.0[cool], LIT);
        assert_eq!(leds.0[cool + 12], LIT);
        assert_eq!(leds.0[cool + 13], 0);
        assert_eq!(leds.0[SIDE_LEDS[0].cool], 0, "empty deck on the left");
        let lit = (0..11).filter(|&k| leds.0[METERS[0] + k] == LIT).count();
        assert_eq!(lit, 7, "0 dB lights up to the 0 dB step");
    }
}
