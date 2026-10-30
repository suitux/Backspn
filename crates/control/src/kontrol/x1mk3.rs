use std::sync::LazyLock;

use super::mono::{self, Canvas};
use super::{analog, button, encoder, map};
use crate::feedback::Feedback;
use crate::modifier::{Condition, Modifiers};
use crate::s4mk3::{
    Analog, Button, Decoder, Encoder, Layout, Leds, ModifierHooks, Shifted, colour,
};
use crate::state::{Control, StateCondition};
use crate::{Action, Input, Interaction, Mapping, deck_letter};

pub const NAME: &str = "Kontrol X1 MK3";
pub const SOURCE: &str = "X1 MK3";
pub const VENDOR_ID: u16 = 0x17CC;
pub const PRODUCT_ID: u16 = 0x2200;

const LED_LEN: usize = 49;
const SINGLE: Option<(u8, u8)> = Some((0x7E, 0x7C));
const LOOP_LED: [usize; 2] = [1, 2];
const BOTTOM_LEDS: [[usize; 6]; 2] = [[43, 44, 45, 46, 47, 48], [41, 40, 39, 38, 37, 36]];
const NUDGE_BEATS: f64 = 32.0;
/// Switches the device from MIDI to HID control on connect.
const APP_MODE: &[u8] = &[0xA0, 0x00, 0xFF];

/// MODE steps the FX section through three layers, kept in this modifier:
/// FX units 1 and 2, units 3 and 4 (skipped with two units), then the
/// channel's EQ and volume.
const LAYER: usize = 0;
const PRIMARY: u8 = 0;
const SECONDARY: u8 = 1;
const MIXER: u8 = 2;

/// Report id of the first display; the others follow in this order.
const FIRST_DISPLAY: u8 = 0xE0;
const LEFT_FX: usize = 0;
const MODE: usize = 1;
const RIGHT_FX: usize = 2;
const LEFT_LOOP: usize = 3;
const RIGHT_LOOP: usize = 4;
const DISPLAYS: usize = 5;

/// Buttons are numbered as bits of the report from its first data byte;
/// most have their own colour LED two places further on.
fn at(index: usize) -> (usize, u8) {
    (1 + index / 8, 1 << (index % 8))
}

fn b(name: &str, index: usize) -> Button {
    let (byte, mask) = at(index);
    button(name, byte, mask, Some(index + 2), None)
}

fn plain(name: &str, index: usize) -> Button {
    let (byte, mask) = at(index);
    button(name, byte, mask, None, None)
}

fn buttons() -> Vec<Button> {
    let (byte, mask) = at(0);
    let mut v = vec![
        button("Left.Shift", byte, mask, Some(0), SINGLE),
        plain("Mode", 31),
    ];
    for (side, first) in [("Left", 0usize), ("Right", 1)] {
        let n = |c: &str| format!("{side}.{c}");
        let pair = 2 * first;
        v.extend([
            b(&n("Play"), 1 + pair),
            b(&n("Sync"), 2 + pair),
            b(&n("Cue"), 5 + pair),
            b(&n("Transport Mode.Reverse"), 6 + pair),
            b(&n("Nudge.Slow"), 9 + pair),
            b(&n("Nudge.Fast"), 10 + pair),
            b(&n("Pads.1"), 17 + pair),
            b(&n("Pads.2"), 18 + pair),
            b(&n("Pads.3"), 13 + pair),
            b(&n("Pads.4"), 14 + pair),
            b(&n("FX.Button 1"), 27 + first),
            b(&n("FX.Button 2"), 25 + first),
            b(&n("FX.Button 3"), 23 + first),
            b(&n("FX.Button 4"), 21 + first),
            b(&n("Assign.Left"), 29 + 3 * first),
            b(&n("Assign.Right"), 30 + 3 * first),
            plain(&n("Loop.Push"), 34 + first),
            plain(&n("Browse.Encoder.Push"), 36 + first),
        ]);
    }
    v.extend(EXTRA.iter().map(|&(name, index)| plain(name, index)));
    v
}

/// The last four bits have no LED and no function of their own (probably the
/// encoders' touch sensors); they are left free for the user's mappings.
const EXTRA: [(&str, usize); 4] = [
    ("Extra.1", 38),
    ("Extra.2", 39),
    ("Extra.3", 40),
    ("Extra.4", 41),
];

