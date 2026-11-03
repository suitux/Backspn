use std::sync::LazyLock;

use super::loop_display::{hidden, loop_size_three_digits, phase_ms, write_seven_segments};
use super::{analog, button, db_meter, encoder, map};
use crate::feedback::Feedback;
use crate::s4mk3::{Analog, Button, Decoder, Encoder, Layout, Leds, Shifted, SoundCard, Switch};
use crate::{Action, Interaction, Mapping};

pub const NAME: &str = "Kontrol Z2";
pub const SOURCE: &str = "Z2";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x1130;

/// Report 0x80 carries LEDs 0–93 and 0x81 LEDs 94–133, so an LED's number is
/// also its place in the buffer.
const LED_REPORTS: &[(u8, usize)] = &[(0x80, 94), (0x81, 40)];
/// Hands the LEDs and displays over to the computer.
const HOST_MODE: &[u8] = &[0xF1, 0x93, 0x40];
const SINGLE: Option<(u8, u8)> = Some((0x7F, 0x0A));
/// Channel A and B line/phono inputs, the microphone and the effect send.
const INPUTS: crate::profile::Inputs = crate::profile::Inputs {
    decks: [Some((2, 3)), Some((4, 5)), None, None],
    microphone: Some((0, 1)),
    send: Some((6, 7)),
};
const METER_ON: u8 = 0x7F;
/// The buttons start at byte 3 of report 0x01, one bit each, low bit first.
const BUTTONS_AT: usize = 3;

fn bit(index: usize) -> (usize, u8) {
    (BUTTONS_AT + index / 8, 1 << (index % 8))
}

fn b(name: &str, index: usize, led: usize) -> Button {
    let (byte, mask) = bit(index);
    button(name, byte, mask, Some(led), SINGLE)
}

fn plain(name: &str, index: usize) -> Button {
    let (byte, mask) = bit(index);
    button(name, byte, mask, None, None)
}

fn pad(name: String, index: usize, rgb: usize) -> Button {
    let (byte, mask) = bit(index);
    Button {
        rgb: Some(rgb),
        ..button(name, byte, mask, None, None)
    }
}

fn buttons() -> Vec<Button> {
    let mut v = vec![
        b("Left.Shift", 32, 5),
        b("Mixer.Snap", 2, 2),
        b("Mixer.Quant", 4, 3),
        b("Mixer.Settings", 3, 4),
        plain("Mixer.Mic", 16),
        plain("Mixer.Browse.Push", 7),
        plain("Mixer.A.Ext", 0),
        plain("Mixer.B.Ext", 1),
        b("Mixer.A.Cue", 10, 128),
        b("Mixer.B.Cue", 11, 129),
        b("Mixer.A.Mixer FX.On", 30, 13),
        b("Mixer.B.Mixer FX.On", 39, 21),
        b("Mixer.A.FX Assign 1", 18, 14),
        b("Mixer.A.FX Assign 2", 19, 15),
        b("Mixer.B.FX Assign 1", 41, 22),
        b("Mixer.B.FX Assign 2", 42, 23),
        b("Left.Deck.C", 24, 8),
        b("Left.Deck.A", 25, 6),
        b("Right.Deck.B", 33, 7),
        b("Right.Deck.D", 34, 9),
        b("Left.Load", 8, 16),
        b("Right.Load", 9, 24),
        b("Left.Sync", 14, 19),
        b("Right.Sync", 15, 27),
        plain("Left.Rel/Int", 12),
        plain("Right.Rel/Int", 13),
        b("Left.FX.Button 1", 17, 12),
        b("Right.FX.Button 1", 40, 20),
        plain("Left.Loop.Push", 22),
        plain("Right.Loop.Push", 45),
    ];
    for k in 0..4 {
        v.push(pad(format!("Left.Pads.{}", k + 1), 26 + k, 28 + 3 * k));
        v.push(pad(format!("Right.Pads.{}", k + 1), 35 + k, 40 + 3 * k));
    }
    v
}

