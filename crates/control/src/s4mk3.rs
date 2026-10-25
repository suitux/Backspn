use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use crate::feedback::Feedback;
use crate::{Action, Input, InputEvent, Interaction, Mapping};

pub mod driver;
pub mod motor;
pub mod screen;

pub const NAME: &str = "Kontrol S4 MK3";
pub const SOURCE: &str = "S4 MK3";

pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1720;
const LED_REPORT: u8 = 0x80;
pub const LED_REPORT_LEN: usize = 94;
const METER_REPORT: u8 = 0x81;
pub const METER_REPORT_LEN: usize = 78;
const ANALOG_DEADBAND: u16 = 6;

#[derive(Debug)]
pub struct Button {
    pub name: String,
    pub byte: usize,
    pub bit: u8,
    pub led: Option<usize>,
    pub single: Option<(u8, u8)>,
    pub rgb: Option<usize>,
}

fn b(name: impl Into<String>, byte: usize, bit: u8, led: Option<usize>) -> Button {
    Button {
        name: name.into(),
        byte: byte + 1,
        bit,
        led,
        single: None,
        rgb: None,
    }
}

fn deck_buttons(left: bool) -> Vec<Button> {
    let side = if left { "Left" } else { "Right" };
    let n = |s: &str| format!("{side}.{s}");
    let mut v = if left {
        vec![
            b(n("Play"), 4, 0, Some(55)),
            b(n("Cue"), 4, 1, Some(8)),
            b(n("Sync"), 5, 7, Some(14)),
            b(n("Master"), 0, 0, Some(15)),
            b(n("Shift"), 5, 1, Some(59)),
            b(n("Pad Mode.Hotcues"), 4, 2, Some(9)),
            b(n("Move.Push"), 6, 2, None),
            b(n("Loop.Push"), 6, 5, None),
            b(n("Browse.Encoder.Push"), 0, 1, None),
            b(n("Transport Mode.Reverse"), 1, 4, Some(60)),
            b(n("Transport Mode.Flux"), 1, 5, Some(61)),
            b(n("Pad Mode.Record"), 4, 3, Some(56)),
            b(n("Pad Mode.Samples"), 4, 4, Some(57)),
            b(n("Pad Mode.Mute"), 4, 5, Some(58)),
            b(n("Pad Mode.Stems"), 5, 0, Some(10)),
            b(n("Deck.A"), 5, 2, Some(12)),
            b(n("Deck.C"), 5, 3, Some(13)),
            b(n("Jog Mode.Jog"), 5, 4, Some(16)),
            b(n("Jog Mode.TT"), 5, 5, Some(17)),
            b(n("Jog Mode.Grid"), 5, 6, Some(18)),
            b(n("Browse.View"), 1, 0, Some(19)),
            b(n("Browse.Add To Playlist"), 1, 1, Some(20)),
            b(n("Browse.Favorite"), 0, 4, Some(21)),
            b(n("Browse.Preview"), 0, 5, Some(22)),
            b(n("Jog.Touch"), 16, 4, None),
        ]
    } else {
        vec![
            b(n("Play"), 12, 0, Some(66)),
            b(n("Cue"), 14, 5, Some(31)),
            b(n("Sync"), 14, 4, Some(37)),
            b(n("Master"), 10, 0, Some(38)),
            b(n("Shift"), 14, 1, Some(70)),
            b(n("Pad Mode.Hotcues"), 12, 2, Some(32)),
            b(n("Move.Push"), 15, 5, None),
            b(n("Loop.Push"), 15, 2, None),
            b(n("Browse.Encoder.Push"), 10, 1, None),
            b(n("Transport Mode.Reverse"), 10, 4, Some(71)),
            b(n("Transport Mode.Flux"), 10, 5, Some(72)),
            b(n("Pad Mode.Record"), 12, 3, Some(67)),
            b(n("Pad Mode.Samples"), 12, 4, Some(68)),
            b(n("Pad Mode.Mute"), 12, 5, Some(69)),
            b(n("Pad Mode.Stems"), 12, 1, Some(33)),
            b(n("Deck.B"), 14, 2, Some(35)),
            b(n("Deck.D"), 14, 3, Some(36)),
            b(n("Jog Mode.Jog"), 14, 0, Some(39)),
            b(n("Jog Mode.TT"), 14, 6, Some(40)),
            b(n("Jog Mode.Grid"), 14, 7, Some(41)),
            b(n("Browse.View"), 9, 0, Some(42)),
            b(n("Browse.Add To Playlist"), 9, 3, Some(43)),
            b(n("Browse.Favorite"), 9, 1, Some(44)),
            b(n("Browse.Preview"), 9, 2, Some(45)),
            b(n("Jog.Touch"), 16, 5, None),
        ]
    };
    let (pad_byte, pad_bits, pad_leds): (usize, [u8; 8], [usize; 8]) = if left {
        (3, [5, 4, 7, 6, 3, 2, 1, 0], [0, 1, 2, 3, 4, 5, 6, 7])
    } else {
        (
            13,
            [5, 4, 7, 6, 3, 2, 1, 0],
            [23, 24, 25, 26, 27, 28, 29, 30],
        )
    };
    for i in 0..8 {
        v.push(b(
            n(&format!("Pads.{}", i + 1)),
            pad_byte,
            pad_bits[i],
            Some(pad_leds[i]),
        ));
    }
    v
}

fn mixer_buttons() -> Vec<Button> {
    let mut v = Vec::new();
    for (ch, (byte, bit, led)) in
        ["A", "B", "C", "D"]
            .iter()
            .zip([(7, 3, 77), (7, 6, 81), (7, 2, 85), (7, 7, 89)])
    {
        v.push(b(format!("Mixer.{ch}.Cue"), byte, bit, Some(led)));
    }
    let fx = [
        ("Left", [(1, 6, 62), (1, 7, 63), (1, 3, 64), (1, 2, 65)]),
        ("Right", [(9, 4, 73), (9, 5, 74), (9, 6, 75), (9, 7, 76)]),
    ];
    for (side, buttons) in fx {
        for (i, (byte, bit, led)) in buttons.into_iter().enumerate() {
            v.push(b(
                format!("{side}.FX.Button {}", i + 1),
                byte,
                bit,
                Some(led),
            ));
        }
    }
    let channels = [
        ("A", (11, 0, 80), (2, 3, 78), (2, 4, 79), (7, 0, 46)),
        ("B", (11, 1, 84), (2, 5, 82), (2, 6, 83), (7, 5, 47)),
        ("C", (2, 1, 88), (2, 0, 86), (2, 2, 87), (7, 1, 48)),
        ("D", (11, 2, 92), (2, 7, 90), (11, 7, 91), (7, 4, 49)),
    ];
    for (ch, ext, fx1, fx2, mixer_fx) in channels {
        for (name, (byte, bit, led)) in [
            ("Ext", ext),
            ("FX Assign 1", fx1),
            ("FX Assign 2", fx2),
            ("Mixer FX.On", mixer_fx),
        ] {
            v.push(b(format!("Mixer.{ch}.{name}"), byte, bit, Some(led)));
        }
    }
    for (name, bit, led) in [
        ("Filter", 7, 54),
        ("1", 5, 50),
        ("2", 1, 51),
        ("3", 6, 52),
        ("4", 0, 53),
    ] {
        v.push(b(
            format!("Mixer.Mixer FX Select.{name}"),
            8,
            bit,
            Some(led),
        ));
    }
    v.push(b("Mixer.Quant", 11, 6, Some(93)));
    v
}

