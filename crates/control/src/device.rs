use serde::{Deserialize, Serialize};

use crate::output::OutMapping;
use crate::{Input, Mapping, MappingSet, profile};

pub type Target = Option<usize>;

pub const KEYBOARD: &str = "Generic Keyboard";
pub const GENERIC_MIDI: &str = "Generic MIDI";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Keyboard,
    Midi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Target,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(id: &u32) -> bool {
    *id == 0
}

pub const COPY_SUFFIX: &str = " copy";

impl Device {
    pub fn keyboard() -> Self {
        Self {
            id: 0,
            name: KEYBOARD.to_owned(),
            preset: None,
            kind: Kind::Keyboard,
            in_port: None,
            out_port: None,
            target: None,
            comment: String::new(),
        }
    }

    pub fn midi(
        name: impl Into<String>,
        in_port: Option<String>,
        out_port: Option<String>,
    ) -> Self {
        Self {
            id: 0,
            name: name.into(),
            preset: None,
            kind: Kind::Midi,
            in_port,
            out_port,
            target: None,
            comment: String::new(),
        }
    }

    pub fn owns(&self, m: &Mapping) -> bool {
        if let Some(id) = m.device {
            return id == self.id && self.id != 0;
        }
        self.owns_by_port(m)
    }

    fn owns_by_port(&self, m: &Mapping) -> bool {
        match (self.kind, &m.input) {
            (Kind::Keyboard, Input::Key(_)) => true,
            (Kind::Midi, Input::Midi { .. } | Input::MidiPair { .. }) => m.port == self.in_port,
            _ => false,
        }
    }

    pub fn owns_output(&self, o: &OutMapping) -> bool {
        if let Some(id) = o.device {
            return id == self.id && self.id != 0;
        }
        self.owns_output_by_port(o)
    }

    fn owns_output_by_port(&self, o: &OutMapping) -> bool {
        self.kind == Kind::Midi && o.port == self.out_port
    }

    pub fn is_default(&self, profiles: &[String]) -> bool {
        match self.kind {
            Kind::Keyboard => true,
            Kind::Midi => profiles.contains(&self.name),
        }
    }

    pub fn rename(&mut self, name: &str, profiles: &[String]) -> bool {
        let name = name.trim();
        if name.is_empty() || self.is_default(profiles) || profiles.iter().any(|p| p == name) {
            return false;
        }
        name.clone_into(&mut self.name);
        true
    }

    pub fn title(&self) -> String {
        if self.comment.is_empty() {
            self.name.clone()
        } else {
            format!("{} - {}", self.name, self.comment)
        }
    }
}

pub fn name_for_port(port: Option<&str>) -> String {
    match port {
        None => GENERIC_MIDI.to_owned(),
        Some(p) => profile::for_port(p).map_or_else(|| p.to_owned(), |profile| profile.name()),
    }
}

fn target_of(devices: &[Device], kind: Kind, port: Option<&str>) -> Target {
    devices
        .iter()
        .find(|d| d.kind == kind && (kind == Kind::Keyboard || d.in_port.as_deref() == port))
        .and_then(|d| d.target)
}

impl MappingSet {
    pub fn sync_devices(&mut self) {
        if !self.devices.iter().any(|d| d.kind == Kind::Keyboard) {
            self.devices.insert(0, Device::keyboard());
        }
        let in_ports: Vec<Option<String>> = self
            .mappings
            .iter()
            .filter(|m| matches!(m.input, Input::Midi { .. } | Input::MidiPair { .. }))
            .map(|m| m.port.clone())
            .collect();
        for port in dedup(in_ports) {
            if !self.midi_devices().any(|d| d.in_port == port) {
                let out = self
                    .outputs
                    .iter()
                    .any(|o| o.port == port)
                    .then(|| port.clone())
                    .flatten();
                self.devices
                    .push(Device::midi(name_for_port(port.as_deref()), port, out));
            }
        }
        let out_ports: Vec<Option<String>> = self.outputs.iter().map(|o| o.port.clone()).collect();
        for port in dedup(out_ports) {
            if self.midi_devices().any(|d| d.out_port == port) {
                continue;
            }
            if let Some(d) = self
                .devices
                .iter_mut()
                .find(|d| d.kind == Kind::Midi && d.in_port == port && d.out_port.is_none())
            {
                d.out_port = port;
            } else {
                self.devices.push(Device::midi(
                    name_for_port(port.as_deref()),
                    port.clone(),
                    port,
                ));
            }
        }
        self.assign_ids();
    }