/// Every knob and fader is a 16-bit value in report 0x02, two bytes apiece
/// from byte 1. The ones not listed (0–2, 4, 5) set levels on the unit itself.
const ANALOGS: &[Analog] = &[
    analog("Mixer.Cue Mix", 0x07),
    analog("Left.FX.Knob 1", 0x0D),
    analog("Left.FX.Knob 2", 0x0F),
    analog("Mixer.A.Gain", 0x11),
    analog("Mixer.A.EQ.High", 0x13),
    analog("Mixer.A.EQ.Mid", 0x15),
    analog("Mixer.A.EQ.Low", 0x17),
    analog("Mixer.A.Mixer FX.Amount", 0x19),
    analog("Right.FX.Knob 1", 0x1B),
    analog("Right.FX.Knob 2", 0x1D),
    analog("Mixer.B.Gain", 0x1F),
    analog("Mixer.B.EQ.High", 0x21),
    analog("Mixer.B.EQ.Mid", 0x23),
    analog("Mixer.B.EQ.Low", 0x25),
    analog("Mixer.B.Mixer FX.Amount", 0x27),
    analog("Mixer.C.Volume", 0x29),
    analog("Mixer.D.Volume", 0x2B),
    analog("Mixer.A.Volume", 0x2D),
    analog("Mixer.B.Volume", 0x2F),
    analog("Mixer.Xfader", 0x31),
    analog("Mixer.Xfader Curve", 0x33),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Browse.Encoder.Turn", 0x01, false),
    encoder("Left.Loop.Turn", 0x01, true),
    encoder("Right.Loop.Turn", 0x02, false),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 64), (0x02, 64)];

/// Four meters, one per channel A–D: an indicator LED and then seven
/// segments, eight LEDs apart.
const METER_FIRST: [usize; 4] = [95, 103, 111, 119];
const METER_DB: [f32; 6] = [-120.0, -12.0, -8.0, -5.2, -3.1, -1.4];

/// Six digits of seven LEDs each from LED 52, three per side.
const DISPLAY_FIRST: [usize; 2] = [52, 73];
const WARNING: usize = 0;
const TIMECODE: usize = 1;
/// The lights beside the C and D volume knobs.
const DECK_VOLUME: [usize; 2] = [10, 11];
/// Rel/Int lights per side: orange, then green.
const REL_INT: [[usize; 2]; 2] = [[17, 18], [25, 26]];

fn meter_leds(feedback: &Feedback, _decoder: &Decoder, leds: &mut Leds) {
    for (ch, &first) in METER_FIRST.iter().enumerate() {
        leds.0[first - 1] = METER_ON;
        db_meter(
            &mut leds.0,
            first,
            &METER_DB,
            feedback.channels[ch].level,
            METER_ON,
        );
    }
}

fn deck_leds(feedback: &Feedback, decoder: &Decoder, layer: [usize; 2], leds: &mut Leds) {
    let lit = |on: bool| SINGLE.map_or(0, |(lit, off)| if on { lit } else { off });
    let phase = phase_ms();
    for (i, side) in ["Left", "Right"].into_iter().enumerate() {
        let deck = &feedback.decks[layer[i]];
        if let Some(led) = decoder.led(&format!("{side}.Sync")) {
            leds.0[led] = lit(deck.synced);
        }
        let digits = if hidden(deck.loop_active, deck.live, phase) {
            [0; 3]
        } else {
            loop_size_three_digits(deck.loop_size)
        };
        write_seven_segments(&mut leds.0, DISPLAY_FIRST[i], &digits, METER_ON);
        let [orange, green] = REL_INT[i];
        leds.0[orange] = lit(false);
        leds.0[green] = lit(true);
        leds.0[DECK_VOLUME[i]] = lit(feedback.decks[i + 2].playing);
    }
    leds.0[WARNING] = lit(warning(
        feedback.audio_overloaded,
        feedback.audio_dropout,
        phase,
    ));
    leds.0[TIMECODE] = lit(false);
}

/// Blinks after a dropout, stays lit while the audio is close to dropping out.
fn warning(overloaded: bool, dropout: bool, phase_ms: u128) -> bool {
    if dropout {
        !hidden(true, false, phase_ms)
    } else {
        overloaded
    }
}

/// The crossfader reverse switch on the front panel: with it on, channel A
/// sits on the right of the crossfader and B on the left.
const XFADER_REVERSE: &str = "Mixer.Xfader Reverse";

const SWITCHES: &[Switch] = &[Switch {
    name: XFADER_REVERSE,
    byte: 8,
    bit: 7,
    bits: 1,
}];

/// Puts channels A and B on their crossfader sides for the switch position.
fn xfader_reverse(channel: usize) -> Mapping {
    let mut m = map(
        XFADER_REVERSE,
        Action::XfaderAssign { channel },
        Interaction::Direct,
    );
    let (normal, reversed) = if channel == 0 { (1.0, 0.0) } else { (0.0, 1.0) };
    m.positions = vec![(0, normal), (1, reversed)];
    m
}

fn default_mappings() -> Vec<Mapping> {
    let mut extra = vec![
        xfader_reverse(0),
        xfader_reverse(1),
        map("Mixer.Snap", Action::Snap, Interaction::Toggle),
        map("Mixer.Mic", Action::MicToggle, Interaction::Trigger),
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
    }
    super::default_mappings(&LAYOUT, extra)
}