const ANALOGS: &[Analog] = &[
    analog("Left.FX.Knob 1", 0x15),
    analog("Left.FX.Knob 2", 0x11),
    analog("Left.FX.Knob 3", 0x0D),
    analog("Left.FX.Knob 4", 0x09),
    analog("Right.FX.Knob 1", 0x17),
    analog("Right.FX.Knob 2", 0x13),
    analog("Right.FX.Knob 3", 0x0F),
    analog("Right.FX.Knob 4", 0x0B),
];

const ENCODERS: &[Encoder] = &[
    encoder("Left.Loop.Turn", 0x07, false),
    encoder("Right.Loop.Turn", 0x07, true),
    encoder("Left.Browse.Encoder.Turn", 0x08, false),
    encoder("Right.Browse.Encoder.Turn", 0x08, true),
];

const INITIAL_REPORTS: &[(u8, usize)] = &[(0x01, 64)];

fn deck_leds(feedback: &Feedback, decoder: &Decoder, layer: [usize; 2], leds: &mut Leds) {
    for (i, side) in ["Left", "Right"].into_iter().enumerate() {
        let deck = &feedback.decks[layer[i]];
        let channel = &feedback.channels[layer[i]];
        let n = |c: &str| format!("{side}.{c}");
        for (k, button) in ["Assign.Left", "Assign.Right"].into_iter().enumerate() {
            leds.set(decoder, &n(button), colour::ORANGE, channel.fx_assign[k]);
        }
        for button in ["Nudge.Slow", "Nudge.Fast"] {
            leds.set(decoder, &n(button), colour::WHITE, false);
        }
        leds.0[LOOP_LED[i]] = if deck.loop_active { 0x7E } else { 0x7C };
        let strip = if deck.loop_active {
            colour::GREEN + colour::BRIGHT
        } else if deck.playing && deck.end_warning {
            colour::RED + colour::BRIGHT
        } else {
            colour::OFF
        };
        for &led in &BOTTOM_LEDS[i] {
            leds.0[led] = strip;
        }
    }
}

fn default_mappings() -> Vec<Mapping> {
    let mut extra = Vec::new();
    for (deck, side) in [(0usize, "Left"), (1, "Right")] {
        for (button, direction) in [("Slow", -1.0), ("Fast", 1.0)] {
            extra.push(map(
                &format!("{side}.Nudge.{button}"),
                Action::TempoBend { deck, direction },
                Interaction::Hold,
            ));
        }
        for (unit, button) in ["Left", "Right"].into_iter().enumerate() {
            extra.push(map(
                &format!("{side}.Assign.{button}"),
                Action::FxAssign {
                    channel: deck,
                    unit,
                },
                Interaction::Toggle,
            ));
        }
    }
    let mut v = Vec::new();
    for m in super::default_mappings(&LAYOUT, extra) {
        let Some((deck, name, control)) = fx_control(&m.input) else {
            v.push(m);
            continue;
        };
        let secondary = on_unit(m.action, deck + 2).map(|action| Mapping {
            action,
            ..m.clone()
        });
        let mixer = mixer_layer(deck, control).map(|(action, interaction)| {
            let mut mixer = map(name, action, interaction);
            mixer.soft_takeover = interaction == Interaction::Direct;
            mixer
        });
        v.push(when(m, PRIMARY));
        v.extend(secondary.map(|m| when(m, SECONDARY)));
        v.extend(mixer.map(|m| when(m, MIXER)));
    }
    for (from, to, units) in [
        (PRIMARY, SECONDARY, Some(4)),
        (PRIMARY, MIXER, Some(2)),
        (SECONDARY, MIXER, None),
        (MIXER, PRIMARY, None),
    ] {
        let mut m = when(
            map(
                "Mode",
                Action::Modifier { index: LAYER },
                Interaction::Direct,
            ),
            from,
        );
        m.gate.value = Some(to);
        if let Some(value) = units {
            m.gate.states.push(StateCondition {
                control: Control::FxUnits,
                channel: None,
                value,
            });
        }
        v.push(m);
    }
    v
}

