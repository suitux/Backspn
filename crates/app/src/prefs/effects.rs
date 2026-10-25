use backspn_engine::{EffectKind, FxRouting};
use eframe::egui::{self, ComboBox, RichText, ScrollArea, Ui};

use super::search;
use super::{Prefs, Settings};
use crate::i18n::tr;

#[allow(clippy::too_many_arguments)]
pub(super) fn pick_list<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    all: &[T],
    chosen: &mut Vec<T>,
    name: impl Fn(T) -> &'static str,
    titles: [&str; 2],
    marks: &mut (Option<T>, Option<usize>),
    keep: usize,
) {
    const LIST: egui::Vec2 = egui::vec2(170.0, 200.0);
    let (available, picked) = marks;
    ui.push_id(id, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                if ui.button(tr("Add")).clicked()
                    && let Some(item) = available.take()
                    && !chosen.contains(&item)
                {
                    chosen.push(item);
                    *picked = Some(chosen.len() - 1);
                }
                list_frame(ui, titles[0], LIST, |ui| {
                    for &item in all {
                        if chosen.contains(&item) {
                            continue;
                        }
                        let row = ui.selectable_label(*available == Some(item), name(item));
                        if row.clicked() {
                            *available = Some(item);
                        }
                        if row.double_clicked() {
                            chosen.push(item);
                            *available = None;
                        }
                    }
                });
            });
            ui.vertical(|ui| {
                if ui
                    .add_enabled(chosen.len() > keep, egui::Button::new(tr("Remove")))
                    .clicked()
                    && let Some(i) = picked.take()
                    && i < chosen.len()
                {
                    chosen.remove(i);
                }
                list_frame(ui, titles[1], LIST, |ui| {
                    for (i, &item) in chosen.iter().enumerate() {
                        if ui
                            .selectable_label(*picked == Some(i), name(item))
                            .clicked()
                        {
                            *picked = Some(i);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let current = picked.filter(|&i| i < chosen.len());
                    if ui
                        .add_enabled(
                            current.is_some_and(|i| i > 0),
                            egui::Button::new(tr("Move Up")),
                        )
                        .clicked()
                        && let Some(i) = current
                    {
                        chosen.swap(i, i - 1);
                        *picked = Some(i - 1);
                    }
                    if ui
                        .add_enabled(
                            current.is_some_and(|i| i + 1 < chosen.len()),
                            egui::Button::new(tr("Move Down")),
                        )
                        .clicked()
                        && let Some(i) = current
                    {
                        chosen.swap(i, i + 1);
                        *picked = Some(i + 1);
                    }
                });
            });
        });
    });
}

pub(super) fn unit_grid(ui: &mut Ui, id: &str, units: usize, mut cell: impl FnMut(&mut Ui, usize)) {
    egui::Grid::new(id)
        .num_columns(4)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            for row in [[0, 1], [2, 3]] {
                for u in row {
                    ui.label(format!("FX{}", u + 1));
                    ui.add_enabled_ui(u < units, |ui| cell(ui, u));
                }
                ui.end_row();
            }
        });
}

pub(super) fn list_frame(ui: &mut Ui, title: &str, size: egui::Vec2, rows: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(size.x);
        ui.label(RichText::new(title).weak());
        ScrollArea::vertical()
            .id_salt(title)
            .max_height(size.y)
            .show(ui, |ui| {
                ui.set_width(size.x);
                rows(ui);
            });
    });
}

pub(super) fn routing_name(r: FxRouting) -> &'static str {
    match r {
        FxRouting::Send => "Send",
        FxRouting::Insert => "Insert",
        FxRouting::PostFader => "Post Fader",
    }
}

pub(super) fn routing_help(r: FxRouting) -> &'static str {
    match r {
        FxRouting::Send => {
            "One effect fed by the assigned decks after their faders; D/W becomes the return level"
        }
        FxRouting::Insert => "On each assigned deck, before its channel strip",
        FxRouting::PostFader => {
            "On each assigned deck, after its fader: tails ring on when the fader closes"
        }
    }
}

impl Prefs {
    pub(super) fn effects(&mut self, ui: &mut Ui, s: &mut Settings) {
        search::heading(ui, "FX unit routing");
        unit_grid(ui, "fx-routing", s.fx_units, |ui, u| {
            ComboBox::from_id_salt(("fx-routing", u))
                .width(110.0)
                .selected_text(routing_name(s.fx_routing[u]))
                .show_ui(ui, |ui| {
                    for r in [FxRouting::Send, FxRouting::Insert, FxRouting::PostFader] {
                        ui.selectable_value(&mut s.fx_routing[u], r, routing_name(r))
                            .on_hover_text(routing_help(r));
                    }
                });
        });
        ui.horizontal(|ui| {
            search::title(ui, "FX units:");
            search::radio(ui, &mut s.fx_units, 2, "2 FX units");
            search::radio(ui, &mut s.fx_units, 4, "4 FX units");
        });
        search::check(
            ui,
            &mut s.fx_restore_params,
            "Restore parameters when switching FX",
        )
        .on_hover_text(tr(
            "Selecting an effect again brings back the settings it last had",
        ));
        ui.add_space(10.0);

        search::heading(ui, "FX panel mode");
        unit_grid(ui, "fx-panel", s.fx_units, |ui, u| {
            crate::fx_view::mode_select(ui, ("fx-panel", u), &mut s.fx_panel_mode[u], 110.0, 14.0);
        });
        ui.add_space(10.0);

        search::heading(ui, "FX pre-selection");
        self.fx_preselection(ui, s);
    }

    pub(super) fn fx_preselection(&mut self, ui: &mut Ui, s: &mut Settings) {
        let all: Vec<EffectKind> = EffectKind::ALL[1..].to_vec();
        pick_list(
            ui,
            "fx",
            &all,
            &mut s.fx_selection,
            |k| k.info().name,
            ["Effects you can add", "Effects you have"],
            &mut self.fx_marks,
            0,
        );
        ui.label(
            RichText::new(tr(
                "The effect selectors offer the effects you have, in this order.",
            ))
            .weak(),
        );
    }
}
