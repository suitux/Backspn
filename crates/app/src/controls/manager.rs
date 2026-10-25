use backspn_control::device::{self, Device, Kind};
use backspn_control::modifier::Gate;
use backspn_control::output::{self, OutMapping};
use backspn_control::{
    Action, Input, InputEvent, Interaction, Learn, Mapping, MidiId, MidiKind, deck_letter, midi,
};
use eframe::egui::{self, Button, ComboBox, RichText, Ui};

use super::{Controls, KEYBOARD};
use crate::i18n::tr;
use crate::modal;
use crate::search_combo::{self, Entry};

mod commands;
mod editors;

use editors::Mode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Selected {
    Native(&'static str),
    Device(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Row {
    In(usize),
    Out(usize),
    Pending,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Pending {
    pub action: Action,
    pub output: bool,
    pub focus: bool,
}

#[derive(Default)]
pub(super) struct Manager {
    pub selected: Option<Selected>,
    pub row: Option<Row>,
    pub pending: Option<Pending>,
    pub ask: Option<modal::Ask<Confirm>>,
    pub editing_comment: bool,
    pub name_draft: Option<(usize, String)>,
    pub focus_name: bool,
    pub table_draft: Option<(usize, String)>,
    pub focus_table_name: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Confirm {
    RemoveDevice(usize),
    DeleteTable(usize),
}

const COLUMNS: [&str; 8] = [
    "Control",
    "I/O",
    "Assignment",
    "Mode",
    "Mapped to",
    "Cond1",
    "Cond2",
    "Comment",
];

const TABLE_HEIGHT: f32 = 220.0;

pub(super) fn bare_name(action: Action) -> String {
    let label = action.label();
    for prefix in ["Deck ", "Ch "] {
        if let Some(rest) = label.strip_prefix(prefix)
            && rest.len() > 2
            && rest.as_bytes()[1] == b' '
        {
            return rest[2..].to_owned();
        }
    }
    if let Some(rest) = label.strip_prefix("FX")
        && rest.len() > 2
        && rest.as_bytes()[0].is_ascii_digit()
        && rest.as_bytes()[1] == b' '
    {
        return rest[2..].to_owned();
    }
    label
}

pub(super) fn assignment_label(mut action: Action, focus: bool) -> String {
    if focus && action.has_deck() {
        return "Device Target".to_owned();
    }
    if action.has_deck() {
        let mut copy = action;
        let deck = copy
            .deck_mut()
            .map(|d| *d)
            .or_else(|| action.channel_mut().map(|c| *c));
        return format!("Deck {}", deck_letter(deck.unwrap_or(0)));
    }
    if let Some(unit) = action.unit_mut() {
        return format!("FX Unit {}", *unit + 1);
    }
    "Global".to_owned()
}

fn condition_cells(gate: &Gate) -> [String; 2] {
    let mut cells = gate
        .conditions
        .iter()
        .map(|c| format!("M{}={}", c.modifier + 1, c.value))
        .chain(gate.states.iter().map(|s| {
            let channel = match (s.channel, s.control.on_unit()) {
                (None, _) => "Focus".to_owned(),
                (Some(u), true) => format!("FX {}", u + 1),
                (Some(d), false) => deck_letter(d).to_string(),
            };
            format!("{channel}.{}={}", s.control.label(), s.value)
        }));
    [
        cells.next().unwrap_or_default(),
        cells.next().unwrap_or_default(),
    ]
}

pub(super) fn in_cells(m: &Mapping) -> [String; 8] {
    let [c1, c2] = condition_cells(&m.gate);
    [
        bare_name(m.action),
        "In".to_owned(),
        assignment_label(m.action, m.focus),
        Mode::of(m).label().to_owned(),
        m.input.to_string(),
        c1,
        c2,
        m.comment.clone(),
    ]
}

pub(super) fn out_cells(o: &OutMapping) -> [String; 8] {
    let gate = Gate {
        conditions: o.conditions.clone(),
        states: o.states.clone(),
        value: None,
    };
    let [c1, c2] = condition_cells(&gate);
    [
        bare_name(o.action),
        "Out".to_owned(),
        assignment_label(o.action, o.focus),
        "Output".to_owned(),
        o.output.to_string(),
        c1,
        c2,
        o.comment.clone(),
    ]
}

pub(super) fn is_factory(m: &Mapping, defaults: &[Mapping]) -> bool {
    defaults.iter().any(|d| {
        d.input == m.input
            && d.action == m.action
            && d.interaction == m.interaction
            && d.gate == m.gate
            && d.invert == m.invert
            && d.focus == m.focus
    })
}

fn pending_cells(p: &Pending) -> [String; 8] {
    [
        bare_name(p.action),
        if p.output { "Out" } else { "In" }.to_owned(),
        assignment_label(p.action, p.focus),
        if p.output { "Output" } else { "—" }.to_owned(),
        "—".to_owned(),
        String::new(),
        String::new(),
        String::new(),
    ]
}

fn is_button(input: &Input) -> bool {
    !matches!(
        input,
        Input::Midi {
            kind: MidiKind::Cc | MidiKind::PitchBend,
            ..
        } | Input::MidiPair { .. }
    )
}

pub(super) fn source_of(device: Option<&Device>, native: Option<&str>, source: &str) -> bool {
    match (device, native) {
        (Some(d), _) => match (d.kind, d.in_port.as_deref()) {
            (Kind::Keyboard, _) => source == KEYBOARD,
            (Kind::Midi, Some(port)) => source == port,
            (Kind::Midi, None) => source != KEYBOARD,
        },
        (None, Some(native)) => source == native,
        (None, None) => false,
    }
}

impl Controls {
    fn selected_device(&self) -> Option<(usize, &Device)> {
        match self.manager.selected {
            Some(Selected::Device(i)) => self.set.devices.get(i).map(|d| (i, d)),
            _ => None,
        }
    }

    pub fn selected_native(&mut self) -> Option<&'static str> {
        self.fix_selection();
        match self.manager.selected {
            Some(Selected::Native(name)) => Some(name),
            _ => None,
        }
    }

    fn selected_native_source(&self) -> Option<&'static str> {
        match self.manager.selected {
            Some(Selected::Native(name)) => self
                .devices
                .iter()
                .find(|d| d.name() == name)
                .map(|d| d.source()),
            _ => None,
        }
    }

    fn rows(&self) -> Vec<(Row, [String; 8], bool)> {
        let mut rows = Vec::new();
        match self.manager.selected {
            Some(Selected::Native(name)) => {
                let defaults = self
                    .devices
                    .iter()
                    .find(|d| d.name() == name)
                    .map(|d| d.default_mappings())
                    .unwrap_or_default();
                for (i, m) in self.set.mappings.iter().enumerate() {
                    if matches!(m.input, Input::Hid(_)) {
                        rows.push((Row::In(i), in_cells(m), is_factory(m, &defaults)));
                    }
                }
            }
            Some(Selected::Device(d)) => {
                if let Some(device) = self.set.devices.get(d) {
                    for (i, m) in self.set.mappings.iter().enumerate() {
                        if device.owns(m) {
                            rows.push((Row::In(i), in_cells(m), false));
                        }
                    }
                    for (i, o) in self.set.outputs.iter().enumerate() {
                        if device.owns_output(o) {
                            rows.push((Row::Out(i), out_cells(o), false));
                        }
                    }
                }
            }
            None => {}
        }
        if let Some(p) = &self.manager.pending {
            rows.push((Row::Pending, pending_cells(p), false));
        }
        rows
    }

    fn fix_selection(&mut self) {
        let valid = match self.manager.selected {
            Some(Selected::Native(name)) => self.devices.iter().any(|d| d.name() == name),
            Some(Selected::Device(i)) => i < self.set.devices.len(),
            None => false,
        };
        if !valid {
            self.manager.selected = self
                .devices
                .first()
                .map(|d| Selected::Native(d.name()))
                .or_else(|| (!self.set.devices.is_empty()).then_some(Selected::Device(0)));
            self.manager.row = None;
            self.manager.pending = None;
            self.learn = None;
        }
        let row_valid = match self.manager.row {
            Some(Row::In(i)) => i < self.set.mappings.len(),
            Some(Row::Out(i)) => i < self.set.outputs.len(),
            Some(Row::Pending) => self.manager.pending.is_some(),
            None => true,
        };
        if !row_valid {
            self.manager.row = None;
            self.learn = None;
        }
    }

    fn select(&mut self, selected: Selected) {
        if self.manager.selected.as_ref() != Some(&selected) {
            self.manager.selected = Some(selected);
            self.manager.row = None;
            self.manager.pending = None;
            self.manager.editing_comment = false;
            self.manager.name_draft = None;
            self.manager.table_draft = None;
            self.learn = None;
            self.message = None;
        }
    }

    pub fn contents(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.sync_tables();
        self.fix_selection();
        self.device_setup(ui, ctx);
        ui.separator();
        self.assignment_table(ui);
        ui.separator();
        self.device_mapping(ui);
        ui.separator();
        self.mapping_details(ui);
        if let Some(message) = &self.message {
            ui.label(RichText::new(message.as_str()).weak());
        }
        match modal::show(ctx, &mut self.manager.ask) {
            Some(Confirm::RemoveDevice(i)) => {
                self.set.remove_device(i);
                self.manager.selected = None;
                self.resend_leds();
                self.save();
            }
            Some(Confirm::DeleteTable(i)) => self.delete_table(i),
            None => {}
        }
    }

    #[allow(clippy::too_many_lines)]
    fn device_setup(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.heading(tr("Device Setup"));
        let title = match self.manager.selected {
            Some(Selected::Native(name)) => name.to_owned(),
            Some(Selected::Device(i)) => self
                .set
                .devices
                .get(i)
                .map(Device::title)
                .unwrap_or_default(),
            None => "—".to_owned(),
        };
        let mut select = None;
        let mut add_profile = None;
        let mut add_generic = false;
        let mut entries = Vec::new();
        for d in &self.devices {
            let chosen = self.manager.selected == Some(Selected::Native(d.name()));
            let tested = backspn_control::profile::native_tested(d.name());
            entries.push(
                Entry::item(Pick::Native(d.name()), listed(d.name(), tested), chosen)
                    .with_hint(hint("", tested)),
            );
        }
        for (i, d) in self.set.devices.iter().enumerate() {
            let chosen = self.manager.selected == Some(Selected::Device(i));
            entries.push(Entry::item(Pick::Device(i), d.title(), chosen));
        }
        entries.push(Entry::Separator);
        entries.push(Entry::Heading(
            tr("Add: loads the model's mappings").to_owned(),
        ));
        for (i, p) in self.profiles.iter().enumerate() {
            if self.set.devices.iter().any(|d| d.name == p.name()) {
                continue;
            }
            entries.push(
                Entry::item(Pick::Profile(i), listed(&p.name(), p.tested), false)
                    .with_hint(hint(&p.notes, p.tested)),
            );
        }
        ui.horizontal(|ui| {
            ui.label(tr("Device"));
            let (response, pick) = search_combo::show(ui, "cm-device", title, 260.0, &entries);
            response.on_hover_text(tr(
                "The controller or mixer in use; a model not listed yet is added with its mappings",
            ));
            match pick {
                Some(Pick::Native(name)) => select = Some(Selected::Native(name)),
                Some(Pick::Device(i)) => select = Some(Selected::Device(i)),
                Some(Pick::Profile(i)) => add_profile = self.profiles.get(i).cloned(),
                None => {}
            }
            if ui
                .button(tr("Add…"))
                .on_hover_text(tr(
                    "A new Generic MIDI device for your own hardware: pick its In-Port, then Add In… and Learn each control",
                ))
                .clicked()
            {
                add_generic = true;
            }
            self.edit_menu(ui);
            if ui
                .button(tr("Import .tsi…"))
                .on_hover_text(tr(
                    "Adds the devices of a .tsi file with their mappings; to add only a mapping, use Import… in Mapping",
                ))
                .clicked()
            {
                self.import_tsi(ctx);
            }
        });
        if let Some(selected) = select {
            self.select(selected);
        }
        if let Some(profile) = add_profile {
            let input = super::profiles::input_for(&profile, &midi::input_ports(), |port| {
                self.set.has_port(port)
            });
            self.add_profile(&profile, &input, ctx);
            if let Some(i) = self
                .set
                .devices
                .iter()
                .position(|d| d.kind == Kind::Midi && d.in_port.as_deref() == Some(&input))
            {
                self.select(Selected::Device(i));
            }
        }
        if add_generic {
            let used: Vec<&str> = self
                .set
                .devices
                .iter()
                .filter_map(|d| d.in_port.as_deref())
                .collect();
            let input = midi::input_ports()
                .into_iter()
                .find(|p| !used.contains(&p.as_str()));
            let output = input.clone().filter(|p| midi::output_ports().contains(p));
            self.set.devices.push(Device::midi(
                device::GENERIC_MIDI,
                input.clone(),
                output.clone(),
            ));
            if let Some(input) = &input
                && !self.set.midi_inputs.contains(input)
            {
                self.set.midi_inputs.push(input.clone());
                self.connect(input, ctx);
            }
            if let Some(output) = &output
                && !self.set.midi_outputs.contains(output)
            {
                self.set.midi_outputs.push(output.clone());
                self.connect_output(output);
            }
            self.select(Selected::Device(self.set.devices.len() - 1));
            self.manager.focus_name = true;
            self.save();
        }
        if let Some((i, _)) = self.selected_device() {
            self.name_row(ui, i);
        }
        if self.manager.editing_comment
            && let Some((i, _)) = self.selected_device()
        {
            ui.horizontal(|ui| {
                ui.label(tr("Comment"));
                let comment = &mut self.set.devices[i].comment;
                if ui.text_edit_singleline(comment).lost_focus() {
                    self.manager.editing_comment = false;
                    self.save();
                }
            });
        }
        match self.manager.selected {
            Some(Selected::Native(name)) => {
                self.native_status(ui, name);
            }
            Some(Selected::Device(i)) => {
                if self.set.devices[i].kind == Kind::Midi {
                    self.port_row(ui, i, ctx);
                }
                self.target_row(ui, i);
            }
            None => {}
        }
        self.mapping_row(ui);
        self.modifier_state(ui);
    }

    fn name_row(&mut self, ui: &mut Ui, i: usize) {
        let names = self.profile_names();
        let (stock, name) = {
            let d = &self.set.devices[i];
            (d.is_default(&names), d.name.clone())
        };
        ui.horizontal(|ui| {
            ui.label(tr("Name"));
            if stock {
                ui.label(name).on_hover_text(tr(
                    "A stock device keeps its name; its mappings are in Mapping below",
                ));
                return;
            }
            if self
                .manager
                .name_draft
                .as_ref()
                .is_none_or(|(d, _)| *d != i)
            {
                self.manager.name_draft = Some((i, name));
            }
            let Some((_, draft)) = &mut self.manager.name_draft else {
                return;
            };
            let field = ui.add(egui::TextEdit::singleline(draft).desired_width(260.0));
            if std::mem::take(&mut self.manager.focus_name) {
                field.request_focus();
            }
            if field.lost_focus() {
                let draft = draft.clone();
                if self.set.devices[i].rename(&draft, &names) {
                    self.save();
                } else if draft.trim() != self.set.devices[i].name {
                    self.message = Some(format!(
                        "\"{}\" cannot be used: the name is empty or a stock model's",
                        draft.trim()
                    ));
                }
                self.manager.name_draft = None;
            }
        });
    }

    fn edit_menu(&mut self, ui: &mut Ui) {
        let mut restore = false;
        let mut remove = None;
        let mut duplicate = None;
        let mut comment = false;
        ui.menu_button(tr("Edit…"), |ui| match self.manager.selected {
            Some(Selected::Native(_)) => {
                if ui.button(tr("Restore default mapping")).clicked() {
                    restore = true;
                    ui.close();
                }
            }
            Some(Selected::Device(i)) => {
                if ui.button(tr("Edit Comment")).clicked() {
                    comment = true;
                    ui.close();
                }
                let has_preset = self
                    .set
                    .owner_of(i)
                    .is_some_and(|o| self.stock_of(&o).is_some());
                if has_preset && ui.button(tr("Restore default mapping")).clicked() {
                    restore = true;
                    ui.close();
                }
                if self.set.devices[i].kind == Kind::Midi
                    && ui
                        .button(tr("Duplicate"))
                        .on_hover_text(tr(
                            "A second device with the same mappings and ports (two units of the same hardware): move one to another In-Port",
                        ))
                        .clicked()
                {
                    duplicate = Some(i);
                    ui.close();
                }
                let names = self.profile_names();
                if !self.set.devices[i].is_default(&names)
                    && ui.button(tr("Delete")).clicked()
                {
                    remove = Some(i);
                    ui.close();
                }
            }
            None => {}
        });
        if comment {
            self.manager.editing_comment = true;
        }
        if let Some(i) = duplicate {
            self.duplicate_device(i);
        }
        if restore {
            self.use_stock();
        }
        if let Some(i) = remove {
            let device = &self.set.devices[i];
            self.manager.ask = Some(modal::Ask::yes_no(
                "Remove device",
                format!(
                    "Do you really want to remove the device: {} from the device list?",
                    device.title()
                ),
                Confirm::RemoveDevice(i),
            ));
        }
    }

    fn duplicate_device(&mut self, i: usize) {
        if let Some(copy) = self.set.duplicate_device(i) {
            self.set.copy_tables(i, copy);
            self.select(Selected::Device(copy));
            self.save();
        }
    }

    fn native_status(&self, ui: &mut Ui, name: &str) {
        let Some(device) = self.devices.iter().find(|d| d.name() == name) else {
            return;
        };
        if device.connected() {
            ui.label(tr("Connected."));
        } else {
            ui.label(RichText::new(tr("Not found (plug it in any time).")).weak());
        }
        for line in device.status() {
            ui.label(RichText::new(line).weak());
        }
        ui.label(RichText::new(tr("Device Target: each side's deck.")).weak());
    }

    fn port_row(&mut self, ui: &mut Ui, i: usize, ctx: &egui::Context) {
        let (in_port, out_port) = {
            let d = &self.set.devices[i];
            (d.in_port.clone(), d.out_port.clone())
        };
        let mut new_in = None;
        let mut new_out = None;
        ui.horizontal(|ui| {
            ui.label(tr("In-Port"));
            new_in = port_combo(
                ui,
                "cm-in-port",
                in_port.as_deref(),
                "All Ports",
                midi::input_ports(),
            )
            .map(PortChoice::port);
            ui.label(tr("Out-Port"));
            new_out = port_combo(
                ui,
                "cm-out-port",
                out_port.as_deref(),
                "None",
                midi::output_ports(),
            )
            .map(PortChoice::port);
        });
        if let Some(port) = new_in
            && port != in_port
        {
            self.set.set_in_port(i, port.clone());
            if let Some(old) = &in_port
                && !self
                    .set
                    .devices
                    .iter()
                    .any(|d| d.in_port.as_deref() == Some(old))
            {
                self.set.midi_inputs.retain(|p| p != old);
                self.ports.retain(|p| p.name != *old);
            }
            if let Some(new) = &port
                && !self.set.midi_inputs.contains(new)
            {
                self.set.midi_inputs.push(new.clone());
                if midi::input_ports().contains(new) {
                    self.connect(new, ctx);
                }
            }
            self.save();
        }
        if let Some(port) = new_out
            && port != out_port
        {
            self.set.set_out_port(i, port.clone());
            if let Some(old) = &out_port
                && !self
                    .set
                    .devices
                    .iter()
                    .any(|d| d.out_port.as_deref() == Some(old))
            {
                self.set.midi_outputs.retain(|p| p != old);
                self.close_output_named(old);
            }
            if let Some(new) = &port
                && !self.set.midi_outputs.contains(new)
            {
                self.set.midi_outputs.push(new.clone());
                if midi::output_ports().contains(new) {
                    self.connect_output(new);
                }
            }
            self.resend_leds();
            self.save();
        }
    }

    fn target_row(&mut self, ui: &mut Ui, i: usize) {
        let label = |t: device::Target| match t {
            None => "Focus".to_owned(),
            Some(d) => format!("Deck {}", deck_letter(d)),
        };
        let mut target = self.set.devices[i].target;
        let before = target;
        ui.horizontal(|ui| {
            ui.label(tr("Device Target"));
            ComboBox::from_id_salt("cm-target")
                .selected_text(label(target))
                .show_ui(ui, |ui| {
                    for t in std::iter::once(None).chain((0..backspn_engine::DECKS).map(Some)) {
                        ui.selectable_value(&mut target, t, label(t));
                    }
                })
                .response
                .on_hover_text(tr(
                    "The deck of the mappings assigned to Device Target: the one with the focus, or a fixed deck",
                ));
        });
        if target != before {
            self.set.devices[i].target = target;
            self.resend_leds();
            self.save();
        }
    }

    fn modifier_state(&self, ui: &mut Ui) {
        let source = match self.manager.selected {
            Some(Selected::Device(i)) => {
                match (self.set.devices[i].kind, &self.set.devices[i].in_port) {
                    (Kind::Keyboard, _) => Some(KEYBOARD.to_owned()),
                    (Kind::Midi, Some(p)) => Some(p.clone()),
                    (Kind::Midi, None) => None,
                }
            }
            _ => None,
        };
        let values = source
            .and_then(|s| self.sources.get(&s))
            .map(|s| s.modifiers.0);
        ui.horizontal(|ui| {
            ui.label(tr("Modifier State"));
            match values {
                Some(values) => {
                    for (i, v) in values.iter().enumerate() {
                        ui.label(RichText::new(format!("M{}={v}", i + 1)).monospace());
                    }
                }
                None => {
                    ui.label(RichText::new("—").weak());
                }
            }
        });
    }

    fn assignment_table(&mut self, ui: &mut Ui) {
        ui.heading(tr("Assignment Table"));
        let rows = self.rows();
        let mut clicked = None;
        let height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        let mut builder = egui_extras::TableBuilder::new(ui)
            .id_salt("cm-table")
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .max_scroll_height(TABLE_HEIGHT)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for i in 0..COLUMNS.len() {
            builder = builder.column(if i + 1 == COLUMNS.len() {
                egui_extras::Column::remainder().at_least(60.0).clip(true)
            } else {
                egui_extras::Column::auto().at_least(40.0).clip(true)
            });
        }
        builder
            .header(20.0, |mut header| {
                for column in COLUMNS {
                    header.col(|ui| {
                        ui.label(RichText::new(tr(column)).strong().small());
                    });
                }
            })
            .body(|body| {
                body.rows(height, rows.len(), |mut row| {
                    let (id, cells, factory) = &rows[row.index()];
                    row.set_selected(self.manager.row == Some(*id));
                    for cell in cells {
                        row.col(|ui| {
                            let text = RichText::new(cell.as_str());
                            ui.add(
                                egui::Label::new(if *factory { text.weak() } else { text })
                                    .selectable(false)
                                    .truncate(),
                            );
                        });
                    }
                    if row.response().clicked() {
                        clicked = Some(*id);
                    }
                });
            });
        if let Some(row) = clicked {
            self.manager.row = Some(row);
            self.learn = None;
        }
        let mut add = None;
        let mut delete = false;
        let mut duplicate = false;
        ui.horizontal(|ui| {
            let editable = self.manager.selected.is_some();
            ui.add_enabled_ui(editable, |ui| {
                if let Some(action) = commands::menu(ui, "Add In…", |_| true) {
                    add = Some((action, false));
                }
                let midi = self
                    .selected_device()
                    .is_some_and(|(_, d)| d.kind == Kind::Midi);
                ui.add_enabled_ui(midi, |ui| {
                    if let Some(action) = commands::menu(ui, "Add Out…", output::supported) {
                        add = Some((action, true));
                    }
                });
            });
            let has_row = self.manager.row.is_some();
            if ui.add_enabled(has_row, Button::new(tr("Delete"))).clicked() {
                delete = true;
            }
            let dup = matches!(self.manager.row, Some(Row::In(_) | Row::Out(_)));
            if ui.add_enabled(dup, Button::new(tr("Duplicate"))).clicked() {
                duplicate = true;
            }
        });
        if let Some((action, output)) = add {
            self.manager.pending = Some(Pending {
                action,
                output,
                focus: action.has_deck(),
            });
            self.manager.row = Some(Row::Pending);
            self.learn = None;
        }
        if delete {
            self.delete_row();
        }
        if duplicate {
            self.duplicate_row();
        }
    }

    fn delete_row(&mut self) {
        if matches!(self.manager.row, Some(Row::In(_) | Row::Out(_))) {
            self.fork_table();
        }
        match self.manager.row.take() {
            Some(Row::In(i)) if i < self.set.mappings.len() => {
                self.set.mappings.remove(i);
                self.save();
            }
            Some(Row::Out(i)) if i < self.set.outputs.len() => {
                self.set.outputs.remove(i);
                self.resend_leds();
                self.save();
            }
            Some(Row::Pending) => self.manager.pending = None,
            _ => {}
        }
        self.learn = None;
    }

    fn duplicate_row(&mut self) {
        if matches!(self.manager.row, Some(Row::In(_) | Row::Out(_))) {
            self.fork_table();
        }
        match self.manager.row {
            Some(Row::In(i)) if i < self.set.mappings.len() => {
                let copy = self.set.mappings[i].clone();
                self.set.mappings.push(copy);
                self.manager.row = Some(Row::In(self.set.mappings.len() - 1));
                self.save();
            }
            Some(Row::Out(i)) if i < self.set.outputs.len() => {
                let copy = self.set.outputs[i].clone();
                self.set.outputs.push(copy);
                self.manager.row = Some(Row::Out(self.set.outputs.len() - 1));
                self.resend_leds();
                self.save();
            }
            _ => {}
        }
    }

    fn row_action(&self) -> Option<(Action, bool)> {
        match self.manager.row? {
            Row::In(i) => self.set.mappings.get(i).map(|m| (m.action, false)),
            Row::Out(i) => self.set.outputs.get(i).map(|o| (o.action, true)),
            Row::Pending => self.manager.pending.as_ref().map(|p| (p.action, p.output)),
        }
    }

    fn device_mapping(&mut self, ui: &mut Ui) {
        ui.heading(tr("Device Mapping"));
        let Some((action, output)) = self.row_action() else {
            ui.label(RichText::new(tr("Select a row, or add a command.")).weak());
            return;
        };
        let mut chosen = None;
        let mut reset = false;
        ui.horizontal(|ui| {
            let learning = self.learn.is_some();
            let label = if learning {
                tr("Waiting… (move a control)")
            } else {
                tr("Learn")
            };
            if ui.add(Button::new(label).selected(learning)).clicked() {
                self.learn = if learning {
                    None
                } else {
                    self.learn_output = output;
                    Some(Learn::new(action))
                };
            }
            ui.label(tr("Mapped to"));
            let current = match self.manager.row {
                Some(Row::In(i)) => self.set.mappings[i].input.to_string(),
                Some(Row::Out(i)) => self.set.outputs[i].output.to_string(),
                _ => "—".to_owned(),
            };
            match self.selected_device().map(|(_, d)| d.kind) {
                Some(Kind::Midi) => chosen = editors::midi_menu(ui, &current),
                Some(Kind::Keyboard) if !output => chosen = editors::key_menu(ui, &current),
                _ => {
                    ui.label(current);
                }
            }
            let has_control = matches!(self.manager.row, Some(Row::In(_) | Row::Out(_)));
            if ui
                .add_enabled(has_control, Button::new(tr("Reset")))
                .on_hover_text(tr("Forgets the control; the command waits for another"))
                .clicked()
            {
                reset = true;
            }
        });
        if let Some(input) = chosen {
            self.assign_input(input);
        }
        if reset {
            self.reset_row();
        }
        let comment: Option<&mut String> = match self.manager.row {
            Some(Row::In(i)) => self.set.mappings.get_mut(i).map(|m| &mut m.comment),
            Some(Row::Out(i)) => self.set.outputs.get_mut(i).map(|o| &mut o.comment),
            _ => None,
        };
        if let Some(comment) = comment {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label(tr("Comment"));
                changed = ui.text_edit_singleline(comment).lost_focus();
            });
            if changed {
                self.fork_table();
                self.save();
            }
        }
        if let Some(Row::In(i)) = self.manager.row
            && matches!(self.manager.selected, Some(Selected::Native(_)))
            && let Some(m) = self.set.mappings.get_mut(i)
            && matches!(m.input, Input::Hid(_))
        {
            let changed = crate::widgets::check(ui, &mut m.override_factory, "Override factory map")
                .on_hover_text(tr(
                    "Only the table maps this control: its built-in function (SHIFT, deck layers, pad modes…) is skipped",
                ))
                .changed();
            if changed {
                self.save();
            }
        }
    }

    fn assign_input(&mut self, input: Input) {
        let learned = Mapping::new(input, Action::Crossfader, Interaction::Direct);
        self.take_learned(learned.input, None);
    }

    fn reset_row(&mut self) {
        if matches!(self.manager.row, Some(Row::In(_) | Row::Out(_))) {
            self.fork_table();
        }
        let pending = match self.manager.row {
            Some(Row::In(i)) if i < self.set.mappings.len() => {
                let m = self.set.mappings.remove(i);
                Some(Pending {
                    action: m.action,
                    output: false,
                    focus: m.focus,
                })
            }
            Some(Row::Out(i)) if i < self.set.outputs.len() => {
                let o = self.set.outputs.remove(i);
                self.resend_leds();
                Some(Pending {
                    action: o.action,
                    output: true,
                    focus: o.focus,
                })
            }
            _ => None,
        };
        if pending.is_some() {
            self.manager.pending = pending;
            self.manager.row = Some(Row::Pending);
            self.learn = None;
            self.save();
        }
    }

    fn take_learned(&mut self, input: Input, interaction: Option<Interaction>) {
        let device = self.selected_device().map(|(_, d)| d.clone());
        match self.manager.row {
            Some(Row::Pending) => {
                let Some(p) = self.manager.pending.take() else {
                    return;
                };
                if p.output {
                    let mut o = OutMapping::new(input, p.action);
                    o.focus = p.focus && p.action.has_deck();
                    o.port = device.as_ref().and_then(|d| d.out_port.clone());
                    o.device = device.as_ref().map(|d| d.id);
                    self.set.outputs.push(o);
                    self.manager.row = Some(Row::Out(self.set.outputs.len() - 1));
                    self.resend_leds();
                } else {
                    let interaction = interaction.unwrap_or_else(|| p.action.default_interaction());
                    let mut m = Mapping::new(input, p.action, interaction);
                    m.focus = p.focus && p.action.has_deck();
                    m.port = device
                        .as_ref()
                        .filter(|d| d.kind == Kind::Midi)
                        .and_then(|d| d.in_port.clone());
                    m.device = device.as_ref().map(|d| d.id);
                    self.set.mappings.push(m);
                    self.manager.row = Some(Row::In(self.set.mappings.len() - 1));
                }
            }
            Some(Row::In(i)) if i < self.set.mappings.len() => {
                let m = &mut self.set.mappings[i];
                if let Some(interaction) = interaction
                    && is_button(&m.input) != is_button(&input)
                {
                    m.interaction = interaction;
                }
                m.input = input;
            }
            Some(Row::Out(i)) if i < self.set.outputs.len() => {
                self.set.outputs[i].output = input;
                self.resend_leds();
            }
            _ => return,
        }
        self.learn = None;
        self.fork_table();
        self.save();
    }

    pub fn learn_from(&mut self, source: &str, event: &InputEvent) -> bool {
        let Some(mut learn) = self.learn else {
            return false;
        };
        let device = self.selected_device().map(|(_, d)| d);
        if !source_of(device, self.selected_native_source(), source) {
            return false;
        }
        let action = learn.action;
        if self.learn_output {
            let Some(o) = output::learn(action, event) else {
                return false;
            };
            self.message = Some(format!("Learned LED {} ← {}", o.output, bare_name(action)));
            self.take_learned(o.output, None);
            return true;
        }
        let Some(learned) = learn.accept(event) else {
            self.learn = Some(learn);
            return false;
        };
        self.message = Some(format!("Learned {} → {}", learned.input, action.label()));
        self.take_learned(learned.input, Some(learned.interaction));
        true
    }

    fn mapping_details(&mut self, ui: &mut Ui) {
        ui.heading(tr("Mapping Details"));
        let changed = match self.manager.row {
            Some(Row::In(i)) if i < self.set.mappings.len() => {
                editors::in_details(ui, &mut self.set.mappings[i])
            }
            Some(Row::Out(i)) if i < self.set.outputs.len() => {
                editors::out_details(ui, &mut self.set.outputs[i])
            }
            Some(Row::Pending) => {
                if let Some(p) = &mut self.manager.pending {
                    editors::assignment(ui, &mut p.action, &mut p.focus);
                }
                false
            }
            _ => false,
        };
        if changed {
            if matches!(self.manager.row, Some(Row::Out(_))) {
                self.resend_leds();
            }
            self.fork_table();
            self.save();
        }
    }
}

#[derive(Clone)]
enum Pick {
    Native(&'static str),
    Device(usize),
    Profile(usize),
}

#[derive(Clone)]
enum PortChoice {
    None,
    Port(String),
}

impl PortChoice {
    fn port(self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Port(p) => Some(p),
        }
    }
}

fn port_combo(
    ui: &mut Ui,
    id: &str,
    current: Option<&str>,
    none: &str,
    mut ports: Vec<String>,
) -> Option<PortChoice> {
    let mut selected_text = current.unwrap_or(none).to_owned();
    if let Some(p) = current
        && !ports.iter().any(|q| q == p)
    {
        ports.push(p.to_owned());
        selected_text = format!("{p} ({})", tr("not connected"));
    }
    let mut entries = vec![Entry::item(PortChoice::None, none, current.is_none())];
    for p in ports {
        let on = current == Some(p.as_str());
        entries.push(Entry::item(PortChoice::Port(p.clone()), p, on));
    }
    search_combo::show(ui, id, selected_text, 200.0, &entries).1
}

pub(super) fn midi_input(channel: u8, kind: MidiKind, number: u8) -> Input {
    MidiId {
        channel,
        kind,
        number,
    }
    .input()
}

fn listed(name: &str, tested: bool) -> String {
    if tested {
        name.to_owned()
    } else {
        format!("{name} {}", tr("(untested)"))
    }
}

fn hint(notes: &str, tested: bool) -> String {
    let untested =
        tr("Not tried with the real device yet: tell us how it goes with Report a Problem…");
    match (notes.is_empty(), tested) {
        (_, true) => notes.to_owned(),
        (true, false) => untested.to_owned(),
        (false, false) => format!("{notes}\n\n{untested}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untested_models_are_marked() {
        assert_eq!(listed("Kontrol S4 MK3", true), "Kontrol S4 MK3");
        assert_eq!(
            listed("Pioneer DJ DDJ-SX", false),
            "Pioneer DJ DDJ-SX (untested)"
        );
        assert_eq!(hint("", true), "");
        assert_eq!(hint("No jogs.", true), "No jogs.");
        assert!(hint("", false).starts_with("Not tried with the real device yet"));
        assert!(hint("No jogs.", false).starts_with("No jogs.\n\nNot tried"));
        assert!(backspn_control::profile::native_tested(
            backspn_control::s4mk3::NAME
        ));
        assert!(!backspn_control::profile::native_tested(
            backspn_control::kontrol::s3::NAME
        ));
        assert!(backspn_control::profile::all().iter().all(|p| !p.tested));
    }

    #[test]
    fn the_page_draws_with_a_row_chosen() {
        let mut c = Controls::detached();
        c.manager.selected = Some(Selected::Device(0));
        let ctx = egui::Context::default();
        for frame in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    c.contents(ui, &ctx);
                });
            });
            out.textures_delta.clear();
            if frame == 0 {
                c.manager.row = Some(Row::In(0));
            }
        }
        assert_eq!(c.manager.row, Some(Row::In(0)));
        assert_eq!(c.set.tables.len(), 1, "the keyboard's table");
    }

    #[test]
    fn without_a_native_controller_none_is_selected() {
        let mut c = Controls::detached();
        assert_eq!(c.selected_native(), None);
        c.manager.selected = Some(Selected::Native("Kontrol S4 MK3"));
        assert_eq!(c.selected_native(), None, "not present: dropped");
    }

    #[test]
    fn factory_rows_are_the_defaults_left_as_they_are() {
        let defaults = vec![Mapping::new(
            Input::Hid("Left.Cue".into()),
            Action::Cue { deck: 0 },
            Interaction::Hold,
        )];
        let mut m = defaults[0].clone();
        assert!(is_factory(&m, &defaults));
        m.override_factory = true;
        assert!(is_factory(&m, &defaults), "override alone changes nothing");
        m.comment = "mine".into();
        assert!(is_factory(&m, &defaults), "a comment neither");
        m.action = Action::Keylock { deck: 0 };
        assert!(!is_factory(&m, &defaults));
        let mut m = defaults[0].clone();
        m.focus = true;
        assert!(!is_factory(&m, &defaults), "edited assignment");
    }

    #[test]
    fn the_control_column_drops_the_deck_channel_and_unit() {
        assert_eq!(bare_name(Action::PlayPause { deck: 1 }), "Play/Pause");
        assert_eq!(bare_name(Action::Volume { channel: 2 }), "Volume");
        assert_eq!(bare_name(Action::FxKnob { unit: 0, knob: 2 }), "Knob 3");
        assert_eq!(bare_name(Action::Crossfader), Action::Crossfader.label());
    }

    #[test]
    fn the_assignment_column() {
        assert_eq!(
            assignment_label(Action::PlayPause { deck: 2 }, true),
            "Device Target"
        );
        assert_eq!(
            assignment_label(Action::PlayPause { deck: 2 }, false),
            "Deck C"
        );
        assert_eq!(
            assignment_label(Action::Volume { channel: 1 }, false),
            "Deck B"
        );
        assert_eq!(
            assignment_label(Action::FxOn { unit: 3 }, true),
            "FX Unit 4"
        );
        assert_eq!(assignment_label(Action::Crossfader, true), "Global");
    }

    #[test]
    fn rows_show_their_columns() {
        let mut m = Mapping::new(
            midi_input(0, MidiKind::Note, 60),
            Action::Cue { deck: 0 },
            Interaction::Hold,
        );
        m.gate
            .conditions
            .push(backspn_control::modifier::Condition {
                modifier: 0,
                value: 1,
            });
        m.comment = "pad".into();
        assert_eq!(
            in_cells(&m),
            [
                "Cue",
                "In",
                "Deck A",
                "Hold",
                "Ch01 Note C3",
                "M1=1",
                "",
                "pad"
            ]
            .map(str::to_owned)
        );
        let mut o = OutMapping::new(
            midi_input(1, MidiKind::Cc, 7),
            Action::Volume { channel: 0 },
        );
        o.focus = true;
        o.states.push(backspn_control::state::StateCondition {
            control: backspn_control::state::Control::Playing,
            channel: Some(1),
            value: 1,
        });
        let cells = out_cells(&o);
        assert_eq!(
            &cells[..5],
            ["Volume", "Out", "Device Target", "Output", "Ch02 CC 007"]
        );
        assert_eq!(cells[5], "B.Play=1");
    }

    #[test]
    fn learn_takes_only_the_selected_devices_events() {
        let keyboard = Device::keyboard();
        assert!(source_of(Some(&keyboard), None, KEYBOARD));
        assert!(!source_of(Some(&keyboard), None, "DDJ-FLX4"));
        let flx4 = Device::midi("x", Some("DDJ-FLX4".into()), None);
        assert!(source_of(Some(&flx4), None, "DDJ-FLX4"));
        assert!(!source_of(Some(&flx4), None, "Other"));
        let any = Device::midi("x", None, None);
        assert!(source_of(Some(&any), None, "Other"));
        assert!(!source_of(Some(&any), None, KEYBOARD));
        assert!(source_of(None, Some("s4mk3"), "s4mk3"));
        assert!(!source_of(None, None, "s4mk3"));
    }

    #[test]
    fn a_learned_control_goes_to_the_pending_command_with_the_devices_port() {
        let mut c = Controls::detached();
        c.set.sync_devices();
        c.set
            .devices
            .push(Device::midi("x", Some("P".into()), Some("Q".into())));
        c.manager.selected = Some(Selected::Device(1));
        c.manager.pending = Some(Pending {
            action: Action::PlayPause { deck: 0 },
            output: false,
            focus: true,
        });
        c.manager.row = Some(Row::Pending);
        c.learn = Some(Learn::new(Action::PlayPause { deck: 0 }));
        let note = InputEvent {
            input: midi_input(0, MidiKind::Note, 60),
            value: 1.0,
            raw: 127,
            pressed: Some(true),
        };
        assert!(!c.learn_from("Other", &note), "another port");
        assert!(c.learn_from("P", &note));
        let m = c.set.mappings.last().unwrap();
        assert_eq!((m.port.as_deref(), m.focus), (Some("P"), true));
        assert_eq!(c.manager.row, Some(Row::In(c.set.mappings.len() - 1)));
        assert!(c.manager.pending.is_none() && c.learn.is_none());
        c.reset_row();
        assert_eq!(c.manager.row, Some(Row::Pending));
        assert!(c.manager.pending.as_ref().is_some_and(|p| p.focus));
    }
}