#[derive(Debug)]
pub struct Switch {
    pub name: &'static str,
    pub byte: usize,
    pub bit: u8,
}

const fn sw(name: &'static str, byte: usize, bit: u8) -> Switch {
    Switch {
        name,
        byte: byte + 1,
        bit,
    }
}

const SWITCHES: &[Switch] = &[
    sw("Mixer.A.Xfader Assign", 17, 4),
    sw("Mixer.B.Xfader Assign", 17, 2),
    sw("Mixer.C.Xfader Assign", 17, 6),
    sw("Mixer.D.Xfader Assign", 17, 0),
    sw("Mixer.Xfader Curve", 18, 0),
];

#[derive(Debug)]
pub struct Jog {
    pub name: &'static str,
    pub byte: usize,
    pub bits: u32,
}

const JOGS: [Jog; 2] = [
    Jog {
        name: "Left.Jog.Turn",
        byte: 12,
        bits: 32,
    },
    Jog {
        name: "Right.Jog.Turn",
        byte: 40,
        bits: 32,
    },
];

pub const JOG_STEPS_PER_TURN: f64 = 2880.0;

#[derive(Debug)]
pub struct Analog {
    pub name: &'static str,
    pub byte: usize,
}

const fn a(name: &'static str, byte: usize) -> Analog {
    Analog {
        name,
        byte: byte + 1,
    }
}

const ANALOGS: &[Analog] = &[
    a("Mixer.A.Volume", 2),
    a("Mixer.B.Volume", 4),
    a("Mixer.C.Volume", 6),
    a("Mixer.D.Volume", 8),
    a("Mixer.A.Gain", 16),
    a("Mixer.B.Gain", 18),
    a("Mixer.C.Gain", 14),
    a("Mixer.D.Gain", 20),
    a("Mixer.A.EQ.High", 44),
    a("Mixer.A.EQ.Mid", 46),
    a("Mixer.A.EQ.Low", 48),
    a("Mixer.B.EQ.High", 50),
    a("Mixer.B.EQ.Mid", 52),
    a("Mixer.B.EQ.Low", 54),
    a("Mixer.C.EQ.High", 38),
    a("Mixer.C.EQ.Mid", 40),
    a("Mixer.C.EQ.Low", 42),
    a("Mixer.D.EQ.High", 56),
    a("Mixer.D.EQ.Mid", 58),
    a("Mixer.D.EQ.Low", 60),
    a("Mixer.A.Mixer FX.Amount", 64),
    a("Mixer.B.Mixer FX.Amount", 66),
    a("Mixer.C.Mixer FX.Amount", 62),
    a("Mixer.D.Mixer FX.Amount", 68),
    a("Mixer.Xfader", 0),
    a("Mixer.Master", 22),
    a("Mixer.Booth", 24),
    a("Mixer.Cue Volume", 26),
    a("Mixer.Cue Mix", 28),
    a("Left.FX.Knob 1", 30),
    a("Left.FX.Knob 2", 32),
    a("Left.FX.Knob 3", 34),
    a("Left.FX.Knob 4", 36),
    a("Right.FX.Knob 1", 70),
    a("Right.FX.Knob 2", 72),
    a("Right.FX.Knob 3", 74),
    a("Right.FX.Knob 4", 76),
    a("Left.Tempo", 12),
    a("Right.Tempo", 10),
];

#[derive(Debug)]
pub struct Encoder {
    pub name: &'static str,
    pub byte: usize,
    pub shift: u8,
}

const fn e(name: &'static str, byte: usize, shift: u8) -> Encoder {
    Encoder {
        name,
        byte: byte + 1,
        shift,
    }
}

const ENCODERS: &[Encoder] = &[
    e("Left.Move.Turn", 19, 0),
    e("Left.Loop.Turn", 19, 4),
    e("Left.Browse.Encoder.Turn", 20, 0),
    e("Right.Move.Turn", 20, 4),
    e("Right.Loop.Turn", 21, 0),
    e("Right.Browse.Encoder.Turn", 21, 4),
];

pub fn is_encoder(name: &str) -> bool {
    name.ends_with(".Turn")
}

pub type Shifted = Option<Option<(Action, Interaction)>>;
pub type ShiftFn = fn(&str) -> Shifted;

const CALIBRATION: [crate::calibration::Group; 2] = [
    crate::calibration::Group {
        name: "FX knobs",
        knobs: &[15, 16, 17, 18, 35, 36, 37, 38],
    },
    crate::calibration::Group {
        name: "EQ and Mixer FX knobs",
        knobs: &[
            7, 8, 9, 10, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 11, 12,
            14, 13,
        ],
    },
];

