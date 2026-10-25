use backspn_control::controller::Controller;
use backspn_control::device::{self, Kind};
use backspn_control::table::{self, For, Owner, Rows, TableFile};
use backspn_control::{Input, MappingSet};
use eframe::egui::{self, Ui};

use super::Controls;
use super::manager::{Confirm, Selected};
use crate::i18n::tr;
use crate::modal;
use crate::search_combo::{self, Entry};

pub(super) fn owns_fn<'a>(
    devices: &'a [Box<dyn Controller>],
    owner: &Owner,
) -> Box<dyn Fn(&Input) -> bool + 'a> {
    match owner {
        Owner::Native(name) => {
            let device = devices.iter().find(|d| d.name() == name);
            Box::new(move |i| device.is_some_and(|d| d.owns_input(i)))
        }
        Owner::Device(_) => Box::new(table::no_native),
    }
}

impl Controls {
    pub(super) fn profile_names(&self) -> Vec<String> {
        self.profiles
            .iter()
            .map(backspn_control::profile::Profile::name)
            .collect()
    }

    pub(super) fn sync_tables(&mut self) {
        let names = self.profile_names();
        self.set.sync_tables(&names);
        for d in &self.devices {
            let owner = Owner::Native(d.name().to_owned());
            if self.set.tables.iter().any(|t| t.owner == owner) {
                continue;
            }
            let owns = |i: &Input| d.owns_input(i);
            self.set
                .sync_native_table(d.name(), &d.default_mappings(), &owns);
        }
    }

    pub(super) fn selected_owner(&self) -> Option<Owner> {
        match self.manager.selected {
            Some(Selected::Native(name)) => Some(Owner::Native(name.to_owned())),
            Some(Selected::Device(i)) => self.set.owner_of(i),
            None => None,
        }
    }

    pub(super) fn stock_of(&self, owner: &Owner) -> Option<(String, Rows)> {
        match owner {
            Owner::Native(name) => {
                let d = self.devices.iter().find(|d| d.name() == name)?;
                Some((
                    name.clone(),
                    Rows {
                        mappings: d.default_mappings(),
                        ..Rows::default()
                    },
                ))
            }
            Owner::Device(id) => {
                let d = self.set.devices.iter().find(|d| d.id == *id)?;
                if d.kind == Kind::Keyboard {
                    return Some((
                        device::KEYBOARD.to_owned(),
                        Rows {
                            mappings: MappingSet::default_keyboard(),
                            ..Rows::default()
                        },
                    ));
                }
                let p = self.profiles.iter().find(|p| p.name() == d.name)?;
                Some((
                    p.name(),
                    Rows {
                        mappings: p.mappings.clone(),
                        outputs: p.outputs.clone(),
                        ..Rows::default()
                    },
                ))
            }
        }
    }

    fn tables_changed(&mut self) {
        self.manager.row = None;
        self.manager.pending = None;
        self.manager.table_draft = None;
        self.learn = None;
        self.resend_leds();
        self.save();
    }

    pub(super) fn fork_table(&mut self) {
        if let Some(owner) = self.selected_owner() {
            self.set.fork_table(&owner);
        }
    }

    pub(super) fn use_stock(&mut self) {
        let Some(owner) = self.selected_owner() else {
            return;
        };
        self.use_stock_of(&owner);
    }

    pub(super) fn use_stock_of(&mut self, owner: &Owner) {
        let Some((name, rows)) = self.stock_of(owner) else {
            return;
        };
        let owns = owns_fn(&self.devices, owner);
        self.set.use_stock(owner.clone(), &name, rows, &owns);
        drop(owns);
        self.tables_changed();
    }

