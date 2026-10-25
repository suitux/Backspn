use backspn_control::modifier::{COUNT, Condition, Gate, MAX};
use backspn_control::output::OutMapping;
use backspn_control::relative::Encoder;
use backspn_control::state::{Control as StateControl, StateCondition};
use backspn_control::{Action, Input, Interaction, Mapping, MidiKind, deck_letter, midi};
use eframe::egui::{self, ComboBox, RichText, Ui};

use crate::i18n::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControllerType {
    Button,
    Fader,
    Encoder,
}

impl ControllerType {
    pub(super) const ALL: [Self; 3] = [Self::Button, Self::Fader, Self::Encoder];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::Fader => "Fader / Knob",
            Self::Encoder => "Encoder",
        }
    }

    pub(super) fn of(m: &Mapping) -> Self {
        let button = matches!(
            m.input,
            Input::Key(_)
                | Input::Hid(_)
                | Input::Midi {
                    kind: MidiKind::Note,
                    ..
                }
        );
        match (m.interaction, m.encoder) {
            (Interaction::Relative, Encoder::Absolute) => Self::Fader,
            (Interaction::Relative, Encoder::Button) => Self::Button,
            (Interaction::Relative, _) => Self::Encoder,
            (Interaction::Direct, _) if !button => Self::Fader,
            _ => Self::Button,
        }
    }

    pub(super) fn editable(m: &Mapping) -> bool {
        matches!(
            m.input,
            Input::Midi {
                kind: MidiKind::Cc,
                ..
            }
        )
    }

    pub(super) fn apply(self, m: &mut Mapping) {
        if Self::of(m) == self {
            return;
        }
        match self {
            Self::Button => {
                if m.action.is_continuous() {
                    m.interaction = Interaction::Relative;
                    m.encoder = Encoder::Button;
                } else {
                    m.interaction = m.action.default_interaction();
                    m.encoder = Encoder::default();
                }
            }
            Self::Fader => {
                m.interaction = Interaction::Direct;
                m.encoder = Encoder::default();
            }
            Self::Encoder => {
                m.interaction = Interaction::Relative;
                m.encoder = Encoder::default();
            }
        }
        m.invert = false;
        m.auto_repeat = false;
        m.on_release = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Trigger,
    Toggle,
    Hold,
    Direct,
    Relative,
    Increment,
    Decrement,
    Reset,
}

impl Mode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Trigger => "Trigger",
            Self::Toggle => "Toggle",
            Self::Hold => "Hold",
            Self::Direct => "Direct",
            Self::Relative => "Relative",
            Self::Increment => "Increment",
            Self::Decrement => "Decrement",
            Self::Reset => "Reset",
        }
    }

    pub(super) fn of(m: &Mapping) -> Self {
        match m.interaction {
            Interaction::Trigger => Self::Trigger,
            Interaction::Toggle => Self::Toggle,
            Interaction::Hold => Self::Hold,
            Interaction::Direct => Self::Direct,
            Interaction::Reset => Self::Reset,
            Interaction::Relative => match (ControllerType::of(m), m.invert) {
                (ControllerType::Button, false) => Self::Increment,
                (ControllerType::Button, true) => Self::Decrement,
                _ => Self::Relative,
            },
        }
    }

    pub(super) fn offered(kind: ControllerType, action: Action) -> Vec<Self> {
        match kind {
            ControllerType::Button => {
                let mut v = vec![Self::Trigger, Self::Toggle, Self::Hold];
                if action.is_continuous() {
                    v.extend([Self::Direct, Self::Increment, Self::Decrement, Self::Reset]);
                }
                v
            }
            ControllerType::Fader => vec![Self::Direct, Self::Relative],
            ControllerType::Encoder => vec![Self::Relative],
        }
    }

    pub(super) fn apply(self, m: &mut Mapping) {
        let kind = ControllerType::of(m);
        m.interaction = match self {
            Self::Trigger => Interaction::Trigger,
            Self::Toggle => Interaction::Toggle,
            Self::Hold => Interaction::Hold,
            Self::Direct => Interaction::Direct,
            Self::Reset => Interaction::Reset,
            Self::Relative | Self::Increment | Self::Decrement => Interaction::Relative,
        };
        match self {
            Self::Increment | Self::Decrement => {
                if kind == ControllerType::Button
                    && !matches!(m.input, Input::Key(_) | Input::Hid(_))
                {
                    m.encoder = Encoder::Button;
                }
                m.invert = self == Self::Decrement;
            }
            Self::Relative if kind == ControllerType::Fader => m.encoder = Encoder::Absolute,
            _ => {}
        }
    }
}