fn when(mut m: Mapping, layer: u8) -> Mapping {
    m.gate.conditions.push(Condition {
        modifier: LAYER,
        value: layer,
    });
    m
}

/// The deck of a control in either FX section, its full name and its name
/// within the section.
fn fx_control(input: &Input) -> Option<(usize, &str, &str)> {
    let Input::Hid(name) = input else {
        return None;
    };
    let (side, control) = name.split_once(".FX.")?;
    let deck = match side {
        "Left" => 0,
        "Right" => 1,
        _ => return None,
    };
    Some((deck, name, control))
}

fn on_unit(action: Action, unit: usize) -> Option<Action> {
    Some(match action {
        Action::FxDryWet { .. } => Action::FxDryWet { unit },
        Action::FxKnob { knob, .. } => Action::FxKnob { unit, knob },
        Action::FxOn { .. } => Action::FxOn { unit },
        Action::FxButton { button, .. } => Action::FxButton { unit, button },
        _ => return None,
    })
}

/// In the mixer layer the knobs set the high, mid and low EQ and the
/// volume of the side's channel; the buttons below them cut those bands and
/// the last one is the channel's cue.
fn mixer_layer(channel: usize, control: &str) -> Option<(Action, Interaction)> {
    let eq = |band| Some((Action::Eq { channel, band }, Interaction::Direct));
    let kill = |band| Some((Action::EqKill { channel, band }, Interaction::Toggle));
    match control {
        "Knob 1" => eq(2),
        "Knob 2" => eq(1),
        "Knob 3" => eq(0),
        "Button 1" => kill(2),
        "Button 2" => kill(1),
        "Button 3" => kill(0),
        "Knob 4" => Some((Action::Volume { channel }, Interaction::Direct)),
        "Button 4" => Some((Action::MonitorCue { channel }, Interaction::Toggle)),
        _ => None,
    }
}

fn displays(feedback: &Feedback, layer: [usize; 2], modifiers: &Modifiers) -> Vec<Vec<u8>> {
    let fx_layer = modifiers.0.get(LAYER).copied().unwrap_or(PRIMARY);
    let mut screens: [Canvas; DISPLAYS] = Default::default();
    fx_screen(&mut screens[LEFT_FX], feedback, 0, fx_layer);
    mode_screen(&mut screens[MODE], layer, fx_layer);
    fx_screen(&mut screens[RIGHT_FX], feedback, 1, fx_layer);
    loop_screen(&mut screens[LEFT_LOOP], feedback, layer[0]);
    loop_screen(&mut screens[RIGHT_LOOP], feedback, layer[1]);
    (FIRST_DISPLAY..)
        .zip(&screens)
        .flat_map(|(id, c)| mono::reports(id, c))
        .collect()
}

/// A tag in the top right corner, filled when `on`.
fn tag(c: &mut Canvas, label: &str, on: bool) {
    let w = mono::text_width(label, 1) + 4;
    let x = mono::WIDTH - w;
    if on {
        c.fill((x, 0), (w, 11), true);
    } else {
        c.outline((x, 0), (w, 11));
    }
    c.text((x + 2, 2), 1, label, !on);
}

fn fx_screen(c: &mut Canvas, f: &Feedback, side: usize, layer: u8) {
    if layer == MIXER {
        let channel = side;
        c.text((0, 0), 2, "EQ", true);
        tag(c, &deck_letter(channel).to_string(), true);
        let eq = f.knobs.eq.get(channel).copied().unwrap_or_default();
        let fader = f.knobs.fader.get(channel).copied().unwrap_or_default();
        let bars = [(eq[2], "HI"), (eq[1], "MID"), (eq[0], "LO"), (fader, "VOL")];
        for (i, (value, label)) in bars.into_iter().enumerate() {
            let x = 6 + i * 31;
            c.outline((x, 20), (22, 34));
            let h = (value.clamp(0.0, 1.0) * 32.0).round() as usize;
            c.fill((x + 1, 53 - h), (20, h), true);
            let lx = (x + 11).saturating_sub(mono::text_width(label, 1) / 2);
            c.text((lx, 57), 1, label, true);
        }
        return;
    }
    let unit = side + if layer == SECONDARY { 2 } else { 0 };
    let Some(fx) = f.fx.get(unit) else {
        return;
    };
    c.text((0, 0), 2, &format!("FX{}", unit + 1), true);
    tag(c, if fx.on { "ON" } else { "OFF" }, fx.on);
    let mode = match fx.mode {
        0 => "GROUP",
        1 => "SINGLE",
        _ => "DRUMS",
    };
    c.text_centred(24, 2, mode);
    for (i, &on) in fx.buttons.iter().enumerate() {
        let at = (6 + i * 41, 50);
        if on {
            c.fill(at, (34, 12), true);
        } else {
            c.outline(at, (34, 12));
        }
    }
}

