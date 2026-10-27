use std::collections::{HashMap, HashSet};

use backspn_library::TrackRow;
use backspn_library::energy;
use backspn_library::suggest::{Criteria, EnergyRule, KeyRule, Reference, Suggestion, suggest};
use eframe::egui::{self, ComboBox, DragValue, RichText, ScrollArea, Ui};

use crate::browser::{BrowserAction, TrackDrag, free_deck};
use crate::covers::{self, CoverCache};
use crate::i18n::tr;
use crate::theme::{self, WindowExt};

pub const WIDTH: f32 = 320.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Master {
    pub deck: usize,
    pub track_id: i64,
    pub bpm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
struct Inputs {
    master: Option<Master>,
    criteria: Criteria,
    played: usize,
    rows: usize,
}

#[derive(Debug, Default)]
pub struct SuggestPanel {
    pub open: bool,
    pub criteria: Criteria,
    configuring: bool,
    cached: Option<(Inputs, Vec<Suggestion>)>,
    covers: Option<CoverCache>,
}

pub struct Context<'a> {
    pub rows: &'a [TrackRow],
    pub index: &'a HashMap<i64, usize>,
    pub master: Option<Master>,
    pub played: &'a HashSet<i64>,
    pub deck_count: usize,
    pub playing: [bool; 4],
}

impl Context<'_> {
    fn row(&self, id: i64) -> Option<&TrackRow> {
        self.index.get(&id).map(|&i| &self.rows[i])
    }
}

impl SuggestPanel {
    pub fn forget_covers(&mut self, tracks: &[i64]) {
        if let Some(covers) = &mut self.covers {
            covers.forget(tracks);
        }
    }

    fn suggestions(&mut self, cx: &Context) -> &[Suggestion] {
        let inputs = Inputs {
            master: cx.master,
            criteria: self.criteria,
            played: cx.played.len(),
            rows: cx.rows.len(),
        };
        if self.cached.as_ref().is_none_or(|(i, _)| *i != inputs) {
            let list = cx
                .master
                .and_then(|m| cx.row(m.track_id).map(|t| (m, t)))
                .map(|(m, t)| {
                    let reference = Reference {
                        track_id: Some(t.id),
                        bpm: m.bpm.or(t.bpm),
                        key: t.musical_key,
                        energy: energy::estimate(t),
                        genre: t.genre.clone(),
                    };
                    suggest(&reference, cx.rows, cx.played, &self.criteria)
                })
                .unwrap_or_default();
            self.cached = Some((inputs, list));
        }
        self.cached.as_ref().map_or(&[], |(_, l)| l)
    }

    pub fn ui(&mut self, ui: &mut Ui, cx: &Context, actions: &mut Vec<BrowserAction>) {
        let t = theme::get(ui.ctx());
        ui.set_width(WIDTH);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("NEXT TRACKS")
                    .size(10.5)
                    .strong()
                    .color(t.text_dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×").on_hover_text(tr("Close")).clicked() {
                    self.open = false;
                }
                if ui
                    .small_button(tr("Configure"))
                    .on_hover_text(tr("Choose how the next tracks are picked"))
                    .clicked()
                {
                    self.configuring = !self.configuring;
                }
            });
        });
        match cx.master.and_then(|m| cx.row(m.track_id).map(|r| (m, r))) {
            Some((m, row)) => {
                ui.label(
                    RichText::new(format!(
                        "After deck {}: {}",
                        backspn_control::deck_letter(m.deck),
                        title(row)
                    ))
                    .color(t.deck(m.deck)),
                );
            }
            None => {
                ui.label(
                    RichText::new(tr(
                        "No track on a master deck. Play a track or make a deck master.",
                    ))
                    .weak(),
                );
            }
        }
        ui.label(
            RichText::new(summary(&self.criteria))
                .size(10.5)
                .color(t.text_dim),
        );
        self.configure_window(ui.ctx());
        ui.separator();
        let list = self.suggestions(cx).to_vec();
        if cx.master.is_some() && list.is_empty() {
            ui.label(RichText::new(tr("Nothing matches; widen the criteria.")).weak());
        }
        let covers = self
            .covers
            .get_or_insert_with(|| CoverCache::new(ui.ctx(), ROW_COVER));
        ScrollArea::vertical()
            .id_salt("suggestions")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for s in &list {
                    let Some(row) = cx.row(s.track_id) else {
                        continue;
                    };
                    let cover = covers.get(ui.ctx(), row);
                    suggestion_row(ui, s, row, cover, cx, actions);
                }
            });
    }

    fn configure_window(&mut self, ctx: &egui::Context) {
        if !self.configuring {
            return;
        }
        let mut open = true;
        egui::Window::new(tr("Next Tracks Settings"))
            .collapsible(false)
            .resizable(false)
            .show_closable(ctx, &mut open, |ui| criteria_ui(ui, &mut self.criteria));
        self.configuring = open;
    }
}

