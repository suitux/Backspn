use backspn_library::{NodeKind, TrackRow};
use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};

use crate::i18n::tr;
use crate::track_filter::TrackFilter;
use crate::{theme, widgets};

use super::{Browser, NodeDrag, SetDrag, Source, SourceDrag, TrackDrag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FavoriteDrag(pub usize);

/// Like `dnd_release_payload`, but leaves a payload of another type for the
/// widget it is meant for: egui drops it otherwise.
fn released<T: std::any::Any + Send + Sync>(
    response: &egui::Response,
) -> Option<std::sync::Arc<T>> {
    egui::DragAndDrop::has_payload_of_type::<T>(&response.ctx)
        .then(|| response.dnd_release_payload::<T>())
        .flatten()
}

pub(super) fn list_statistics(rows: &[&TrackRow]) -> String {
    let seconds: f64 = rows.iter().filter_map(|t| t.duration_s).sum();
    let kib: u64 = rows.iter().filter_map(|t| t.filesize_kib).sum();
    let s = seconds.round() as u64;
    let time = if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    };
    let size = if kib >= 1024 * 1024 {
        format!("{:.1} GB", kib as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} MB", kib as f64 / 1024.0)
    };
    format!("{} tracks · {time} · {size}", rows.len())
}

pub(super) const SEARCH_HELP: &str = "Words are looked for in every attribute; a space means AND.\n\
    [ATTRIBUTE OPERATOR] VALUE, joined with & (and) or | (or).\n\
    Attributes: $ALL $ARTIST $TITLE $GENRE $FILENAME $RELEASE $LABEL $COMMENT $RATING \
    $REMIXER $PRODUCER $BITRATE $BPM $KEY…\n\
    Operators: == <> > < >= <= % (contains) !% ~ (around)\n\
    Quote values with several words: $ARTIST == \"Sonic Mayhem\"";

pub(super) const SEARCH_HISTORY: usize = 10;