    pub(super) fn mapping_row(&mut self, ui: &mut Ui) {
        let Some(owner) = self.selected_owner() else {
            return;
        };
        let indices = self.set.tables_of(&owner);
        let active = self.set.active_table(&owner);
        let stock = self.stock_of(&owner).map(|(name, _)| name);
        let stock_missing = stock.as_ref().filter(|name| {
            !indices.iter().any(|&i| {
                let t = &self.set.tables[i];
                t.untouched && t.stock.as_ref() == Some(*name)
            })
        });
        let mut choose = None;
        let mut want_stock = false;
        let mut duplicate = false;
        let mut import = false;
        let mut export = false;
        let mut delete = false;
        ui.horizontal(|ui| {
            ui.label(tr("Mapping"));
            let title = active.map_or_else(|| "—".to_owned(), |i| self.set.tables[i].name.clone());
            let mut entries: Vec<Entry<Option<usize>>> = indices
                .iter()
                .map(|&i| {
                    let t = &self.set.tables[i];
                    Entry::item(Some(i), t.name.clone(), t.active)
                })
                .collect();
            if let Some(name) = stock_missing {
                entries.push(Entry::Separator);
                entries.push(Entry::Heading(tr("Stock mapping").to_owned()));
                entries.push(Entry::item(None, name.clone(), false));
            }
            let (response, pick) = search_combo::show(ui, "cm-table", title, 260.0, &entries);
            response.on_hover_text(tr(
                "What the device's controls do; the one chosen is in use, the others are kept",
            ));
            match pick {
                Some(Some(i)) => choose = Some(i),
                Some(None) => want_stock = true,
                None => {}
            }
            duplicate = ui
                .button(tr("Duplicate"))
                .on_hover_text(tr("A copy of this mapping to change without losing it"))
                .clicked();
            import = ui
                .button(tr("Import…"))
                .on_hover_text(tr(
                    "Adds a mapping from a file (one exported from this app, or a .tsi); yours stay",
                ))
                .clicked();
            export = ui
                .button(tr("Export…"))
                .on_hover_text(tr("Saves this mapping to a file, to use it on another computer"))
                .clicked();
            let deletable = active.is_some_and(|i| self.set.can_delete_table(i));
            delete = ui
                .add_enabled(deletable, egui::Button::new(tr("Delete")))
                .on_disabled_hover_text(tr(
                    "The stock mapping and a device's only mapping stay",
                ))
                .clicked();
        });
        if let Some(i) = active {
            self.table_name_row(ui, i);
        }
        if let Some(i) = choose {
            let owns = owns_fn(&self.devices, &owner);
            self.set.activate_table(i, &owns);
            drop(owns);
            self.tables_changed();
        }
        if want_stock {
            self.use_stock_of(&owner);
        }
        if duplicate && let Some(i) = active {
            let owns = owns_fn(&self.devices, &owner);
            self.set.duplicate_table(i, &owns);
            drop(owns);
            self.tables_changed();
            self.manager.focus_table_name = true;
        }
        if import {
            self.import_table(&owner);
        }
        if export && let Some(i) = active {
            self.export_table(i, &owner);
        }
        if delete && let Some(i) = active {
            self.manager.ask = Some(modal::Ask::yes_no(
                "Delete mapping",
                format!(
                    "Do you really want to delete the mapping: {}?",
                    self.set.tables[i].name
                ),
                Confirm::DeleteTable(i),
            ));
        }
    }

    pub(super) fn delete_table(&mut self, i: usize) {
        let Some(owner) = self.set.tables.get(i).map(|t| t.owner.clone()) else {
            return;
        };
        let owns = owns_fn(&self.devices, &owner);
        self.set.delete_table(i, &owns);
        drop(owns);
        self.tables_changed();
    }