pub(super) fn assignment(ui: &mut Ui, action: &mut Action, focus: &mut bool) -> bool {
    let before = (*action, *focus);
    ui.horizontal(|ui| {
        ui.label(tr("Assignment"));
        let label = super::assignment_label(*action, *focus);
        if action.has_deck() {
            ComboBox::from_id_salt("cm-assignment")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(*focus, "Device Target").clicked() {
                        *focus = true;
                    }
                    for d in 0..backspn_engine::DECKS {
                        let current = !*focus && *action == action.on_deck(d);
                        if ui
                            .selectable_label(current, format!("Deck {}", deck_letter(d)))
                            .clicked()
                        {
                            *focus = false;
                            *action = action.on_deck(d);
                        }
                    }
                });
        } else if let Some(unit) = action.unit_mut() {
            ComboBox::from_id_salt("cm-assignment")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for u in 0..backspn_engine::DECKS {
                        ui.selectable_value(unit, u, format!("FX Unit {}", u + 1));
                    }
                });
        } else {
            ui.label(label);
        }
    });
    before != (*action, *focus)
}

pub(super) fn in_details(ui: &mut Ui, m: &mut Mapping) -> bool {
    let before = m.clone();
    let modifier = matches!(m.action, Action::Modifier { .. });
    ui.horizontal(|ui| {
        ui.label(tr("Modifier Conditions"));
        gate_editor(ui, &mut m.gate, modifier);
    });
    let kind = ControllerType::of(m);
    ui.horizontal(|ui| {
        ui.label(tr("Type of Controller"));
        if ControllerType::editable(m) {
            let mut chosen = kind;
            ComboBox::from_id_salt("cm-type")
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for t in ControllerType::ALL {
                        ui.selectable_value(&mut chosen, t, t.label());
                    }
                });
            chosen.apply(m);
        } else {
            ui.label(kind.label());
        }
    });
    let kind = ControllerType::of(m);
    ui.horizontal(|ui| {
        ui.label(tr("Interaction Mode"));
        let mode = Mode::of(m);
        let mut chosen = mode;
        ComboBox::from_id_salt("cm-mode")
            .selected_text(mode.label())
            .show_ui(ui, |ui| {
                for o in Mode::offered(kind, m.action) {
                    ui.selectable_value(&mut chosen, o, o.label());
                }
            });
        if chosen != mode {
            chosen.apply(m);
        }
    });
    assignment(ui, &mut m.action, &mut m.focus);
    match ControllerType::of(m) {
        ControllerType::Button => button_options(ui, m),
        ControllerType::Encoder => encoder_options(ui, m),
        ControllerType::Fader => fader_options(ui, m),
    }
    *m != before
}

fn button_options(ui: &mut Ui, m: &mut Mapping) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Button Options")).weak());
        crate::widgets::check(ui, &mut m.invert, "Invert")
            .on_hover_text(tr("Press and release swap; steps go the other way"));
        if matches!(Mode::of(m), Mode::Increment | Mode::Decrement) {
            crate::widgets::check(ui, &mut m.auto_repeat, "Auto Repeat")
                .on_hover_text(tr("Held, it repeats every 100 ms after 300–400 ms"));
            crate::widgets::check(ui, &mut m.on_release, "On release")
                .on_hover_text(tr("Steps when released instead of pressed"));
        }
    });
}