impl Browser {
    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        let t = theme::get(ui.ctx());
        ui.horizontal(|ui| {
            let search = ui.add(
                TextEdit::singleline(&mut self.search)
                    .hint_text("🔍  Search (e.g. techno $BPM ~ 128)")
                    .desired_width(280.0)
                    .margin(egui::Margin::symmetric(8, 4)),
            );
            let search = search.on_hover_text(SEARCH_HELP);
            if search.changed() {
                self.recompute();
            }
            if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.remember_search();
            }
            let history = ui
                .add_enabled(
                    !self.search_history.is_empty(),
                    egui::Button::new("▾").small(),
                )
                .on_hover_text(tr("Search history"));
            egui::Popup::menu(&history).show(|ui| {
                for past in self.search_history.clone() {
                    if ui.button(&past).clicked() {
                        self.search = past;
                        self.recompute();
                        ui.close();
                    }
                }
            });
            if let Some(e) = &self.search_error {
                ui.colored_label(t.danger, format!("Search: {e}"))
                    .on_hover_text(SEARCH_HELP);
            }
            let rows: Vec<&TrackRow> = self.view.iter().filter_map(|&id| self.row(id)).collect();
            widgets::chip(ui, &list_statistics(&rows), t.text_dim).on_hover_text(tr(
                "Tracks, total playing time and total size of the list shown",
            ));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                #[cfg(not(target_os = "macos"))]
                {
                    if ui.button(tr("Export Collection…")).clicked() {
                        self.export_collection();
                    }
                    if ui.button(tr("Import Collection…")).clicked() {
                        self.import_collection();
                    }
                    if ui.button(tr("Import NML…")).clicked() {
                        self.import_nml();
                    }
                    if ui
                        .button(tr("Import Library XML…"))
                        .on_hover_text(tr(super::LIBRARY_XML_HINT))
                        .clicked()
                    {
                        self.import_library_xml();
                    }
                }
                if ui.button(tr("Add files…")).clicked()
                    && let Some(files) = rfd::FileDialog::new()
                        .add_filter("Audio", crate::AUDIO_EXTENSIONS)
                        .pick_files()
                {
                    let ids = self.add_files(&files);
                    self.analyse_imported(&ids);
                }
                let filter_label = if self.filter.active() {
                    "Filter ●"
                } else {
                    "Filter"
                };
                if ui
                    .add(egui::Button::selectable(self.filter.open, filter_label))
                    .on_hover_text(tr("Narrow the list by BPM, key, rating and colour"))
                    .clicked()
                {
                    self.filter.open = !self.filter.open;
                    if !self.filter.open && self.filter.active() {
                        self.filter = TrackFilter::default();
                        self.recompute();
                    }
                }
                if ui
                    .add(egui::Button::selectable(
                        self.suggestions.open,
                        "Suggest next",
                    ))
                    .on_hover_text(tr("Tracks that mix well after the one on the master deck"))
                    .clicked()
                {
                    self.suggestions.open = !self.suggestions.open;
                }
            });
        });
    }

    pub(super) fn favorites_bar(&mut self, ui: &mut Ui) {
        use crate::favorites::SLOTS;
        self.favorites.resize(SLOTS, String::new());
        let t = theme::get(ui.ctx());
        let width = ((ui.available_width() - 4.0 * (SLOTS as f32 - 1.0)) / SLOTS as f32).max(40.0);
        let mut show = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for slot in 0..SLOTS {
                let target = self.favorite_source(&self.favorites[slot]);
                let name = self.favorite_label(&self.favorites[slot]);
                let selected = target.as_ref() == Some(&self.source);
                let text = if name.is_empty() {
                    "·".to_owned()
                } else {
                    name
                };
                // Filled slots take the theme's text colour, so hovering inverts it.
                let text = RichText::new(text).size(11.0);
                let text = if target.is_some() {
                    text
                } else {
                    text.color(t.text_dim)
                };
                let response = ui
                    .add_sized(
                        egui::vec2(width, 20.0),
                        egui::Button::new(text)
                            .selected(selected)
                            .truncate(),
                    )
                    .interact(egui::Sense::drag())
                    .on_hover_text(tr(
                        "Drop a list of the sidebar here, or tracks to add them. Drag a favorite onto another to swap them",
                    ));
                if !self.favorites[slot].is_empty() {
                    response.dnd_set_drag_payload(FavoriteDrag(slot));
                }
                if response.clicked() && target.is_some() {
                    show = Some(slot);
                }
                let list = response.dnd_hover_payload::<NodeDrag>().is_some()
                    || response.dnd_hover_payload::<SetDrag>().is_some()
                    || response.dnd_hover_payload::<SourceDrag>().is_some()
                    || response
                        .dnd_hover_payload::<FavoriteDrag>()
                        .is_some_and(|from| from.0 != slot);
                let tracks = response.dnd_hover_payload::<TrackDrag>().is_some()
                    && self.takes_tracks(target.as_ref());
                if list || tracks {
                    ui.painter().rect_stroke(
                        response.rect,
                        2.0,
                        egui::Stroke::new(2.0, t.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                if let Some(from) = released::<FavoriteDrag>(&response) {
                    self.favorites.swap(from.0, slot);
                }
                let dropped = if let Some(drag) = released::<NodeDrag>(&response) {
                    Some(Source::Node(drag.0))
                } else if let Some(drag) = released::<SetDrag>(&response) {
                    Some(Source::Set(drag.0))
                } else {
                    released::<SourceDrag>(&response).map(|drag| drag.0.clone())
                };
                if let Some(path) = dropped.and_then(|s| self.favorite_slot(&s)) {
                    self.favorites[slot] = path;
                }
                if let Some(drag) = released::<TrackDrag>(&response) {
                    self.drop_on_favorite(slot, target.as_ref(), &drag.tracks);
                }
                response.context_menu(|ui| {
                    let shown = self.favorite_slot(&self.source);
                    if ui
                        .add_enabled(shown.is_some(), egui::Button::new(tr("Use the list shown")))
                        .clicked()
                    {
                        if let Some(path) = shown {
                            self.favorites[slot] = path;
                        }
                        ui.close();
                    }
                    if ui.button(tr("Clear")).clicked() {
                        self.favorites[slot].clear();
                        ui.close();
                    }
                });
            }
        });
        if let Some(slot) = show {
            let slot = self.favorites[slot].clone();
            self.show_favorite(&slot);
        }
    }

    /// Name and icon of a list being dragged, for the drag preview.
    pub(super) fn dragged_list(&self, ctx: &egui::Context) -> Option<super::table::DraggedList> {
        use egui::DragAndDrop;
        let (source, slot) = if let Some(d) = DragAndDrop::payload::<NodeDrag>(ctx) {
            (Some(Source::Node(d.0)), None)
        } else if let Some(d) = DragAndDrop::payload::<SetDrag>(ctx) {
            (Some(Source::Set(d.0)), None)
        } else if let Some(d) = DragAndDrop::payload::<SourceDrag>(ctx) {
            (Some(d.0.clone()), None)
        } else {
            (None, Some(DragAndDrop::payload::<FavoriteDrag>(ctx)?.0))
        };
        let slot = match (&source, slot) {
            (Some(source), _) => self.favorite_slot(source)?,
            (None, Some(slot)) => self.favorites.get(slot)?.clone(),
            (None, None) => return None,
        };
        let text = self.favorite_label(&slot);
        (!text.is_empty()).then_some(super::table::DraggedList {
            text,
            favorite: source.is_none(),
        })
    }

    fn takes_tracks(&self, target: Option<&Source>) -> bool {
        match target {
            Some(Source::Node(id)) => self
                .nodes
                .iter()
                .any(|n| n.id == *id && n.kind == NodeKind::Playlist),
            Some(_) => false,
            None => true,
        }
    }

    pub(super) fn drop_on_favorite(
        &mut self,
        slot: usize,
        target: Option<&Source>,
        tracks: &[i64],
    ) {
        if !self.takes_tracks(target) {
            return;
        }
        let tracks: Vec<i64> = tracks.iter().filter_map(|&id| self.resolve(id)).collect();
        let result = match target {
            Some(Source::Node(id)) => self.with_lib(|lib| lib.add_to_playlist(*id, &tracks)),
            Some(_) => Ok(()),
            None => {
                let created = self.with_lib_value(|lib| {
                    let name = lib.unique_node_name(None, "New Playlist")?;
                    let id = lib.create_node(None, NodeKind::Playlist, &name, None)?;
                    lib.add_to_playlist(id, &tracks)?;
                    Ok(id)
                });
                created.map(|id| {
                    self.refresh();
                    if let Some(path) = crate::favorites::node_path(&self.nodes, id) {
                        self.favorites[slot] = path;
                    }
                })
            }
        };
        Self::report(result);
        self.recompute();
    }

    pub(super) fn remember_search(&mut self) {
        let search = self.search.trim().to_owned();
        if search.is_empty() {
            return;
        }
        self.search_history.retain(|s| *s != search);
        self.search_history.insert(0, search);
        self.search_history.truncate(SEARCH_HISTORY);
    }
}