    fn assign_ids(&mut self) {
        let mut next = self.devices.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        for d in &mut self.devices {
            if d.id == 0 {
                d.id = next;
                next += 1;
            }
        }
        for m in &mut self.mappings {
            if m.device.is_none() {
                m.device = self
                    .devices
                    .iter()
                    .find(|d| d.owns_by_port(m))
                    .map(|d| d.id);
            }
        }
        for o in &mut self.outputs {
            if o.device.is_none() {
                o.device = self
                    .devices
                    .iter()
                    .find(|d| d.owns_output_by_port(o))
                    .map(|d| d.id);
            }
        }
    }

    pub fn duplicate_device(&mut self, index: usize) -> Option<usize> {
        self.sync_devices();
        let mut copy = self.devices.get(index)?.clone();
        copy.id = self.devices.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        copy.name.push_str(COPY_SUFFIX);
        let source = self.devices[index].clone();
        let mappings: Vec<Mapping> = self
            .mappings
            .iter()
            .filter(|m| source.owns(m))
            .map(|m| Mapping {
                device: Some(copy.id),
                ..m.clone()
            })
            .collect();
        let outputs: Vec<OutMapping> = self
            .outputs
            .iter()
            .filter(|o| source.owns_output(o))
            .map(|o| OutMapping {
                device: Some(copy.id),
                ..o.clone()
            })
            .collect();
        self.mappings.extend(mappings);
        self.outputs.extend(outputs);
        self.devices.push(copy);
        Some(self.devices.len() - 1)
    }

    fn midi_devices(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|d| d.kind == Kind::Midi)
    }

    pub fn target_deck(&self, m: &Mapping, focus: usize) -> usize {
        let kind = match m.input {
            Input::Key(_) => Kind::Keyboard,
            _ => Kind::Midi,
        };
        target_of(&self.devices, kind, m.port.as_deref()).unwrap_or(focus)
    }

    pub fn output_target_deck(&self, o: &OutMapping, focus: usize) -> usize {
        self.midi_devices()
            .find(|d| d.out_port == o.port)
            .and_then(|d| d.target)
            .unwrap_or(focus)
    }

    pub fn add_device(&mut self, device: Device) {
        self.devices
            .retain(|d| !(d.kind == Kind::Midi && d.in_port == device.in_port));
        self.devices.push(device);
    }

    pub fn remove_device(&mut self, index: usize) {
        if index >= self.devices.len() {
            return;
        }
        let device = self.devices.remove(index);
        match device.kind {
            Kind::Keyboard => self.mappings.retain(|m| !matches!(m.input, Input::Key(_))),
            Kind::Midi => {
                self.mappings.retain(|m| !device.owns(m));
                self.outputs.retain(|o| !device.owns_output(o));
                if let Some(out) = &device.out_port {
                    self.port_messages.retain(|m| m.port != *out);
                }
            }
        }
    }

    pub fn set_in_port(&mut self, index: usize, port: Option<String>) {
        let Some(device) = self.devices.get(index) else {
            return;
        };
        if device.kind != Kind::Midi || device.in_port == port {
            return;
        }
        let device = device.clone();
        for m in &mut self.mappings {
            if device.owns(m) {
                m.port.clone_from(&port);
            }
        }
        self.devices[index].in_port = port;
    }

    pub fn set_out_port(&mut self, index: usize, port: Option<String>) {
        let Some(device) = self.devices.get(index) else {
            return;
        };
        if device.kind != Kind::Midi || device.out_port == port {
            return;
        }
        let old = device.out_port.clone();
        let device = device.clone();
        for o in &mut self.outputs {
            if device.owns_output(o) {
                o.port.clone_from(&port);
            }
        }
        if let (Some(old), Some(new)) = (&old, &port) {
            for m in &mut self.port_messages {
                if m.port == *old {
                    m.port.clone_from(new);
                }
            }
        }
        self.devices[index].out_port = port;
    }
}