fn mode_screen(c: &mut Canvas, layer: [usize; 2], fx_layer: u8) {
    let title = match fx_layer {
        PRIMARY => "FX 1 2",
        SECONDARY => "FX 3 4",
        _ => "MIXER",
    };
    c.text_centred(8, 2, title);
    c.text((8, 40), 2, &deck_letter(layer[0]).to_string(), true);
    c.text_right((mono::WIDTH - 8, 40), 2, &deck_letter(layer[1]).to_string());
}

fn loop_label(index: u8) -> String {
    let exponent = i32::from(index.min(10)) - 5;
    if exponent >= 0 {
        (1u32 << exponent).to_string()
    } else {
        format!("1/{}", 1u32 << -exponent)
    }
}

fn loop_screen(c: &mut Canvas, f: &Feedback, deck: usize) {
    let Some(d) = f.decks.get(deck) else {
        return;
    };
    c.text((0, 0), 2, &deck_letter(deck).to_string(), true);
    tag(c, "LOOP", d.loop_active);
    c.text_centred(18, 4, &loop_label(d.loop_size));
    if !d.loaded {
        return;
    }
    let bottom = mono::HEIGHT - mono::text_height(1);
    c.text(
        (0, bottom),
        1,
        &format!("{:+.1}%", (d.tempo - 1.0) * 100.0),
        true,
    );
    let left = (d.duration_s - d.position_s).max(0.0) as u64;
    let time = format!("-{}:{:02}", left / 60, left % 60);
    c.text_right((mono::WIDTH, bottom), 1, &time);
}

/// EQ band of each mixer-layer button, high to low.
const KILL_BANDS: [usize; 3] = [2, 1, 0];

/// SHIFT on an FX control acts on the units of the current layer; the mixer
/// layer has none.
fn shifted_in_layer(name: &str, modifiers: &Modifiers) -> Shifted {
    if fx_control(&Input::Hid(name.to_owned())).is_none() {
        return shifted(name);
    }
    let (side, control) = name.split_once(".FX.")?;
    let deck = usize::from(side == "Right");
    let unit = match modifiers.0.get(LAYER).copied().unwrap_or(PRIMARY) {
        SECONDARY => deck + 2,
        MIXER => return Some(None),
        _ => deck,
    };
    let fx_slot = |slot| (Action::FxSelect { unit, slot }, Interaction::Relative);
    Some(Some(match control {
        "Button 1" => (Action::FxMode { unit }, Interaction::Relative),
        "Button 2" => fx_slot(0),
        "Button 3" => fx_slot(1),
        "Button 4" => fx_slot(2),
        _ => return shifted(name),
    }))
}

/// The FX buttons show the units of the current layer, or in the mixer
/// layer the cut bands and the cue of the side's channel.
fn layer_leds(feedback: &Feedback, decoder: &Decoder, modifiers: &Modifiers, leds: &mut Leds) {
    let layer = modifiers.0.get(LAYER).copied().unwrap_or(PRIMARY);
    for (side, name) in ["Left", "Right"].into_iter().enumerate() {
        let button = |b: usize| format!("{name}.FX.Button {b}");
        match layer {
            SECONDARY => {
                let unit = &feedback.fx[side + 2];
                leds.set(decoder, &button(1), colour::ORANGE, unit.on);
                for (b, &on) in unit.buttons.iter().enumerate() {
                    leds.set(decoder, &button(b + 2), colour::ORANGE, on);
                }
            }
            MIXER => {
                let kill = feedback.knobs.kill[side];
                for (b, &band) in KILL_BANDS.iter().enumerate() {
                    leds.set(decoder, &button(b + 1), colour::RED, kill[band]);
                }
                let cue = feedback.channels[side].cue;
                leds.set(decoder, &button(4), colour::WHITE, cue);
            }
            _ => {}
        }
    }
}