#[derive(Debug, Clone, Copy)]
pub struct SoundCard {
    pub name: &'static str,
    pub master: (u16, u16),
    pub monitor: (u16, u16),
    pub buffer_frames: u32,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Layout {
    pub name: &'static str,
    pub source: &'static str,
    pub vendor_id: u16,
    pub product_id: u16,
    pub buttons: Vec<Button>,
    pub switches: &'static [Switch],
    pub analogs: &'static [Analog],
    pub encoders: &'static [Encoder],
    pub jogs: &'static [Jog],
    pub jog_steps_per_turn: f64,
    pub analog_report: u8,
    pub jog_report: u8,
    pub encoder_report: u8,
    pub led_reports: &'static [(u8, usize)],
    pub meter_report: Option<(u8, usize)>,
    pub initial_reports: &'static [(u8, usize)],
    pub motors: bool,
    pub sound_card: Option<SoundCard>,
    pub calibration: &'static [crate::calibration::Group],
    pub stored_calibration: Option<&'static crate::poti::StoredCalibration>,
    pub rings: bool,
    pub screens: bool,
    pub tempo_leds: Option<[usize; 2]>,
    pub wheel_leds: Option<[[usize; 8]; 2]>,
    pub strip_leds: Option<[[usize; 6]; 2]>,
    pub meters: Option<fn(&Feedback) -> Meters>,
    pub meter_leds: Option<fn(&Feedback, &Decoder, &mut Leds)>,
    pub deck_leds: Option<DeckLeds>,
    pub default_mappings: fn() -> Vec<Mapping>,
    pub shifted: ShiftFn,
    pub upgrade: Option<fn(&Mapping) -> Option<Mapping>>,
    pub jog_button_toggles_touch: bool,
    pub samples_button_stems: bool,
    pub samples_button_toggles: bool,
    pub fx_select_toggles: bool,
    pub mx2_transport: bool,
}

pub type DeckLeds = fn(&Feedback, &Decoder, [usize; 2], &mut Leds);

impl Layout {
    pub fn led_len(&self) -> usize {
        self.led_reports.iter().map(|(_, len)| len).sum()
    }

    pub fn has(&self, name: &str) -> bool {
        self.buttons.iter().any(|b| b.name == name)
            || self.switches.iter().any(|s| s.name == name)
            || self.analogs.iter().any(|a| a.name == name)
            || self.encoders.iter().any(|e| e.name == name)
            || self.jogs.iter().any(|j| j.name == name)
    }

    pub fn has_input(&self, mapping: &Mapping) -> bool {
        matches!(&mapping.input, Input::Hid(name) if self.has(name))
    }
}

pub static S4: LazyLock<Layout> = LazyLock::new(|| {
    let mut buttons = deck_buttons(true);
    buttons.extend(deck_buttons(false));
    buttons.extend(mixer_buttons());
    Layout {
        name: NAME,
        source: SOURCE,
        vendor_id: VENDOR_ID,
        product_id: PRODUCT_ID,
        buttons,
        switches: SWITCHES,
        analogs: ANALOGS,
        encoders: ENCODERS,
        jogs: &JOGS,
        jog_steps_per_turn: JOG_STEPS_PER_TURN,
        analog_report: 0x02,
        jog_report: 0x03,
        encoder_report: 0x01,
        led_reports: &[(LED_REPORT, LED_REPORT_LEN)],
        meter_report: Some((METER_REPORT, METER_REPORT_LEN)),
        initial_reports: &INITIAL_REPORTS,
        motors: true,
        calibration: &CALIBRATION,
        stored_calibration: None,
        sound_card: Some(SoundCard {
            name: NAME,
            master: (0, 1),
            monitor: (2, 3),
            buffer_frames: 256,
        }),
        rings: true,
        screens: true,
        tempo_leds: Some(driver::TEMPO_LED),
        wheel_leds: None,
        strip_leds: None,
        meters: Some(driver::meters),
        meter_leds: None,
        deck_leds: None,
        default_mappings,
        shifted,
        upgrade: Some(driver::upgraded),
        jog_button_toggles_touch: false,
        samples_button_stems: false,
        samples_button_toggles: false,
        fx_select_toggles: false,
        mx2_transport: false,
    }
});

pub struct Decoder {
    layout: &'static Layout,
    previous: HashMap<u8, Vec<u8>>,
    potis: Vec<Option<crate::poti::Poti>>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new(&S4)
    }
}

impl Decoder {
    pub fn new(layout: &'static Layout) -> Self {
        let mut decoder = Self {
            layout,
            previous: HashMap::new(),
            potis: vec![None; layout.analogs.len()],
        };
        if let Some(stored) = layout.stored_calibration {
            decoder.set_potis(&stored.defaults());
        }
        decoder
    }

    pub fn set_potis(&mut self, potis: &[crate::poti::Poti]) {
        let Some(stored) = self.layout.stored_calibration else {
            return;
        };
        for (slot, analog) in self.potis.iter_mut().zip(self.layout.analogs) {
            *slot = potis
                .iter()
                .enumerate()
                .find(|&(i, _)| stored.analog_byte(i) == analog.byte)
                .map(|(_, p)| *p);
        }
    }

    pub fn layout(&self) -> &'static Layout {
        self.layout
    }
}

impl std::fmt::Debug for Decoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decoder").finish_non_exhaustive()
    }
}

fn event(name: &str, value: f32, raw: u8, pressed: Option<bool>) -> InputEvent {
    InputEvent {
        input: Input::Hid(name.to_owned()),
        value,
        raw,
        pressed,
    }
}

impl Decoder {
    pub fn decode(&mut self, report: &[u8]) -> Vec<InputEvent> {
        let Some(&id) = report.first() else {
            return Vec::new();
        };
        let previous = self.previous.insert(id, report.to_vec());
        let mut out = Vec::new();
        if id == 0x01 {
            self.decode_buttons(report, previous.as_deref(), &mut out);
        }
        if id == self.layout.encoder_report
            && let Some(prev) = previous.as_deref()
        {
            self.decode_encoders(report, prev, &mut out);
        }
        if id == self.layout.analog_report {
            self.decode_analogs(id, report, previous.as_deref(), &mut out);
        }
        if id == self.layout.jog_report
            && let Some(prev) = previous.as_deref()
        {
            self.decode_jogs(report, prev, &mut out);
        }
        out
    }

    fn decode_buttons(&self, report: &[u8], previous: Option<&[u8]>, out: &mut Vec<InputEvent>) {
        for switch in self.layout.switches {
            let read = |r: &[u8]| r.get(switch.byte).map(|v| (v >> switch.bit) & 0x03);
            let Some(now) = read(report) else { continue };
            if previous.and_then(read) != Some(now) {
                out.push(event(switch.name, f32::from(now.min(2)) / 2.0, now, None));
            }
        }
        let Some(prev) = previous else { return };
        for button in self.layout.buttons.iter().filter(|b| b.byte != 0) {
            let bit = |r: &[u8]| {
                r.get(button.byte)
                    .is_some_and(|v| v & (1 << button.bit) != 0)
            };
            let (now, before) = (bit(report), bit(prev));
            if now != before {
                out.push(event(
                    &button.name,
                    if now { 1.0 } else { 0.0 },
                    0,
                    Some(now),
                ));
            }
        }
    }

