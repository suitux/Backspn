use crate::s4mk3::{Analog, Button, Encoder, Jog, Layout, Shifted};
use crate::{Action, Input, Interaction, Mapping};

pub mod d2;
pub mod f1;
pub mod loop_display;
pub mod mono;
pub mod mx2;
pub mod s2mk2;
pub mod s2mk3;
pub mod s3;
pub mod s4mk2;
pub mod s5;
pub mod s8;
pub mod x1mk2;
pub mod x1mk3;
pub mod z1;
pub mod z2;

fn button(
    name: impl Into<String>,
    byte: usize,
    mask: u8,
    led: Option<usize>,
    single: Option<(u8, u8)>,
) -> Button {
    Button {
        name: name.into(),
        byte,
        bit: u8::try_from(mask.trailing_zeros()).unwrap_or(0),
        led,
        single,
        rgb: None,
    }
}

const fn analog(name: &'static str, byte: usize) -> Analog {
    Analog { name, byte }
}

const fn encoder(name: &'static str, byte: usize, high: bool) -> Encoder {
    Encoder {
        name,
        byte,
        shift: if high { 4 } else { 0 },
    }
}

const fn jog(name: &'static str, byte: usize, bits: u32) -> Jog {
    Jog { name, byte, bits }
}

fn default_mappings(layout: &Layout, extra: Vec<Mapping>) -> Vec<Mapping> {
    let mut v: Vec<Mapping> = crate::s4mk3::default_mappings()
        .into_iter()
        .filter(|m| layout.has_input(m))
        .collect();
    for (name, action) in [
        ("Mixer.Cue Mix", Action::CueMix),
        ("Mixer.Cue Volume", Action::CueVolume),
    ] {
        if layout.has(name) {
            v.push(map(name, action, Interaction::Direct));
        }
    }
    v.extend(extra);
    v
}

fn map(name: &str, action: Action, interaction: Interaction) -> Mapping {
    Mapping::new(Input::Hid(name.to_owned()), action, interaction)
}

fn side_extras(layout: &Layout) -> Vec<Mapping> {
    let mut v = Vec::new();
    for (deck, side) in [(0usize, "Left"), (1, "Right")] {
        let keylock = format!("{side}.Keylock");
        if layout.has(&keylock) {
            v.push(map(&keylock, Action::Keylock { deck }, Interaction::Toggle));
        }
        let quant = format!("{side}.Quant");
        if layout.has(&quant) {
            v.push(map(&quant, Action::Quantize, Interaction::Toggle));
        }
        let load = format!("{side}.Browse.Load");
        if layout.has(&load) {
            v.push(map(
                &load,
                Action::LoadSelected { deck },
                Interaction::Trigger,
            ));
        }
    }
    v
}

fn db(level: f32) -> f32 {
    20.0 * level.max(1e-9).log10()
}

fn db_meter(bytes: &mut [u8], first: usize, thresholds: &[f32], level: f32, on: u8) {
    let value = db(level);
    for (i, &at) in thresholds.iter().enumerate() {
        if let Some(b) = bytes.get_mut(first + i) {
            *b = if value >= at { on } else { 0 };
        }
    }
    if let Some(b) = bytes.get_mut(first + thresholds.len()) {
        *b = if level > 1.0 { on } else { 0 };
    }
}

fn linear_meter(bytes: &mut [u8], first: usize, segments: usize, level: f32, on: u8) {
    for i in 0..segments {
        let threshold = if i == 0 {
            f32::EPSILON
        } else {
            i as f32 / segments as f32
        };
        if let Some(b) = bytes.get_mut(first + i) {
            *b = if level >= threshold { on } else { 0 };
        }
    }
    if let Some(b) = bytes.get_mut(first + segments) {
        *b = if level > 1.0 { on } else { 0 };
    }
}

fn common_shifted(name: &str) -> Shifted {
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
        "Move.Turn" => (Action::BeatJump { deck, beats: 1.0 }, Interaction::Relative),
        "Jog.Turn" => (Action::JogSearch { deck }, Interaction::Relative),
        "Browse.Encoder.Turn" => (Action::BrowserTree, Interaction::Relative),
        "Browse.Add To Playlist" => (Action::PrepListJump, Interaction::Trigger),
        "Quant" => (Action::Snap, Interaction::Toggle),
        "Tempo" => return Some(None),
        _ => return None,
    }))
}

pub fn layouts() -> [&'static Layout; 13] {
    [
        &s2mk3::LAYOUT,
        &s3::LAYOUT,
        &mx2::LAYOUT,
        &z1::LAYOUT,
        &s4mk2::LAYOUT,
        &s2mk2::LAYOUT,
        &x1mk2::LAYOUT,
        &x1mk3::LAYOUT,
        &f1::LAYOUT,
        &z2::LAYOUT,
        &s8::LAYOUT,
        &s5::LAYOUT,
        &d2::LAYOUT,
    ]
}

/// What a native controller has that its settings page can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one flag per hardware feature"
)]
pub struct Features {
    pub jogs: bool,
    pub tempo_fader: bool,
    pub motors: bool,
    pub screens: bool,
    pub displays: bool,
    pub calibration: bool,
    pub stored_calibration: bool,
    pub touch_strip: bool,
    pub sound_card: bool,
}