fn criteria_ui(ui: &mut Ui, c: &mut Criteria) {
    egui::Grid::new("suggest-criteria")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label(tr("Tempo range"));
            let mut percent = c.bpm_tolerance * 100.0;
            if ui
                .add(
                    DragValue::new(&mut percent)
                        .range(1.0..=20.0)
                        .speed(0.2)
                        .prefix("± ")
                        .suffix(" %"),
                )
                .changed()
            {
                c.bpm_tolerance = percent / 100.0;
            }
            ui.end_row();
            ui.label(tr("Key"));
            ComboBox::from_id_salt("suggest-key")
                .width(120.0)
                .selected_text(tr(key_rule_name(c.key)))
                .show_ui(ui, |ui| {
                    for rule in [KeyRule::Compatible, KeyRule::Same, KeyRule::Any] {
                        ui.selectable_value(&mut c.key, rule, tr(key_rule_name(rule)));
                    }
                });
            ui.end_row();
            ui.label(tr("Energy"));
            ComboBox::from_id_salt("suggest-energy")
                .width(120.0)
                .selected_text(tr(energy_rule_name(c.energy)))
                .show_ui(ui, |ui| {
                    for rule in [
                        EnergyRule::Similar,
                        EnergyRule::Up,
                        EnergyRule::Down,
                        EnergyRule::Any,
                    ] {
                        ui.selectable_value(&mut c.energy, rule, tr(energy_rule_name(rule)));
                    }
                });
            ui.end_row();
            ui.label(tr("Energy range"));
            ui.add_enabled(
                c.energy != EnergyRule::Any,
                DragValue::new(&mut c.energy_range)
                    .range(0.5..=9.0)
                    .speed(0.05)
                    .fixed_decimals(1)
                    .prefix("± "),
            )
            .on_hover_text(tr(
                "Largest energy change allowed, on a scale of 1 to 10. A comment such as \"Energy 7\" sets a track's energy.",
            ));
            ui.end_row();
            ui.label(tr("Results"));
            ui.add(DragValue::new(&mut c.count).range(5..=200));
            ui.end_row();
        });
    crate::widgets::check(ui, &mut c.same_genre, "Same genre");
    crate::widgets::check(ui, &mut c.skip_played, "Hide played");
    ui.separator();
    if ui.button(tr("Reset to Defaults")).clicked() {
        *c = Criteria::default();
    }
}

fn summary(c: &Criteria) -> String {
    let mut parts = vec![
        format!("BPM ±{:.1}%", c.bpm_tolerance * 100.0),
        format!("{} {}", tr("Key"), tr(key_rule_name(c.key))),
    ];
    if c.energy != EnergyRule::Any {
        parts.push(format!(
            "{} {} ±{:.1}",
            tr("Energy"),
            tr(energy_rule_name(c.energy)),
            c.energy_range
        ));
    }
    if c.same_genre {
        parts.push(tr("Same genre").to_owned());
    }
    parts.join(" · ")
}