fn encoder_options(ui: &mut Ui, m: &mut Mapping) {
    let label = |e: Encoder| match e {
        Encoder::TwosComplement => "7Fh/01h",
        Encoder::Offset => "3Fh/41h",
        Encoder::Rotary => "Rotary",
        Encoder::Absolute | Encoder::Button => "—",
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Rotary Encoder")).weak());
        ui.label(tr("Enc.-Mode"));
        ComboBox::from_id_salt("cm-encoder")
            .selected_text(label(m.encoder))
            .show_ui(ui, |ui| {
                for e in [Encoder::TwosComplement, Encoder::Offset, Encoder::Rotary] {
                    ui.selectable_value(&mut m.encoder, e, label(e));
                }
            })
            .response
            .on_hover_text(tr("How the encoder sends its turns"));
        ui.label(tr("Rotary Sensitivity"));
        ui.add(
            egui::DragValue::new(&mut m.sensitivity)
                .range(0.0..=300.0)
                .suffix(" %"),
        );
        ui.label(tr("Rotary Acceleration"));
        let mut acceleration = m.acceleration * 100.0;
        ui.add(
            egui::DragValue::new(&mut acceleration)
                .range(0.0..=100.0)
                .suffix(" %"),
        )
        .on_hover_text(tr("Faster turns move more"));
        m.acceleration = acceleration / 100.0;
        crate::widgets::check(ui, &mut m.invert, "Invert");
    });
}

fn fader_options(ui: &mut Ui, m: &mut Mapping) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Fader / Knob")).weak());
        crate::widgets::check(ui, &mut m.invert, "Invert");
        if m.interaction == Interaction::Direct {
            crate::widgets::check(ui, &mut m.soft_takeover, "Soft Takeover")
                .on_hover_text(tr("The fader acts once it reaches the current value"));
        }
    });
}

pub(super) fn out_details(ui: &mut Ui, o: &mut OutMapping) -> bool {
    let before = o.clone();
    let mut gate = Gate {
        conditions: std::mem::take(&mut o.conditions),
        states: std::mem::take(&mut o.states),
        value: None,
    };
    ui.horizontal(|ui| {
        ui.label(tr("Modifier Conditions"));
        gate_editor(ui, &mut gate, false);
    });
    o.conditions = gate.conditions;
    o.states = gate.states;
    ui.horizontal(|ui| {
        ui.label(tr("Type of Controller"));
        ui.label("LED");
    });
    assignment(ui, &mut o.action, &mut o.focus);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("LED Options")).weak());
        ui.label(tr("Controller Range"));
        let mut own = o.range.is_none();
        if crate::widgets::check(ui, &mut own, "Own")
            .on_hover_text(tr("The control's own range"))
            .changed()
        {
            o.range = (!own).then_some((0.0, 1.0));
        }
        if let Some((lo, hi)) = &mut o.range {
            ui.add(egui::DragValue::new(lo).speed(0.01).prefix("min "));
            ui.add(egui::DragValue::new(hi).speed(0.01).prefix("max "));
        }
        ui.label(tr("Midi Range"));
        let (lo, hi) = &mut o.midi_range;
        ui.add(egui::DragValue::new(lo).range(0..=127).prefix("min "));
        ui.add(egui::DragValue::new(hi).range(0..=127).prefix("max "));
        crate::widgets::check(ui, &mut o.invert, "Invert");
        crate::widgets::check(ui, &mut o.blend, "Blend")
            .on_hover_text(tr("The level follows the value within the range"));
    });
    *o != before
}

