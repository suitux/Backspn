use serde::{Deserialize, Serialize};

use crate::device::{Device, Kind};
use crate::output::OutMapping;
use crate::{Input, Mapping, MappingSet, PairRange};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Owner {
    Device(u32),
    Native(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub id: u32,
    pub owner: Owner,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub untouched: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub active: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<Mapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<OutMapping>,
}

pub const CUSTOM_SUFFIX: &str = " (custom)";
pub const COPY_SUFFIX: &str = " copy";

pub type Owns<'a> = &'a dyn Fn(&Input) -> bool;

pub fn no_native(_: &Input) -> bool {
    false
}

impl MappingSet {
    fn device_by_id(&self, id: u32) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn owner_of(&self, index: usize) -> Option<Owner> {
        self.devices.get(index).map(|d| Owner::Device(d.id))
    }

    pub fn tables_of(&self, owner: &Owner) -> Vec<usize> {
        (0..self.tables.len())
            .filter(|&i| self.tables[i].owner == *owner)
            .collect()
    }

    pub fn active_table(&self, owner: &Owner) -> Option<usize> {
        self.tables
            .iter()
            .position(|t| t.owner == *owner && t.active)
    }

    fn next_table_id(&self) -> u32 {
        self.tables.iter().map(|t| t.id).max().unwrap_or(0) + 1
    }

    fn is_live(&self, owner: &Owner, m: &Mapping, owns: Owns) -> bool {
        match owner {
            Owner::Device(id) => self.device_by_id(*id).is_some_and(|d| d.owns(m)),
            Owner::Native(_) => owns(&m.input),
        }
    }

    fn is_live_output(&self, owner: &Owner, o: &OutMapping) -> bool {
        match owner {
            Owner::Device(id) => self.device_by_id(*id).is_some_and(|d| d.owns_output(o)),
            Owner::Native(_) => false,
        }
    }

    fn take_live(&mut self, owner: &Owner, owns: Owns) -> (Vec<Mapping>, Vec<OutMapping>) {
        let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.mappings)
            .into_iter()
            .partition(|m| self.is_live(owner, m, owns));
        self.mappings = rest;
        let (outs, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.outputs)
            .into_iter()
            .partition(|o| self.is_live_output(owner, o));
        self.outputs = rest;
        (mine, outs)
    }

    fn live_rows(&self, owner: &Owner, owns: Owns) -> (Vec<Mapping>, Vec<OutMapping>) {
        (
            self.mappings
                .iter()
                .filter(|m| self.is_live(owner, m, owns))
                .cloned()
                .collect(),
            self.outputs
                .iter()
                .filter(|o| self.is_live_output(owner, o))
                .cloned()
                .collect(),
        )
    }

    fn put_live(&mut self, owner: &Owner, mappings: Vec<Mapping>, outputs: Vec<OutMapping>) {
        let device = match owner {
            Owner::Device(id) => self.device_by_id(*id).cloned(),
            Owner::Native(_) => None,
        };
        for mut m in mappings {
            if let Some(d) = &device {
                m.device = Some(d.id);
                if d.kind == Kind::Midi {
                    m.port.clone_from(&d.in_port);
                }
            } else {
                m.device = None;
            }
            self.mappings.push(m);
        }
        if let Some(d) = device.filter(|d| d.kind == Kind::Midi) {
            for mut o in outputs {
                o.device = Some(d.id);
                o.port.clone_from(&d.out_port);
                self.outputs.push(o);
            }
        }
    }

    pub fn sync_tables(&mut self, profiles: &[String]) {
        self.sync_devices();
        let ids: Vec<u32> = self.devices.iter().map(|d| d.id).collect();
        self.tables
            .retain(|t| !matches!(t.owner, Owner::Device(id) if !ids.contains(&id)));
        for i in 0..self.devices.len() {
            let owner = Owner::Device(self.devices[i].id);
            if self.tables.iter().any(|t| t.owner == owner) {
                continue;
            }
            let device = &mut self.devices[i];
            let (name, stock, untouched) = match device.kind {
                Kind::Keyboard => {
                    let stock = crate::device::KEYBOARD.to_owned();
                    let keys: Vec<Mapping> = self
                        .mappings
                        .iter()
                        .filter(|m| matches!(m.input, Input::Key(_)))
                        .cloned()
                        .collect();
                    let untouched = same_rows(&keys, &MappingSet::default_keyboard());
                    let name = if untouched {
                        stock.clone()
                    } else {
                        format!("{stock}{CUSTOM_SUFFIX}")
                    };
                    (name, Some(stock), untouched)
                }
                Kind::Midi => match device.preset.take() {
                    Some(model) => {
                        let name = std::mem::replace(&mut device.name, model.clone());
                        (name, Some(model), false)
                    }
                    None if profiles.contains(&device.name) => {
                        (device.name.clone(), Some(device.name.clone()), true)
                    }
                    None => (device.name.clone(), None, false),
                },
            };
            let id = self.next_table_id();
            self.tables.push(Table {
                id,
                owner,
                name,
                stock,
                untouched,
                active: true,
                mappings: Vec::new(),
                outputs: Vec::new(),
            });
        }
    }

    pub fn sync_native_table(&mut self, name: &str, defaults: &[Mapping], owns: Owns) {
        let owner = Owner::Native(name.to_owned());
        if self.tables.iter().any(|t| t.owner == owner) {
            return;
        }
        let (live, _) = self.live_rows(&owner, owns);
        let untouched = same_rows(&live, defaults);
        let id = self.next_table_id();
        self.tables.push(Table {
            id,
            owner,
            name: if untouched {
                name.to_owned()
            } else {
                format!("{name}{CUSTOM_SUFFIX}")
            },
            stock: Some(name.to_owned()),
            untouched,
            active: true,
            mappings: Vec::new(),
            outputs: Vec::new(),
        });
    }

    pub fn activate_table(&mut self, index: usize, owns: Owns) {
        let Some(table) = self.tables.get(index) else {
            return;
        };
        if table.active {
            return;
        }
        let owner = table.owner.clone();
        let (mappings, outputs) = self.take_live(&owner, owns);
        if let Some(old) = self.active_table(&owner) {
            let old = &mut self.tables[old];
            old.active = false;
            old.mappings = mappings;
            old.outputs = outputs;
        }
        let table = &mut self.tables[index];
        table.active = true;
        let mappings = std::mem::take(&mut table.mappings);
        let outputs = std::mem::take(&mut table.outputs);
        self.put_live(&owner, mappings, outputs);
    }

    pub fn add_table(&mut self, owner: Owner, name: &str, rows: Rows, owns: Owns) -> usize {
        let id = self.next_table_id();
        self.tables.push(Table {
            id,
            owner,
            name: name.to_owned(),
            stock: rows.stock,
            untouched: rows.untouched,
            active: false,
            mappings: rows.mappings,
            outputs: rows.outputs,
        });
        let index = self.tables.len() - 1;
        self.activate_table(index, owns);
        index
    }

    pub fn use_stock(&mut self, owner: Owner, name: &str, rows: Rows, owns: Owns) -> usize {
        let found = self
            .tables
            .iter()
            .position(|t| t.owner == owner && t.untouched && t.stock.as_deref() == Some(name));
        match found {
            Some(i) => {
                self.activate_table(i, owns);
                i
            }
            None => self.add_table(
                owner,
                name,
                Rows {
                    stock: Some(name.to_owned()),
                    untouched: true,
                    ..rows
                },
                owns,
            ),
        }
    }

    pub fn duplicate_table(&mut self, index: usize, owns: Owns) -> Option<usize> {
        let table = self.tables.get(index)?.clone();
        let (mappings, outputs) = if table.active {
            self.live_rows(&table.owner, owns)
        } else {
            (table.mappings, table.outputs)
        };
        let rows = Rows {
            mappings,
            outputs,
            stock: table.stock,
            untouched: false,
        };
        Some(self.add_table(
            table.owner,
            &format!("{}{COPY_SUFFIX}", table.name),
            rows,
            owns,
        ))
    }

    pub fn can_delete_table(&self, index: usize) -> bool {
        self.tables.get(index).is_some_and(|t| {
            !t.untouched && self.tables.iter().filter(|o| o.owner == t.owner).count() > 1
        })
    }

    pub fn delete_table(&mut self, index: usize, owns: Owns) -> bool {
        if !self.can_delete_table(index) {
            return false;
        }
        let table = self.tables.remove(index);
        if table.active {
            self.take_live(&table.owner, owns);
            if let Some(next) = self.tables.iter().position(|t| t.owner == table.owner) {
                self.activate_table(next, owns);
            }
        }
        true
    }

    pub fn rename_table(&mut self, index: usize, name: &str) -> bool {
        let name = name.trim();
        match self.tables.get_mut(index) {
            Some(t) if !t.untouched && !name.is_empty() => {
                name.clone_into(&mut t.name);
                true
            }
            _ => false,
        }
    }

    pub fn fork_table(&mut self, owner: &Owner) {
        if let Some(i) = self.active_table(owner) {
            let t = &mut self.tables[i];
            if t.untouched {
                t.untouched = false;
                t.name.push_str(CUSTOM_SUFFIX);
            }
        }
    }

    pub fn copy_tables(&mut self, index: usize, copy: usize) {
        let (Some(from), Some(to)) = (self.owner_of(index), self.owner_of(copy)) else {
            return;
        };
        self.tables.retain(|t| t.owner != to);
        let mut next = self.next_table_id();
        let copies: Vec<Table> = self
            .tables
            .iter()
            .filter(|t| t.owner == from)
            .map(|t| {
                next += 1;
                Table {
                    id: next - 1,
                    owner: to.clone(),
                    ..t.clone()
                }
            })
            .collect();
        self.tables.extend(copies);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rows {
    pub mappings: Vec<Mapping>,
    pub outputs: Vec<OutMapping>,
    pub stock: Option<String>,
    pub untouched: bool,
}

fn same_rows(a: &[Mapping], b: &[Mapping]) -> bool {
    a.len() == b.len()
        && b.iter()
            .all(|d| a.iter().any(|m| m.input == d.input && m.action == d.action))
}

pub const EXTENSION: &str = "bsmap";
const FORMAT: &str = "backspn-mapping";
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum For {
    Keyboard,
    Midi,
    Native(String),
}

#[derive(Serialize, Deserialize)]
struct Raw {
    format: String,
    version: u32,
    #[serde(rename = "for")]
    target: For,
    #[serde(default)]
    model: String,
    name: String,
    #[serde(default)]
    mappings: Vec<serde_json::Value>,
    #[serde(default)]
    outputs: Vec<serde_json::Value>,
    #[serde(default)]
    pair_ranges: Vec<PairRange>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableFile {
    pub target: For,
    pub model: String,
    pub name: String,
    pub mappings: Vec<Mapping>,
    pub outputs: Vec<OutMapping>,
    pub pair_ranges: Vec<PairRange>,
    pub skipped: usize,
}

pub fn file_name(name: &str) -> String {
    let base: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let base = base.trim().trim_start_matches('.');
    format!(
        "{}.{EXTENSION}",
        if base.is_empty() { "mapping" } else { base }
    )
}

pub fn parse(text: &str) -> Result<TableFile, String> {
    let raw: Raw = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if raw.format != FORMAT {
        return Err("not a mapping file".to_owned());
    }
    if raw.version > VERSION {
        return Err(format!(
            "made by a newer version (format {}; this one reads {VERSION})",
            raw.version
        ));
    }
    let total = raw.mappings.len() + raw.outputs.len();
    let mappings: Vec<Mapping> = raw
        .mappings
        .into_iter()
        .filter_map(|m| serde_json::from_value(m).ok())
        .collect();
    let outputs: Vec<OutMapping> = raw
        .outputs
        .into_iter()
        .filter_map(|o| serde_json::from_value(o).ok())
        .collect();
    Ok(TableFile {
        skipped: total - mappings.len() - outputs.len(),
        target: raw.target,
        model: raw.model,
        name: raw.name,
        mappings,
        outputs,
        pair_ranges: raw.pair_ranges,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    For(For),
}

pub fn from_tsi(device: &backspn_formats::tsi::Device) -> TableFile {
    let kind = device.name.to_ascii_lowercase();
    let target = if kind.contains("keyboard") {
        For::Keyboard
    } else if kind.contains("s4 mk3") {
        For::Native(crate::s4mk3::NAME.to_owned())
    } else {
        For::Midi
    };
    let mut import = crate::tsi_import::import(device);
    for m in &mut import.mappings {
        m.port = None;
        m.device = None;
    }
    for o in &mut import.outputs {
        o.port = None;
        o.device = None;
    }
    TableFile {
        target,
        model: device.name.clone(),
        name: if device.comment.is_empty() {
            device.name.clone()
        } else {
            device.comment.clone()
        },
        mappings: import.mappings,
        outputs: import.outputs,
        pair_ranges: import.pair_ranges,
        skipped: import.skipped,
    }
}

impl MappingSet {
    pub fn kind_of(&self, owner: &Owner) -> Option<For> {
        match owner {
            Owner::Native(name) => Some(For::Native(name.clone())),
            Owner::Device(id) => self.device_by_id(*id).map(|d| match d.kind {
                Kind::Keyboard => For::Keyboard,
                Kind::Midi => For::Midi,
            }),
        }
    }

    pub fn export_table(&self, index: usize, owns: Owns) -> Option<String> {
        let table = self.tables.get(index)?;
        let target = self.kind_of(&table.owner)?;
        let model = match &table.owner {
            Owner::Native(name) => name.clone(),
            Owner::Device(id) => self.device_by_id(*id)?.name.clone(),
        };
        let (mut mappings, mut outputs) = if table.active {
            self.live_rows(&table.owner, owns)
        } else {
            (table.mappings.clone(), table.outputs.clone())
        };
        for m in &mut mappings {
            m.port = None;
            m.device = None;
        }
        for o in &mut outputs {
            o.port = None;
            o.device = None;
        }
        let pair_ranges = self
            .pair_ranges
            .iter()
            .filter(|r| {
                mappings.iter().any(|m| {
                    matches!(m.input, Input::MidiPair { lsb, msb } if lsb == r.lsb && msb == r.msb)
                })
            })
            .copied()
            .collect();
        let raw = Raw {
            format: FORMAT.to_owned(),
            version: VERSION,
            target,
            model,
            name: table.name.clone(),
            mappings: mappings
                .iter()
                .filter_map(|m| serde_json::to_value(m).ok())
                .collect(),
            outputs: outputs
                .iter()
                .filter_map(|o| serde_json::to_value(o).ok())
                .collect(),
            pair_ranges,
        };
        serde_json::to_string_pretty(&raw).ok()
    }

    pub fn import_table(
        &mut self,
        owner: Owner,
        file: TableFile,
        owns: Owns,
    ) -> Result<usize, Mismatch> {
        if self.kind_of(&owner).as_ref() != Some(&file.target) {
            return Err(Mismatch::For(file.target));
        }
        for r in file.pair_ranges {
            self.set_pair_range(r);
        }
        let rows = Rows {
            mappings: file.mappings,
            outputs: file.outputs,
            ..Rows::default()
        };
        Ok(self.add_table(owner, &file.name, rows, owns))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, Interaction, MidiId, MidiKind};

    fn note(n: u8) -> Input {
        Input::Midi {
            channel: 0,
            kind: MidiKind::Note,
            number: n,
        }
    }

    fn cc(n: u8) -> MidiId {
        MidiId {
            channel: 0,
            kind: MidiKind::Cc,
            number: n,
        }
    }

    fn row(input: Input, action: Action) -> Mapping {
        Mapping::new(input, action, Interaction::Trigger)
    }

    fn on(port: &str, input: Input, action: Action) -> Mapping {
        Mapping {
            port: Some(port.to_owned()),
            ..row(input, action)
        }
    }

    const MODEL: &str = "Brand Model";

    fn profiles() -> Vec<String> {
        vec![MODEL.to_owned()]
    }

    fn set() -> MappingSet {
        let mut set = MappingSet {
            mappings: MappingSet::default_keyboard(),
            ..MappingSet::default()
        };
        set.devices.push(Device::keyboard());
        set.devices
            .push(Device::midi(MODEL, Some("P".into()), Some("P".into())));
        set.devices.push(Device::midi(
            "Generic MIDI",
            Some("G".into()),
            Some("G".into()),
        ));
        set.mappings
            .push(on("P", note(1), Action::PlayPause { deck: 0 }));
        set.mappings.push(on("G", note(1), Action::Cue { deck: 1 }));
        set.mappings.push(on(
            "G",
            Input::MidiPair {
                lsb: cc(33),
                msb: cc(1),
            },
            Action::Volume { channel: 0 },
        ));
        set.pair_ranges.push(PairRange {
            lsb: cc(33),
            msb: cc(1),
            min: 0.0,
            max: 1023.0,
        });
        set.outputs.push(OutMapping {
            port: Some("G".into()),
            ..OutMapping::new(note(5), Action::Cue { deck: 1 })
        });
        set.sync_tables(&profiles());
        set
    }

    fn owner(set: &MappingSet, i: usize) -> Owner {
        set.owner_of(i).unwrap()
    }

    fn live(set: &MappingSet, i: usize) -> usize {
        let d = &set.devices[i];
        set.mappings.iter().filter(|m| d.owns(m)).count()
    }

    #[test]
    fn every_device_gets_its_table() {
        let set = set();
        assert_eq!(set.tables.len(), 3);
        let keyboard = &set.tables[set.active_table(&owner(&set, 0)).unwrap()];
        assert!(keyboard.untouched, "the keys as they came");
        let model = &set.tables[set.active_table(&owner(&set, 1)).unwrap()];
        assert!(model.untouched);
        assert_eq!(model.name, MODEL);
        let generic = &set.tables[set.active_table(&owner(&set, 2)).unwrap()];
        assert!(!generic.untouched);
        assert_eq!(generic.stock, None);
    }

    #[test]
    fn a_profile_copy_from_before_tables_becomes_its_table() {
        let mut set = MappingSet::default();
        let mut d = Device::midi(format!("{MODEL}{CUSTOM_SUFFIX}"), Some("P".into()), None);
        d.preset = Some(MODEL.into());
        set.devices.push(d);
        set.sync_tables(&profiles());
        let d = set.devices.iter().find(|d| d.kind == Kind::Midi).unwrap();
        assert_eq!(d.name, MODEL, "the device is the model again");
        assert_eq!(d.preset, None);
        let t = &set.tables[set.active_table(&Owner::Device(d.id)).unwrap()];
        assert_eq!(t.name, format!("{MODEL}{CUSTOM_SUFFIX}"));
        assert_eq!(t.stock.as_deref(), Some(MODEL));
        assert!(!t.untouched);
    }

    #[test]
    fn choosing_a_table_swaps_the_rows() {
        let mut set = set();
        let generic = owner(&set, 2);
        let first = set.active_table(&generic).unwrap();
        let rows = Rows {
            mappings: vec![row(note(9), Action::Sync { deck: 0 })],
            ..Rows::default()
        };
        let second = set.add_table(generic.clone(), "Other", rows, &no_native);
        assert_eq!(set.active_table(&generic), Some(second));
        assert_eq!(live(&set, 2), 1);
        let m = set
            .mappings
            .iter()
            .find(|m| m.action == Action::Sync { deck: 0 })
            .unwrap();
        assert_eq!(m.port.as_deref(), Some("G"), "on the device's port");
        assert!(set.outputs.is_empty(), "the LED stays with its table");
        assert_eq!(set.tables[first].mappings.len(), 2);

        set.activate_table(first, &no_native);
        assert_eq!(live(&set, 2), 2);
        assert_eq!(set.outputs.len(), 1);
        assert_eq!(set.tables[second].mappings.len(), 1);
        assert_eq!(live(&set, 1), 1, "other devices untouched");
    }

    #[test]
    fn a_table_goes_to_another_computer_and_stays_beside_the_others() {
        let from = set();
        let t = from.active_table(&owner(&from, 2)).unwrap();
        let text = from.export_table(t, &no_native).unwrap();
        let file = parse(&text).unwrap();
        assert_eq!(file.target, For::Midi);
        assert_eq!(file.mappings.len(), 2);
        assert!(file.mappings.iter().all(|m| m.port.is_none()));
        assert_eq!(file.outputs.len(), 1);
        assert_eq!(file.pair_ranges.len(), 1);

        let mut to = MappingSet::default();
        to.devices.push(Device::midi(
            "Generic MIDI",
            Some("X".into()),
            Some("X".into()),
        ));
        to.mappings.push(on("X", note(3), Action::Sync { deck: 1 }));
        to.sync_tables(&profiles());
        let i = to
            .devices
            .iter()
            .position(|d| d.kind == Kind::Midi)
            .unwrap();
        let own = to.active_table(&owner(&to, i)).unwrap();
        let got = to.import_table(owner(&to, i), file, &no_native).unwrap();
        assert_eq!(to.active_table(&owner(&to, i)), Some(got));
        assert_eq!(live(&to, i), 2);
        assert!(
            to.mappings
                .iter()
                .all(|m| m.port.as_deref() == Some("X") || matches!(m.input, Input::Key(_)))
        );
        assert_eq!(
            to.tables[own].mappings.len(),
            1,
            "the friend's own table stays"
        );
        assert_eq!(to.pair_ranges.len(), 1);
    }

    #[test]
    fn a_native_table_goes_only_to_that_controller() {
        let hid = |n: &str| Input::Hid(n.to_owned());
        let owns = |i: &Input| matches!(i, Input::Hid(_));
        let defaults = vec![row(hid("Left.Play"), Action::PlayPause { deck: 0 })];
        let mut from = MappingSet {
            mappings: vec![row(hid("Left.Play"), Action::Sync { deck: 0 })],
            ..MappingSet::default()
        };
        from.sync_native_table("S4", &defaults, &owns);
        let s4 = Owner::Native("S4".into());
        let t = from.active_table(&s4).unwrap();
        assert!(!from.tables[t].untouched, "changed from its defaults");
        let text = from.export_table(t, &owns).unwrap();

        let mut to = MappingSet {
            mappings: defaults.clone(),
            ..MappingSet::default()
        };
        to.sync_native_table("S4", &defaults, &owns);
        to.sync_tables(&[]);
        let mine = to.active_table(&s4).unwrap();
        assert!(to.tables[mine].untouched);
        let keyboard = to.owner_of(0).unwrap();
        assert_eq!(
            to.import_table(keyboard, parse(&text).unwrap(), &owns),
            Err(Mismatch::For(For::Native("S4".into())))
        );
        let other = Owner::Native("S2".into());
        assert!(
            to.import_table(other, parse(&text).unwrap(), &owns)
                .is_err()
        );
        let got = to
            .import_table(s4.clone(), parse(&text).unwrap(), &owns)
            .unwrap();
        assert_eq!(to.active_table(&s4), Some(got));
        assert!(
            to.mappings
                .iter()
                .any(|m| m.action == Action::Sync { deck: 0 })
        );
        assert!(
            !to.mappings
                .iter()
                .any(|m| m.action == Action::PlayPause { deck: 0 })
        );
        to.activate_table(mine, &owns);
        assert!(
            to.mappings
                .iter()
                .any(|m| m.action == Action::PlayPause { deck: 0 })
        );
    }

    #[test]
    fn stock_tables_fork_on_the_first_change_and_come_back() {
        let mut set = set();
        let model = owner(&set, 1);
        let t = set.active_table(&model).unwrap();
        assert!(!set.rename_table(t, "Mine"), "stock as it came");
        assert!(!set.can_delete_table(t));
        set.fork_table(&model);
        assert_eq!(set.tables[t].name, format!("{MODEL}{CUSTOM_SUFFIX}"));
        assert!(set.rename_table(t, " Mine "));
        assert_eq!(set.tables[t].name, "Mine");
        assert!(!set.rename_table(t, "  "));

        let stock_rows = Rows {
            mappings: vec![row(note(1), Action::PlayPause { deck: 0 })],
            ..Rows::default()
        };
        let stock = set.use_stock(model.clone(), MODEL, stock_rows.clone(), &no_native);
        assert_ne!(stock, t, "the user's table stays");
        assert!(set.tables[stock].untouched);
        assert_eq!(
            set.use_stock(model.clone(), MODEL, stock_rows, &no_native),
            stock
        );
        assert_eq!(set.tables_of(&model).len(), 2);
    }

    #[test]
    fn duplicate_and_delete() {
        let mut set = set();
        let generic = owner(&set, 2);
        let t = set.active_table(&generic).unwrap();
        assert!(!set.can_delete_table(t), "the only table");
        let copy = set.duplicate_table(t, &no_native).unwrap();
        assert_eq!(set.tables[copy].name, "Generic MIDI copy");
        assert_eq!(set.active_table(&generic), Some(copy));
        assert_eq!(live(&set, 2), 2);
        assert_eq!(set.tables[t].mappings.len(), 2);
        assert!(set.delete_table(copy, &no_native));
        assert_eq!(set.active_table(&generic), Some(t), "the other takes over");
        assert_eq!(live(&set, 2), 2);
    }

    #[test]
    fn a_copied_device_takes_its_tables() {
        let mut set = set();
        let generic = owner(&set, 2);
        let t = set.active_table(&generic).unwrap();
        set.duplicate_table(t, &no_native);
        let copy = set.duplicate_device(2).unwrap();
        set.copy_tables(2, copy);
        assert_eq!(set.tables_of(&owner(&set, copy)).len(), 2);
        set.remove_device(copy);
        set.sync_tables(&profiles());
        assert_eq!(set.tables.len(), 4, "the removed device's tables go");
    }

    #[test]
    fn a_tsi_device_gives_a_table_for_its_kind() {
        let mut d = backspn_formats::tsi::Device {
            name: "Generic MIDI".into(),
            comment: "Knobs".into(),
            ..Default::default()
        };
        let t = from_tsi(&d);
        assert_eq!(t.target, For::Midi);
        assert_eq!(t.name, "Knobs");
        d.name = "Generic Keyboard".into();
        assert_eq!(from_tsi(&d).target, For::Keyboard);
        d.name = "Traktor Kontrol S4 MK3".into();
        assert_eq!(from_tsi(&d).target, For::Native(crate::s4mk3::NAME.into()));
    }

    #[test]
    fn tables_survive_saving_with_unknown_rows_skipped() {
        let mut set = set();
        let generic = owner(&set, 2);
        let t = set.active_table(&generic).unwrap();
        set.duplicate_table(t, &no_native);
        let text = serde_json::to_string(&set).unwrap();
        let (back, skipped) = MappingSet::from_json(&text).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(back, set);
        let (back, skipped) =
            MappingSet::from_json(&text.replacen("\"Cue\"", "\"SomethingNew\"", 1)).unwrap();
        assert_eq!(skipped, 1);
        assert_eq!(back.tables.len(), set.tables.len(), "the table stays");
    }

    #[test]
    fn the_file_name_is_the_table_name() {
        assert_eq!(file_name("Xavi's S4"), "Xavi's S4.bsmap");
        assert_eq!(file_name("A/B: C?"), "A-B- C-.bsmap");
        assert_eq!(file_name(" .. "), "mapping.bsmap");
    }

    #[test]
    fn other_files_are_refused_and_unknown_rows_skipped() {
        assert!(parse("{}").is_err());
        assert!(parse(r#"{"format":"x","version":1,"for":"Midi","name":"a"}"#).is_err());
        assert!(
            parse(r#"{"format":"backspn-mapping","version":9,"for":"Midi","name":"a"}"#).is_err()
        );
        let set = set();
        let t = set.active_table(&owner(&set, 2)).unwrap();
        let text = set.export_table(t, &no_native).unwrap();
        let file = parse(&text.replacen("\"Cue\"", "\"SomethingNew\"", 1)).unwrap();
        assert_eq!(file.skipped, 1);
    }
}

#[cfg(test)]
mod real_data {
    #[test]
    #[ignore = "reads BACKSPN_MAPPINGS_COPY"]
    fn the_users_file_gets_tables() {
        let path = std::env::var("BACKSPN_MAPPINGS_COPY").unwrap();
        let (mut set, skipped) = crate::MappingSet::load(std::path::Path::new(&path)).unwrap();
        let rows = set.mappings.len();
        let names: Vec<String> = crate::profile::all()
            .iter()
            .map(crate::profile::Profile::name)
            .collect();
        set.sync_tables(&names);
        let owns = |i: &crate::Input| matches!(i, crate::Input::Hid(_));
        set.sync_native_table(crate::s4mk3::NAME, &crate::s4mk3::default_mappings(), &owns);
        for t in &set.tables {
            println!(
                "{:?} {:?} stock={:?} untouched={} active={}",
                t.owner, t.name, t.stock, t.untouched, t.active
            );
        }
        println!("skipped {skipped}, rows {rows} -> {}", set.mappings.len());
        assert_eq!(rows, set.mappings.len());
        let text = serde_json::to_string(&set).unwrap();
        let (back, _) = crate::MappingSet::from_json(&text).unwrap();
        assert_eq!(back, set);
    }
}
