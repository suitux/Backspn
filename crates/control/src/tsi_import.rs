use backspn_formats::tsi::{Device, Mapping as TsiMapping};

use crate::modifier::{self, Condition, Gate};
use crate::output::{self, OutMapping};
use crate::relative::Encoder;
use crate::state::{self, StateCondition};
use crate::{Action, Input, Interaction, Mapping, MidiKind, MoveMode, MoveSize, midi};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Import {
    pub mappings: Vec<Mapping>,
    pub outputs: Vec<OutMapping>,
    pub skipped: usize,
    pub pair_ranges: Vec<crate::PairRange>,
    pub device: Option<crate::device::Device>,
}

fn device_entry(device: &Device) -> Option<crate::device::Device> {
    let kind = device.name.to_ascii_lowercase();
    if kind.contains("keyboard") || kind.contains("s4 mk3") {
        return None;
    }
    let mut d = crate::device::Device::midi(
        device.name.clone(),
        port(device, &device.in_port),
        port(device, &device.out_port),
    );
    d.comment.clone_from(&device.comment);
    d.target = usize::try_from(device.target)
        .ok()
        .filter(|&t| t > 0)
        .map(|t| t - 1);
    Some(d)
}

fn size_beats(index: i32) -> f64 {
    2f64.powi(index - 5)
}

fn load_action(command: u32, deck: usize) -> Option<Action> {
    let mode = match command {
        2176 => return Some(Action::LoadNext { deck }),
        2177 => return Some(Action::LoadPrevious { deck }),
        3076 => return Some(Action::LoadSelected { deck }),
        3079 => crate::LoadMode::Alternative,
        3086 => crate::LoadMode::Track,
        3087 => crate::LoadMode::Stems,
        _ => return None,
    };
    Some(Action::LoadSelectedAs { deck, mode })
}

fn sample_action(m: &TsiMapping) -> Option<Action> {
    if let Some(part) = crate::SampleSlotPart::from_command(m.command) {
        let (deck, slot) = crate::stem::slot_assignment(m.assignment)?;
        return Some(Action::SampleSlot { deck, slot, part });
    }
    let deck = usize::try_from(m.assignment).ok().filter(|&d| d < 4)?;
    if let Some(part) = crate::SamplePart::from_command(m.command) {
        return Some(Action::SampleDeck { deck, part });
    }
    let (slot, cell) = crate::sample_deck::cell_of(m.command)?;
    Some(Action::SampleCell { deck, slot, cell })
}

fn stem_action(m: &TsiMapping) -> Option<Action> {
    let (deck, stem) = crate::stem::slot_assignment(m.assignment)?;
    Some(Action::Stem {
        deck,
        stem,
        part: crate::StemPart::from_command(m.command)?,
    })
}

fn clock_action(command: u32) -> Option<Action> {
    Some(match command {
        2469 => Action::MasterTap,
        2470 => Action::MasterTick,
        2468 => Action::ClockSend,
        2473 => Action::ClockResync,
        62 => Action::ClockExternal,
        2808 => Action::ClockLink,
        2811 => Action::LinkDownbeat,
        _ => return None,
    })
}

fn focus_action(m: &TsiMapping) -> Option<Action> {
    Some(match m.command {
        9 if m.options.set_to_value => Action::DeckFocus {
            deck: usize::try_from(m.value).ok().filter(|&d| d < 4)?,
        },
        2588 => Action::FocusToggle,
        _ => return None,
    })
}

const JOG_TURNS_PER_STEP: f32 = 32.0 / 127.0 * 0.01 * 8192.0 * 0.0015 / 1.8;

