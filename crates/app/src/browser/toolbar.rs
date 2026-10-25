use backspn_library::{NodeKind, TrackRow};
use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};

use crate::i18n::tr;
use crate::track_filter::TrackFilter;
use crate::{theme, widgets};

use super::{Browser, NodeDrag, Source, TrackDrag};

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
        use crate::favorites::{SLOTS, label, node_path, resolve};
        self.favorites.resize(SLOTS, String::new());
        let t = theme::get(ui.ctx());
        let width = ((ui.available_width() - 4.0 * (SLOTS as f32 - 1.0)) / SLOTS as f32).max(40.0);
        let mut show = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for slot in 0..SLOTS {
                let target = resolve(&self.nodes, &self.favorites[slot]);
                let name = label(&self.nodes, &self.favorites[slot]);
                let selected = self.shows(target);
                let text = if name.is_empty() {
                    "·".to_owned()
                } else {
                    name
                };
                let colour = if target.is_some() { t.text } else { t.text_dim };
                let response = ui
                    .add_sized(
                        egui::vec2(width, 20.0),
                        egui::Button::new(RichText::new(text).size(11.0).color(colour))
                            .selected(selected)
                            .truncate(),
                    )
                    .on_hover_text(tr("Drop a playlist here, or tracks to add them"));
                if response.clicked() {
                    show = target;
                }
                if let Some(drag) = response.dnd_release_payload::<NodeDrag>()
                    && let Some(path) = node_path(&self.nodes, drag.0)
                {
                    self.favorites[slot] = path;
                }
                if let Some(drag) = response.dnd_release_payload::<TrackDrag>() {
                    self.drop_on_favorite(slot, target, &drag.tracks);
                }
                response.context_menu(|ui| {
                    if ui.button(tr("Use the list shown")).clicked() {
                        let path = match &self.source {
                            Source::Collection => Some("$COLLECTION".to_owned()),
                            Source::History(_) => Some("$HISTORY".to_owned()),
                            Source::Recordings => Some("$RECORDER".to_owned()),
                            Source::Node(id) => node_path(&self.nodes, *id),
                            Source::Folder(_)
                            | Source::Set(_)
                            | Source::Loops
                            | Source::External(..) => None,
                        };
                        if let Some(path) = path {
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
        if show.is_some() {
            self.show_favorite(show);
        }
    }

    pub(super) fn drop_on_favorite(
        &mut self,
        slot: usize,
        target: Option<crate::favorites::Target>,
        tracks: &[i64],
    ) {
        let tracks: Vec<i64> = tracks.iter().filter_map(|&id| self.resolve(id)).collect();
        let result = match target {
            Some(crate::favorites::Target::Node(id))
                if self
                    .nodes
                    .iter()
                    .any(|n| n.id == id && n.kind == NodeKind::Playlist) =>
            {
                self.with_lib(|lib| lib.add_to_playlist(id, &tracks))
            }
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
            _ => Ok(()),
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
