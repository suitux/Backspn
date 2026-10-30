use serde::{Deserialize, Serialize};

use crate::feedback::Feedback;
use crate::modifier::{Condition, Modifiers};
use crate::{Action, Input, InputEvent, MidiKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutMapping {
    pub output: Input,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<crate::state::StateCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<(f32, f32)>,
    #[serde(default = "full_midi_range")]
    pub midi_range: (u8, u8),
    #[serde(default)]
    pub blend: bool,
    #[serde(default)]
    pub invert: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

fn full_midi_range() -> (u8, u8) {
    (0, 127)
}

impl OutMapping {
    pub fn new(output: Input, action: Action) -> Self {
        Self {
            output,
            action,
            conditions: Vec::new(),
            states: Vec::new(),
            range: None,
            midi_range: full_midi_range(),
            blend: false,
            invert: false,
            focus: false,
            port: None,
            device: None,
            comment: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutValue {
    OnOff(bool),
    Number { value: f32, min: f32, max: f32 },
    Pulse(bool),
}

pub const PULSE: std::time::Duration = std::time::Duration::from_millis(100);

pub fn pulses(action: Action) -> bool {
    matches!(
        action,
        Action::TempoSync { .. }
            | Action::PhaseSync { .. }
            | Action::CopyPhase { .. }
            | Action::BeatTap { .. }
            | Action::MasterTap
            | Action::ClockResync
            | Action::CueStore { .. }
            | Action::CueSetStore { .. }
            | Action::CueDelete { .. }
            | Action::LoopIn { .. }
            | Action::LoopOut { .. }
            | Action::ReturnToStart { .. }
            | Action::AutoFade { .. }
            | Action::DuplicateDeck { .. }
            | Action::LoadNext { .. }
            | Action::LoadPrevious { .. }
            | Action::LoadSelected { .. }
            | Action::LoadSelectedAs { .. }
            | Action::LoadLastRecording { .. }
            | Action::RecordCut
            | Action::FxLfoReset
            | Action::FxSnapshotStore { .. }
            | Action::FxSnapshotLoad { .. }
            | Action::PatternReset { .. }
            | Action::PatternClear { .. }
    )
}

fn loop_recorder(
    part: crate::LoopRecorderPart,
    r: &crate::feedback::LoopRecorderFeedback,
) -> Option<OutValue> {
    use crate::LoopRecorderPart as P;
    let number = |value: u8, max: f32| OutValue::Number {
        value: f32::from(value),
        min: 0.0,
        max,
    };
    let unit = |value: f32| OutValue::Number {
        value,
        min: 0.0,
        max: 1.0,
    };
    Some(match part {
        P::Record => OutValue::OnOff(matches!(r.state, 3 | 4)),
        P::Play => OutValue::OnOff(matches!(r.state, 2 | 4)),
        P::Size => number(r.size, 3.0),
        P::Source => number(r.source, 3.0),
        P::DryWet => unit(r.dry_wet),
        P::Position => unit(r.position),
        P::Undo => number(r.undo, 2.0),
        P::State => number(r.state, 4.0),
        P::Delete => return None,
    })
}

#[derive(Debug, Clone, Default)]
pub struct Pulses(Vec<(Action, std::time::Instant)>);

impl Pulses {
    pub fn fire(&mut self, action: Action, now: std::time::Instant) {
        self.0.retain(|(a, _)| *a != action);
        self.0.push((action, now));
    }

    pub fn lit(&mut self, now: std::time::Instant) -> Vec<Action> {
        self.0
            .retain(|(_, at)| now.saturating_duration_since(*at) < PULSE);
        self.0.iter().map(|(a, _)| *a).collect()
    }

    pub fn next_change(&self, now: std::time::Instant) -> Option<std::time::Duration> {
        self.0
            .iter()
            .map(|(_, at)| (*at + PULSE).saturating_duration_since(now))
            .min()
    }
}

#[allow(clippy::too_many_lines)]
pub fn value(action: Action, f: &Feedback, modifiers: &Modifiers) -> Option<OutValue> {
    let on = OutValue::OnOff;
    let unit = |value: f32| OutValue::Number {
        value,
        min: 0.0,
        max: 1.0,
    };
    let deck = |d: usize| f.decks.get(d);
    let channel = |c: usize| f.channels.get(c);
    let k = &f.knobs;
    Some(match action {
        Action::PlayPause { deck: d } => on(deck(d)?.playing),
        Action::Sync { deck: d } => on(deck(d)?.synced),
        Action::Master { deck: d } => on(deck(d)?.master),
        Action::Keylock { deck: d } => on(deck(d)?.keylock),
        Action::LoopActive { deck: d } => on(deck(d)?.loop_active),
        Action::Slip { deck: d } => on(deck(d)?.slip),
        Action::Reverse { deck: d } => on(deck(d)?.reverse),
        Action::DeckTick { deck: d } => on(*f.ticks.get(d)?),
        Action::MasterTick => on(f.master_tick),
        Action::PadFx { deck: d, bank, pad } => {
            on(pad < 16 && deck(d)?.pad_fx.get(bank)? & (1 << pad) != 0)
        }
        Action::LoopRecorder { part } => loop_recorder(part, &f.loop_recorder)?,
        Action::Quantize => on(f.quantize),
        Action::TempoRange { deck: d } => OutValue::Number {
            value: f32::from(deck(d)?.tempo_range),
            min: 0.0,
            max: 13.0,
        },
        Action::Hotcue { deck: d, slot } => OutValue::Number {
            value: deck(d)?.hotcues.get(slot)?.type_code() as f32,
            min: 0.0,
            max: 5.0,
        },
        Action::MonitorCue { channel: c } => on(channel(c)?.cue),
        Action::Mute { channel: c } => on(channel(c)?.mute),
        Action::MixerFxOn { channel: c } => on(channel(c)?.mixer_fx_on),
        Action::FxAssign { channel: c, unit } | Action::FxAssignHeld { channel: c, unit } => {
            on(*channel(c)?.fx_assign.get(unit)?)
        }
        Action::FxOn { unit } => on(f.fx.get(unit)?.on),
        Action::FxMode { unit } => OutValue::Number {
            value: f32::from(f.fx.get(unit)?.mode),
            min: 0.0,
            max: 2.0,
        },
        Action::FxButton { unit, button } => on(*f.fx.get(unit)?.buttons.get(button)?),
        Action::Volume { channel: c } => unit(*k.fader.get(c)?),
        Action::Gain { channel: c } => unit(*k.gain.get(c)?),
        Action::Eq { channel: c, band } => unit(*k.eq.get(c)?.get(band)?),
        Action::EqKill { channel: c, band } => on(*k.kill.get(c)?.get(band)?),
        Action::Filter { channel: c } => unit(*k.filter.get(c)?),
        Action::Crossfader => unit(k.crossfader),
        Action::MasterVolume => unit(k.master),
        Action::CueVolume => unit(k.cue_volume),
        Action::CueMix => unit(k.cue_mix),
        Action::Modifier { index } => OutValue::Number {
            value: f32::from(*modifiers.0.get(index)?),
            min: 0.0,
            max: f32::from(crate::modifier::MAX),
        },
        Action::SampleSlot {
            deck: d,
            slot,
            part,
        } => {
            let s = deck(d)?.sample_deck?.slots.get(slot).copied()?;
            match part {
                crate::SampleSlotPart::State => OutValue::Number {
                    value: f32::from(s.state),
                    min: 0.0,
                    max: 2.0,
                },
                crate::SampleSlotPart::Play => on(s.state == 2),
                crate::SampleSlotPart::Keylock => on(s.keylock),
                crate::SampleSlotPart::Punch => on(s.punch),
                crate::SampleSlotPart::PlayMode => on(!s.one_shot),
                crate::SampleSlotPart::TriggerType => on(s.gate),
                _ => return None,
            }
        }
        Action::SampleCell {
            deck: d,
            slot,
            cell,
        } => OutValue::Number {
            value: f32::from(*deck(d)?.sample_deck?.slots.get(slot)?.cells.get(cell)?),
            min: 0.0,
            max: 2.0,
        },
        Action::SampleDeck { deck: d, part } => {
            let r = deck(d)?.sample_deck?;
            match part {
                crate::SamplePart::QuantizeOn => on(r.quantize_on),
                crate::SamplePart::QuantizeSelector => OutValue::Number {
                    value: f32::from(r.quantize_index),
                    min: 0.0,
                    max: 7.0,
                },
                crate::SamplePart::Page => OutValue::Number {
                    value: f32::from(r.page),
                    min: 0.0,
                    max: 3.0,
                },
                _ => return None,
            }
        }
        Action::DeckType { deck: d } => OutValue::Number {
            value: f32::from(u8::from(deck(d)?.sample_deck.is_some())),
            min: 0.0,
            max: 2.0,
        },
        _ => return None,
    })
}

pub fn position(action: Action, f: &Feedback) -> Option<f32> {
    match value(action, f, &Modifiers::default())? {
        OutValue::Number { value, min, max } if max > min => Some((value - min) / (max - min)),
        _ => None,
    }
}

pub fn supported(action: Action) -> bool {
    pulses(action) || value(action, &Feedback::default(), &Modifiers::default()).is_some()
}

pub fn learn(action: Action, event: &InputEvent) -> Option<OutMapping> {
    let midi = matches!(
        event.input,
        Input::Midi {
            kind: MidiKind::Note | MidiKind::Cc,
            ..
        }
    );
    (midi && event.pressed != Some(false) && supported(action))
        .then(|| OutMapping::new(event.input.clone(), action))
}

pub fn level(m: &OutMapping, v: OutValue) -> u8 {
    let x = match v {
        OutValue::Pulse(lit) => {
            return if lit { m.midi_range.1 } else { m.midi_range.0 };
        }
        OutValue::OnOff(lit) => f32::from(u8::from(lit)),
        OutValue::Number { value, min, max } => {
            let (lo, hi) = m.range.unwrap_or((min, max));
            if m.blend {
                let x = (value - lo) / (hi - lo);
                if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
            } else {
                f32::from(u8::from(lo <= value && value <= hi))
            }
        }
    };
    let x = if m.invert { 1.0 - x } else { x };
    let (lo, hi) = (f32::from(m.midi_range.0), f32::from(m.midi_range.1));
    (lo + x * (hi - lo)).clamp(0.0, 127.0) as u8
}

pub fn message(output: &Input, level: u8) -> Option<[u8; 3]> {
    let Input::Midi {
        channel,
        kind,
        number,
    } = *output
    else {
        return None;
    };
    let status = match kind {
        MidiKind::Note => 0x90,
        MidiKind::Cc => 0xB0,
        MidiKind::PitchBend => return Some([0xE0 | (channel & 0x0F), level & 0x7F, 0]),
    };
    Some([status | (channel & 0x0F), number & 0x7F, level & 0x7F])
}

#[derive(Debug, Clone, Default)]
pub struct Sender {
    last: Vec<Option<u8>>,
    flagged: Vec<bool>,
}

impl Sender {
    pub fn update(
        &mut self,
        mappings: &[OutMapping],
        f: &Feedback,
        modifiers: &Modifiers,
        lit: &[Action],
        force: bool,
    ) -> Vec<[u8; 3]> {
        if self.last.len() != mappings.len() {
            self.last = vec![None; mappings.len()];
            self.flagged = vec![false; mappings.len()];
        }
        let mut out = Vec::new();
        for (i, m) in mappings.iter().enumerate() {
            if !modifiers.hold(&m.conditions) || !crate::state::hold(&m.states, f) {
                self.flagged[i] = true;
                continue;
            }
            let action = if m.focus {
                m.action.on_deck(f.focus)
            } else {
                m.action
            };
            let v = if pulses(action) {
                OutValue::Pulse(lit.contains(&action))
            } else {
                let Some(v) = value(action, f, modifiers) else {
                    continue;
                };
                v
            };
            let level = level(m, v);
            if force || self.flagged[i] || self.last[i] != Some(level) {
                out.extend(message(&m.output, level));
            }
            self.last[i] = Some(level);
            self.flagged[i] = false;
        }
        out
    }

    pub fn reset(&mut self) {
        self.last.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_kill_lights_while_the_band_is_cut() {
        let mut f = Feedback::default();
        let kill = Action::EqKill {
            channel: 2,
            band: 1,
        };
        let m = Modifiers::default();
        assert_eq!(value(kill, &f, &m), Some(OutValue::OnOff(false)));
        f.knobs.kill[2][1] = true;
        assert_eq!(value(kill, &f, &m), Some(OutValue::OnOff(true)));
    }
    use crate::feedback::HotcueKind;

    fn note(number: u8) -> Input {
        Input::Midi {
            channel: 1,
            kind: MidiKind::Note,
            number,
        }
    }

    fn number(value: f32) -> OutValue {
        OutValue::Number {
            value,
            min: 0.0,
            max: 1.0,
        }
    }

    #[test]
    fn levels_follow_the_rules() {
        let mut m = OutMapping::new(note(1), Action::Volume { channel: 0 });
        assert_eq!(level(&m, OutValue::OnOff(true)), 127);
        assert_eq!(level(&m, OutValue::OnOff(false)), 0);
        assert_eq!(level(&m, number(0.3)), 127);
        m.range = Some((0.5, 1.0));
        assert_eq!(level(&m, number(0.3)), 0);
        m.blend = true;
        assert_eq!(level(&m, number(0.75)), 63);
        assert_eq!(level(&m, number(0.2)), 0);
        m.range = Some((0.5, 0.5));
        assert_eq!((level(&m, number(0.5)), level(&m, number(0.6))), (0, 127));
        m.range = Some((0.5, 1.0));
        m.midi_range = (10, 20);
        m.invert = true;
        assert_eq!(level(&m, number(1.0)), 10);
        assert_eq!(level(&m, OutValue::OnOff(false)), 20);
    }

    #[test]
    fn trigger_commands_pulse_for_100_ms() {
        let sync = Action::TempoSync { deck: 0 };
        let mut m = OutMapping::new(note(1), sync);
        m.invert = true;
        m.midi_range = (5, 100);
        let mappings = [m];
        let (f, free) = (Feedback::default(), Modifiers::default());
        let mut s = Sender::default();
        let mut pulses = Pulses::default();
        let t = std::time::Instant::now();
        assert_eq!(
            s.update(&mappings, &f, &free, &pulses.lit(t), false),
            [[0x91, 1, 5]]
        );
        pulses.fire(sync, t);
        assert_eq!(
            s.update(&mappings, &f, &free, &pulses.lit(t), false),
            [[0x91, 1, 100]]
        );
        let at = |ms| t + std::time::Duration::from_millis(ms);
        pulses.fire(sync, at(60));
        assert_eq!(pulses.next_change(at(60)), Some(PULSE));
        assert_eq!(
            s.update(&mappings, &f, &free, &pulses.lit(at(120)), false),
            Vec::<[u8; 3]>::new()
        );
        assert_eq!(
            s.update(&mappings, &f, &free, &pulses.lit(at(160)), false),
            [[0x91, 1, 5]]
        );
        assert_eq!(pulses.next_change(at(160)), None);
        assert!(supported(sync) && !super::pulses(Action::PlayPause { deck: 0 }));
    }

    #[test]
    fn hotcue_types_light_inside_the_range() {
        let m = OutMapping::new(note(1), Action::Hotcue { deck: 0, slot: 0 });
        let mut f = Feedback::default();
        let v = |f: &Feedback| value(m.action, f, &Modifiers::default()).unwrap();
        assert_eq!(level(&m, v(&f)), 0);
        f.decks[0].hotcues[0] = HotcueKind::Loop;
        assert_eq!(level(&m, v(&f)), 127);
    }

    #[test]
    fn messages_are_notes_or_ccs() {
        assert_eq!(message(&note(60), 127), Some([0x91, 60, 127]));
        let cc = Input::Midi {
            channel: 0,
            kind: MidiKind::Cc,
            number: 7,
        };
        assert_eq!(message(&cc, 5), Some([0xB0, 7, 5]));
        assert_eq!(message(&Input::Key("A".into()), 1), None);
        let bend = Input::Midi {
            channel: 2,
            kind: MidiKind::PitchBend,
            number: 0,
        };
        assert_eq!(message(&bend, 100), Some([0xE2, 100, 0]));
    }

    #[test]
    fn state_conditions_gate_outputs() {
        let mut play = OutMapping::new(note(1), Action::PlayPause { deck: 0 });
        play.states = vec![crate::state::StateCondition {
            control: crate::state::Control::FxMode,
            channel: Some(0),
            value: 1,
        }];
        let mut f = Feedback::default();
        let mut s = Sender::default();
        let free = Modifiers::default();
        let mappings = [play];
        assert_eq!(
            s.update(&mappings, &f, &free, &[], false),
            Vec::<[u8; 3]>::new()
        );
        f.fx[0].mode = 1;
        assert_eq!(s.update(&mappings, &f, &free, &[], false), [[0x91, 1, 0]]);
    }

    #[test]
    fn focus_outputs_show_the_focused_deck() {
        let mut play = OutMapping::new(note(1), Action::PlayPause { deck: 0 });
        play.focus = true;
        let mut f = Feedback::default();
        f.decks[1].playing = true;
        let mut s = Sender::default();
        let free = Modifiers::default();
        let mappings = [play];
        assert_eq!(s.update(&mappings, &f, &free, &[], false), [[0x91, 1, 0]]);
        f.focus = 1;
        assert_eq!(s.update(&mappings, &f, &free, &[], false), [[0x91, 1, 127]]);
    }

    #[test]
    fn sends_on_change_and_again_after_the_conditions_held_again() {
        let mut play = OutMapping::new(note(1), Action::PlayPause { deck: 0 });
        play.conditions = vec![Condition {
            modifier: 0,
            value: 0,
        }];
        let mappings = [play];
        let mut f = Feedback::default();
        let mut s = Sender::default();
        let free = Modifiers::default();
        assert_eq!(s.update(&mappings, &f, &free, &[], false), [[0x91, 1, 0]]);
        assert_eq!(
            s.update(&mappings, &f, &free, &[], false),
            Vec::<[u8; 3]>::new()
        );
        f.decks[0].playing = true;
        assert_eq!(s.update(&mappings, &f, &free, &[], false), [[0x91, 1, 127]]);
        let shifted = Modifiers([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        f.decks[0].playing = false;
        assert_eq!(
            s.update(&mappings, &f, &shifted, &[], false),
            Vec::<[u8; 3]>::new()
        );
        f.decks[0].playing = true;
        assert_eq!(s.update(&mappings, &f, &free, &[], false), [[0x91, 1, 127]]);
        assert_eq!(s.update(&mappings, &f, &free, &[], true), [[0x91, 1, 127]]);
        s.reset();
        assert_eq!(s.update(&mappings, &f, &free, &[], false), [[0x91, 1, 127]]);
    }

    #[test]
    fn modifiers_show_their_value() {
        let mut m = OutMapping::new(note(2), Action::Modifier { index: 1 });
        m.range = Some((3.0, 3.0));
        let mods = Modifiers([0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let v = value(m.action, &Feedback::default(), &mods).unwrap();
        assert_eq!(level(&m, v), 127);
        assert!(supported(Action::PlayPause { deck: 3 }));
        assert!(!supported(Action::Cue { deck: 0 }));
    }

    #[test]
    fn learning_takes_the_pressed_note_or_cc() {
        let press = |input, pressed| InputEvent {
            input,
            value: 1.0,
            raw: 127,
            pressed,
        };
        let play = Action::PlayPause { deck: 0 };
        let m = learn(play, &press(note(5), Some(true))).unwrap();
        assert_eq!((m.output, m.midi_range), (note(5), (0, 127)));
        assert!(learn(play, &press(note(5), Some(false))).is_none());
        assert!(learn(play, &press(Input::Key("A".into()), Some(true))).is_none());
        assert!(learn(Action::Cue { deck: 0 }, &press(note(5), Some(true))).is_none());
    }
}
