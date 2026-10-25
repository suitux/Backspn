use std::sync::mpsc::Sender;

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use crate::{Input, InputEvent, MidiKind};

const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub fn note_name(number: u8) -> String {
    format!(
        "{}{}",
        NAMES[usize::from(number % 12)],
        i32::from(number / 12) - 2
    )
}

pub fn note_number(name: &str) -> Option<u8> {
    let split = name.find(|c: char| c == '-' || c.is_ascii_digit())?;
    let (pitch, octave) = name.split_at(split);
    let pc = NAMES.iter().position(|n| n.eq_ignore_ascii_case(pitch))? as i32;
    let octave: i32 = octave.parse().ok()?;
    u8::try_from((octave + 2) * 12 + pc)
        .ok()
        .filter(|&n| n < 128)
}

pub fn relative_steps(raw: u8) -> f32 {
    if raw == 0 || raw == 64 {
        0.0
    } else if raw < 64 {
        f32::from(raw)
    } else {
        f32::from(raw) - 128.0
    }
}

pub fn parse(bytes: &[u8]) -> Option<InputEvent> {
    let status = *bytes.first()?;
    let channel = status & 0x0F;
    let d1 = *bytes.get(1)? & 0x7F;
    let d2 = bytes.get(2).map_or(0, |b| b & 0x7F);
    let (kind, number, value, raw, pressed) = match status & 0xF0 {
        0x90 if d2 > 0 => (MidiKind::Note, d1, f32::from(d2) / 127.0, d2, Some(true)),
        0x80 | 0x90 => (MidiKind::Note, d1, 0.0, 0, Some(false)),
        0xB0 => (MidiKind::Cc, d1, f32::from(d2) / 127.0, d2, None),
        0xE0 => {
            let v = (u16::from(d2) << 7) | u16::from(d1);
            (MidiKind::PitchBend, 0, f32::from(v) / 16_383.0, d2, None)
        }
        _ => return None,
    };
    Some(InputEvent {
        input: Input::Midi {
            channel,
            kind,
            number,
        },
        value,
        raw,
        pressed,
    })
}

pub fn parse_clock(bytes: &[u8]) -> Option<(u8, u16)> {
    let status = *bytes.first()?;
    match status {
        0xF2 => {
            let lsb = u16::from(*bytes.get(1)? & 0x7F);
            let msb = u16::from(*bytes.get(2)? & 0x7F);
            Some((status, (msb << 7) | lsb))
        }
        0xF8 | 0xFA | 0xFB | 0xFC => Some((status, 0)),
        _ => None,
    }
}

pub type ClockHandler = Box<dyn FnMut(u8, u16, u64) + Send>;

pub fn input_ports() -> Vec<String> {
    let Ok(midi) = MidiInput::new("Backspn") else {
        return Vec::new();
    };
    midi.ports()
        .iter()
        .filter_map(|p| midi.port_name(p).ok())
        .collect()
}

pub struct Port {
    pub name: String,
    _connection: MidiInputConnection<()>,
}

impl std::fmt::Debug for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Port")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Port {
    pub fn open(
        name: &str,
        events: Sender<(String, InputEvent)>,
        on_event: impl Fn() + Send + 'static,
        mut clock: Option<ClockHandler>,
    ) -> Result<Self, String> {
        let midi = MidiInput::new("Backspn").map_err(|e| e.to_string())?;
        let port = midi
            .ports()
            .into_iter()
            .find(|p| midi.port_name(p).is_ok_and(|n| n == name))
            .ok_or_else(|| format!("MIDI input {name:?} not found"))?;
        let port_name = name.to_owned();
        let connection = midi
            .connect(
                &port,
                "backspn-in",
                move |stamp, bytes, ()| {
                    if let Some((status, spp)) = parse_clock(bytes) {
                        if let Some(clock) = &mut clock {
                            clock(status, spp, stamp);
                        }
                    } else if let Some(event) = parse(bytes) {
                        drop(events.send((port_name.clone(), event)));
                        on_event();
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            name: name.to_owned(),
            _connection: connection,
        })
    }
}

pub fn output_ports() -> Vec<String> {
    let Ok(midi) = MidiOutput::new("Backspn") else {
        return Vec::new();
    };
    midi.ports()
        .iter()
        .filter_map(|p| midi.port_name(p).ok())
        .collect()
}

pub struct Output {
    pub name: String,
    connection: MidiOutputConnection,
}

impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Output {
    pub fn open(name: &str) -> Result<Self, String> {
        let midi = MidiOutput::new("Backspn").map_err(|e| e.to_string())?;
        let port = midi
            .ports()
            .into_iter()
            .find(|p| midi.port_name(p).is_ok_and(|n| n == name))
            .ok_or_else(|| format!("MIDI output {name:?} not found"))?;
        let connection = midi
            .connect(&port, "backspn-out")
            .map_err(|e| e.to_string())?;
        Ok(Self {
            name: name.to_owned(),
            connection,
        })
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.connection.send(bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_notes_ccs_and_bends() {
        let on = parse(&[0x91, 60, 127]).unwrap();
        assert_eq!(
            on.input,
            Input::Midi {
                channel: 1,
                kind: MidiKind::Note,
                number: 60
            }
        );
        assert_eq!(on.pressed, Some(true));
        assert_eq!(parse(&[0x91, 60, 0]).unwrap().pressed, Some(false));
        assert_eq!(parse(&[0x81, 60, 64]).unwrap().pressed, Some(false));
        let cc = parse(&[0xB0, 7, 64]).unwrap();
        assert!((cc.value - 64.0 / 127.0).abs() < 1e-6 && cc.pressed.is_none());
        let bend = parse(&[0xE2, 0x7F, 0x7F]).unwrap();
        assert!((bend.value - 1.0).abs() < 1e-6);
        assert!(parse(&[0xF8]).is_none());
    }

    #[test]
    fn clock_messages_and_the_song_position() {
        assert_eq!(parse_clock(&[0xF8]), Some((0xF8, 0)));
        assert_eq!(parse_clock(&[0xFA]), Some((0xFA, 0)));
        assert_eq!(parse_clock(&[0xF2, 0x10, 0x01]), Some((0xF2, 0x90)));
        assert_eq!(parse_clock(&[0xF2, 0x10]), None);
        assert_eq!(parse_clock(&[0x90, 60, 127]), None);
    }

    #[test]
    fn note_names_round_trip() {
        assert_eq!(note_name(60), "C3");
        assert_eq!(note_name(0), "C-2");
        assert_eq!(note_number("C#3"), Some(61));
        assert_eq!(note_number("C-2"), Some(0));
        for n in 0..128 {
            assert_eq!(note_number(&note_name(n)), Some(n));
        }
    }

    #[test]
    fn relative_encoder_steps() {
        assert_eq!(relative_steps(1), 1.0);
        assert_eq!(relative_steps(127), -1.0);
        assert_eq!(relative_steps(64), 0.0);
    }
}
