use std::path::{Path, PathBuf};

use backspn_engine::fx_pattern::{PATTERNS, RESOLUTIONS, pattern_index, pitch_semitones};
use eframe::egui::{self, RichText, Ui};

use super::Prefs;
use super::search;
use crate::fx_view::resolution_label;
use crate::i18n::tr;
use crate::kits::{EXTENSION, Kit, KitLibrary, OLD_EXTENSION};
use crate::modal::{self, Ask};
use crate::{theme, widgets};

#[derive(Debug, Default)]
pub(super) struct KitEditor {
    selected: Option<String>,
    pattern: usize,
    error: Option<String>,
    delete_ask: Option<Ask<String>>,
}

impl Prefs {
    pub(super) fn drum_machine(&mut self, ui: &mut Ui, kits: &mut KitLibrary) {
        search::heading(ui, "Kits");
        self.kit_toolbar(ui, kits);
        if let Some(e) = &self.kit.error {
            ui.label(RichText::new(e).color(theme::get(ui.ctx()).danger));
        }
        ui.add_space(6.0);
        let selected = self
            .kit
            .selected
            .clone()
            .filter(|id| kits.find(id).is_some())
            .or_else(|| kits.kits().first().map(|k| k.id.clone()));
        self.kit.selected.clone_from(&selected);
        let Some(id) = selected else {
            ui.label(RichText::new(tr("No kits yet: make one with New.")).weak());
            return;
        };
        let Some(mut kit) = kits.find(&id).map(|k| k.kit.clone()) else {
            return;
        };
        let before = kit.clone();
        kit_details(ui, &mut kit);
        ui.add_space(8.0);
        self.kit_sounds(ui, &id, &mut kit, kits);
        ui.add_space(8.0);
        self.kit_grid(ui, &mut kit);
        if kit != before
            && let Err(e) = kits.save(&id, &kit)
        {
            self.kit.error = Some(e.to_string());
        }
    }

    fn kit_toolbar(&mut self, ui: &mut Ui, kits: &mut KitLibrary) {
        if let Some(id) = modal::show(ui.ctx(), &mut self.kit.delete_ask) {
            if let Err(e) = kits.delete(&id) {
                self.kit.error = Some(e.to_string());
            }
            self.kit.selected = None;
        }
        let result = ui
            .horizontal(|ui| -> anyhow::Result<()> {
                let current = self
                    .kit
                    .selected
                    .as_ref()
                    .and_then(|id| kits.find(id))
                    .map_or_else(|| "—".to_owned(), |k| k.kit.name.clone());
                egui::ComboBox::from_id_salt("kit-pick")
                    .width(180.0)
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for k in kits.kits() {
                            let on = self.kit.selected.as_ref() == Some(&k.id);
                            if ui.selectable_label(on, &k.kit.name).clicked() {
                                self.kit.selected = Some(k.id.clone());
                            }
                        }
                    });
                if ui.button(tr("New")).clicked() {
                    self.kit.selected = Some(kits.create("New kit")?);
                }
                let selected = self.kit.selected.clone();
                ui.add_enabled_ui(selected.is_some(), |ui| -> anyhow::Result<()> {
                    let Some(id) = selected else { return Ok(()) };
                    if ui.button(tr("Duplicate")).clicked() {
                        self.kit.selected = Some(kits.duplicate(&id)?);
                    }
                    if ui.button(tr("Delete")).clicked() {
                        self.kit.delete_ask = Some(ask_delete(kits, &id));
                    }
                    if ui.button(tr("Export…")).clicked()
                        && let Some(to) = rfd::FileDialog::new()
                            .add_filter("Kit", &[EXTENSION])
                            .set_file_name(format!("{id}.{EXTENSION}"))
                            .save_file()
                    {
                        kits.export(&id, &to)?;
                    }
                    Ok(())
                })
                .inner?;
                if ui.button(tr("Import…")).clicked()
                    && let Some(from) = rfd::FileDialog::new()
                        .add_filter("Kit", &[EXTENSION, OLD_EXTENSION])
                        .pick_file()
                {
                    self.kit.selected = Some(kits.import(&from)?);
                }
                Ok(())
            })
            .inner;
        match result {
            Ok(()) => {}
            Err(e) => self.kit.error = Some(e.to_string()),
        }
    }
}

fn kit_details(ui: &mut Ui, kit: &mut Kit) {
    egui::Grid::new("kit-details")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            search::title(ui, "Name");
            ui.text_edit_singleline(&mut kit.name);
            ui.end_row();
            search::title(ui, "Author");
            ui.text_edit_singleline(&mut kit.author);
            ui.end_row();
            let [decay, pitch, pattern, volume] = &mut kit.knobs;
            search::title(ui, "Decay when picked");
            ui.add(egui::Slider::new(decay, 0.0..=1.0).show_value(false));
            ui.end_row();
            search::title(ui, "Pitch when picked");
            ui.add(
                egui::Slider::new(pitch, 0.0..=1.0)
                    .custom_formatter(|v, _| format!("{:+.1} st", pitch_semitones(v as f32))),
            );
            ui.end_row();
            search::title(ui, "Pattern when picked");
            let mut n = pattern_index(*pattern) + 1;
            if ui
                .add(egui::DragValue::new(&mut n).range(1..=PATTERNS).prefix("P"))
                .changed()
            {
                *pattern = (n as f32 - 0.5) / PATTERNS as f32;
            }
            ui.end_row();
            search::title(ui, "Volume when picked");
            ui.add(egui::Slider::new(volume, 0.0..=1.0).show_value(false));
            ui.end_row();
        });
}