pub(super) fn gate_editor(ui: &mut Ui, gate: &mut Gate, modifier: bool) -> bool {
    let before = gate.clone();
    ui.horizontal(|ui| {
        if modifier {
            let mut set = gate.value.is_some();
            if crate::widgets::check(ui, &mut set, "Set to value")
                .on_hover_text(tr("Off: the mode's own value"))
                .changed()
            {
                gate.value = set.then_some(0);
            }
            if let Some(v) = &mut gate.value {
                ui.add(egui::DragValue::new(v).range(0..=MAX));
            }
        }
        let mut drop = None;
        for (n, c) in gate.conditions.iter_mut().enumerate() {
            let mut shown = c.modifier + 1;
            let last = COUNT.max(shown);
            ui.add(egui::DragValue::new(&mut shown).range(1..=last).prefix("M"));
            c.modifier = shown - 1;
            ui.label("=");
            ui.add(egui::DragValue::new(&mut c.value).range(0..=MAX));
            if ui
                .small_button("−")
                .on_hover_text(tr("Remove the condition"))
                .clicked()
            {
                drop = Some(n);
            }
        }
        if let Some(n) = drop {
            gate.conditions.remove(n);
        }
        let mut drop = None;
        for (n, c) in gate.states.iter_mut().enumerate() {
            ui.push_id(("state", n), |ui| state_editor(ui, c));
            if ui
                .small_button("−")
                .on_hover_text(tr("Remove the condition"))
                .clicked()
            {
                drop = Some(n);
            }
        }
        if let Some(n) = drop {
            gate.states.remove(n);
        }
        if gate.conditions.len() + gate.states.len() < 2
            && ui
                .small_button("+ M")
                .on_hover_text(tr("Only when a modifier has a value"))
                .clicked()
        {
            gate.conditions.push(Condition {
                modifier: 0,
                value: 0,
            });
        }
        if gate.conditions.len() + gate.states.len() < 2
            && ui
                .small_button("+ C")
                .on_hover_text(tr("Only when a control of the app has a value"))
                .clicked()
        {
            gate.states.push(StateCondition {
                control: StateControl::Playing,
                channel: None,
                value: 1,
            });
        }
    });
    *gate != before
}

fn state_editor(ui: &mut Ui, c: &mut StateCondition) {
    let channel_label = |channel: Option<usize>, unit: bool| match (channel, unit) {
        (None, _) => "Focus".to_owned(),
        (Some(u), true) => format!("FX {}", u + 1),
        (Some(d), false) => deck_letter(d).to_string(),
    };
    let unit = c.control.on_unit();
    ComboBox::from_id_salt("channel")
        .selected_text(channel_label(c.channel, unit))
        .show_ui(ui, |ui| {
            for channel in std::iter::once(None).chain((0..4).map(Some)) {
                ui.selectable_value(&mut c.channel, channel, channel_label(channel, unit));
            }
        });
    ComboBox::from_id_salt("control")
        .selected_text(c.control.label())
        .show_ui(ui, |ui| {
            for control in StateControl::all() {
                ui.selectable_value(&mut c.control, control, control.label());
            }
        });
    let (lo, hi) = c.control.range();
    ui.label("=");
    ui.add(egui::DragValue::new(&mut c.value).range(lo..=hi));
}

pub(super) fn midi_menu(ui: &mut Ui, current: &str) -> Option<Input> {
    let mut chosen = None;
    ui.menu_button(current, |ui| {
        for channel in 0..16u8 {
            ui.menu_button(format!("Ch{:02}", channel + 1), |ui| {
                ui.menu_button("Note", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for number in 0..=127u8 {
                                if ui.button(midi::note_name(number)).clicked() {
                                    chosen =
                                        Some(super::midi_input(channel, MidiKind::Note, number));
                                    ui.close();
                                }
                            }
                        });
                });
                ui.menu_button("CC", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for number in 0..=127u8 {
                                if ui.button(format!("{number:03}")).clicked() {
                                    chosen = Some(super::midi_input(channel, MidiKind::Cc, number));
                                    ui.close();
                                }
                            }
                        });
                });
                if ui.button("Pitch Bend").clicked() {
                    chosen = Some(super::midi_input(channel, MidiKind::PitchBend, 0));
                    ui.close();
                }
            });
        }
    });
    chosen
}

fn key_names() -> Vec<String> {
    let mut v: Vec<String> = (b'A'..=b'Z').map(|c| char::from(c).to_string()).collect();
    v.extend((0..=9).map(|d| d.to_string()));
    v.extend((1..=12).map(|f| format!("F{f}")));
    v.extend(
        [
            "Up",
            "Down",
            "Left",
            "Right",
            "Space",
            "Enter",
            "Escape",
            "Tab",
            "Backspace",
            "Delete",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "-",
            "=",
            "[",
            "]",
            ";",
            "'",
            ",",
            ".",
            "/",
            "\\",
            "`",
        ]
        .map(str::to_owned),
    );
    v
}