fn shifted(name: &str) -> Shifted {
    let (side, control) = name.split_once('.')?;
    let deck = match side {
        "Left" => 0,
        "Right" => 1,
        _ => return None,
    };
    Some(Some(match control {
        "Play" => (Action::BeatTap { deck }, Interaction::Trigger),
        "Sync" => (Action::Master { deck }, Interaction::Trigger),
        "Transport Mode.Reverse" => (Action::Slip { deck }, Interaction::Toggle),
        "Nudge.Slow" => (
            Action::BeatJump {
                deck,
                beats: -NUDGE_BEATS,
            },
            Interaction::Trigger,
        ),
        "Nudge.Fast" => (
            Action::BeatJump {
                deck,
                beats: NUDGE_BEATS,
            },
            Interaction::Trigger,
        ),
        "Loop.Turn" => (Action::LoopMove { deck }, Interaction::Relative),
        "Assign.Left" | "Assign.Right" => {
            let unit = if control == "Assign.Left" { 2 } else { 3 };
            (
                Action::FxAssign {
                    channel: deck,
                    unit,
                },
                Interaction::Toggle,
            )
        }
        _ => return super::common_shifted(name),
    }))
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
    stored_calibration: None,
    rings: false,
    screens: None,
    tempo_leds: None,
    wheel_leds: None,
    strip_leds: None,
    meters: None,
    meter_leds: None,
    deck_leds: Some(deck_leds),
    default_mappings,
    shifted,
    modifier_hooks: Some(ModifierHooks {
        shifted: shifted_in_layer,
        leds: layer_leds,
    }),
    upgrade: None,
    jog_button_toggles_touch: false,
    samples_button_stems: false,
    samples_button_toggles: false,
    fx_select_toggles: false,
    mx2_transport: false,
    shared_shift: true,
    connect_reports: &[APP_MODE],
    touch_strips: &[],
    displays: Some(displays),
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

    fn find(name: &str) -> Option<Action> {
        default_mappings()
            .into_iter()
            .find(|m| m.input == Input::Hid(name.into()))
            .map(|m| m.action)
    }

    #[test]
    fn layout_is_consistent() {
        crate::s4mk3::tests::check_layout(&LAYOUT);
        assert_eq!(LAYOUT.buttons.len(), 42, "every bit of the report");
        let leds: Vec<usize> = LAYOUT.buttons.iter().filter_map(|b| b.led).collect();
        for led in LOOP_LED.iter().chain(BOTTOM_LEDS.iter().flatten()) {
            assert!(*led < LED_LEN);
            assert!(!leds.contains(led), "LED {led} shared");
        }
    }

    #[test]
    fn buttons_follow_their_bit() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 25];
        r[0] = 0x01;
        assert!(d.decode(&r).iter().all(|e| e.pressed.is_none()));
        r[3] = 0x02;
        let events: Vec<_> = d
            .decode(&r)
            .into_iter()
            .filter(|e| e.pressed.is_some())
            .collect();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].input, Input::Hid("Left.Pads.1".into()));
        assert_eq!(d.led("Left.Pads.1"), Some(19));
        assert_eq!(d.led("Right.Assign.Right"), Some(35));
    }

    #[test]
    fn knobs_read_twelve_bits() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 25];
        r[0] = 0x01;
        r[0x15] = 0xFF;
        r[0x16] = 0x0F;
        let events = d.decode(&r);
        let v = events
            .iter()
            .find(|e| e.input == Input::Hid("Left.FX.Knob 1".into()))
            .expect("dry/wet");
        assert!((v.value - 1.0).abs() < 1e-6);
    }

    #[test]
    fn each_side_drives_its_deck_and_fx_unit() {
        assert_eq!(find("Right.Play"), Some(Action::PlayPause { deck: 1 }));
        assert_eq!(
            find("Left.Pads.3"),
            Some(Action::Hotcue { deck: 0, slot: 2 })
        );
        assert_eq!(find("Left.FX.Knob 1"), Some(Action::FxDryWet { unit: 0 }));
        assert_eq!(
            find("Right.FX.Button 4"),
            Some(Action::FxButton { unit: 1, button: 2 })
        );
        assert_eq!(
            find("Left.Nudge.Slow"),
            Some(Action::TempoBend {
                deck: 0,
                direction: -1.0
            })
        );
        assert_eq!(
            find("Right.Assign.Left"),
            Some(Action::FxAssign {
                channel: 1,
                unit: 0
            })
        );
        assert_eq!(
            find("Left.Loop.Turn"),
            Some(Action::LoopSizeStep { deck: 0 })
        );
        assert_eq!(find("Mode"), Some(Action::Modifier { index: LAYER }));
    }

    fn in_layer(name: &str, layer: u8) -> Vec<Action> {
        default_mappings()
            .into_iter()
            .filter(|m| m.input == Input::Hid(name.into()))
            .filter(|m| m.gate.conditions.iter().any(|c| c.value == layer))
            .map(|m| m.action)
            .collect()
    }

    #[test]
    fn mode_steps_through_the_fx_layers() {
        assert_eq!(
            in_layer("Left.FX.Knob 2", PRIMARY),
            [Action::FxKnob { unit: 0, knob: 0 }]
        );
        assert_eq!(
            in_layer("Right.FX.Knob 1", SECONDARY),
            [Action::FxDryWet { unit: 3 }]
        );
        assert_eq!(
            in_layer("Right.FX.Knob 1", MIXER),
            [Action::Eq {
                channel: 1,
                band: 2
            }]
        );
        assert_eq!(
            in_layer("Left.FX.Knob 4", MIXER),
            [Action::Volume { channel: 0 }]
        );
        assert_eq!(
            in_layer("Left.FX.Button 4", MIXER),
            [Action::MonitorCue { channel: 0 }]
        );
        assert_eq!(
            in_layer("Left.FX.Button 2", MIXER),
            [Action::EqKill {
                channel: 0,
                band: 1
            }]
        );
        assert_eq!(
            in_layer("Right.FX.Button 1", MIXER),
            [Action::EqKill {
                channel: 1,
                band: 2
            }]
        );
    }

    #[test]
    fn mode_cycles_its_layer_once_per_press() {
        let set = crate::MappingSet {
            mappings: default_mappings(),
            ..crate::MappingSet::default()
        };
        let mut source = crate::relative::Source::default();
        let mut shown = Feedback {
            fx_units: 4,
            ..Feedback::default()
        };
        let press = crate::InputEvent {
            input: Input::Hid("Mode".into()),
            value: 1.0,
            raw: 1,
            pressed: Some(true),
        };
        let release = crate::InputEvent {
            pressed: Some(false),
            ..press.clone()
        };
        for expected in [SECONDARY, MIXER, PRIMARY, SECONDARY] {
            set.map_source(&press, &mut source, &shown, &|_| None);
            set.map_source(&release, &mut source, &shown, &|_| None);
            assert_eq!(source.modifiers.0[LAYER], expected);
        }
        shown.fx_units = 2;
        for expected in [MIXER, PRIMARY, MIXER] {
            set.map_source(&press, &mut source, &shown, &|_| None);
            set.map_source(&release, &mut source, &shown, &|_| None);
            assert_eq!(source.modifiers.0[LAYER], expected, "two units skip 3/4");
        }
    }

    #[test]
    fn the_spare_bits_decode_and_map_to_nothing() {
        let mut d = Decoder::new(&LAYOUT);
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        d.decode(&r);
        let (byte, mask) = at(41);
        r[byte] |= mask;
        let events: Vec<_> = d
            .decode(&r)
            .into_iter()
            .filter(|e| e.pressed.is_some())
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].input, Input::Hid("Extra.4".into()));
        assert!(EXTRA.iter().all(|(name, _)| find(name).is_none()));
    }

    #[test]
    fn shift_on_fx_buttons_follows_the_layer() {
        let mut m = Modifiers::default();
        assert_eq!(
            shifted_in_layer("Right.FX.Button 3", &m),
            Some(Some((
                Action::FxSelect { unit: 1, slot: 1 },
                Interaction::Relative
            )))
        );
        m.0[LAYER] = SECONDARY;
        assert_eq!(
            shifted_in_layer("Right.FX.Button 1", &m),
            Some(Some((Action::FxMode { unit: 3 }, Interaction::Relative)))
        );
        m.0[LAYER] = MIXER;
        assert_eq!(shifted_in_layer("Left.FX.Button 2", &m), Some(None));
        assert_eq!(
            shifted_in_layer("Left.Play", &m),
            shifted("Left.Play"),
            "the rest keeps its SHIFT action"
        );
    }

    #[test]
    fn fx_buttons_light_the_current_layer() {
        let d = Decoder::new(&LAYOUT);
        let mut f = Feedback::default();
        f.fx[3].on = true;
        f.knobs.kill[0][1] = true;
        let led = |name: &str| d.led(name).expect(name);
        let mut m = Modifiers::default();
        m.0[LAYER] = SECONDARY;
        let mut leds = Leds::for_layout(&LAYOUT);
        layer_leds(&f, &d, &m, &mut leds);
        assert_ne!(
            leds.0[led("Right.FX.Button 1")],
            leds.0[led("Left.FX.Button 1")],
            "unit 4 on, unit 3 off"
        );
        m.0[LAYER] = MIXER;
        let mut leds = Leds::for_layout(&LAYOUT);
        layer_leds(&f, &d, &m, &mut leds);
        assert_ne!(
            leds.0[led("Left.FX.Button 2")],
            leds.0[led("Left.FX.Button 1")],
            "mid cut, high not"
        );
    }

    #[test]
    fn displays_redraw_all_five_screens() {
        let mut f = Feedback::default();
        f.decks[0].loaded = true;
        f.decks[0].loop_size = 3;
        f.decks[0].loop_active = true;
        f.fx[1].on = true;
        let reports = displays(&f, [0, 1], &Modifiers::default());
        assert_eq!(reports.len(), DISPLAYS * 4);
        let ids: Vec<u8> = reports.iter().map(|r| r[0]).collect();
        assert_eq!(&ids[..4], &[0xE0; 4]);
        assert_eq!(&ids[16..], &[0xE4; 4]);
        assert_eq!(loop_label(3), "1/4");
        assert_eq!(loop_label(9), "16");
        let mut mixer = Modifiers::default();
        mixer.0[LAYER] = MIXER;
        assert_ne!(
            displays(&f, [0, 1], &mixer)[..4],
            reports[..4],
            "left FX screen changes"
        );
        assert_eq!(
            displays(&f, [0, 1], &mixer)[12..],
            reports[12..],
            "loop screens stay"
        );
    }

    #[test]
    fn shift_reaches_the_second_layer() {
        assert_eq!(
            shifted("Right.Nudge.Fast"),
            Some(Some((
                Action::BeatJump {
                    deck: 1,
                    beats: NUDGE_BEATS
                },
                Interaction::Trigger
            )))
        );
        assert_eq!(
            shifted("Left.Pads.2"),
            Some(Some((
                Action::DeleteHotcue { deck: 0, slot: 1 },
                Interaction::Trigger
            )))
        );
        assert_eq!(
            shifted("Left.Loop.Turn"),
            Some(Some((Action::LoopMove { deck: 0 }, Interaction::Relative)))
        );
    }

    #[test]
    fn bottom_strip_shows_the_loop() {
        let mut f = Feedback::default();
        f.decks[1].loop_active = true;
        f.channels[0].fx_assign[1] = true;
        let d = Decoder::new(&LAYOUT);
        let mut leds = Leds::for_layout(&LAYOUT);
        deck_leds(&f, &d, [0, 1], &mut leds);
        assert!(
            BOTTOM_LEDS[1]
                .iter()
                .all(|&i| leds.0[i] == colour::GREEN + colour::BRIGHT)
        );
        assert!(BOTTOM_LEDS[0].iter().all(|&i| leds.0[i] == colour::OFF));
        assert_eq!(leds.0[LOOP_LED[1]], 0x7E);
        let assign = d.led("Left.Assign.Right").expect("led");
        assert_eq!(leds.0[assign], colour::ORANGE + colour::BRIGHT);
    }
}