    fn decode_encoders(&self, report: &[u8], prev: &[u8], out: &mut Vec<InputEvent>) {
        for enc in self.layout.encoders {
            let nibble = |r: &[u8]| r.get(enc.byte).map(|v| (v >> enc.shift) & 0x0F);
            if let (Some(now), Some(before)) = (nibble(report), nibble(prev)) {
                let delta = ((i16::from(now) - i16::from(before) + 8).rem_euclid(16)) - 8;
                if delta != 0 {
                    let raw = (delta as i8 as u8) & 0x7F;
                    out.push(event(enc.name, f32::from(delta), raw, None));
                }
            }
        }
    }

    fn decode_analogs(
        &mut self,
        id: u8,
        report: &[u8],
        previous: Option<&[u8]>,
        out: &mut Vec<InputEvent>,
    ) {
        for (analog, poti) in self.layout.analogs.iter().zip(&self.potis) {
            let read = |r: &[u8]| {
                r.get(analog.byte..analog.byte + 2)
                    .map(|v| u16::from_le_bytes([v[0], v[1]]) & 0x0FFF)
            };
            let Some(now) = read(report) else { continue };
            let changed = previous.and_then(read).is_none_or(|before| {
                now.abs_diff(before) >= ANALOG_DEADBAND || now == 0 || now == 4095
            });
            let unchanged = previous.and_then(read) == Some(now);
            if changed && !unchanged {
                let value = poti.map_or(f32::from(now) / 4095.0, |p| p.value(now));
                out.push(event(analog.name, value, (now >> 5) as u8, None));
            }
        }
        if let Some(prev) = previous {
            let mut kept = report.to_vec();
            for analog in self.layout.analogs {
                let r = analog.byte..analog.byte + 2;
                let reported = out
                    .iter()
                    .any(|e| e.input == Input::Hid(analog.name.to_owned()));
                if !reported && prev.len() >= r.end && kept.len() >= r.end {
                    kept[r.clone()].copy_from_slice(&prev[r]);
                }
            }
            self.previous.insert(id, kept);
        }
    }

    fn decode_jogs(&self, report: &[u8], prev: &[u8], out: &mut Vec<InputEvent>) {
        for jog in self.layout.jogs {
            let read = |r: &[u8]| {
                r.get(jog.byte..jog.byte + 4)
                    .map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
            };
            if let (Some(now), Some(before)) = (read(report), read(prev)) {
                let delta = jog_delta(now, before, jog.bits);
                if delta != 0 {
                    let raw = (delta.clamp(-64, 63) as i8 as u8) & 0x7F;
                    out.push(event(jog.name, delta as f32, raw, None));
                }
            }
        }
    }

    pub fn led(&self, name: &str) -> Option<usize> {
        self.layout
            .buttons
            .iter()
            .find(|b| b.name == name)
            .and_then(|b| b.led)
    }

    fn button(&self, name: &str) -> Option<&Button> {
        self.layout.buttons.iter().find(|b| b.name == name)
    }
}

pub fn jog_delta(now: u32, before: u32, bits: u32) -> i32 {
    let mask: u64 = if bits >= 32 {
        u64::from(u32::MAX)
    } else {
        (1u64 << bits) - 1
    };
    let diff = u64::from(now).wrapping_sub(u64::from(before)) & mask;
    let signed = if diff > mask / 2 {
        diff as i64 - (mask as i64 + 1)
    } else {
        diff as i64
    };
    i32::try_from(signed).unwrap_or(0)
}

pub mod colour {
    pub const OFF: u8 = 0;
    pub const RED: u8 = 4;
    pub const ORANGE: u8 = 12;
    pub const YELLOW: u8 = 20;
    pub const GREEN: u8 = 28;
    pub const MINT: u8 = 32;
    pub const SKY: u8 = 40;
    pub const BLUE: u8 = 44;
    pub const WHITE: u8 = 68;
    pub const BRIGHT: u8 = 2;
    pub const MIXER_FX: [u8; 5] = [ORANGE, RED, GREEN, BLUE, YELLOW];

    const PALETTE: [[u8; 3]; 16] = [
        [255, 0, 0],
        [255, 64, 0],
        [255, 128, 0],
        [255, 176, 0],
        [255, 255, 0],
        [160, 255, 0],
        [0, 255, 0],
        [0, 255, 128],
        [0, 255, 255],
        [0, 176, 255],
        [0, 64, 255],
        [96, 0, 255],
        [160, 0, 255],
        [208, 0, 255],
        [255, 0, 255],
        [255, 0, 128],
    ];

    pub fn rgb(code: u8) -> [u8; 3] {
        let index = usize::from(code / 4);
        match index {
            0 => [0, 0, 0],
            1..=16 => PALETTE[index - 1],
            _ => [255, 255, 255],
        }
    }