fn energy_rule_name(rule: EnergyRule) -> &'static str {
    match rule {
        EnergyRule::Similar => "Similar",
        EnergyRule::Up => "Up",
        EnergyRule::Down => "Down",
        EnergyRule::Any => "Any",
    }
}

fn key_rule_name(rule: KeyRule) -> &'static str {
    match rule {
        KeyRule::Compatible => "Compatible",
        KeyRule::Same => "Same",
        KeyRule::Any => "Any",
    }
}

fn energy_text(energy: f64, delta: Option<f64>) -> String {
    let change = match delta {
        Some(d) if d >= 0.5 => format!(" ↑{d:.0}"),
        Some(d) if d <= -0.5 => format!(" ↓{:.0}", -d),
        _ => String::new(),
    };
    format!("E{energy:.0}{change}")
}

fn title(row: &TrackRow) -> String {
    if row.artist.is_empty() {
        row.title.clone()
    } else {
        format!("{} - {}", row.artist, row.title)
    }
}

const ROW_HEIGHT: f32 = 52.0;
const ROW_COVER: f32 = 44.0;

fn suggestion_row(
    ui: &mut Ui,
    s: &Suggestion,
    row: &TrackRow,
    cover: Option<&egui::TextureHandle>,
    cx: &Context,
    actions: &mut Vec<BrowserAction>,
) {
    let t = theme::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::click_and_drag(),
    );
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 0.0, t.control);
    }
    let inner = rect.shrink2(egui::vec2(4.0, (ROW_HEIGHT - ROW_COVER) / 2.0));
    let cover_rect = egui::Rect::from_min_size(inner.min, egui::vec2(ROW_COVER, ROW_COVER));
    covers::paint(
        ui.painter(),
        cover_rect,
        cover.map(egui::TextureHandle::id),
        t.text_dim,
    );
    let text =
        egui::Rect::from_min_max(egui::pos2(cover_rect.right() + 8.0, inner.top()), inner.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(text), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add(egui::Label::new(RichText::new(&row.title).color(t.text)).truncate());
        ui.add(
            egui::Label::new(RichText::new(&row.artist).size(11.0).color(t.text_dim)).truncate(),
        );
        ui.horizontal(|ui| {
            let bpm = row.bpm.map_or_else(String::new, |b| format!("{b:.1}"));
            ui.label(RichText::new(bpm).monospace().size(11.0));
            if let Some(o) = s.bpm_offset {
                ui.label(
                    RichText::new(format!("±{:.1}%", o * 100.0))
                        .size(10.5)
                        .color(t.text_dim),
                );
            }
            let key_colour = match s.key_distance {
                Some(0) => t.accent,
                Some(_) => t.text,
                None => t.text_dim,
            };
            ui.label(
                RichText::new(&row.key_text)
                    .monospace()
                    .size(11.0)
                    .color(key_colour),
            );
            if let Some(e) = s.energy {
                ui.label(
                    RichText::new(energy_text(e, s.energy_delta))
                        .monospace()
                        .size(11.0),
                )
                .on_hover_text(tr(
                    "Estimated energy (1–10) and change from the master track",
                ));
            }
            if !row.genre.is_empty() {
                ui.add(
                    egui::Label::new(RichText::new(&row.genre).size(10.5).color(t.text_dim))
                        .truncate(),
                );
            }
        });
    });
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.dnd_set_drag_payload(TrackDrag::single(row.id, title(row)));
    if response.double_clicked() {
        actions.push(BrowserAction::Load {
            deck: free_deck(cx.deck_count, cx.playing),
            track: row.id,
        });
    }
    response.context_menu(|ui| {
        for deck in 0..cx.deck_count {
            if ui
                .button(format!(
                    "Load to deck {}",
                    backspn_control::deck_letter(deck)
                ))
                .clicked()
            {
                actions.push(BrowserAction::Load {
                    deck,
                    track: row.id,
                });
                ui.close();
            }
        }
    });
}