pub(super) fn key_menu(ui: &mut Ui, current: &str) -> Option<Input> {
    let mut chosen = None;
    let names = key_names();
    let mut list = |ui: &mut Ui, prefix: &str| {
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for name in &names {
                    if ui.button(name).clicked() {
                        chosen = Some(Input::Key(format!("{prefix}{name}")));
                        ui.close();
                    }
                }
            });
    };
    ui.menu_button(current, |ui| {
        ui.menu_button(tr("Key"), |ui| list(ui, ""));
        ui.menu_button("Shift+", |ui| list(ui, "Shift+"));
        ui.menu_button("Ctrl+", |ui| list(ui, "Ctrl+"));
        ui.menu_button("Alt+", |ui| list(ui, "Alt+"));
    });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(interaction: Interaction) -> Mapping {
        Mapping::new(
            super::super::midi_input(0, MidiKind::Cc, 7),
            Action::Volume { channel: 0 },
            interaction,
        )
    }

    #[test]
    fn the_type_follows_the_control_and_its_options() {
        let note = Mapping::new(
            super::super::midi_input(0, MidiKind::Note, 60),
            Action::PlayPause { deck: 0 },
            Interaction::Trigger,
        );
        assert_eq!(ControllerType::of(&note), ControllerType::Button);
        assert!(!ControllerType::editable(&note));
        let fader = cc(Interaction::Direct);
        assert_eq!(ControllerType::of(&fader), ControllerType::Fader);
        let mut encoder = cc(Interaction::Relative);
        assert_eq!(ControllerType::of(&encoder), ControllerType::Encoder);
        encoder.encoder = Encoder::Button;
        assert_eq!(ControllerType::of(&encoder), ControllerType::Button);
        assert_eq!(Mode::of(&encoder), Mode::Increment);
        encoder.invert = true;
        assert_eq!(Mode::of(&encoder), Mode::Decrement);
        encoder.encoder = Encoder::Absolute;
        assert_eq!(ControllerType::of(&encoder), ControllerType::Fader);
        assert_eq!(Mode::of(&encoder), Mode::Relative);
    }

    #[test]
    fn changing_the_type_and_mode_sets_the_options() {
        let mut m = cc(Interaction::Direct);
        ControllerType::Encoder.apply(&mut m);
        assert_eq!(
            (m.interaction, m.encoder),
            (Interaction::Relative, Encoder::TwosComplement)
        );
        ControllerType::Button.apply(&mut m);
        assert_eq!(ControllerType::of(&m), ControllerType::Button);
        Mode::Decrement.apply(&mut m);
        assert_eq!(
            (m.interaction, m.encoder, m.invert),
            (Interaction::Relative, Encoder::Button, true)
        );
        Mode::Increment.apply(&mut m);
        assert!(!m.invert);
        ControllerType::Fader.apply(&mut m);
        assert_eq!((m.interaction, m.invert), (Interaction::Direct, false));
        Mode::Relative.apply(&mut m);
        assert_eq!(m.encoder, Encoder::Absolute);
        assert_eq!(
            Mode::offered(ControllerType::Button, Action::PlayPause { deck: 0 }),
            [Mode::Trigger, Mode::Toggle, Mode::Hold]
        );
        assert!(
            Mode::offered(ControllerType::Button, Action::Volume { channel: 0 })
                .contains(&Mode::Increment)
        );
    }

    #[test]
    fn key_names_match_the_keyboard_naming() {
        let names = key_names();
        assert!(names.contains(&"A".to_owned()));
        assert!(names.contains(&"F12".to_owned()));
        assert!(names.contains(&"Left".to_owned()));
        assert_eq!(
            crate::keys::name(egui::Key::ArrowLeft, egui::Modifiers::NONE),
            "Left"
        );
        assert_eq!(
            crate::keys::name(egui::Key::F12, egui::Modifiers::NONE),
            "F12"
        );
    }
}