    fn table_name_row(&mut self, ui: &mut Ui, i: usize) {
        let (untouched, name) = {
            let t = &self.set.tables[i];
            (t.untouched, t.name.clone())
        };
        ui.horizontal(|ui| {
            ui.label(tr("Mapping name"));
            if untouched {
                ui.label(name).on_hover_text(tr(
                    "The stock mapping keeps its name; change a row or Duplicate it to get your own",
                ));
                return;
            }
            if self
                .manager
                .table_draft
                .as_ref()
                .is_none_or(|(t, _)| *t != i)
            {
                self.manager.table_draft = Some((i, name));
            }
            let Some((_, draft)) = &mut self.manager.table_draft else {
                return;
            };
            let field = ui.add(egui::TextEdit::singleline(draft).desired_width(260.0));
            if std::mem::take(&mut self.manager.focus_table_name) {
                field.request_focus();
            }
            if field.lost_focus() {
                let draft = draft.clone();
                if self.set.rename_table(i, &draft) {
                    self.save();
                } else if draft.trim() != self.set.tables[i].name {
                    self.message = Some(tr("A mapping needs a name").to_owned());
                }
                self.manager.table_draft = None;
            }
        });
    }

    fn export_table(&mut self, i: usize, owner: &Owner) {
        let owns = owns_fn(&self.devices, owner);
        let text = self.set.export_table(i, &owns);
        drop(owns);
        let Some(text) = text else {
            return;
        };
        let name = self.set.tables[i].name.clone();
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Mapping", &[table::EXTENSION])
            .set_file_name(table::file_name(&name))
            .save_file()
        else {
            return;
        };
        self.message = Some(match std::fs::write(&path, text) {
            Ok(()) => format!("Exported {name} to {}", path.display()),
            Err(e) => format!("Cannot write {}: {e}", path.display()),
        });
    }

    fn import_table(&mut self, owner: &Owner) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Mapping", &[table::EXTENSION, "tsi"])
            .pick_file()
        else {
            return;
        };
        match read_tables(&path) {
            Ok(files) => self.add_imported(owner, files),
            Err(e) => self.message = Some(format!("Cannot read {}: {e}", path.display())),
        }
    }

    pub(super) fn add_imported(&mut self, owner: &Owner, files: Vec<TableFile>) {
        let Some(kind) = self.set.kind_of(owner) else {
            return;
        };
        let found: Vec<For> = files.iter().map(|f| f.target.clone()).collect();
        let Some(file) = files.into_iter().find(|f| f.target == kind) else {
            self.message = Some(format!(
                "The file has no mapping for this device (it has: {})",
                found.iter().map(for_name).collect::<Vec<_>>().join(", ")
            ));
            return;
        };
        let (name, rows, skipped) = (
            file.name.clone(),
            file.mappings.len() + file.outputs.len(),
            file.skipped,
        );
        let owns = owns_fn(&self.devices, owner);
        let result = self.set.import_table(owner.clone(), file, &owns);
        drop(owns);
        if result.is_ok() {
            self.message = Some(format!(
                "Added the mapping {name}: {rows} rows ({skipped} unsupported skipped)"
            ));
            self.tables_changed();
        }
    }
}

pub(super) fn read_tables(path: &std::path::Path) -> Result<Vec<TableFile>, String> {
    let tsi = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("tsi"));
    if tsi {
        let tsi = backspn_formats::tsi::Tsi::from_path(path).map_err(|e| e.to_string())?;
        return Ok(tsi.devices.iter().map(table::from_tsi).collect());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(vec![table::parse(&text)?])
}

fn for_name(target: &For) -> String {
    match target {
        For::Keyboard => device::KEYBOARD.to_owned(),
        For::Midi => "MIDI".to_owned(),
        For::Native(name) => name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_ours_and_a_tsi_both_give_tables() {
        let dir = std::env::temp_dir().join(format!("bs-tables-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut set = MappingSet::default();
        set.devices
            .push(device::Device::midi("Knobs", Some("K".into()), None));
        set.sync_tables(&[]);
        let i = set.active_table(&set.owner_of(1).unwrap()).unwrap();
        let ours = dir.join("knobs.bsmap");
        std::fs::write(&ours, set.export_table(i, &table::no_native).unwrap()).unwrap();
        let files = read_tables(&ours).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].target, For::Midi);
        assert!(read_tables(&dir.join("missing.bsmap")).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn targets_read_in_words() {
        assert_eq!(for_name(&For::Midi), "MIDI");
        assert_eq!(for_name(&For::Native("S4".into())), "S4");
    }
}