#[allow(clippy::too_many_lines)]
fn action(m: &TsiMapping) -> Option<Action> {
    let deck = usize::try_from(m.assignment).ok().filter(|&d| d < 4);
    let value = m.value;
    Some(match m.command {
        100 => Action::PlayPause { deck: deck? },
        206 => Action::Cue { deck: deck? },
        204 => Action::Cup { deck: deck? },
        122 | 124 | 125 => sync_action(m.command, deck?)?,
        2293 => Action::Master { deck: deck? },
        400 | 405 => Action::Keylock { deck: deck? },
        402 => Action::KeyAdjust { deck: deck? },
        403 if value != 0 => Action::KeySemitone {
            deck: deck?,
            direction: if value >= 1 { 1.0 } else { -1.0 },
        },
        2328 => Action::Hotcue {
            deck: deck?,
            slot: usize::try_from(value).ok().filter(|&s| s < 8)?,
        },
        2331 => Action::DeleteHotcue {
            deck: deck?,
            slot: usize::try_from(value).ok().filter(|&s| s < 8)?,
        },
        2308 => Action::CueStore { deck: deck? },
        213 => Action::CueSetStore { deck: deck? },
        2309 => Action::CueDelete { deck: deck? },
        2306 if value != 0 => Action::CueJump {
            deck: deck?,
            direction: f64::from(value.signum()),
        },
        2310 => Action::CueMove { deck: deck? },
        2327 => Action::CueType {
            deck: deck?,
            kind: Some(value).filter(|v| (0..6).contains(v))?,
        },
        2315 => Action::MapHotcue {
            deck: deck?,
            slot: usize::try_from(value).ok().filter(|&s| s < 8)?,
        },
        62 | 2468..=2470 | 2473 | 2808 | 2811 => clock_action(m.command)?,
        2240 => Action::BeatTap { deck: deck? },
        2252 => Action::DeckTick { deck: deck? },
        2255 => Action::CopyPhase { deck: deck? },
        2400 => Action::DuplicateDeck {
            deck: deck?,
            from: usize::try_from(value).ok().filter(|&d| d < 4)?,
        },
        2401..=2404 => Action::DuplicateDeck {
            deck: deck?,
            from: (m.command - 2401) as usize,
        },
        127 | 10 | 105 | 2113 | 2114 | 2408 | 2409 => mixer_action(m.command, deck)?,
        2392 => Action::LoopIn { deck: deck? },
        2393 => Action::LoopOut { deck: deck? },
        202 => Action::LoopActive { deck: deck? },
        19 => Action::TempoRange { deck: deck? },
        2317 => Action::LoopSize {
            deck: deck?,
            beats: size_beats(value),
        },
        2351 if matches!(m.interaction, 5 | 6) => Action::Move {
            deck: deck?,
            direction: if m.interaction == 6 { -1.0 } else { 1.0 },
        },
        2391 | 2372 | 2351 | 2380 => move_action(m.command, deck?, value)?,
        123 => Action::Tempo { deck: deck? },
        406 => Action::TempoBend {
            deck: deck?,
            direction: f64::from(value.signum()),
        },
        102 => Action::Volume { channel: deck? },
        117 => Action::Gain { channel: deck? },
        301..=303 | 316 => Action::Eq {
            channel: deck?,
            band: match m.command {
                316 => 3,
                c => (c - 301) as usize,
            },
        },
        320 => Action::Filter { channel: deck? },
        119 => Action::MonitorCue { channel: deck? },
        5 => Action::Crossfader,
        6 => Action::MasterVolume,
        369 => Action::FxOn { unit: deck? },
        365 => Action::FxDryWet { unit: deck? },
        366..=368 => Action::FxKnob {
            unit: deck?,
            knob: (m.command - 366) as usize,
        },
        370..=372 => Action::FxButton {
            unit: deck?,
            button: (m.command - 370) as usize,
        },
        362..=364 => Action::FxSelect {
            unit: deck?,
            slot: (m.command - 362) as usize,
        },
        323 => Action::FxLfoReset,
        326 => Action::FxSnapshotStore { unit: deck? },
        335 if matches!(m.interaction, 4..=6) => Action::FxMode { unit: deck? },
        333 => Action::FxSnapshotLoad { unit: deck? },
        373 | 382..=398 | 408 => pattern_action(m.command, deck?)?,
        3200 => Action::BrowserUpDown,
        2176 | 2177 | 3076 | 3079 | 3086 | 3087 => load_action(m.command, deck?)?,
        232 | 239 | 249 | 250 | 251 | 259 => stem_action(m)?,
        229
        | 230
        | 233..=237
        | 241
        | 242
        | 244..=247
        | 255
        | 256
        | 258
        | 260
        | 265..=268
        | 601..=733
        | 2000
        | 2002..=2004 => sample_action(m)?,
        120 if m.interaction == 4 => Action::JogTurn { deck: deck? },
        2187 => Action::JogTouch { deck: deck? },
        2192 => Action::FastLoop { deck: deck? },
        2196 if matches!(m.interaction, 4..=6) => Action::LoopSizeStep { deck: deck? },
        2350 => Action::Slip { deck: deck? },
        319 => Action::MixerFxOn { channel: deck? },
        4209 => Action::BrowserView,
        3328 => Action::BrowserTree,
        3456 if matches!(m.interaction, 4..=6) => Action::BrowserFavoriteStep,
        3137 => Action::PreviewLoad,
        211 if matches!(m.interaction, 4..=6) => Action::PreviewSeek,
        8 => Action::CueVolume,
        17 => Action::CueMix,
        2302 => Action::DeckType { deck: deck? },
        2311 => Action::Snap,
        2056 => Action::RecordStop,
        29 => Action::RecordGain,
        3084 => Action::LoadLastRecording { deck: deck? },
        2313 => Action::Quantize,
        280..=289 => Action::LoopRecorder {
            part: crate::LoopRecorderPart::from_command(m.command)?,
        },
        c => modifier_action(c).or_else(|| focus_action(m))?,
    })
}

fn pattern_action(command: u32, unit: usize) -> Option<Action> {
    Some(match command {
        373 => Action::PatternKit { unit },
        382..=397 => Action::PatternStep {
            unit,
            step: (command - 382) as usize,
        },
        398 => Action::PatternReset { unit },
        408 => Action::PatternClear { unit },
        _ => return None,
    })
}

fn sync_action(command: u32, deck: usize) -> Option<Action> {
    Some(match command {
        125 => Action::Sync { deck },
        122 => Action::TempoSync { deck },
        124 => Action::PhaseSync { deck },
        _ => return None,
    })
}

fn mixer_action(command: u32, deck: Option<usize>) -> Option<Action> {
    Some(match command {
        127 => Action::Balance { channel: deck? },
        10 => Action::MasterBalance,
        105 => Action::Mute { channel: deck? },
        2113 => Action::AutoFade { direction: -1.0 },
        2114 => Action::AutoFade { direction: 1.0 },
        2408 | 2409 => Action::XfaderSide {
            channel: deck?,
            right: command == 2409,
        },
        _ => return None,
    })
}

fn move_action(command: u32, deck: usize, value: i32) -> Option<Action> {
    Some(match command {
        2391 => Action::MoveMode {
            deck,
            mode: *MoveMode::ALL.get(usize::try_from(value).ok()?)?,
        },
        2372 => Action::MoveSize {
            deck,
            size: MoveSize::new(u8::try_from(value).ok()?)?,
        },
        2351 if value != 0 => Action::Move {
            deck,
            direction: f64::from(value.signum()),
        },
        2380 if value != 0 => Action::BeatJump {
            deck,
            beats: f64::from(value.signum())
                * u8::try_from(value.unsigned_abs() - 1)
                    .ok()
                    .and_then(MoveSize::new)
                    .and_then(MoveSize::beats)?,
        },
        _ => return None,
    })
}

fn input(device: &Device, control: &str) -> Option<Input> {
    let name = device.name.to_ascii_lowercase();
    if name.contains("keyboard") {
        return (!control.is_empty()).then(|| Input::Key(control.to_owned()));
    }
    if name.contains("s4 mk3") {
        return (!control.is_empty()).then(|| Input::Hid(control.to_owned()));
    }
    if let Some((a, b)) = control.split_once('+') {
        return Some(Input::MidiPair {
            lsb: midi_input(a)?.midi_id()?,
            msb: midi_input(b)?.midi_id()?,
        });
    }
    midi_input(control)
}