fn dedup<T: PartialEq>(items: Vec<T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_users_own_devices_can_be_renamed() {
        let profiles = vec!["Brand Model".to_owned()];
        let mut mine = Device::midi(GENERIC_MIDI, None, None);
        assert!(mine.rename("  Knob box ", &profiles));
        assert_eq!(mine.name, "Knob box");
        assert!(!mine.rename("   ", &profiles), "empty");
        assert!(
            !mine.rename("Brand Model", &profiles),
            "a stock model's name"
        );
        assert_eq!(mine.name, "Knob box");
        let mut stock = Device::midi("Brand Model", None, None);
        assert!(!stock.rename("Other", &profiles));
        assert!(!Device::keyboard().rename("Keys", &profiles));
    }

    #[test]
    fn only_the_users_own_devices_can_be_deleted() {
        let profiles = vec!["Brand Model".to_owned()];
        assert!(Device::keyboard().is_default(&profiles));
        let model = Device::midi("Brand Model", Some("P".into()), None);
        assert!(model.is_default(&profiles), "a model as it came");
        let mut copy = model.clone();
        copy.name.push_str(COPY_SUFFIX);
        assert!(!copy.is_default(&profiles));
        assert!(!Device::midi(GENERIC_MIDI, None, None).is_default(&profiles));
    }

    use super::*;
    use crate::{Action, Interaction, MidiKind};

    fn midi(number: u8, port: Option<&str>, deck: usize) -> Mapping {
        Mapping {
            port: port.map(str::to_owned),
            ..Mapping::new(
                Input::Midi {
                    channel: 0,
                    kind: MidiKind::Note,
                    number,
                },
                Action::PlayPause { deck },
                Interaction::Trigger,
            )
        }
    }

    fn output(number: u8, port: Option<&str>) -> OutMapping {
        OutMapping {
            port: port.map(str::to_owned),
            ..OutMapping::new(
                Input::Midi {
                    channel: 0,
                    kind: MidiKind::Note,
                    number,
                },
                Action::PlayPause { deck: 0 },
            )
        }
    }

    #[test]
    fn devices_get_ids_and_their_mappings_follow_them_not_the_port() {
        let mut set = MappingSet {
            mappings: vec![midi(60, Some("DDJ-FLX4"), 0), midi(61, Some("DDJ-FLX4"), 1)],
            outputs: vec![output(60, Some("DDJ-FLX4"))],
            ..MappingSet::default()
        };
        set.sync_devices();
        let ddj = set.devices[1].clone();
        assert_ne!(ddj.id, 0);
        assert_eq!(set.mappings[0].device, Some(ddj.id));
        assert_eq!(set.outputs[0].device, Some(ddj.id));
        let copy = set.duplicate_device(1).unwrap();
        let copy_device = set.devices[copy].clone();
        assert_eq!(copy_device.name, "AlphaTheta DDJ-FLX4 copy");
        assert_eq!(copy_device.in_port, ddj.in_port);
        assert_ne!(copy_device.id, ddj.id);
        assert_eq!(set.mappings.len(), 4);
        assert_eq!(set.outputs.len(), 2);
        assert_eq!(set.mappings.iter().filter(|m| ddj.owns(m)).count(), 2);
        assert_eq!(
            set.mappings.iter().filter(|m| copy_device.owns(m)).count(),
            2
        );
        set.set_in_port(copy, Some("Other".into()));
        assert_eq!(set.mappings.iter().filter(|m| ddj.owns(m)).count(), 2);
        assert!(
            set.mappings
                .iter()
                .filter(|m| m.device == Some(copy_device.id))
                .all(|m| m.port.as_deref() == Some("Other"))
        );
        assert!(
            set.mappings
                .iter()
                .filter(|m| m.device == Some(ddj.id))
                .all(|m| m.port.as_deref() == Some("DDJ-FLX4"))
        );
        set.remove_device(copy);
        assert_eq!(set.mappings.len(), 2);
        assert_eq!(set.outputs.len(), 1);
    }

    #[test]
    fn devices_come_from_the_ports_of_mappings_and_outputs() {
        let mut set = MappingSet {
            mappings: vec![
                Mapping::new(
                    Input::Key("W".into()),
                    Action::PlayPause { deck: 0 },
                    Interaction::Trigger,
                ),
                midi(60, Some("DDJ-FLX4"), 0),
                midi(61, None, 0),
            ],
            outputs: vec![output(60, Some("DDJ-FLX4")), output(62, Some("Lights"))],
            ..MappingSet::default()
        };
        set.sync_devices();
        let names: Vec<(String, Option<String>, Option<String>)> = set
            .devices
            .iter()
            .map(|d| (d.name.clone(), d.in_port.clone(), d.out_port.clone()))
            .collect();
        assert_eq!(
            names,
            [
                (KEYBOARD.to_owned(), None, None),
                (
                    "AlphaTheta DDJ-FLX4".to_owned(),
                    Some("DDJ-FLX4".to_owned()),
                    Some("DDJ-FLX4".to_owned())
                ),
                (GENERIC_MIDI.to_owned(), None, None),
                (
                    "Lights".to_owned(),
                    Some("Lights".to_owned()),
                    Some("Lights".to_owned())
                ),
            ]
        );
        set.sync_devices();
        assert_eq!(set.devices.len(), 4);
        assert!(set.devices[1].owns(&set.mappings[1]));
        assert!(!set.devices[1].owns(&set.mappings[2]));
        assert!(set.devices[2].owns(&set.mappings[2]));
        assert!(set.devices[0].owns(&set.mappings[0]));
        assert!(set.devices[1].owns_output(&set.outputs[0]));
    }

    #[test]
    fn the_device_target_resolves_focus_mappings() {
        let mut set = MappingSet {
            mappings: vec![midi(60, Some("X"), 0), midi(60, Some("Y"), 0)],
            outputs: vec![output(60, Some("X"))],
            ..MappingSet::default()
        };
        set.sync_devices();
        set.devices[1].target = Some(2);
        assert_eq!(set.target_deck(&set.mappings[0], 1), 2, "deck C");
        assert_eq!(set.target_deck(&set.mappings[1], 1), 1, "Focus");
        assert_eq!(set.output_target_deck(&set.outputs[0], 1), 2);
    }

    #[test]
    fn ports_move_with_their_device() {
        let mut set = MappingSet {
            mappings: vec![midi(60, Some("X"), 0)],
            outputs: vec![output(60, Some("X"))],
            port_messages: vec![crate::session::PortMessages {
                port: "X".into(),
                ..Default::default()
            }],
            ..MappingSet::default()
        };
        set.sync_devices();
        set.set_in_port(1, None);
        assert_eq!(set.mappings[0].port, None);
        set.set_out_port(1, Some("Z".into()));
        assert_eq!(set.outputs[0].port.as_deref(), Some("Z"));
        assert_eq!(set.port_messages[0].port, "Z");
        set.remove_device(1);
        assert!(set.mappings.is_empty() && set.outputs.is_empty());
        assert_eq!(set.port_messages.len(), 0);
    }
}
