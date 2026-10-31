use std::sync::LazyLock;

use super::{linear_meter, loop_display, map};
use crate::feedback::Feedback;
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Jog, Layout, Leds, Shifted};
use crate::{Action, Interaction, LoopRecorderPart, Mapping};

pub mod usb;

pub const NAME: &str = "Kontrol S4 MK1";
pub const SOURCE: &str = "S4 MK1";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0xBAFF;

/// The device speaks USB bulk, not HID: its blocks are rebuilt here as the
/// reports the shared decoder reads.
const BUTTON_REPORT: u8 = 0x01;
const ANALOG_REPORT: u8 = 0x02;
const JOG_REPORT: u8 = 0x03;
const ENCODER_REPORT: u8 = 0x04;
const BUTTON_BYTES: usize = 12;
const TOUCH_BYTE: usize = 1 + BUTTON_BYTES;
const ANALOGS_LEN: usize = 36;
const ENCODERS_LEN: usize = 9;
/// Analog values are held back until every analog block has arrived, or
/// this many reads have passed, so no control starts from a false zero.
const ANALOG_WAIT: u32 = 50;

const LED_COUNT: usize = 192;
pub const LED_MAX: u8 = 0x1F;
const SINGLE: Option<(u8, u8)> = Some((LED_MAX, 0x04));
const JOG_BITS: u32 = 10;
const JOG_STEPS: f64 = 1024.0;

/// Wheel distance sensors: the analog index of each side.
const DISTANCE: [usize; 2] = [10, 11];
/// How far the reading must rise over its rest value to count as a touch,
/// and where it falls back to released.
const TOUCH_ON: u16 = 400;
const TOUCH_OFF: u16 = 250;