fn midi_input(control: &str) -> Option<Input> {
    let mut parts = control.split('.');
    let channel: u8 = parts.next()?.strip_prefix("Ch")?.parse().ok()?;
    let channel = channel.checked_sub(1).filter(|&c| c < 16)?;
    let (kind, number) = match parts.next()? {
        "CC" => (MidiKind::Cc, parts.next()?.parse().ok()?),
        "Note" => (MidiKind::Note, midi::note_number(parts.next()?)?),
        "PitchBend" => (MidiKind::PitchBend, 0),
        _ => return None,
    };
    Some(Input::Midi {
        channel,
        kind,
        number,
    })
}

fn port(device: &Device, name: &str) -> Option<String> {
    let kind = device.name.to_ascii_lowercase();
    let midi = !kind.contains("keyboard") && !kind.contains("s4 mk3");
    (midi && !matches!(name, "" | "None" | "All Ports")).then(|| name.to_owned())
}

fn interaction(mode: u32, action: Action) -> (Interaction, bool) {
    if mode == 7 {
        return (Interaction::Reset, false);
    }
    if !action.is_continuous() {
        let i = match action.default_interaction() {
            Interaction::Hold => Interaction::Hold,
            Interaction::Toggle if mode == 1 => Interaction::Toggle,
            _ => Interaction::Trigger,
        };
        return (i, false);
    }
    match mode {
        4 | 5 => (Interaction::Relative, false),
        6 => (Interaction::Relative, true),
        _ => (Interaction::Direct, false),
    }
}

fn modifier_interaction(mode: u32) -> (Interaction, bool) {
    match mode {
        7 => (Interaction::Reset, false),
        0 => (Interaction::Trigger, false),
        1 => (Interaction::Toggle, false),
        3 => (Interaction::Direct, false),
        4 | 5 => (Interaction::Relative, false),
        6 => (Interaction::Relative, true),
        _ => (Interaction::Hold, false),
    }
}

fn gate(m: &TsiMapping, action: Action) -> Option<Gate> {
    let mut gate = Gate::default();
    for c in m.options.conditions.iter().flatten() {
        if MODIFIERS.contains(&c.control) {
            gate.conditions.push(Condition {
                modifier: (c.control - MODIFIERS.start()) as usize,
                value: u8::try_from(c.value).ok()?,
            });
        } else {
            gate.states.push(StateCondition {
                control: state::Control::from_id(c.control)?,
                channel: usize::try_from(c.channel).ok(),
                value: c.value,
            });
        }
    }
    gate.value = (matches!(action, Action::Modifier { .. }) && m.options.set_to_value)
        .then(|| u8::try_from(m.value.clamp(0, i32::from(modifier::MAX))).unwrap_or(0));
    Some(gate)
}

const MODIFIERS: std::ops::RangeInclusive<u32> = 2548..=2555;

fn modifier_action(command: u32) -> Option<Action> {
    MODIFIERS.contains(&command).then(|| Action::Modifier {
        index: (command - MODIFIERS.start()) as usize,
    })
}

fn out_mapping(device: &Device, m: &TsiMapping) -> Option<OutMapping> {
    let action = match m.command {
        2333..=2340 => Action::Hotcue {
            deck: usize::try_from(m.assignment).ok().filter(|&d| d < 4)?,
            slot: (m.command - 2333) as usize,
        },
        _ => action(m)?,
    };
    if !output::supported(action) {
        return None;
    }
    let o = &m.options;
    let range = o
        .range
        .0
        .as_f64()
        .zip(o.range.1.as_f64())
        .map(|(lo, hi)| (lo as f32, hi as f32));
    let midi = |v: i32| u8::try_from(v.clamp(0, 127)).unwrap_or(0);
    let gate = gate(m, action)?;
    Some(OutMapping {
        conditions: gate.conditions,
        states: gate.states,
        range,
        midi_range: (midi(o.midi_range.0), midi(o.midi_range.1)),
        blend: o.blend,
        invert: o.output_invert,
        comment: m.comment.clone(),
        ..OutMapping::new(input(device, &m.control)?, action)
    })
}

fn encoder(device: &Device, m: &TsiMapping) -> Encoder {
    if matches!(m.interaction, 5 | 6) && m.options.controller == 0 {
        return Encoder::Button;
    }
    if m.options.controller == 1 {
        return Encoder::Absolute;
    }
    match device.controls.iter().find(|c| c.name == m.control) {
        Some(c) if c.encoder == 0 => Encoder::Offset,
        _ => Encoder::TwosComplement,
    }
}

fn resolve_target(device: &Device, m: &TsiMapping) -> (TsiMapping, bool) {
    let mut m = m.clone();
    m.command = current_command(m.command);
    if device.target != 0 {
        for c in m.options.conditions.iter_mut().flatten() {
            if c.channel == -1 {
                c.channel = i32::try_from(device.target - 1).unwrap_or(0);
            }
        }
    }
    if m.assignment != u32::MAX {
        return (m, false);
    }
    m.assignment = device.target.saturating_sub(1);
    (m, device.target == 0)
}

pub fn current_command(command: u32) -> u32 {
    match command {
        225 => 120,
        113 => 121,
        329 | 330 => 362,
        331 => 363,
        332 => 364,
        340 | 352 => 368,
        341 | 351 => 367,
        342 | 350 => 366,
        343 | 360 => 365,
        344 | 355 => 372,
        345 | 354 => 371,
        346 | 353 => 370,
        347 => 369,
        2301 => 335,
        257 => 2004,
        2189 => 2192,
        2379 => 2380,
        c => c,
    }
}

