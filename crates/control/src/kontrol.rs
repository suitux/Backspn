use crate::s4mk3::{Analog, Button, Encoder, Jog, Layout, Shifted};
use crate::{Action, Input, Interaction, Mapping};

pub mod loop_display;
pub mod mx2;
pub mod s2mk2;
pub mod s2mk3;
pub mod s3;
pub mod s4mk2;
pub mod z1;

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

pub fn layouts() -> [&'static Layout; 6] {
    [
        &s2mk3::LAYOUT,
        &s3::LAYOUT,
        &mx2::LAYOUT,
        &z1::LAYOUT,
        &s4mk2::LAYOUT,
        &s2mk2::LAYOUT,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s4mk3::Decoder;

    #[test]
    fn layouts_are_consistent() {
        for l in layouts() {
            crate::s4mk3::tests::check_layout(l);
            assert!(
                l.has("Mixer.A.Volume") && l.has("Mixer.Xfader"),
                "{}",
                l.name
            );
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
            let name = if l.has("Left.Play") {
                "Left.Play"
            } else {
                "Mixer.A.Cue"
            };
            let play = l.buttons.iter().find(|b| b.name == name).expect(name);
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
            let volume = l
                .analogs
                .iter()
                .find(|a| a.name == "Mixer.A.Volume")
                .expect("volume");
            let mut r = vec![0u8; 80];
            r[0] = l.analog_report;
            r[volume.byte] = 0xFF;
            r[volume.byte + 1] = 0x0F;
            let events = d.decode(&r);
            let v = events
                .iter()
                .find(|e| e.input == Input::Hid("Mixer.A.Volume".into()))
                .expect("volume event");
            assert!((v.value - 1.0).abs() < 1e-6);
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