fn bit(name: impl Into<String>, index: usize, led: Option<usize>) -> Button {
    Button {
        name: name.into(),
        byte: 1 + index / 8,
        bit: u8::try_from(index % 8).unwrap_or(0),
        led,
        single: led.and(SINGLE),
        rgb: None,
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

/// One deck: (button bit, LED) for each control, the right deck shifted.
struct DeckBits {
    side: &'static str,
    pads: [(usize, usize); 4],
    shift: (usize, usize),
    sync: (usize, usize),
    cue: (usize, usize),
    play: (usize, usize),
    deck_select: (usize, usize),
    remix: [(usize, usize); 4],
    loop_in: (usize, usize),
    loop_out: (usize, usize),
    load: (usize, usize),
    tempo_up: (usize, usize),
    tempo_down: (usize, usize),
    loop_push: usize,
    move_push: usize,
    fx: [(usize, usize); 4],
    fx_mode: (usize, usize),
    indicators: [(&'static str, usize); 4],
}

const DECKS: [DeckBits; 2] = [
    DeckBits {
        side: "Left",
        pads: [(0, 22), (2, 20), (4, 18), (6, 16)],
        shift: (1, 24),
        sync: (3, 27),
        cue: (5, 26),
        play: (7, 25),
        deck_select: (8, 36),
        remix: [(9, 31), (11, 30), (13, 29), (15, 28)],
        loop_in: (10, 47),
        loop_out: (12, 46),
        load: (14, 44),
        tempo_up: (16, 33),
        tempo_down: (17, 32),
        loop_push: 18,
        move_push: 19,
        fx: [(65, 153), (66, 154), (67, 155), (68, 156)],
        fx_mode: (69, 157),
        indicators: [
            ("Deck.A", 37),
            ("Display.Master", 34),
            ("Display.Key Lock", 35),
            ("Display.On Air", 39),
        ],
    },
    DeckBits {
        side: "Right",
        pads: [(56, 78), (58, 76), (60, 74), (62, 72)],
        shift: (57, 64),
        sync: (59, 67),
        cue: (61, 66),
        play: (63, 65),
        deck_select: (48, 188),
        remix: [(49, 71), (51, 70), (53, 69), (55, 68)],
        loop_in: (50, 183),
        loop_out: (52, 182),
        load: (54, 180),
        tempo_up: (40, 185),
        tempo_down: (41, 184),
        loop_push: 42,
        move_push: 43,
        fx: [(89, 129), (90, 130), (91, 131), (92, 132)],
        fx_mode: (93, 133),
        indicators: [
            ("Deck.B", 189),
            ("Display.Master", 186),
            ("Display.Key Lock", 187),
            ("Display.On Air", 191),
        ],
    },
];

/// Mixer buttons: name, bit and LED.
const MIXER: &[(&str, usize, Option<usize>)] = &[
    ("Mixer.LoopRec.Size", 24, Some(119)),
    ("Mixer.LoopRec.Record", 25, Some(118)),
    ("Mixer.LoopRec.Undo", 26, Some(127)),
    ("Mixer.LoopRec.Play", 27, Some(126)),
    ("Mixer.Browse.View", 28, Some(121)),
    ("Mixer.Snap", 29, Some(113)),
    ("Mixer.Master", 30, Some(112)),
    ("Mixer.Quant", 31, Some(120)),
    ("Mixer.C.Cue", 32, Some(117)),
    ("Mixer.A.Cue", 33, Some(116)),
    ("Mixer.B.Cue", 34, Some(125)),
    ("Mixer.D.Cue", 35, Some(124)),
    ("Mixer.Browse.Push", 36, None),
    ("Mixer.C.FX Assign 1", 72, Some(151)),
    ("Mixer.C.FX Assign 2", 73, Some(150)),
    ("Mixer.A.FX Assign 1", 74, Some(149)),
    ("Mixer.A.FX Assign 2", 75, Some(148)),
    ("Mixer.B.FX Assign 1", 76, Some(147)),
    ("Mixer.B.FX Assign 2", 77, Some(146)),
    ("Mixer.D.FX Assign 1", 78, Some(145)),
    ("Mixer.D.FX Assign 2", 79, Some(144)),
    ("Mixer.Footswitch", 80, None),
];

fn buttons() -> Vec<Button> {
    let mut v = Vec::new();
    for d in &DECKS {
        let n = |c: &str| format!("{}.{c}", d.side);
        let lit = |name: String, (index, led): (usize, usize)| bit(name, index, Some(led));
        for (i, &p) in d.pads.iter().enumerate() {
            v.push(lit(n(&format!("Pads.{}", i + 1)), p));
        }
        for (i, &r) in d.remix.iter().enumerate() {
            v.push(lit(n(&format!("Remix.{}", i + 1)), r));
        }
        for (i, &f) in d.fx.iter().enumerate() {
            v.push(lit(n(&format!("FX.Button {}", i + 1)), f));
        }
        for (name, b) in [
            ("Shift", d.shift),
            ("Sync", d.sync),
            ("Cue", d.cue),
            ("Play", d.play),
            ("Deck Select", d.deck_select),
            ("Loop.In", d.loop_in),
            ("Loop.Out", d.loop_out),
            ("Load", d.load),
            ("Tempo Up", d.tempo_up),
            ("Tempo Down", d.tempo_down),
            ("FX.Mode", d.fx_mode),
        ] {
            v.push(lit(n(name), b));
        }
        v.push(bit(n("Loop.Push"), d.loop_push, None));
        v.push(bit(n("Move.Push"), d.move_push, None));
        v.push(Button {
            byte: TOUCH_BYTE,
            ..bit(n("Jog.Touch"), usize::from(d.side == "Right"), None)
        });
        for (k, (name, led)) in (0u8..).zip(d.indicators) {
            v.push(led_only(
                &n(name),
                k + if d.side == "Left" { 0 } else { 4 },
                led,
            ));
        }
    }
    v.extend(
        MIXER
            .iter()
            .map(|&(name, index, led)| bit(name, index, led)),
    );
    v
}

/// Analog controls by their index in the device's blocks.
const ANALOG_NAMES: [Option<&str>; ANALOGS_LEN] = [
    Some("Mixer.D.Volume"),
    Some("Mixer.B.Volume"),
    Some("Mixer.A.Volume"),
    Some("Mixer.C.Volume"),
    Some("Mixer.LoopRec.Volume"),
    Some("Left.Tempo"),
    Some("Right.Tempo"),
    Some("Mixer.Xfader"),
    Some("Mixer.Mic Volume"),
    Some("Mixer.Cue Mix"),
    Some("Left.Jog.Pressure"),
    Some("Right.Jog.Pressure"),
    Some("Mixer.D.Mixer FX.Amount"),
    Some("Mixer.D.EQ.Low"),
    Some("Mixer.D.EQ.Mid"),
    Some("Mixer.D.EQ.High"),
    Some("Right.FX.Knob 1"),
    Some("Right.FX.Knob 2"),
    Some("Right.FX.Knob 3"),
    Some("Right.FX.Knob 4"),
    Some("Mixer.B.Mixer FX.Amount"),
    Some("Mixer.B.EQ.Low"),
    Some("Mixer.B.EQ.Mid"),
    Some("Mixer.B.EQ.High"),
    Some("Mixer.A.Mixer FX.Amount"),
    Some("Mixer.A.EQ.Low"),
    Some("Mixer.A.EQ.Mid"),
    Some("Mixer.A.EQ.High"),
    Some("Mixer.C.Mixer FX.Amount"),
    Some("Mixer.C.EQ.Low"),
    Some("Mixer.C.EQ.Mid"),
    Some("Mixer.C.EQ.High"),
    Some("Left.FX.Knob 1"),
    Some("Left.FX.Knob 2"),
    Some("Left.FX.Knob 3"),
    Some("Left.FX.Knob 4"),
];

static ANALOGS: LazyLock<Vec<Analog>> = LazyLock::new(|| {
    ANALOG_NAMES
        .iter()
        .enumerate()
        .filter_map(|(k, name)| {
            name.map(|name| Analog {
                name,
                byte: 1 + 2 * k,
            })
        })
        .collect()
});

/// Endless knobs in the order the wheel block carries them.
const ENCODER_NAMES: [&str; ENCODERS_LEN] = [
    "Mixer.Browse.Turn",
    "Left.Move.Turn",
    "Left.Loop.Turn",
    "Right.Move.Turn",
    "Right.Loop.Turn",
    "Mixer.A.Gain",
    "Mixer.B.Gain",
    "Mixer.C.Gain",
    "Mixer.D.Gain",
];

static ENCODERS: LazyLock<Vec<Encoder>> = LazyLock::new(|| {
    ENCODER_NAMES
        .iter()
        .enumerate()
        .map(|(i, &name)| Encoder {
            name,
            byte: 1 + i,
            shift: 0,
        })
        .collect()
});

const JOGS: &[Jog] = &[
    Jog {
        name: "Left.Jog.Turn",
        byte: 1,
        bits: JOG_BITS,
    },
    Jog {
        name: "Right.Jog.Turn",
        byte: 5,
        bits: JOG_BITS,
    },
];

/// Where each analog value sits in its block: (block, word, analog index).
const ANALOG_WORDS: &[(u16, usize, usize)] = &[
    (2, 1, 0),
    (2, 2, 1),
    (2, 3, 2),
    (2, 4, 3),
    (2, 6, 4),
    (2, 7, 7),
    (3, 3, 6),
    (3, 4, 5),
    (3, 6, 8),
    (3, 7, 9),
    (4, 1, 10),
    (4, 2, 11),
    (4, 3, 12),
    (4, 4, 13),
    (4, 5, 14),
    (4, 6, 15),
    (4, 7, 16),
    (5, 1, 17),
    (5, 2, 18),
    (5, 3, 19),
    (5, 4, 20),
    (5, 5, 21),
    (5, 6, 22),
    (5, 7, 23),
    (6, 1, 24),
    (6, 2, 25),
    (6, 3, 26),
    (6, 4, 27),
    (6, 5, 28),
    (6, 6, 29),
    (6, 7, 30),
    (7, 1, 31),
    (7, 2, 32),
    (7, 3, 33),
    (7, 4, 34),
    (7, 5, 35),
];

pub const BLOCK_LEN: usize = 16;

/// A wheel's touch, read from its distance sensor against the value it
/// rests at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Touch {
    rest: Option<u16>,
    pub on: bool,
}

impl Touch {
    pub fn update(&mut self, value: u16) {
        let rest = *self.rest.get_or_insert(value);
        let over = value.saturating_sub(rest);
        if self.on {
            self.on = over >= TOUCH_OFF;
        } else {
            self.on = over > TOUCH_ON;
            if !self.on {
                // The rest value follows the sensor down at once and up slowly.
                self.rest = Some(if value < rest {
                    value
                } else {
                    rest + (value - rest) / 16
                });
            }
        }
    }
}

/// The latest state of every block, kept to rebuild whole reports.
#[derive(Debug, Clone)]
pub struct Blocks {
    buttons: [u8; BUTTON_BYTES],
    analogs: [u16; ANALOGS_LEN],
    encoders: [u8; ENCODERS_LEN],
    wheels: [u16; 2],
    touch: [Touch; 2],
    /// Analog blocks seen so far, one bit per block 2 to 7.
    seen: u8,
    reads: u32,
    analogs_sent: bool,
}

impl Default for Blocks {
    fn default() -> Self {
        Self {
            buttons: [0; BUTTON_BYTES],
            analogs: [0; ANALOGS_LEN],
            encoders: [0; ENCODERS_LEN],
            wheels: [0; 2],
            touch: [Touch::default(); 2],
            seen: 0,
            reads: 0,
            analogs_sent: false,
        }
    }
}

impl Blocks {
    /// The reports that one bulk read changed.
    pub fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut changed = [false; 4];
        self.reads = self.reads.saturating_add(1);
        for block in data.as_chunks::<BLOCK_LEN>().0 {
            let id = u16::from_be_bytes([block[0], block[1]]);
            match id {
                0 => {
                    self.buttons.copy_from_slice(&block[4..4 + BUTTON_BYTES]);
                    changed[0] = true;
                }
                1 => {
                    self.wheels = [
                        u16::from(block[9]) | (u16::from(block[8] & 0x03) << 8),
                        u16::from(block[13]) | (u16::from(block[12] & 0x03) << 8),
                    ];
                    self.encoders = [
                        block[3] & 0x0F,
                        block[4] >> 4,
                        block[4] & 0x0F,
                        block[5] >> 4,
                        block[5] & 0x0F,
                        block[6] >> 4,
                        block[6] & 0x0F,
                        block[7] >> 4,
                        block[7] & 0x0F,
                    ];
                    changed[2] = true;
                    changed[3] = true;
                }
                2..=7 => {
                    for &(_, word, k) in ANALOG_WORDS.iter().filter(|w| w.0 == id) {
                        let value = u16::from_be_bytes([block[2 * word], block[2 * word + 1]]);
                        self.analogs[k] = value & 0x0FFF;
                        if let Some(side) = DISTANCE.iter().position(|&d| d == k) {
                            self.touch[side].update(self.analogs[k]);
                            changed[0] = true;
                        }
                    }
                    self.seen |= 1 << (id - 2);
                    changed[1] = true;
                }
                _ => break,
            }
        }
        let mut out = Vec::new();
        if changed[0] {
            out.push(self.button_report());
        }
        let analogs_ready = self.seen == 0x3F || self.reads >= ANALOG_WAIT;
        if analogs_ready && (changed[1] || !self.analogs_sent) {
            self.analogs_sent = true;
            out.push(self.analog_report());
        }
        if changed[2] {
            out.push(self.jog_report());
        }
        if changed[3] {
            out.push(self.encoder_report());
        }
        out
    }

    fn button_report(&self) -> Vec<u8> {
        let mut r = vec![BUTTON_REPORT];
        r.extend_from_slice(&self.buttons);
        r.push(u8::from(self.touch[0].on) | u8::from(self.touch[1].on) << 1);
        r
    }

    fn analog_report(&self) -> Vec<u8> {
        let mut r = vec![ANALOG_REPORT];
        for v in self.analogs {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r
    }

    fn encoder_report(&self) -> Vec<u8> {
        let mut r = vec![ENCODER_REPORT];
        r.extend_from_slice(&self.encoders);
        r
    }

    fn jog_report(&self) -> Vec<u8> {
        let mut r = vec![JOG_REPORT];
        for w in self.wheels {
            r.extend_from_slice(&u32::from(w).to_le_bytes());
        }
        r
    }
}

/// The LED messages that take the device from `sent` to `now`: index and
/// level, only for the LEDs that changed.
pub fn led_messages(sent: Option<&[u8]>, now: &[u8]) -> Vec<[u8; 2]> {
    (0u8..)
        .zip(now)
        .filter(|&(i, v)| sent.and_then(|s| s.get(usize::from(i))) != Some(v))
        .map(|(i, v)| [i, (*v).min(LED_MAX)])
        .collect()
}

/// VU meters: the first LED of channels A to D; six segments, then clip.
const METER_FIRST: [usize; 4] = [97, 89, 105, 81];
const METER_SEGMENTS: usize = 6;

fn meter_leds(feedback: &Feedback, _decoder: &Decoder, leds: &mut Leds) {
    for (ch, &first) in METER_FIRST.iter().enumerate() {
        linear_meter(
            &mut leds.0,
            first,
            METER_SEGMENTS,
            feedback.channels[ch].level,
            LED_MAX,
        );
    }
}

/// The dot LED of each loop-size digit, left digit first.
const DIGITS: [[usize; 2]; 2] = [[48, 56], [168, 160]];

/// The LEDs of one digit in segment-bit order (dot, middle, lower right,
/// upper right, top, upper left, lower left, bottom).
fn segment_leds(dot: usize) -> [usize; 8] {
    [
        dot,
        dot + 1,
        dot + 5,
        dot + 6,
        dot + 7,
        dot + 2,
        dot + 3,
        dot + 4,
    ]
}

fn deck_leds(feedback: &Feedback, decoder: &Decoder, layer: [usize; 2], leds: &mut Leds) {
    let phase = loop_display::phase_ms();
    for (&deck, digits) in layer.iter().zip(&DIGITS) {
        let d = &feedback.decks[deck];
        let shown = loop_display::loop_display(d.loop_size, d.loop_active, d.live, phase);
        for (mask, &dot) in shown.iter().zip(digits) {
            for (k, led) in segment_leds(dot).into_iter().enumerate() {
                leds.0[led] = if mask & (1 << k) != 0 { LED_MAX } else { 0 };
            }
        }
    }
    super::s4mk2::remix_leds(feedback, decoder, layer, leds, LED_MAX);
    state_leds(feedback, layer, leds);
}

/// LEDs that show state rather than a mapped control.
const SAMPLES_LED: [usize; 2] = [38, 190];
const BROWSE_VIEW_LED: usize = 121;
const LOOP_REC_LEDS: LoopRecLeds = LoopRecLeds {
    record: 118,
    size: 119,
    play: 126,
    undo: 127,
};
const QUANT_LED: usize = 10;
const HEADPHONE_LED: usize = 11;
const WARNING_LED: usize = 15;
/// Channels A to D: lit while the deck is on one of the two sides.
const ACTIVE_LEDS: [usize; 4] = [114, 122, 115, 123];

struct LoopRecLeds {
    record: usize,
    size: usize,
    play: usize,
    undo: usize,
}

/// A button LED: full when on, dim when off.
fn button_level(on: bool) -> u8 {
    match SINGLE {
        Some((lit, dim)) => {
            if on {
                lit
            } else {
                dim
            }
        }
        None => 0,
    }
}

/// An indicator LED: full when on, dark when off.
fn indicator_level(on: bool) -> u8 {
    if on { LED_MAX } else { 0 }
}

fn state_leds(feedback: &Feedback, layer: [usize; 2], leds: &mut Leds) {
    for (i, d) in DECKS.iter().enumerate() {
        let deck = layer[i];
        leds.0[d.deck_select.1] = button_level(deck >= 2);
        leds.0[SAMPLES_LED[i]] = indicator_level(feedback.decks[deck].sample_deck.is_some());
    }
    leds.0[BROWSE_VIEW_LED] = button_level(feedback.browser.maximized);
    let rec = &feedback.loop_recorder;
    leds.0[LOOP_REC_LEDS.record] = button_level(matches!(rec.state, 3 | 4));
    leds.0[LOOP_REC_LEDS.play] = button_level(matches!(rec.state, 2..=4));
    leds.0[LOOP_REC_LEDS.size] = button_level(rec.state != 0);
    leds.0[LOOP_REC_LEDS.undo] = button_level(rec.undo != 0);
    leds.0[QUANT_LED] = indicator_level(feedback.quantize);
    leds.0[HEADPHONE_LED] = indicator_level(feedback.channels.iter().any(|c| c.cue));
    leds.0[WARNING_LED] = indicator_level(feedback.audio_overloaded || feedback.audio_dropout);
    for (ch, &led) in ACTIVE_LEDS.iter().enumerate() {
        leds.0[led] = indicator_level(layer.contains(&ch));
    }
}

fn default_mappings() -> Vec<Mapping> {
    let mut v: Vec<Mapping> = super::s4mk2::default_mappings()
        .into_iter()
        .filter(|m| LAYOUT.has_input(m))
        .collect();
    for (deck, side) in [(0usize, "Left"), (1, "Right")] {
        for (button, direction) in [("Tempo Up", 1.0), ("Tempo Down", -1.0)] {
            v.push(map(
                &format!("{side}.{button}"),
                Action::TempoBend { deck, direction },
                Interaction::Hold,
            ));
        }
    }
    for (name, part) in [
        ("Mixer.LoopRec.Size", LoopRecorderPart::Size),
        ("Mixer.LoopRec.Undo", LoopRecorderPart::Undo),
        ("Mixer.LoopRec.Play", LoopRecorderPart::Play),
    ] {
        v.push(map(
            name,
            Action::LoopRecorder { part },
            Interaction::Trigger,
        ));
    }
    v.push(map(
        "Mixer.LoopRec.Volume",
        Action::LoopRecorder {
            part: LoopRecorderPart::DryWet,
        },
        Interaction::Direct,
    ));
    v.push(map(
        "Mixer.Browse.View",
        Action::BrowserView,
        Interaction::Trigger,
    ));
    v
}

fn shifted(name: &str) -> Shifted {
    super::s4mk2::shifted(name)
}

pub static LAYOUT: LazyLock<Layout> = LazyLock::new(|| Layout {
    name: NAME,
    source: SOURCE,
    vendor_id: VENDOR_ID,
    product_id: PRODUCT_ID,
    buttons: buttons(),
    switches: &[],
    analogs: &ANALOGS,
    encoders: &ENCODERS,
    erps: &[],
    jogs: JOGS,
    jog_steps_per_turn: JOG_STEPS,
    analog_report: ANALOG_REPORT,
    jog_report: JOG_REPORT,
    encoder_report: ENCODER_REPORT,
    led_reports: &[(0, LED_COUNT)],
    meter_report: None,
    initial_reports: &[],
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
    touch_strips: &[],
    displays: None,
});

pub fn start(
    events: crate::controller::EventSender,
    wake: crate::controller::Waker,
) -> Result<Box<dyn crate::controller::Controller>, String> {
    crate::s4mk3::driver::start_on(&LAYOUT, events, wake, usb::supervise)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Input, InputEvent};

    fn block(id: u16) -> [u8; BLOCK_LEN] {
        let mut b = [0u8; BLOCK_LEN];
        b[..2].copy_from_slice(&id.to_be_bytes());
        b
    }

    fn decode(d: &mut Decoder, blocks: &mut Blocks, data: &[u8]) -> Vec<InputEvent> {
        blocks.feed(data).iter().flat_map(|r| d.decode(r)).collect()
    }

    fn named<'a>(events: &'a [InputEvent], name: &str) -> Option<&'a InputEvent> {
        events.iter().find(|e| e.input == Input::Hid(name.into()))
    }

    #[test]
    fn button_bits_reach_their_named_controls() {
        let (mut d, mut blocks) = (Decoder::new(&LAYOUT), Blocks::default());
        decode(&mut d, &mut blocks, &block(0));
        for (index, name) in [
            (7, "Left.Play"),
            (1, "Left.Shift"),
            (29, "Mixer.Snap"),
            (31, "Mixer.Quant"),
            (48, "Right.Deck Select"),
            (63, "Right.Play"),
            (93, "Right.FX.Mode"),
        ] {
            let mut b = block(0);
            b[4 + index / 8] |= 1 << (index % 8);
            let events = decode(&mut d, &mut blocks, &b);
            assert_eq!(
                named(&events, name).and_then(|e| e.pressed),
                Some(true),
                "{name}: {events:?}"
            );
            decode(&mut d, &mut blocks, &block(0));
        }
    }

    #[test]
    fn analogs_wait_for_every_analog_block() {
        let analog =
            |reports: Vec<Vec<u8>>| reports.iter().filter(|r| r[0] == ANALOG_REPORT).count();
        let mut blocks = Blocks::default();
        for id in 2..7 {
            assert_eq!(analog(blocks.feed(&block(id))), 0, "block {id}");
        }
        assert_eq!(analog(blocks.feed(&block(7))), 1);
        let mut late = Blocks::default();
        for _ in 1..ANALOG_WAIT {
            assert_eq!(analog(late.feed(&block(2))), 0);
        }
        assert_eq!(analog(late.feed(&[])), 1, "sent once the wait is over");
        assert_eq!(analog(late.feed(&[])), 0);
    }

    #[test]
    fn analogs_arrive_big_endian_and_leave_as_named_values() {
        let (mut d, mut blocks) = (Decoder::new(&LAYOUT), Blocks::default());
        for id in 2..8 {
            decode(&mut d, &mut blocks, &block(id));
        }
        let mut b = block(2);
        b[14..16].copy_from_slice(&0x0FFFu16.to_be_bytes());
        let events = decode(&mut d, &mut blocks, &b);
        let xfader = named(&events, "Mixer.Xfader").expect("crossfader");
        assert!((xfader.value - 1.0).abs() < 1e-6);
        let mut b = block(7);
        b[4..6].copy_from_slice(&0x0800u16.to_be_bytes());
        let events = decode(&mut d, &mut blocks, &b);
        let wet = named(&events, "Left.FX.Knob 1").expect("dry/wet");
        assert!((wet.value - f32::from(0x800u16) / 4095.0).abs() < 1e-3);
    }

    #[test]
    fn wheels_and_encoders_step_from_the_wheel_block() {
        let (mut d, mut blocks) = (Decoder::new(&LAYOUT), Blocks::default());
        let mut b = block(1);
        b[8] = 0x03;
        b[9] = 0xFE;
        decode(&mut d, &mut blocks, &b);
        let mut b = block(1);
        b[9] = 0x02;
        b[6] = 0x10;
        let events = decode(&mut d, &mut blocks, &b);
        let jog = named(&events, "Left.Jog.Turn").expect("jog");
        assert!((jog.value - 4.0).abs() < 1e-6, "wraps across the top");
        let gain = named(&events, "Mixer.A.Gain").expect("gain");
        assert!((gain.value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_wheel_is_touched_once_its_sensor_rises_over_rest() {
        let mut t = Touch::default();
        for v in [1000, 990] {
            t.update(v);
            assert!(!t.on);
        }
        t.update(990 + TOUCH_ON + 1);
        assert!(t.on);
        t.update(990 + TOUCH_OFF);
        assert!(t.on, "held until it falls under the release level");
        t.update(990 + TOUCH_OFF - 1);
        assert!(!t.on);
    }

    #[test]
    fn the_touch_reaches_the_jog_touch_control() {
        let (mut d, mut blocks) = (Decoder::new(&LAYOUT), Blocks::default());
        decode(&mut d, &mut blocks, &block(4));
        let mut b = block(4);
        b[4..6].copy_from_slice(&(TOUCH_ON + 10).to_be_bytes());
        let events = decode(&mut d, &mut blocks, &b);
        assert_eq!(
            named(&events, "Right.Jog.Touch").and_then(|e| e.pressed),
            Some(true)
        );
    }

    #[test]
    fn an_unknown_block_ends_the_read() {
        let mut blocks = Blocks::default();
        let mut data = block(9).to_vec();
        data.extend_from_slice(&block(0));
        assert_eq!(blocks.feed(&data), Vec::<Vec<u8>>::new());
    }

    #[test]
    fn leds_go_out_only_when_they_change_and_within_range() {
        let now = [0u8, 0x7F, 3];
        assert_eq!(
            led_messages(None, &now),
            [[0, 0], [1, LED_MAX], [2, 3]],
            "everything on the first frame"
        );
        assert_eq!(led_messages(Some(&[0, 0x7F, 1]), &now), [[2, 3]]);
        assert_eq!(led_messages(Some(&now), &now), Vec::<[u8; 2]>::new());
    }

    #[test]
    fn meters_and_loop_digits_light_their_leds() {
        let mut f = Feedback::default();
        f.channels[0].level = 2.0;
        f.decks[1].loop_size = 5;
        let d = Decoder::new(&LAYOUT);
        let mut leds = Leds::for_layout(&LAYOUT);
        meter_leds(&f, &d, &mut leds);
        assert_eq!(&leds.0[97..104], &[LED_MAX; 7], "six segments and clip");
        deck_leds(&f, &d, [0, 1], &mut leds);
        let one = loop_display::loop_size_digits(5)[1];
        let lit: Vec<usize> = segment_leds(DIGITS[1][1])
            .into_iter()
            .enumerate()
            .filter(|&(k, _)| one & (1 << k) != 0)
            .map(|(_, led)| led)
            .collect();
        assert_eq!(lit, [165, 166], "a one is the two right segments");
        assert!(lit.iter().all(|&l| leds.0[l] == LED_MAX));
    }

    #[test]
    fn state_leds_follow_the_layers_and_the_loop_recorder() {
        let mut f = Feedback::default();
        f.loop_recorder.state = 2;
        f.channels[1].cue = true;
        f.browser.maximized = true;
        let mut leds = Leds::for_layout(&LAYOUT);
        state_leds(&f, [2, 1], &mut leds);
        assert_eq!(leds.0[DECKS[0].deck_select.1], LED_MAX, "left shows deck C");
        assert_eq!(leds.0[DECKS[1].deck_select.1], button_level(false));
        assert_eq!(leds.0[LOOP_REC_LEDS.play], LED_MAX);
        assert_eq!(leds.0[LOOP_REC_LEDS.record], button_level(false));
        assert_eq!(leds.0[HEADPHONE_LED], LED_MAX);
        assert_eq!(leds.0[BROWSE_VIEW_LED], LED_MAX);
        let active: Vec<u8> = ACTIVE_LEDS.iter().map(|&l| leds.0[l]).collect();
        assert_eq!(active, [0, LED_MAX, LED_MAX, 0], "decks C and B");
    }

    #[test]
    fn state_leds_have_their_own_lights() {
        let used: Vec<usize> = LAYOUT.buttons.iter().filter_map(|b| b.led).collect();
        let own = [
            SAMPLES_LED[0],
            SAMPLES_LED[1],
            QUANT_LED,
            HEADPHONE_LED,
            WARNING_LED,
        ]
        .into_iter()
        .chain(ACTIVE_LEDS);
        for led in own {
            assert!(led < LED_COUNT && !used.contains(&led), "{led}");
        }
    }

    #[test]
    fn defaults_follow_the_mk2_and_add_the_mk1_buttons() {
        let defaults = default_mappings();
        let find = |name: &str| {
            defaults
                .iter()
                .find(|m| m.input == Input::Hid(name.into()))
                .map(|m| m.action)
        };
        assert_eq!(find("Left.Loop.In"), Some(Action::LoopIn { deck: 0 }));
        assert_eq!(
            find("Right.Tempo Down"),
            Some(Action::TempoBend {
                deck: 1,
                direction: -1.0
            })
        );
        assert_eq!(
            find("Mixer.LoopRec.Volume"),
            Some(Action::LoopRecorder {
                part: LoopRecorderPart::DryWet
            })
        );
        assert!(defaults.iter().all(|m| LAYOUT.has_input(m)));
        assert_eq!(
            shifted("Left.Cue"),
            Some(Some((
                Action::ReturnToStart { deck: 0 },
                Interaction::Trigger
            )))
        );
    }
}