pub fn import(device: &Device) -> Import {
    let mut out = Import {
        device: device_entry(device),
        ..Import::default()
    };
    for c in &device.controls {
        if let Some(Input::MidiPair { lsb, msb }) = input(device, &c.name) {
            out.pair_ranges.push(crate::PairRange {
                lsb,
                msb,
                min: c.min,
                max: c.max,
            });
        }
    }
    for m in &device.mappings {
        let (m, focus) = resolve_target(device, m);
        let m = &m;
        if m.output {
            match out_mapping(device, m) {
                Some(mut o) => {
                    o.focus = focus && o.action.has_deck();
                    o.port = port(device, &device.out_port);
                    out.outputs.push(o);
                }
                None => out.skipped += 1,
            }
            continue;
        }
        let converted = action(m)
            .zip(input(device, &m.control))
            .and_then(|(action, input)| Some((action, input, gate(m, action)?)));
        match converted {
            Some((action, input, gate)) => {
                let (interaction, invert) = match action {
                    Action::Modifier { .. } => modifier_interaction(m.interaction),
                    _ => interaction(m.interaction, action),
                };
                let mut mapping = Mapping::new(input, action, interaction);
                mapping.gate = gate;
                mapping.comment.clone_from(&m.comment);
                if matches!(m.interaction, 5 | 6) {
                    mapping.invert = invert;
                    mapping.on_release = m.options.invert;
                } else {
                    mapping.invert = m.options.invert;
                }
                if interaction == Interaction::Relative {
                    mapping.encoder = encoder(device, m);
                    mapping.sensitivity = m.options.sensitivity;
                    mapping.acceleration = m.options.acceleration;
                    if matches!(action, Action::JogTurn { .. }) {
                        mapping.scale = JOG_TURNS_PER_STEP;
                    }
                }
                mapping.auto_repeat = matches!(m.interaction, 5 | 6) && m.options.auto_repeat;
                mapping.soft_takeover =
                    interaction == Interaction::Direct && m.options.soft_takeover;
                mapping.focus = focus && action.has_deck();
                mapping.port = port(device, &device.in_port);
                out.mappings.push(mapping);
            }
            None => out.skipped += 1,
        }
    }
    out
}

fn used_modifiers(import: &Import) -> std::collections::BTreeSet<usize> {
    let mappings = import.mappings.iter().flat_map(|m| {
        let set = match m.action {
            Action::Modifier { index } => Some(index),
            _ => None,
        };
        m.gate.conditions.iter().map(|c| c.modifier).chain(set)
    });
    let outputs = import
        .outputs
        .iter()
        .flat_map(|o| o.conditions.iter().map(|c| c.modifier));
    mappings.chain(outputs).collect()
}

fn renumber_modifiers(import: &mut Import, to: &std::collections::HashMap<usize, usize>) {
    let new = |m: usize| to.get(&m).copied().unwrap_or(m);
    for m in &mut import.mappings {
        if let Action::Modifier { index } = &mut m.action {
            *index = new(*index);
        }
        for c in &mut m.gate.conditions {
            c.modifier = new(c.modifier);
        }
    }
    for o in &mut import.outputs {
        for c in &mut o.conditions {
            c.modifier = new(c.modifier);
        }
    }
}