impl Features {
    pub fn of(layout: &Layout) -> Self {
        Self {
            jogs: !layout.jogs.is_empty(),
            tempo_fader: layout.has("Left.Tempo") || layout.has("Right.Tempo"),
            motors: layout.motors,
            screens: layout.screens.is_some(),
            displays: layout.displays.is_some(),
            calibration: !layout.calibration.is_empty(),
            stored_calibration: layout.stored_calibration.is_some(),
            touch_strip: !layout.touch_strips.is_empty(),
            sound_card: layout.sound_card.is_some(),
        }
    }
}

/// The layout of a native controller by its model name, the S4 MK3 included.
pub fn layout(name: &str) -> Option<&'static Layout> {
    std::iter::once(&*crate::s4mk3::S4)
        .chain(layouts())
        .find(|l| l.name == name)
}

pub fn features(name: &str) -> Option<Features> {
    layout(name).map(Features::of)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s4mk3::Decoder;

    #[test]
    fn every_model_has_features_and_they_follow_the_hardware() {
        for m in crate::controller::models() {
            assert!(features(m.name).is_some(), "{}", m.name);
        }
        let s4 = features(crate::s4mk3::NAME).expect("S4");
        assert!(s4.jogs && s4.tempo_fader && s4.motors && s4.screens && s4.calibration);
        let x1 = features(x1mk2::NAME).expect("X1 MK2");
        assert!(x1.touch_strip && !x1.jogs && !x1.tempo_fader && !x1.motors);
        assert!(features(x1mk3::NAME).expect("X1 MK3").displays);
        assert!(features(z2::NAME).expect("Z2").sound_card);
        assert!(features(z1::NAME).expect("Z1").stored_calibration);
        assert_eq!(features("Unknown"), None);
    }

    #[test]
    fn layouts_are_consistent() {
        for l in layouts() {
            crate::s4mk3::tests::check_layout(l);
            if l.has("Mixer.A.Volume") {
                assert!(l.has("Mixer.Xfader"), "{}", l.name);
            }
            if !l.jogs.is_empty() {
                assert!(l.has("Left.Play") && l.has("Right.Play"), "{}", l.name);
                assert!(l.jogs.len() == 2 && l.encoders.len() >= 6, "{}", l.name);
            }
        }
    }

    #[test]
    fn reports_decode_to_named_controls() {
        for l in layouts() {
            let mut d = Decoder::new(l);
            let play = &l.buttons[0];
            let name = play.name.as_str();
            let mut r = vec![0u8; 64];
            r[0] = 0x01;
            assert!(d.decode(&r).iter().all(|e| e.pressed.is_none()));
            r[play.byte] |= 1 << play.bit;
            let events: Vec<_> = d
                .decode(&r)
                .into_iter()
                .filter(|e| e.pressed.is_some())
                .collect();
            assert_eq!(events.len(), 1, "{}: {events:?}", l.name);
            assert_eq!(events[0].input, Input::Hid(name.into()));
            assert_eq!(events[0].pressed, Some(true));
            let analog = &l.analogs[0];
            let mut r = vec![0u8; 80];
            r[0] = l.analog_report;
            r[analog.byte] = 0xFF;
            r[analog.byte + 1] = 0x0F;
            let events = d.decode(&r);
            let v = events
                .iter()
                .find(|e| e.input == Input::Hid(analog.name.into()))
                .expect("analog event");
            assert!((v.value - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn led_numbers_start_after_the_report_id() {
        for (layout, name, byte, len) in [
            (&*s2mk3::LAYOUT, "Left.Play", 0x0B, 0x3D),
            (&*s3::LAYOUT, "Left.Play", 0x10, 267),
            (&*mx2::LAYOUT, "Left.Play", 0x1D, 0x69),
            (&*z1::LAYOUT, "Mixer.Mode", 0x12, 0x15),
        ] {
            let d = Decoder::new(layout);
            let mut leds = crate::s4mk3::Leds::for_layout(layout);
            leds.set(&d, name, crate::s4mk3::colour::WHITE, true);
            let (_, bytes) = leds.reports(layout).next().expect("a report");
            assert_eq!(bytes.len(), len, "{}", layout.name);
            let lit: Vec<usize> = (0..bytes.len()).filter(|&i| bytes[i] != 0).collect();
            assert_eq!(lit, [byte], "{}", layout.name);
        }
    }

    #[test]
    fn meters_follow_their_thresholds() {
        let mut bytes = [0u8; 8];
        db_meter(
            &mut bytes,
            1,
            &[-30.0, -24.0, -18.0, -12.0, -6.0],
            0.1,
            0x7E,
        );
        assert_eq!(&bytes[1..7], &[0x7E, 0x7E, 0, 0, 0, 0]);
        db_meter(
            &mut bytes,
            1,
            &[-30.0, -24.0, -18.0, -12.0, -6.0],
            1.2,
            0x7E,
        );
        assert_eq!(&bytes[1..7], &[0x7E; 6], "clip lit over full scale");
        let mut bytes = [0u8; 16];
        linear_meter(&mut bytes, 0, 14, 0.5, 0x7F);
        assert_eq!(&bytes[..8], &[0x7F; 8]);
        assert!(bytes[8..15].iter().all(|&b| b == 0));
    }
}