fn shifted(name: &str) -> Shifted {
    match name {
        "Mixer.A.Mixer FX.On" => Some(Some((Action::BeatTap { deck: 0 }, Interaction::Trigger))),
        "Mixer.B.Mixer FX.On" => Some(Some((Action::BeatTap { deck: 1 }, Interaction::Trigger))),
        _ => super::common_shifted(name),
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
    erps: &[],
    jogs: &[],
    jog_steps_per_turn: 1.0,
    analog_report: 0x02,
    jog_report: 0x03,
    encoder_report: 0x01,
    led_reports: LED_REPORTS,
    meter_report: None,
    initial_reports: INITIAL_REPORTS,
    motors: false,
    sound_card: Some(SoundCard {
        name: NAME,
        master: (0, 1),
        monitor: (2, 3),
        buffer_frames: 512,
        inputs: INPUTS,
    }),
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
    shared_shift: true,
    connect_reports: &[HOST_MODE],
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

    fn find(name: &str) -> Option<(Action, Interaction)> {
        default_mappings()
            .into_iter()
            .find(|m| m.input == Input::Hid(name.into()))
            .map(|m| (m.action, m.interaction))
    }

    #[test]
    fn buttons_count_bits_from_byte_three() {
        assert_eq!(bit(0), (3, 0x01));
        assert_eq!(bit(10), (4, 0x04));
        assert_eq!(bit(32), (7, 0x01));
        assert_eq!(bit(45), (8, 0x20));
    }

    #[test]
    fn the_mixer_maps_both_channels_and_the_deck_volumes() {
        assert_eq!(
            find("Mixer.A.EQ.High"),
            Some((
                Action::Eq {
                    channel: 0,
                    band: 2
                },
                Interaction::Direct
            ))
        );
        assert_eq!(
            find("Mixer.B.Mixer FX.Amount"),
            Some((Action::Filter { channel: 1 }, Interaction::Direct))
        );
        assert_eq!(
            find("Mixer.C.Volume"),
            Some((Action::Volume { channel: 2 }, Interaction::Direct))
        );
        assert_eq!(
            find("Mixer.A.Cue"),
            Some((Action::MonitorCue { channel: 0 }, Interaction::Toggle))
        );
        assert_eq!(
            find("Mixer.Xfader Curve"),
            Some((Action::XfaderCurve, Interaction::Direct))
        );
        assert_eq!(
            find("Mixer.Cue Mix"),
            Some((Action::CueMix, Interaction::Direct))
        );
        assert_eq!(
            find("Mixer.Snap"),
            Some((Action::Snap, Interaction::Toggle))
        );
        assert_eq!(
            find("Mixer.Quant"),
            Some((Action::Quantize, Interaction::Toggle))
        );
        assert_eq!(find("Mixer.Settings"), None, "a menu on the unit");
    }

    #[test]
    fn each_side_drives_its_fx_unit_and_deck() {
        assert_eq!(
            find("Right.FX.Knob 1"),
            Some((Action::FxDryWet { unit: 1 }, Interaction::Direct))
        );
        assert_eq!(
            find("Left.FX.Button 1"),
            Some((Action::FxOn { unit: 0 }, Interaction::Toggle))
        );
        assert_eq!(
            find("Right.Load"),
            Some((Action::LoadSelected { deck: 1 }, Interaction::Trigger))
        );
        assert_eq!(
            find("Left.Pads.4"),
            Some((Action::Hotcue { deck: 0, slot: 3 }, Interaction::Hold))
        );
        assert_eq!(
            find("Right.Loop.Turn"),
            Some((Action::LoopSizeStep { deck: 1 }, Interaction::Relative))
        );
        assert_eq!(
            shifted("Mixer.B.Mixer FX.On"),
            Some(Some((Action::BeatTap { deck: 1 }, Interaction::Trigger)))
        );
    }

    #[test]
    fn knobs_come_in_report_two_and_buttons_in_report_one() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 64];
        r[0] = 0x02;
        r[0x31] = 0xFF;
        r[0x32] = 0x0F;
        let events = d.decode(&r);
        let x = events
            .iter()
            .find(|e| e.input == Input::Hid("Mixer.Xfader".into()))
            .expect("crossfader");
        assert!((x.value - 1.0).abs() < 1e-6);
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        d.decode(&r);
        r[4] |= 0x04;
        let events = d.decode(&r);
        assert!(
            events
                .iter()
                .any(|e| e.input == Input::Hid("Mixer.A.Cue".into()) && e.pressed == Some(true))
        );
    }

    #[test]
    fn meters_light_from_the_bottom() {
        let mut f = Feedback::default();
        f.channels[0].level = 0.5;
        f.channels[3].level = 1.5;
        let mut leds = Leds::for_layout(&LAYOUT);
        meter_leds(&f, &Decoder::new(&LAYOUT), &mut leds);
        assert_eq!(
            &leds.0[95..102],
            &[METER_ON, METER_ON, METER_ON, 0, 0, 0, 0]
        );
        assert!(leds.0[103..110].iter().all(|&b| b == 0), "silent channel B");
        assert_eq!(
            &leds.0[119..126],
            &[METER_ON; 7],
            "channel D, clip lit over full scale"
        );
        assert!(
            [94, 102, 110, 118].iter().all(|&i| leds.0[i] == METER_ON),
            "indicators"
        );
        let reports: Vec<_> = leds.reports(&LAYOUT).map(|(id, b)| (id, b.len())).collect();
        assert_eq!(reports, [(0x80, 94), (0x81, 40)]);
    }

    #[test]
    fn deck_select_one_is_the_outer_deck() {
        let l = &*LAYOUT;
        let led = |name: &str| {
            l.buttons
                .iter()
                .find(|b| b.name == name)
                .and_then(|b| b.led)
        };
        assert_eq!(bit(24), (6, 0x01));
        assert_eq!(led("Left.Deck.C"), Some(8));
        assert_eq!(led("Left.Deck.A"), Some(6));
        assert_eq!(led("Right.Deck.B"), Some(7));
        assert_eq!(led("Right.Deck.D"), Some(9));
    }

    #[test]
    fn displays_show_each_side_s_loop_size() {
        let mut f = Feedback::default();
        f.decks[0].loop_size = 7;
        f.decks[3].loop_size = 2;
        f.decks[3].playing = true;
        f.audio_overloaded = true;
        let d = Decoder::new(&LAYOUT);
        let mut leds = Leds::for_layout(&LAYOUT);
        deck_leds(&f, &d, [0, 3], &mut leds);
        let mut want = [0u8; 21];
        write_seven_segments(&mut want, 0, &loop_size_three_digits(7), METER_ON);
        assert_eq!(&leds.0[52..73], &want, "left: 4 beats");
        write_seven_segments(&mut want, 0, &loop_size_three_digits(2), METER_ON);
        assert_eq!(&leds.0[73..94], &want, "right: 1/8");
        let (on, off) = SINGLE.expect("single colour");
        assert_eq!(leds.0[WARNING], on);
        assert_eq!(leds.0[TIMECODE], off);
        assert_eq!(leds.0[DECK_VOLUME[0]], off, "deck C stopped");
        assert_eq!(leds.0[DECK_VOLUME[1]], on, "deck D playing");
        assert_eq!([leds.0[17], leds.0[18]], [off, on], "internal playback");
        assert!(leds.0.len() >= 134);
    }

    #[test]
    fn the_reverse_switch_swaps_the_crossfader_sides() {
        let defaults = default_mappings();
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 10];
        r[0] = 0x01;
        let side = |events: &[crate::InputEvent], channel: usize| {
            let e = events
                .iter()
                .find(|e| e.input == Input::Hid(XFADER_REVERSE.into()))
                .expect("switch reported");
            let m = defaults
                .iter()
                .find(|m| m.action == Action::XfaderAssign { channel })
                .expect("mapped");
            crate::trigger(m, e)
        };
        let normal = d.decode(&r);
        assert_eq!(side(&normal, 0), Some(crate::Trigger::Set(1.0)), "A left");
        assert_eq!(side(&normal, 1), Some(crate::Trigger::Set(0.0)), "B right");
        r[8] = 0x80;
        let reversed = d.decode(&r);
        assert_eq!(side(&reversed, 0), Some(crate::Trigger::Set(0.0)));
        assert_eq!(side(&reversed, 1), Some(crate::Trigger::Set(1.0)));
    }

    #[test]
    fn the_warning_blinks_after_a_dropout() {
        assert!(!warning(false, false, 0));
        assert!(
            warning(true, false, 0) && warning(true, false, 200),
            "steady"
        );
        assert!(warning(false, true, 0));
        assert!(!warning(false, true, 200), "blinks");
        assert!(!warning(true, true, 200), "the dropout wins");
    }

    #[test]
    fn the_unit_is_its_own_sound_card() {
        let card = LAYOUT.sound_card.expect("sound card");
        assert_eq!((card.master, card.monitor), ((0, 1), (2, 3)));
        assert_eq!(card.inputs.decks[..2], [Some((2, 3)), Some((4, 5))]);
        assert_eq!(card.inputs.microphone, Some((0, 1)));
        assert_eq!(card.inputs.send, Some((6, 7)));
    }
}