#[must_use]
pub fn separate_modifiers(imports: &mut [Import]) -> bool {
    use std::collections::{BTreeSet, HashMap};
    let mut taken: HashMap<Option<String>, BTreeSet<usize>> = HashMap::new();
    let mut fits = true;
    for import in imports.iter_mut() {
        let Some(device) = &import.device else {
            continue;
        };
        let used = used_modifiers(import);
        let taken = taken.entry(device.in_port.clone()).or_default();
        let mut free = (0..modifier::COUNT).filter(|m| !taken.contains(m) && !used.contains(m));
        let mut to = HashMap::new();
        for &m in used.iter().filter(|m| taken.contains(m)) {
            match free.next() {
                Some(f) => {
                    to.insert(m, f);
                }
                None => fits = false,
            }
        }
        renumber_modifiers(import, &to);
        taken.extend(used.iter().map(|m| to.get(m).copied().unwrap_or(*m)));
    }
    fits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_focus_controls_import() {
        let mut focus = m("Ch01.Note.C3", 9, 0, 0, 2);
        focus.options.set_to_value = true;
        assert_eq!(action(&focus), Some(Action::DeckFocus { deck: 2 }));
        focus.options.set_to_value = false;
        assert_eq!(action(&focus), None, "no deck to focus");
        let toggle = m("Ch01.Note.C3", 2588, 0, 0, 0);
        assert_eq!(action(&toggle), Some(Action::FocusToggle));
    }

    #[test]
    fn the_factory_mappings_jog_loop_and_headphone_controls_import() {
        assert_eq!(
            action(&m("", 120, 4, 0, 0)),
            Some(Action::JogTurn { deck: 0 })
        );
        let jump = |mode| match action(&m("", 2351, mode, 0, 0)) {
            Some(Action::Move { direction, .. }) => Some(direction),
            _ => None,
        };
        assert_eq!((jump(5), jump(6), jump(4)), (Some(1.0), Some(-1.0), None));
        let mut jog = m("Ch01.CC.006", 120, 4, 0, 0);
        jog.options.sensitivity = 100.0;
        let r = import(&device("Generic MIDI", vec![jog]));
        assert!(
            (r.mappings[0].scale * 1.8 - 0.031).abs() < 0.001,
            "{}",
            r.mappings[0].scale
        );
        assert_eq!(
            action(&m("", 2187, 2, 1, 0)),
            Some(Action::JogTouch { deck: 1 })
        );
        assert_eq!(
            action(&m("", 2192, 2, 0, 0)),
            Some(Action::FastLoop { deck: 0 })
        );
        assert_eq!(
            action(&m("", 2196, 5, 0, 7)),
            Some(Action::LoopSizeStep { deck: 0 })
        );
        assert_eq!(
            action(&m("", 2196, 3, 0, 3)),
            None,
            "selecting a size alone"
        );
        assert_eq!(
            action(&m("", 2350, 1, 1, 0)),
            Some(Action::Slip { deck: 1 })
        );
        assert_eq!(
            action(&m("", 319, 1, 1, 0)),
            Some(Action::MixerFxOn { channel: 1 })
        );
        assert_eq!(action(&m("", 4209, 1, 0, 0)), Some(Action::BrowserView));
        assert_eq!(action(&m("", 3328, 4, 0, 0)), Some(Action::BrowserTree));
        assert_eq!(
            action(&m("", 3456, 4, 0, 0)),
            Some(Action::BrowserFavoriteStep)
        );
        assert_eq!(action(&m("", 3137, 0, 0, 0)), Some(Action::PreviewLoad));
        assert_eq!(action(&m("", 211, 4, 0, 0)), Some(Action::PreviewSeek));
        assert_eq!(action(&m("", 211, 3, 0, 0)), None, "a position: not ours");
        assert_eq!(
            action(&m("", 335, 5, 1, 0)),
            Some(Action::FxMode { unit: 1 })
        );
        assert_eq!(action(&m("", 8, 3, 0, 0)), Some(Action::CueVolume));
        assert_eq!(action(&m("", 17, 3, 0, 0)), Some(Action::CueMix));
    }

    #[test]
    fn the_master_clock_controls_import() {
        assert_eq!(clock_action(2468), Some(Action::ClockSend));
        assert_eq!(clock_action(2473), Some(Action::ClockResync));
        assert_eq!(clock_action(2469), Some(Action::MasterTap));
        assert_eq!(clock_action(2470), Some(Action::MasterTick));
        assert_eq!(clock_action(62), Some(Action::ClockExternal));
        assert_eq!(clock_action(2471), None);
    }

    fn device(name: &str, mappings: Vec<TsiMapping>) -> Device {
        Device {
            name: name.into(),
            mappings,
            ..Device::default()
        }
    }

    fn m(control: &str, command: u32, mode: u32, assignment: u32, value: i32) -> TsiMapping {
        TsiMapping {
            control: control.into(),
            command,
            interaction: mode,
            assignment,
            value,
            ..TsiMapping::default()
        }
    }

    #[test]
    fn old_ids_import_as_the_commands_they_became() {
        let old = |command, mode| {
            let (m, _) = resolve_target(
                &device("Generic MIDI", Vec::new()),
                &m("Ch01.CC.001", command, mode, 0, 0),
            );
            action(&m)
        };
        assert_eq!(old(343, 3), Some(Action::FxDryWet { unit: 0 }));
        assert_eq!(old(340, 3), Some(Action::FxKnob { unit: 0, knob: 2 }));
        assert_eq!(old(350, 3), Some(Action::FxKnob { unit: 0, knob: 0 }));
        assert_eq!(old(347, 1), Some(Action::FxOn { unit: 0 }));
        assert_eq!(old(346, 1), Some(Action::FxButton { unit: 0, button: 0 }));
        assert_eq!(old(331, 5), Some(Action::FxSelect { unit: 0, slot: 1 }));
        assert_eq!(old(2301, 5), Some(Action::FxMode { unit: 0 }));
        assert_eq!(old(2189, 0), Some(Action::FastLoop { deck: 0 }));
        assert_eq!(current_command(100), 100);
    }

    #[test]
    fn devices_on_one_port_keep_their_modifiers_apart() {
        let on_port = |port: &str, set: usize, checked: usize| {
            let shift = Mapping::new(
                Input::Midi {
                    channel: 0,
                    kind: MidiKind::Note,
                    number: 1,
                },
                Action::Modifier { index: set },
                Interaction::Hold,
            );
            let mut play = Mapping::new(
                Input::Midi {
                    channel: 0,
                    kind: MidiKind::Note,
                    number: 2,
                },
                Action::PlayPause { deck: 0 },
                Interaction::Trigger,
            );
            play.gate.conditions.push(Condition {
                modifier: checked,
                value: 7,
            });
            let mut led = OutMapping::new(play.input.clone(), Action::PlayPause { deck: 0 });
            led.conditions = play.gate.conditions.clone();
            Import {
                mappings: vec![shift, play],
                outputs: vec![led],
                device: Some(crate::device::Device::midi(
                    "Generic MIDI",
                    Some(port.into()),
                    None,
                )),
                ..Import::default()
            }
        };
        let mut imports = [
            on_port("Ready", 1, 1),
            on_port("Ready", 1, 0),
            on_port("Other", 1, 1),
        ];
        assert!(separate_modifiers(&mut imports));
        let modifiers = |i: &Import| {
            let Action::Modifier { index } = i.mappings[0].action else {
                unreachable!()
            };
            (
                index,
                i.mappings[1].gate.conditions[0].modifier,
                i.outputs[0].conditions[0].modifier,
            )
        };
        assert_eq!(
            modifiers(&imports[0]),
            (1, 1, 1),
            "the first keeps its numbers"
        );
        assert_eq!(
            modifiers(&imports[1]),
            (2, 0, 0),
            "M2 taken: the first free one"
        );
        assert_eq!(modifiers(&imports[2]), (1, 1, 1), "another port");
    }

    #[test]
    fn midi_devices_tie_their_mappings_to_their_ports() {
        let mut d = device("Generic MIDI", vec![m("Ch01.Note.C3", 100, 0, 1, 0)]);
        d.in_port = "DDJ-FLX4".into();
        assert_eq!(import(&d).mappings[0].port.as_deref(), Some("DDJ-FLX4"));
        d.in_port = "All Ports".into();
        assert_eq!(import(&d).mappings[0].port, None);
        let mut k = device("Generic Keyboard", vec![m("Q", 100, 0, 1, 0)]);
        k.in_port = "Keyboard".into();
        assert_eq!(import(&k).mappings[0].port, None);
    }

    #[test]
    fn device_target_mappings_follow_the_focus_or_a_deck() {
        let target = u32::MAX;
        let mut d = device(
            "Generic MIDI",
            vec![
                m("Ch01.Note.C3", 100, 0, target, 0),
                m("Ch01.Note.D3", 100, 0, 1, 0),
                m("Ch01.Note.E3", 2313, 1, target, 0),
            ],
        );
        let r = import(&d);
        assert_eq!(r.mappings[0].action, Action::PlayPause { deck: 0 });
        assert!(r.mappings[0].focus, "Focus");
        assert!(!r.mappings[1].focus, "its own deck");
        assert!(!r.mappings[2].focus, "Quantize has no deck");
        d.target = 3;
        let r = import(&d);
        assert_eq!(r.mappings[0].action, Action::PlayPause { deck: 2 });
        assert!(!r.mappings[0].focus, "deck C");
    }

    #[test]
    fn conditions_on_controls_import_with_their_channel() {
        use backspn_formats::tsi::Condition as TsiCondition;
        let mut pad = m("Ch01.Note.C3", 2328, 0, u32::MAX, 0);
        pad.options.conditions = [
            Some(TsiCondition {
                control: 2333,
                channel: -1,
                value: 5,
            }),
            Some(TsiCondition {
                control: 2301,
                channel: 1,
                value: 2,
            }),
        ];
        let mut out = pad.clone();
        out.output = true;
        let mut d = device("Generic MIDI", vec![pad, out]);
        let r = import(&d);
        let states = &r.mappings[0].gate.states;
        assert_eq!(
            states[0],
            StateCondition {
                control: state::Control::HotcueType { slot: 0 },
                channel: None,
                value: 5,
            }
        );
        assert_eq!(
            (states[1].control, states[1].channel),
            (state::Control::FxMode, Some(1))
        );
        assert_eq!(r.outputs[0].states.len(), 2, "outputs too");
        d.target = 2;
        let r = import(&d);
        assert_eq!(r.mappings[0].gate.states[0].channel, Some(1), "deck B");
    }

    #[test]
    fn a_shift_layer_from_modifiers() {
        use backspn_formats::tsi::{Condition as TsiCondition, MappingOptions};
        let options = |set_to_value, conditions| MappingOptions {
            set_to_value,
            conditions,
            ..MappingOptions::default()
        };
        let when_m1 = |value| {
            [
                Some(TsiCondition {
                    control: 2548,
                    channel: -1,
                    value,
                }),
                None,
            ]
        };
        let mut shift = m("Ch01.Note.C1", 2548, 2, 0, 1);
        shift.options = options(true, [None, None]);
        let mut play = m("Ch01.Note.C3", 100, 1, 0, 0);
        play.options = options(false, when_m1(0));
        let mut cue = m("Ch01.Note.C3", 206, 2, 0, 0);
        cue.options = options(false, when_m1(1));
        let mut other = m("Ch01.Note.D3", 100, 1, 0, 0);
        other.options = options(
            false,
            [
                Some(TsiCondition {
                    control: 736,
                    channel: 0,
                    value: 1,
                }),
                None,
            ],
        );
        let r = import(&device("Generic MIDI", vec![shift, play, cue, other]));
        assert_eq!(r.skipped, 1);
        let set = crate::MappingSet {
            mappings: r.mappings,
            midi_inputs: Vec::new(),
            ..crate::MappingSet::default()
        };
        let note = |number, down| crate::InputEvent {
            input: Input::Midi {
                channel: 0,
                kind: MidiKind::Note,
                number,
            },
            value: if down { 1.0 } else { 0.0 },
            raw: 0,
            pressed: Some(down),
        };
        let mut mods = crate::relative::Source::default();
        assert_eq!(
            set.map_source(
                &note(60, true),
                &mut mods,
                &crate::feedback::Feedback::default(),
                &|_| None
            ),
            [(Action::PlayPause { deck: 0 }, crate::Trigger::Press)]
        );
        assert_eq!(
            set.map_source(
                &note(36, true),
                &mut mods,
                &crate::feedback::Feedback::default(),
                &|_| None
            ),
            Vec::new()
        );
        assert_eq!(mods.modifiers.0[0], 1);
        assert_eq!(
            set.map_source(
                &note(60, true),
                &mut mods,
                &crate::feedback::Feedback::default(),
                &|_| None
            ),
            [(Action::Cue { deck: 0 }, crate::Trigger::Press)]
        );
        set.map_source(
            &note(36, false),
            &mut mods,
            &crate::feedback::Feedback::default(),
            &|_| None,
        );
        assert_eq!(mods.modifiers.0[0], 0);
    }

    #[test]
    fn relative_options_import() {
        use backspn_formats::tsi::{ControlDef, MappingOptions};
        let options = |controller, invert| MappingOptions {
            controller,
            invert,
            sensitivity: 50.0,
            acceleration: 0.5,
            midi_range: (0, 127),
            ..MappingOptions::default()
        };
        let mut encoder = m("Ch01.CC.010", 3200, 4, 0, 0);
        encoder.options = options(2, true);
        let mut knob = m("Ch01.CC.011", 365, 4, 0, 0);
        knob.options = options(1, false);
        let mut button = m("Ch01.Note.C3", 100, 0, 0, 0);
        button.options = options(0, true);
        let mut fader = m("Ch01.CC.012", 102, 3, 0, 0);
        fader.options = MappingOptions {
            soft_takeover: true,
            ..options(1, false)
        };
        let mut inc = m("Ch01.Note.D3", 3200, 5, 0, 0);
        inc.options = MappingOptions {
            auto_repeat: true,
            ..options(0, false)
        };
        let mut dec = m("Ch01.CC.013", 3200, 6, 0, 0);
        dec.options = options(0, true);
        let mut d = device("Generic MIDI", vec![encoder, knob, button, fader, inc, dec]);
        d.controls = vec![ControlDef {
            name: "Ch01.CC.010".into(),
            encoder: 0,
            ..ControlDef::default()
        }];
        let r = import(&d);
        let [enc, knob, button, fader, inc, dec] = &r.mappings[..] else {
            panic!("{:?}", r.mappings);
        };
        assert_eq!(enc.encoder, Encoder::Offset);
        assert!(enc.invert);
        assert_eq!((enc.sensitivity, enc.acceleration), (50.0, 0.5));
        assert_eq!(knob.encoder, Encoder::Absolute);
        assert!(fader.soft_takeover && !knob.soft_takeover);
        assert!(inc.auto_repeat && !enc.auto_repeat);
        assert_eq!(
            (dec.encoder, dec.invert, dec.on_release),
            (Encoder::Button, true, true)
        );
        assert!(!enc.on_release && !inc.on_release);
        assert_eq!(
            interaction(7, Action::Volume { channel: 0 }).0,
            Interaction::Reset
        );
        assert_eq!(modifier_interaction(7).0, Interaction::Reset);
        assert!(button.invert && button.interaction == Interaction::Trigger);
    }

    #[test]
    fn led_outputs_import() {
        use backspn_formats::tsi::{MappingOptions, Typed};
        let out = |control, command, assignment, value, options| TsiMapping {
            output: true,
            options,
            ..m(control, command, 8, assignment, value)
        };
        let plain = MappingOptions {
            midi_range: (0, 127),
            ..MappingOptions::default()
        };
        let volume = MappingOptions {
            range: (Typed::Float(0.0), Typed::Float(1.0)),
            midi_range: (10, 200),
            blend: true,
            output_invert: true,
            ..plain
        };
        let r = import(&device(
            "Generic MIDI",
            vec![
                out("Ch02.Note.C0", 100, 1, 0, plain),
                out("Ch01.Note.E1", 2335, 0, -1, plain),
                out("Ch01.CC.007", 102, 0, 0, volume),
                out("Ch01.Note.D1", 206, 0, 0, plain),
            ],
        ));
        assert_eq!((r.skipped, r.mappings.len()), (1, 0));
        assert_eq!(r.outputs[0].action, Action::PlayPause { deck: 1 });
        assert_eq!(
            (r.outputs[0].range, r.outputs[0].midi_range),
            (None, (0, 127))
        );
        assert_eq!(r.outputs[1].action, Action::Hotcue { deck: 0, slot: 2 });
        let v = &r.outputs[2];
        assert_eq!(
            (v.range, v.midi_range, v.blend, v.invert),
            (Some((0.0, 1.0)), (10, 127), true, true)
        );
    }

    #[test]
    fn move_panel_mappings() {
        let d = device(
            "Generic Keyboard",
            vec![
                m("1", 2391, 0, 0, 3),
                m("2", 2372, 0, 1, 1),
                m("3", 2351, 2, 0, -1),
                m("4", 2391, 0, 0, 4),
            ],
        );
        let r = import(&d);
        assert_eq!(r.skipped, 1, "no mode 4");
        let actions: Vec<Action> = r.mappings.iter().map(|m| m.action).collect();
        assert_eq!(
            actions,
            [
                Action::MoveMode {
                    deck: 0,
                    mode: MoveMode::LoopOut
                },
                Action::MoveSize {
                    deck: 1,
                    size: MoveSize::FINE
                },
                Action::Move {
                    deck: 0,
                    direction: -1.0
                },
            ]
        );
    }

    #[test]
    fn keyboard_mappings() {
        let d = device(
            "Generic Keyboard",
            vec![
                m("W", 100, 1, 0, 0),
                m("A", 206, 2, 1, 0),
                m("7", 2328, 2, 1, 1),
                m("Shift+3", 2317, 3, 0, 5),
                m("E", 2380, 2, 0, -9),
                m("Shift+R", 2380, 2, 0, 7),
                m("Backspace", 99_999, 0, 0, 0),
            ],
        );
        let r = import(&d);
        assert_eq!(r.skipped, 1, "an unknown command is skipped");
        let actions: Vec<Action> = r.mappings.iter().map(|m| m.action).collect();
        assert_eq!(
            actions,
            [
                Action::PlayPause { deck: 0 },
                Action::Cue { deck: 1 },
                Action::Hotcue { deck: 1, slot: 1 },
                Action::LoopSize {
                    deck: 0,
                    beats: 1.0
                },
                Action::BeatJump {
                    deck: 0,
                    beats: -4.0
                },
                Action::BeatJump {
                    deck: 0,
                    beats: 1.0
                },
            ]
        );
        assert_eq!(r.mappings[1].interaction, Interaction::Hold);
        assert_eq!(r.mappings[0].input, Input::Key("W".into()));
    }

    #[test]
    fn tempo_sync_and_phase_sync() {
        let r = import(&device(
            "Generic Keyboard",
            vec![m("T", 122, 0, 1, 0), m("P", 124, 0, 2, 0)],
        ));
        assert_eq!(r.mappings[0].action, Action::TempoSync { deck: 1 });
        assert_eq!(r.mappings[1].action, Action::PhaseSync { deck: 2 });
    }

    #[test]
    fn master_tempo_tap() {
        let r = import(&device(
            "Generic Keyboard",
            vec![
                m("T", 2469, 0, 0, 0),
                m("Y", 2240, 0, 2, 0),
                m("U", 2470, 1, 0, 0),
                m("I", 2252, 1, 1, 0),
            ],
        ));
        assert_eq!(r.mappings[0].action, Action::MasterTap);
        assert_eq!(r.mappings[1].action, Action::BeatTap { deck: 2 });
        assert_eq!(r.mappings[2].action, Action::MasterTick);
        assert_eq!(r.mappings[3].action, Action::DeckTick { deck: 1 });
    }

    #[test]
    fn load_variants_and_stem_controls() {
        let r = import(&device(
            "Generic MIDI",
            vec![
                m("Ch01.Note.C1", 3087, 0, 1, 0),
                m("Ch01.Note.D1", 3079, 0, 0, 0),
                m("Ch01.Note.G#0", 239, 1, 13, 1),
                m("Ch01.CC.010", 251, 3, 8, 0),
            ],
        ));
        let actions: Vec<Action> = r.mappings.iter().map(|m| m.action).collect();
        assert_eq!(
            actions,
            [
                Action::LoadSelectedAs {
                    deck: 1,
                    mode: crate::LoadMode::Stems
                },
                Action::LoadSelectedAs {
                    deck: 0,
                    mode: crate::LoadMode::Alternative
                },
                Action::Stem {
                    deck: 1,
                    stem: 1,
                    part: crate::StemPart::FxOn
                },
                Action::Stem {
                    deck: 0,
                    stem: 0,
                    part: crate::StemPart::Volume
                },
            ]
        );
    }

    #[test]
    fn load_next_and_previous() {
        let r = import(&device(
            "Generic Keyboard",
            vec![m("N", 2176, 0, 1, 0), m("P", 2177, 0, 2, 0)],
        ));
        assert_eq!(r.mappings[0].action, Action::LoadNext { deck: 1 });
        assert_eq!(r.mappings[1].action, Action::LoadPrevious { deck: 2 });
    }

    #[test]
    fn cue_point_commands() {
        let d = device(
            "Generic Keyboard",
            vec![
                m("A", 2308, 0, 1, 0),
                m("B", 213, 0, 1, 0),
                m("C", 2309, 0, 1, 0),
                m("D", 2306, 0, 1, -1),
                m("E", 2306, 0, 1, 0),
                m("F", 2310, 4, 1, 0),
                m("G", 2327, 0, 1, 5),
                m("H", 2327, 0, 1, 7),
                m("I", 2315, 0, 1, 3),
            ],
        );
        let r = import(&d);
        assert_eq!(r.skipped, 2, "no direction, unknown type");
        let actions: Vec<Action> = r.mappings.iter().map(|m| m.action).collect();
        assert_eq!(
            actions,
            [
                Action::CueStore { deck: 1 },
                Action::CueSetStore { deck: 1 },
                Action::CueDelete { deck: 1 },
                Action::CueJump {
                    deck: 1,
                    direction: -1.0
                },
                Action::CueMove { deck: 1 },
                Action::CueType { deck: 1, kind: 5 },
                Action::MapHotcue { deck: 1, slot: 3 },
            ]
        );
        assert_eq!(r.mappings[4].interaction, Interaction::Relative);
    }

    #[test]
    fn s4_mk3_mappings_become_hid_inputs() {
        let d = device(
            backspn_formats::interop::TSI_S4_MK3,
            vec![
                m("Left.Play", 100, 1, 2, 0),
                m("Mixer.B.Volume", 102, 3, 1, 0),
            ],
        );
        let r = import(&d);
        assert_eq!(r.skipped, 0);
        assert_eq!(r.mappings[0].input, Input::Hid("Left.Play".into()));
        assert_eq!(r.mappings[0].action, Action::PlayPause { deck: 2 });
        assert_eq!(
            (r.mappings[1].input.clone(), r.mappings[1].interaction),
            (Input::Hid("Mixer.B.Volume".into()), Interaction::Direct)
        );
    }

    #[test]
    fn a_plus_b_control_is_a_14_bit_pair_with_b_the_high_half() {
        let mut d = device(
            "Generic MIDI",
            vec![m("Ch01.CC.039+Ch01.CC.007", 102, 3, 1, 0)],
        );
        d.controls = vec![backspn_formats::tsi::ControlDef {
            name: "Ch01.CC.039+Ch01.CC.007".into(),
            max: 1023.0,
            ..Default::default()
        }];
        let r = import(&d);
        assert_eq!(r.skipped, 0);
        let cc = |number| crate::MidiId {
            channel: 0,
            kind: MidiKind::Cc,
            number,
        };
        assert_eq!(
            r.mappings[0].input,
            Input::MidiPair {
                lsb: cc(39),
                msb: cc(7)
            }
        );
        assert_eq!(r.mappings[0].interaction, Interaction::Direct);
        assert_eq!(
            r.pair_ranges,
            [crate::PairRange {
                lsb: cc(39),
                msb: cc(7),
                min: 0.0,
                max: 1023.0
            }]
        );
    }

    #[test]
    fn midi_mappings() {
        let d = device(
            "Generic MIDI",
            vec![
                m("Ch01.CC.007", 102, 3, 2, 0),
                m("Ch02.Note.C3", 100, 1, 1, 0),
                m("Ch01.CC.020", 3200, 6, 0, 0),
                m("Bogus", 100, 1, 0, 0),
                m("Ch01.CC.021", 3200, 5, 0, 0),
            ],
        );
        let r = import(&d);
        assert_eq!(r.skipped, 1);
        assert_eq!(
            (r.mappings[3].interaction, r.mappings[3].invert),
            (Interaction::Relative, false),
            "5 increments"
        );
        assert_eq!(
            r.mappings[0].input,
            Input::Midi {
                channel: 0,
                kind: MidiKind::Cc,
                number: 7
            }
        );
        assert_eq!(
            (r.mappings[0].action, r.mappings[0].interaction),
            (Action::Volume { channel: 2 }, Interaction::Direct)
        );
        assert_eq!(
            r.mappings[1].input,
            Input::Midi {
                channel: 1,
                kind: MidiKind::Note,
                number: 60
            }
        );
        assert_eq!(
            (r.mappings[2].interaction, r.mappings[2].invert),
            (Interaction::Relative, true)
        );
    }

    #[test]
    fn sample_deck_commands_and_the_deck_type() {
        let r = import(&device(
            "Generic MIDI",
            vec![
                m("Ch01.Note.C2", 2004, 2, 9, 0),
                m("Ch01.Note.D2", 237, 1, 14, 0),
                m("Ch01.Note.E2", 230, 1, 2, 0),
                m("Ch01.CC.020", 229, 3, 0, 0),
                m("Ch01.Note.F2", 601 + 16 * 2 + 5, 2, 3, 0),
                m("Ch01.Note.G2", 729, 2, 1, 0),
                m("Ch01.CC.021", 2302, 3, 1, 0),
            ],
        ));
        let actions: Vec<Action> = r.mappings.iter().map(|m| m.action).collect();
        assert_eq!(
            actions,
            [
                Action::SampleSlot {
                    deck: 0,
                    slot: 1,
                    part: crate::SampleSlotPart::Primary
                },
                Action::SampleSlot {
                    deck: 1,
                    slot: 2,
                    part: crate::SampleSlotPart::Keylock
                },
                Action::SampleDeck {
                    deck: 2,
                    part: crate::SamplePart::QuantizeOn
                },
                Action::SampleDeck {
                    deck: 0,
                    part: crate::SamplePart::QuantizeSelector
                },
                Action::SampleCell {
                    deck: 3,
                    slot: 2,
                    cell: 5
                },
                Action::SampleDeck {
                    deck: 1,
                    part: crate::SamplePart::CellLoadModifier
                },
                Action::DeckType { deck: 1 },
            ]
        );
        assert_eq!(r.mappings[0].interaction, Interaction::Hold);
        assert_eq!(r.mappings[1].interaction, Interaction::Toggle);
        assert_eq!(r.mappings[4].interaction, Interaction::Hold);
        assert_eq!(r.mappings[5].interaction, Interaction::Hold);
    }
}
