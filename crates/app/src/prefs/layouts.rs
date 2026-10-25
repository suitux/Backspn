use eframe::egui::{self, RichText, Ui};

use super::Settings;
use super::search;
use crate::i18n::tr;
use crate::layouts::{self, DeckSetup, NameError, ScreenLayout};
use crate::modal::{self, Ask};

#[derive(Debug, Default)]
pub(super) struct LayoutEditor {
    new_name: String,
    new_error: Option<NameError>,
    renaming: Option<(usize, String)>,
    rename_error: Option<NameError>,
    delete_ask: Option<Ask<String>>,
    pub scroll_to: bool,
}

enum RowAction {
    Load,
    Update,
    Rename,
    Move(isize),
    Delete,
}

impl LayoutEditor {
    pub(super) fn ui(&mut self, ui: &mut Ui, s: &mut Settings, setup: &DeckSetup) -> Option<usize> {
        if let Some(name) = modal::show(ui.ctx(), &mut self.delete_ask) {
            layouts::remove(&mut s.layouts, &name);
            self.renaming = None;
        }
        let heading = search::heading(ui, "Layouts");
        if std::mem::take(&mut self.scroll_to) {
            heading.scroll_to_me(Some(egui::Align::TOP));
        }
        ui.label(
            RichText::new(tr(
                "A layout keeps the whole set-up but the tracks and the mixer: the decks with their \
                 type, size and sample set, the FX units with their mode and effects, the \
                 crossfader, the browser's parts and the waveform's look. Save it as it is now, \
                 then switch layouts from the selector in the top bar.",
            ))
            .weak(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            search::title(ui, "Save the screen as:");
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text(tr("Layout name"))
                    .desired_width(180.0),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button(tr("Save")).clicked() || enter {
                let screen = ScreenLayout::of(s);
                match layouts::add(&mut s.layouts, &self.new_name, screen, setup.clone()) {
                    Ok(_) => {
                        self.new_name.clear();
                        self.new_error = None;
                    }
                    Err(e) => self.new_error = Some(e),
                }
            }
        });
        if let Some(e) = self.new_error {
            ui.label(RichText::new(tr(e.message())).color(ui.visuals().error_fg_color));
        }
        ui.add_space(6.0);
        if s.layouts.is_empty() {
            ui.label(RichText::new(tr("No layout saved yet.")).weak());
            return None;
        }
        let load = self.table(ui, s, setup);
        if let Some(e) = self.rename_error {
            ui.label(RichText::new(tr(e.message())).color(ui.visuals().error_fg_color));
        }
        load
    }

    fn table(&mut self, ui: &mut Ui, s: &mut Settings, setup: &DeckSetup) -> Option<usize> {
        let active = layouts::active(&s.layouts, &ScreenLayout::of(s), setup);
        let last = s.layouts.len() - 1;
        let mut action = None;
        egui::Grid::new("saved-layouts")
            .striped(true)
            .num_columns(3)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.label(RichText::new(tr("Name")).strong());
                ui.label(RichText::new(tr("Decks")).strong());
                ui.label(RichText::new(tr("Actions")).strong());
                ui.end_row();
                for (i, layout) in s.layouts.iter().enumerate() {
                    self.name_cell(ui, i, &layout.name, active == Some(i));
                    ui.label(layouts::deck_summary(layout, tr));
                    ui.horizontal(|ui| {
                        if let Some(a) = row_buttons(ui, i, last) {
                            action = Some((i, a));
                        }
                    });
                    ui.end_row();
                }
            });
        self.finish_rename(ui, s);
        match action {
            Some((i, RowAction::Load)) => return Some(i),
            Some((i, RowAction::Update)) => {
                s.layouts[i].layout = ScreenLayout::of(s);
                s.layouts[i].setup = setup.clone();
            }
            Some((i, RowAction::Rename)) => {
                self.renaming = Some((i, s.layouts[i].name.clone()));
                self.rename_error = None;
            }
            Some((i, RowAction::Move(delta))) => {
                layouts::move_by(&mut s.layouts, i, delta);
                self.renaming = None;
            }
            Some((i, RowAction::Delete)) => {
                self.delete_ask = Some(ask_delete(&s.layouts[i].name));
            }
            None => {}
        }
        None
    }

    fn name_cell(&mut self, ui: &mut Ui, i: usize, name: &str, on_screen: bool) {
        if let Some((row, text)) = &mut self.renaming
            && *row == i
        {
            let field = ui.add(egui::TextEdit::singleline(text).desired_width(160.0));
            if !field.has_focus() && !field.lost_focus() {
                field.request_focus();
            }
            return;
        }
        let text = if on_screen {
            format!("● {name}")
        } else {
            name.to_owned()
        };
        ui.label(text)
            .on_hover_text(if on_screen { tr("On screen now") } else { "" });
    }

    fn finish_rename(&mut self, ui: &Ui, s: &mut Settings) {
        let Some((i, text)) = &self.renaming else {
            return;
        };
        if ui.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            self.renaming = None;
            self.rename_error = None;
        } else if ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
            self.rename_error = layouts::rename(&mut s.layouts, *i, text).err();
            if self.rename_error.is_none() {
                self.renaming = None;
            }
        }
    }
}

fn row_buttons(ui: &mut Ui, i: usize, last: usize) -> Option<RowAction> {
    let mut action = None;
    let mut button = |ui: &mut Ui, text: &str, hint: &str, enabled: bool, a: RowAction| {
        if ui
            .add_enabled(enabled, egui::Button::new(tr(text)))
            .on_hover_text(tr(hint))
            .clicked()
        {
            action = Some(a);
        }
    };
    button(
        ui,
        "Load",
        "Show this layout on the screen",
        true,
        RowAction::Load,
    );
    button(
        ui,
        "Update",
        "Save the screen as it is now into this layout",
        true,
        RowAction::Update,
    );
    button(
        ui,
        "Rename",
        "Type the new name and press Enter",
        true,
        RowAction::Rename,
    );
    button(ui, "▲", "Move up", i > 0, RowAction::Move(-1));
    button(ui, "▼", "Move down", i < last, RowAction::Move(1));
    button(ui, "Delete", "Delete this layout", true, RowAction::Delete);
    action
}

fn ask_delete(name: &str) -> Ask<String> {
    Ask::yes_no(
        "Delete layout",
        format!("{} “{name}”?", tr("Delete the layout")),
        name.to_owned(),
    )
}