    pub fn nearest(rgb: [u8; 3]) -> u8 {
        let distance = |p: &[u8; 3]| -> u32 {
            p.iter()
                .zip(rgb)
                .map(|(a, b)| u32::from(a.abs_diff(b)).pow(2))
                .sum()
        };
        let best = PALETTE
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| distance(p))
            .map_or(0, |(i, _)| i);
        4 * (best as u8 + 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leds(pub Vec<u8>);

impl Default for Leds {
    fn default() -> Self {
        Self(vec![0; LED_REPORT_LEN])
    }
}

impl Leds {
    pub fn for_layout(layout: &Layout) -> Self {
        Self(vec![0; layout.led_len()])
    }

    pub fn set(&mut self, decoder: &Decoder, name: &str, colour: u8, on: bool) {
        let Some(button) = decoder.button(name) else {
            return;
        };
        if let Some(base) = button.rgb {
            let rgb = colour::rgb(colour);
            for (k, c) in rgb.iter().enumerate() {
                if let Some(byte) = self.0.get_mut(base + k) {
                    let full = u16::from(*c) * 0x7F / 255;
                    *byte = if on { full } else { full / 8 } as u8;
                }
            }
            return;
        }
        if let Some(i) = button.led
            && i < self.0.len()
        {
            self.0[i] = match button.single {
                Some((lit, off)) => {
                    if on && colour != colour::OFF {
                        lit
                    } else {
                        off
                    }
                }
                None => colour + if on { colour::BRIGHT } else { 0 },
            };
        }
    }

    pub fn reports<'a>(&'a self, layout: &'a Layout) -> impl Iterator<Item = (u8, &'a [u8])> {
        let mut at = 0;
        layout.led_reports.iter().map(move |&(id, len)| {
            let end = (at + len).min(self.0.len());
            let slice = &self.0[at.min(end)..end];
            at += len;
            (id, slice)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meters(pub Vec<u8>);

impl Default for Meters {
    fn default() -> Self {
        Self(vec![0; METER_REPORT_LEN])
    }
}

const RING_REPORT: u8 = 0x32;
const RING_REPORT_LEN: usize = 40;
pub const RING_STEPS: u16 = 2880;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RingMode {
    #[default]
    Off,
    Spot,
    Flash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JogRing {
    pub mode: RingMode,
    pub position: u16,
    pub colour: u8,
}

impl JogRing {
    pub fn report(&self, side: u8) -> [u8; RING_REPORT_LEN + 1] {
        let mut r = [0u8; RING_REPORT_LEN + 1];
        r[0] = RING_REPORT;
        r[1] = side;
        r[2] = match self.mode {
            RingMode::Off => 0,
            RingMode::Spot => 2,
            RingMode::Flash => 3,
        };
        let [lo, hi] = (self.position % RING_STEPS).to_le_bytes();
        r[3] = lo;
        r[4] = hi;
        r[5] = self.colour;
        r
    }
}

pub struct Device {
    layout: &'static Layout,
    leds: Sender<Leds>,
    meters: Sender<Meters>,
    rings: Sender<[JogRing; 2]>,
    motors: Sender<motor::Settings>,
    calibration: Sender<Vec<[u8; crate::calibration::REPORT_LEN + 1]>>,
    running: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("connected", &self.connected())
            .finish_non_exhaustive()
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Device {
    pub fn start(
        layout: &'static Layout,
        events: Sender<(String, InputEvent)>,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let (led_tx, led_rx) = std::sync::mpsc::channel::<Leds>();
        let (meter_tx, meter_rx) = std::sync::mpsc::channel::<Meters>();
        let (ring_tx, ring_rx) = std::sync::mpsc::channel::<[JogRing; 2]>();
        let (motor_tx, motor_rx) = std::sync::mpsc::channel::<motor::Settings>();
        let (calibration_tx, calibration_rx) = std::sync::mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));
        let (alive, online) = (Arc::clone(&running), Arc::clone(&connected));
        std::thread::Builder::new()
            .name(layout.source.to_ascii_lowercase().replace(' ', ""))
            .spawn(move || {
                let outputs = Outputs {
                    leds: led_rx,
                    meters: meter_rx,
                    rings: ring_rx,
                    motors: motor_rx,
                    calibration: calibration_rx,
                };
                supervise(layout, &events, &outputs, &alive, &online, &wake);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            layout,
            leds: led_tx,
            meters: meter_tx,
            rings: ring_tx,
            motors: motor_tx,
            calibration: calibration_tx,
            running,
            connected,
        })
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn layout(&self) -> &'static Layout {
        self.layout
    }

    pub fn send_leds(&self, leds: Leds) {
        drop(self.leds.send(leds));
    }

    pub fn send_meters(&self, meters: Meters) {
        drop(self.meters.send(meters));
    }

    pub fn send_rings(&self, rings: [JogRing; 2]) {
        let _ = self.rings.send(rings);
    }

    pub fn send_motors(&self, settings: motor::Settings) {
        let _ = self.motors.send(settings);
    }

    pub fn calibrate(
        &self,
        group: &crate::calibration::Group,
        step: crate::calibration::Step,
    ) -> bool {
        self.connected()
            && self
                .calibration
                .send(crate::calibration::reports(step, group))
                .is_ok()
    }
}

struct Outputs {
    leds: Receiver<Leds>,
    meters: Receiver<Meters>,
    rings: Receiver<[JogRing; 2]>,
    motors: Receiver<motor::Settings>,
    calibration: Receiver<Vec<[u8; crate::calibration::REPORT_LEN + 1]>>,
}

#[derive(Default)]
struct Sent {
    leds: Option<Leds>,
    meters: Option<Meters>,
    rings: Option<[JogRing; 2]>,
    motors: Option<motor::Settings>,
}

const INITIAL_REPORTS: [(u8, usize); 2] = [(0x01, 26), (0x02, 79)];

const RETRY: Duration = Duration::from_secs(2);

fn supervise(
    layout: &'static Layout,
    events: &Sender<(String, InputEvent)>,
    outputs: &Outputs,
    running: &AtomicBool,
    connected: &AtomicBool,
    wake: &dyn Fn(),
) {
    let Ok(mut api) = hidapi::HidApi::new() else {
        return;
    };
    let (vid, pid) = (layout.vendor_id, layout.product_id);
    let mut last = Sent::default();
    while running.load(Ordering::Relaxed) {
        drop(api.refresh_devices());
        let present = api
            .device_list()
            .any(|d| d.vendor_id() == vid && d.product_id() == pid);
        if let Some(device) = present.then(|| api.open(vid, pid).ok()).flatten() {
            connected.store(true, Ordering::Relaxed);
            wake();
            run(layout, &device, events, outputs, &mut last, running, wake);
            connected.store(false, Ordering::Relaxed);
            wake();
        }
        outputs.calibration.try_iter().for_each(drop);
        let until = std::time::Instant::now() + RETRY;
        while running.load(Ordering::Relaxed) && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run(
    layout: &'static Layout,
    device: &hidapi::HidDevice,
    events: &Sender<(String, InputEvent)>,
    outputs: &Outputs,
    last: &mut Sent,
    running: &AtomicBool,
    wake: &dyn Fn(),
) {
    let source = layout.source;
    let mut decoder = Decoder::new(layout);
    if let Some(stored) = layout.stored_calibration
        && let Some(potis) = read_calibration(device, stored)
    {
        decoder.set_potis(&potis);
    }
    let mut motors = motor::Motors::default();
    if let Some(settings) = last.motors {
        motors.set(settings);
    }
    let mut buf = [0u8; 512];
    for &(id, len) in layout.initial_reports {
        let mut report = vec![0u8; len];
        report[0] = id;
        if let Ok(n) = device.get_input_report(&mut report)
            && n > 0
        {
            for e in decoder.decode(&report[..n]) {
                drop(events.send((source.into(), e)));
            }
        }
    }
    wake();
    let (mut leds_dirty, mut rings_dirty) = (last.leds.is_some(), last.rings.is_some());
    let mut meters_dirty = last.meters.is_some();
    while running.load(Ordering::Relaxed) {
        if let Some(latest) = outputs.leds.try_iter().last() {
            last.leds = Some(latest);
            leds_dirty = true;
        }
        if let Some(latest) = outputs.meters.try_iter().last() {
            last.meters = Some(latest);
            meters_dirty = true;
        }
        if let Some(latest) = outputs.rings.try_iter().last() {
            last.rings = Some(latest);
            rings_dirty = true;
        }
        if let Some(latest) = outputs.motors.try_iter().last() {
            last.motors = Some(latest);
            motors.set(latest);
        }
        for batch in outputs.calibration.try_iter() {
            for report in batch {
                drop(device.write(&report));
            }
        }
        if leds_dirty && let Some(state) = &last.leds {
            for (id, bytes) in state.reports(layout) {
                let mut report = Vec::with_capacity(bytes.len() + 1);
                report.push(id);
                report.extend_from_slice(bytes);
                drop(device.write(&report));
            }
            leds_dirty = false;
        }
        if meters_dirty
            && let Some(meters) = &last.meters
            && let Some((id, len)) = layout.meter_report
        {
            let mut report = Vec::with_capacity(len + 1);
            report.push(id);
            report.extend_from_slice(&meters.0);
            report.resize(len + 1, 0);
            drop(device.write(&report));
            meters_dirty = false;
        }
        if rings_dirty
            && layout.rings
            && let Some(rings) = &last.rings
        {
            for (side, ring) in (0u8..).zip(rings) {
                drop(device.write(&ring.report(side)));
            }
            rings_dirty = false;
        }
        match device.read_timeout(&mut buf, 10) {
            Ok(0) => {}
            Ok(n) => {
                if layout.motors && buf[0] == motor::JOG_REPORT {
                    for report in motors.step(&buf[..n]) {
                        drop(device.write(&report));
                    }
                }
                let decoded = decoder.decode(&buf[..n]);
                if !decoded.is_empty() {
                    for e in decoded {
                        drop(events.send((source.into(), e)));
                    }
                    wake();
                }
            }
            Err(_) => return,
        }
    }
    if layout.motors
        && let Some(stop) = motors.release()
    {
        drop(device.write(&stop));
    }
}

fn read_calibration(
    device: &hidapi::HidDevice,
    stored: &crate::poti::StoredCalibration,
) -> Option<Vec<crate::poti::Poti>> {
    use crate::poti::REPORT_DATA;
    let mut data = Vec::with_capacity(stored.reports.len() * REPORT_DATA);
    for &id in stored.reports {
        let mut report = [0u8; REPORT_DATA + 1];
        report[0] = id;
        let n = device.get_feature_report(&mut report).ok()?;
        if n != REPORT_DATA + 1 {
            return None;
        }
        data.extend_from_slice(&report[1..]);
    }
    stored.parse(&data)
}

#[allow(clippy::too_many_lines)]
pub fn default_mappings() -> Vec<Mapping> {
    let mut v = Vec::new();
    let mut map = |name: &str, action: Action, interaction: Interaction| {
        v.push(Mapping::new(
            Input::Hid(name.to_owned()),
            action,
            interaction,
        ));
    };
    for (deck, side) in [(0usize, "Left"), (1, "Right")] {
        map(
            &format!("{side}.Play"),
            Action::PlayPause { deck },
            Interaction::Trigger,
        );
        map(
            &format!("{side}.Cue"),
            Action::Cue { deck },
            Interaction::Hold,
        );
        map(
            &format!("{side}.Sync"),
            Action::Sync { deck },
            Interaction::Toggle,
        );
        map(
            &format!("{side}.Master"),
            Action::Master { deck },
            Interaction::Trigger,
        );
        for slot in 0..8 {
            map(
                &format!("{side}.Pads.{}", slot + 1),
                Action::Hotcue { deck, slot },
                Interaction::Hold,
            );
        }
        map(
            &format!("{side}.Transport Mode.Flux"),
            Action::Slip { deck },
            Interaction::Toggle,
        );
        map(
            &format!("{side}.Transport Mode.Reverse"),
            Action::Reverse { deck },
            Interaction::Hold,
        );
        map(
            &format!("{side}.Tempo"),
            Action::Tempo { deck },
            Interaction::Direct,
        );
        map(
            &format!("{side}.Loop.Push"),
            Action::FastLoop { deck },
            Interaction::Trigger,
        );
        map(
            &format!("{side}.Loop.Turn"),
            Action::LoopSizeStep { deck },
            Interaction::Relative,
        );
        map(
            &format!("{side}.Move.Turn"),
            Action::LoopMove { deck },
            Interaction::Relative,
        );
        map(
            &format!("{side}.Browse.Encoder.Turn"),
            Action::BrowserUpDown,
            Interaction::Relative,
        );
        map(
            &format!("{side}.Browse.Encoder.Push"),
            Action::LoadSelected { deck },
            Interaction::Trigger,
        );
        map(
            &format!("{side}.Browse.Add To Playlist"),
            Action::PrepListToggle,
            Interaction::Trigger,
        );
        map(
            &format!("{side}.Browse.View"),
            Action::BrowserView,
            Interaction::Trigger,
        );
        map(
            &format!("{side}.Browse.Preview"),
            Action::BrowserPreview,
            Interaction::Hold,
        );
        map(
            &format!("{side}.Browse.Favorite"),
            Action::BrowserFavorites,
            Interaction::Hold,
        );
        map(
            &format!("{side}.Jog.Touch"),
            Action::JogTouch { deck },
            Interaction::Hold,
        );
        map(
            &format!("{side}.Jog.Turn"),
            Action::JogTurn { deck },
            Interaction::Relative,
        );
        for (button, turntable) in [("TT", true), ("Jog", false)] {
            map(
                &format!("{side}.Jog Mode.{button}"),
                Action::JogMode { deck, turntable },
                Interaction::Trigger,
            );
        }
        let unit = deck;
        map(
            &format!("{side}.FX.Knob 1"),
            Action::FxDryWet { unit },
            Interaction::Direct,
        );
        map(
            &format!("{side}.FX.Button 1"),
            Action::FxOn { unit },
            Interaction::Toggle,
        );
        for k in 0..3 {
            map(
                &format!("{side}.FX.Knob {}", k + 2),
                Action::FxKnob { unit, knob: k },
                Interaction::Direct,
            );
            map(
                &format!("{side}.FX.Button {}", k + 2),
                Action::FxButton { unit, button: k },
                Interaction::Toggle,
            );
        }
    }
    for (channel, ch) in ["A", "B", "C", "D"].iter().enumerate() {
        map(
            &format!("Mixer.{ch}.Volume"),
            Action::Volume { channel },
            Interaction::Direct,
        );
        map(
            &format!("Mixer.{ch}.Gain"),
            Action::Gain { channel },
            Interaction::Direct,
        );
        map(
            &format!("Mixer.{ch}.EQ.High"),
            Action::Eq { channel, band: 2 },
            Interaction::Direct,
        );
        map(
            &format!("Mixer.{ch}.EQ.Mid"),
            Action::Eq { channel, band: 1 },
            Interaction::Direct,
        );
        map(
            &format!("Mixer.{ch}.EQ.Low"),
            Action::Eq { channel, band: 0 },
            Interaction::Direct,
        );
        map(
            &format!("Mixer.{ch}.Mixer FX.Amount"),
            Action::Filter { channel },
            Interaction::Direct,
        );
        map(
            &format!("Mixer.{ch}.Cue"),
            Action::MonitorCue { channel },
            Interaction::Toggle,
        );
        map(
            &format!("Mixer.{ch}.Mixer FX.On"),
            Action::MixerFxOn { channel },
            Interaction::Hold,
        );
    }
    for (slot, name) in ["Filter", "1", "2", "3", "4"].into_iter().enumerate() {
        map(
            &format!("Mixer.Mixer FX Select.{name}"),
            Action::MixerFxSelect { slot },
            Interaction::Hold,
        );
    }
    for (channel, ch) in ["A", "B", "C", "D"].iter().enumerate() {
        for unit in 0..2 {
            map(
                &format!("Mixer.{ch}.FX Assign {}", unit + 1),
                Action::FxAssign { channel, unit },
                Interaction::Toggle,
            );
        }
        map(
            &format!("Mixer.{ch}.Xfader Assign"),
            Action::XfaderAssign { channel },
            Interaction::Direct,
        );
        map(
            &format!("Mixer.{ch}.Ext"),
            Action::LiveInput { deck: channel },
            Interaction::Trigger,
        );
    }
    map(
        "Mixer.Xfader Curve",
        Action::XfaderCurve,
        Interaction::Direct,
    );
    map("Mixer.Xfader", Action::Crossfader, Interaction::Direct);
    map("Mixer.Master", Action::MasterVolume, Interaction::Direct);
    map("Mixer.Quant", Action::Quantize, Interaction::Toggle);
    for m in &mut v {
        m.invert = matches!(m.action, Action::Tempo { .. });
    }
    v
}

pub fn shifted(name: &str) -> Option<Option<(Action, Interaction)>> {
    let (side, control) = name.split_once('.')?;
    let deck = match side {
        "Left" => 0,
        "Right" => 1,
        "Mixer" => {
            return (control == "Quant").then_some(Some((Action::Snap, Interaction::Toggle)));
        }
        _ => return None,
    };
    if let Some(pad) = control.strip_prefix("Pads.") {
        let slot = pad.parse::<usize>().ok()?.checked_sub(1)?;
        return Some(Some((
            Action::DeleteHotcue { deck, slot },
            Interaction::Trigger,
        )));
    }
    Some(Some(match control {
        "Cue" => (Action::ReturnToStart { deck }, Interaction::Trigger),
        "Master" => (Action::TempoReset { deck }, Interaction::Trigger),
        "Move.Turn" => (Action::BeatJump { deck, beats: 1.0 }, Interaction::Relative),
        "Jog.Turn" => (Action::JogSearch { deck }, Interaction::Relative),
        "Browse.Encoder.Turn" => (Action::BrowserTree, Interaction::Relative),
        "Browse.Add To Playlist" => (Action::PrepListJump, Interaction::Trigger),
        "Loop.Turn" => (Action::KeyAdjust { deck }, Interaction::Relative),
        "Loop.Push" => (Action::Keylock { deck }, Interaction::Trigger),
        "Play" => (Action::MotorOff { deck }, Interaction::Trigger),
        "Sync" => (Action::TempoLock { deck }, Interaction::Trigger),
        "Tempo" => return Some(None),
        _ => return None,
    }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn report1(set: &[(usize, u8)]) -> Vec<u8> {
        let mut r = vec![0u8; 42];
        r[0] = 0x01;
        for &(byte, bit) in set {
            r[byte + 1] |= 1 << bit;
        }
        r
    }

    #[test]
    fn buttons_report_edges_after_the_baseline() {
        let mut d = Decoder::default();
        let first = d.decode(&report1(&[]));
        assert!(
            first.iter().all(|e| e.pressed.is_none()),
            "first report is the baseline for buttons (switches report their position)"
        );
        assert_eq!(first.len(), SWITCHES.len());
        let pressed = d.decode(&report1(&[(4, 0)]));
        assert_eq!(pressed, [event("Left.Play", 1.0, 0, Some(true))]);
        let released = d.decode(&report1(&[]));
        assert_eq!(released[0].pressed, Some(false));
        let pad = d.decode(&report1(&[(13, 0)]));
        assert_eq!(pad[0].input, Input::Hid("Right.Pads.8".into()));
    }

    #[test]
    fn encoders_give_signed_steps_across_the_wrap() {
        let mut d = Decoder::default();
        let mut r = report1(&[]);
        r[20] = 0x0F;
        d.decode(&r);
        r[20] = 0x01;
        let ev = d.decode(&r);
        assert_eq!(ev[0].input, Input::Hid("Left.Move.Turn".into()));
        assert_eq!(crate::midi::relative_steps(ev[0].raw), 2.0);
        r[20] = 0x0F;
        assert_eq!(crate::midi::relative_steps(d.decode(&r)[0].raw), -2.0);
    }

    #[test]
    fn analog_values_with_deadband() {
        let mut d = Decoder::default();
        let mut r = vec![0u8; 80];
        r[0] = 0x02;
        r[3..5].copy_from_slice(&2048u16.to_le_bytes());
        let first = d.decode(&r);
        assert!(first.iter().any(
            |e| e.input == Input::Hid("Mixer.A.Volume".into()) && (e.value - 0.5).abs() < 0.01
        ));
        r[3..5].copy_from_slice(&2050u16.to_le_bytes());
        assert!(
            d.decode(&r)
                .iter()
                .all(|e| e.input != Input::Hid("Mixer.A.Volume".into())),
            "noise ignored"
        );
        r[3..5].copy_from_slice(&4095u16.to_le_bytes());
        assert!(d.decode(&r).iter().any(
            |e| e.input == Input::Hid("Mixer.A.Volume".into()) && (e.value - 1.0).abs() < 1e-6
        ));
    }

    #[test]
    fn switches_report_positions() {
        let mut d = Decoder::default();
        d.decode(&report1(&[]));
        let ev = d.decode(&report1(&[(17, 5)]));
        assert_eq!(ev, [event("Mixer.A.Xfader Assign", 1.0, 2, None)]);
    }

    #[test]
    fn jog_counters_give_signed_steps() {
        let mut d = Decoder::default();
        let mut r = vec![0u8; 60];
        r[0] = 0x03;
        r[12..16].copy_from_slice(&5u32.to_le_bytes());
        assert!(d.decode(&r).is_empty(), "baseline");
        r[12..16].copy_from_slice(&2u32.to_le_bytes());
        r[40..44].copy_from_slice(&100u32.to_le_bytes());
        let ev = d.decode(&r);
        assert_eq!(ev[0].input, Input::Hid("Left.Jog.Turn".into()));
        assert!((ev[0].value + 3.0).abs() < 1e-6);
        assert!((ev[1].value - 100.0).abs() < 1e-6);
        r[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!((d.decode(&r)[0].value + 3.0).abs() < 1e-6);
    }

    fn all_controls() -> Vec<String> {
        layout_controls(&S4)
    }

    pub(crate) fn layout_controls(l: &Layout) -> Vec<String> {
        let mut names: Vec<String> = l.buttons.iter().map(|b| b.name.clone()).collect();
        names.extend(l.analogs.iter().map(|a| a.name.to_owned()));
        names.extend(l.encoders.iter().map(|e| e.name.to_owned()));
        names.extend(l.switches.iter().map(|s| s.name.to_owned()));
        names.extend(l.jogs.iter().map(|j| j.name.to_owned()));
        names
    }

    pub(crate) fn check_layout(l: &Layout) {
        let names = layout_controls(l);
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "duplicate control names");
        let mut bits = std::collections::HashSet::new();
        let mut leds = std::collections::HashSet::new();
        for b in &l.buttons {
            assert!(bits.insert((b.byte, b.bit)), "{} shares a bit", b.name);
            if let Some(led) = b.led {
                assert!(led < l.led_len(), "{} LED out of range", b.name);
                assert!(leds.insert(led), "{} shares an LED", b.name);
            }
            if let Some(base) = b.rgb {
                assert!(base + 2 < l.led_len(), "{} RGB out of range", b.name);
                for k in 0..3 {
                    assert!(leds.insert(base + k), "{} shares an LED", b.name);
                }
            }
        }
        for m in (l.default_mappings)() {
            assert!(
                l.has_input(&m),
                "default mapping on unknown control {}",
                m.input
            );
        }
    }

    #[test]
    fn control_tables_are_consistent() {
        let names = all_controls();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "duplicate control names");
        check_layout(&S4);
        let d = Decoder::default();
        let mut bits = std::collections::HashSet::new();
        let mut leds = std::collections::HashSet::new();
        for b in &d.layout.buttons {
            assert!(bits.insert((b.byte, b.bit)), "{} shares a bit", b.name);
            if let Some(led) = b.led {
                assert!(led < LED_REPORT_LEN, "{} LED out of range", b.name);
                assert!(leds.insert(led), "{} shares an LED", b.name);
            }
        }
        for sw in SWITCHES {
            for bit in [sw.bit, sw.bit + 1] {
                assert!(bits.insert((sw.byte, bit)), "{} overlaps a button", sw.name);
            }
        }
        for e in ENCODERS {
            for bit in e.shift..e.shift + 4 {
                assert!(bits.insert((e.byte, bit)), "{} overlaps", e.name);
            }
        }
        let mut words = std::collections::HashSet::new();
        for a in ANALOGS {
            assert!(words.insert(a.byte), "{} shares a word", a.name);
        }
    }

    #[test]
    fn default_mapping_uses_only_real_controls() {
        let names = all_controls();
        for m in default_mappings() {
            let Input::Hid(name) = &m.input else {
                panic!("{:?} is not a HID input", m.input)
            };
            assert!(names.contains(name), "{name} is not decoded");
            assert_eq!(
                m.interaction == Interaction::Relative,
                is_encoder(name),
                "{name}: relative iff encoder"
            );
        }
    }

    #[test]
    fn shifted_functions_exist_only_for_real_controls() {
        let names = all_controls();
        let with_shift: Vec<&String> = names.iter().filter(|n| shifted(n).is_some()).collect();
        assert!(with_shift.iter().any(|n| *n == "Left.Pads.8"));
        assert!(with_shift.iter().any(|n| *n == "Mixer.Quant"));
        assert_eq!(shifted("Left.Shift"), None, "SHIFT itself");
        assert_eq!(
            shifted("Right.Pads.1"),
            Some(Some((
                Action::DeleteHotcue { deck: 1, slot: 0 },
                Interaction::Trigger
            )))
        );
        for name in [
            "Cue",
            "Master",
            "Move.Turn",
            "Jog.Turn",
            "Browse.Encoder.Turn",
        ] {
            let full = format!("Left.{name}");
            assert!(names.contains(&full), "{full}");
            assert!(matches!(shifted(&full), Some(Some(_))), "{full}");
        }
    }

    #[test]
    fn jog_ring_reports() {
        let ring = JogRing {
            mode: RingMode::Spot,
            position: 2880 + 300,
            colour: colour::BLUE + colour::BRIGHT,
        };
        let r = ring.report(1);
        assert_eq!(r.len(), 41);
        assert_eq!(
            &r[..6],
            &[0x32, 1, 2, 44, 1, 46],
            "position wraps: 300 = 0x012c"
        );
        assert!(r[6..].iter().all(|&b| b == 0));
        assert_eq!(JogRing::default().report(0)[2], 0, "off");
    }

    #[test]
    fn leds_go_to_their_bytes() {
        let d = Decoder::default();
        let mut leds = Leds::default();
        leds.set(&d, "Left.Play", colour::GREEN, true);
        leds.set(&d, "Right.Pads.1", colour::BLUE, false);
        assert_eq!(leds.0[55], colour::GREEN + colour::BRIGHT);
        assert_eq!(leds.0[23], colour::BLUE);
    }

    #[test]
    fn default_mapping_covers_both_decks() {
        let m = default_mappings();
        assert!(
            m.iter().any(|m| m.input == Input::Hid("Right.Cue".into())
                && m.action == Action::Cue { deck: 1 })
        );
        assert!(m.len() > 60);
    }

    #[test]
    fn colours_go_to_the_nearest_palette_entry() {
        assert_eq!(colour::nearest([250, 10, 10]), colour::RED);
        assert_eq!(colour::nearest([0, 70, 250]), colour::BLUE);
        assert_eq!(colour::nearest([255, 128, 0]), colour::ORANGE);
        assert_eq!(colour::nearest([0, 255, 0]), colour::GREEN);
        assert_eq!(colour::nearest([0, 170, 255]), colour::SKY);
    }
}