impl Prefs {
    fn kit_sounds(&mut self, ui: &mut Ui, id: &str, kit: &mut Kit, kits: &mut KitLibrary) {
        search::heading(ui, "Sounds");
        let mut remove = None;
        egui::Grid::new("kit-sounds")
            .num_columns(5)
            .spacing([10.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for (i, sound) in kit.sounds.iter_mut().enumerate() {
                    ui.add_sized([130.0, 20.0], egui::TextEdit::singleline(&mut sound.name));
                    ui.label(RichText::new(&sound.file).weak());
                    ui.add(
                        egui::DragValue::new(&mut sound.level_db)
                            .range(-24.0..=12.0)
                            .speed(0.1)
                            .suffix(" dB"),
                    )
                    .on_hover_text(tr("Level"));
                    if ui
                        .button("▶")
                        .on_hover_text(tr("Preview in the headphones"))
                        .clicked()
                    {
                        match kits.preview(id, &sound.file) {
                            Ok(track) => self.preview = Some(track),
                            Err(e) => self.kit.error = Some(e.to_string()),
                        }
                    }
                    if ui
                        .button("×")
                        .on_hover_text(tr("Remove the sound"))
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if ui.button(tr("Add sounds…")).clicked()
            && let Some(files) = rfd::FileDialog::new()
                .add_filter("Audio", crate::AUDIO_EXTENSIONS)
                .pick_files()
        {
            self.add_sounds(id, kit, &files, kits);
        }
        if let Some(i) = remove {
            match kits.remove_sound(id, i) {
                Ok(()) => *kit = kits.find(id).map_or_else(|| kit.clone(), |k| k.kit.clone()),
                Err(e) => self.kit.error = Some(e.to_string()),
            }
        }
    }

    fn add_sounds(&mut self, id: &str, kit: &mut Kit, files: &[PathBuf], kits: &mut KitLibrary) {
        if let Err(e) = kits.save(id, kit) {
            self.kit.error = Some(e.to_string());
            return;
        }
        for file in files {
            if let Err(e) = kits.add_sound(id, file) {
                self.kit.error = Some(format!("{}: {e}", file_label(file)));
            }
        }
        if let Some(k) = kits.find(id) {
            kit.clone_from(&k.kit);
        }
    }

    fn kit_grid(&mut self, ui: &mut Ui, kit: &mut Kit) {
        search::heading(ui, "Patterns");
        ui.horizontal(|ui| {
            for n in 0..PATTERNS {
                ui.selectable_value(&mut self.kit.pattern, n, format!("P{}", n + 1));
            }
        });
        let n = self.kit.pattern.min(PATTERNS - 1);
        let accent = theme::get(ui.ctx()).accent;
        egui::Grid::new("kit-grid")
            .num_columns(4)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                for sound in &mut kit.sounds {
                    ui.label(&sound.name);
                    let steps = &mut sound.patterns[n];
                    if let Some(i) =
                        widgets::step_row(ui, steps.bits, steps.cells(), None, accent, 560.0)
                    {
                        let on = !steps.on(i);
                        steps.set(i, on);
                    }
                    if ui.small_button(tr("Clear")).clicked() {
                        steps.bits = 0;
                    }
                    let mut per_beat = steps.per_beat;
                    egui::ComboBox::from_id_salt(("kit-resolution", &sound.file, n))
                        .width(56.0)
                        .selected_text(resolution_label(per_beat))
                        .show_ui(ui, |ui| {
                            for r in RESOLUTIONS {
                                ui.selectable_value(&mut per_beat, r, resolution_label(r));
                            }
                        })
                        .response
                        .on_hover_text(tr("Steps of a beat: the grid and what ROLL repeats at"));
                    if per_beat != steps.per_beat {
                        *steps = steps.rescaled(per_beat);
                    }
                    ui.end_row();
                }
            });
        if kit.sounds.is_empty() {
            ui.label(RichText::new(tr("Add sounds to write their patterns.")).weak());
        }
    }
}

fn file_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn ask_delete(kits: &KitLibrary, id: &str) -> Ask<String> {
    let name = kits.find(id).map_or(id, |k| k.kit.name.as_str());
    Ask::yes_no(
        "Delete kit",
        format!("Delete the kit “{name}” and its sound files?"),
        id.to_owned(),
    )
}
