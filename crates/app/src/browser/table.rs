use std::path::PathBuf;

use backspn_library::NodeKind;
use eframe::egui::{self, Align, Layout, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::columns::{BrowserColumn, CellContext, ColumnLayout, sort_clicked, stars};
use crate::i18n::tr;
use crate::theme;
use crate::track_edit::TrackEdit;

use super::files::reveal;
use super::{Browser, BrowserAction, Source, TrackDrag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColumnDrag(BrowserColumn);

#[derive(Clone)]
pub(super) enum Pending {
    Click(usize, bool, bool),
    SelectAll,
    AddTo(i64),
    RemoveFrom(i64),
    Consolidate(i64),
    MoveEntries(i64, Vec<usize>, usize),
    RemoveFromHistory(i64),
    PurgeRecordings,
    Cover(bool),
    Rate(u8),
    Color(u8),
    ResetPlayed,
    Rescan(bool),
    Edit,
    StartEdit(i64, BrowserColumn),
    SearchInPlaylists(i64),
    Preview(i64),
    PreviewFrom(i64, f32),
    Analyse(bool),
    Lock(bool),
    RelocateOne(i64),
    DeleteFromCollection,
    GenerateStems(i64),
    DeleteStems(i64),
}

pub(super) fn drop_line(row: &egui::Response, colour: egui::Color32) {
    row.ctx.layer_painter(row.layer_id).hline(
        row.rect.x_range(),
        row.rect.top(),
        egui::Stroke::new(2.0, colour),
    );
}

pub(super) fn drag_preview(ctx: &egui::Context, list: Option<DraggedList>) {
    let text = match egui::DragAndDrop::payload::<TrackDrag>(ctx) {
        Some(drag) => format!("♫  {}", drag.title),
        None => match list {
            Some(list) => {
                if list_ghost(ctx, &list) {
                    return;
                }
                list.text
            }
            None => return,
        },
    };
    let Some(pos) = ctx.pointer_latest_pos() else {
        return;
    };
    let t = theme::get(ctx);
    egui::Area::new(egui::Id::unique("track-drag"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(14.0, 10.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(t.surface)
                .stroke(egui::Stroke::new(1.0, t.accent))
                .corner_radius(0)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.label(RichText::new(text).color(t.text));
                });
        });
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
}

pub(super) struct DraggedList {
    pub text: String,
    pub favorite: bool,
}

/// Draws a dragged list as a copy of the widget it was taken from, the same
/// size and held at the same point. False when that widget is not known.
fn list_ghost(ctx: &egui::Context, list: &DraggedList) -> bool {
    let Some(rect) = ctx
        .dragged_id()
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
    else {
        return false;
    };
    let (Some(pos), Some(origin)) = (
        ctx.pointer_latest_pos(),
        ctx.input(|i| i.pointer.press_origin()),
    ) else {
        return false;
    };
    let rect = rect.translate(pos - origin);
    let t = theme::get(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::unique("list-drag"),
    ));
    painter.rect(
        rect,
        2.0,
        t.surface,
        egui::Stroke::new(1.0, t.accent),
        egui::StrokeKind::Inside,
    );
    let (anchor, at, size) = if list.favorite {
        (egui::Align2::CENTER_CENTER, rect.center(), 11.0)
    } else {
        (
            egui::Align2::LEFT_CENTER,
            rect.left_center() + egui::vec2(4.0, 0.0),
            13.5,
        )
    };
    painter.with_clip_rect(rect).text(
        at,
        anchor,
        &list.text,
        egui::FontId::proportional(size),
        t.text,
    );
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    true
}

impl Browser {
    #[allow(clippy::too_many_lines)]
    pub(super) fn table(
        &mut self,
        ui: &mut Ui,
        deck_count: usize,
        actions: &mut Vec<BrowserAction>,
    ) {
        let played = &self.played.clone();
        let keys = self.keys;
        let previewed = self.preview.track();
        let preview_at = {
            let s = &self.preview_state;
            (s.duration_s > 0.0).then(|| (s.position_s / s.duration_s) as f32)
        };
        let t = theme::get(ui.ctx());
        let header = |ui: &mut Ui, this: &mut Self, column: BrowserColumn| {
            let (arrow, color) = match this.sort.iter().position(|&(c, _)| c == column) {
                Some(0) if this.sort[0].1 => (" ▲", t.accent),
                Some(0) => (" ▼", t.accent),
                Some(_) if this.sort[1].1 => (" △", t.text_dim),
                Some(_) => (" ▽", t.text_dim),
                None => ("", t.text_dim),
            };
            let text = RichText::new(format!("{}{arrow}", column.name().to_uppercase()))
                .size(10.5)
                .strong()
                .color(color);
            ui.add(egui::Label::new(text).selectable(false));
            // The whole cell reacts, not only the text.
            let cell = ui.max_rect();
            let mut zone = ui.interact(
                cell,
                egui::Id::unique(("column-header", column)),
                egui::Sense::click_and_drag(),
            );
            if column.sortable() {
                zone = zone.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
            zone.dnd_set_drag_payload(ColumnDrag(column));
            let after = |pos: egui::Pos2| pos.x > cell.center().x;
            if let (Some(_), Some(pos)) = (
                zone.dnd_hover_payload::<ColumnDrag>(),
                ui.ctx().pointer_hover_pos(),
            ) {
                let x = if after(pos) {
                    cell.right()
                } else {
                    cell.left()
                };
                ui.painter()
                    .vline(x, cell.y_range(), egui::Stroke::new(2.0, t.accent));
            }
            if let (Some(drag), Some(pos)) = (
                zone.dnd_release_payload::<ColumnDrag>(),
                ui.ctx().pointer_interact_pos(),
            ) {
                this.columns.move_column(drag.0, column, after(pos));
            }
            if column.sortable() && zone.clicked() {
                this.sort = sort_clicked(&this.sort, column);
                this.remember_sorting();
                this.recompute();
            }
            if zone.secondary_clicked()
                && let Some(pos) = zone.interact_pointer_pos()
            {
                this.columns_menu_at = Some((pos, ui.ctx().cumulative_frame_nr()));
                this.columns_query.clear();
            }
        };
        let in_playlist = match self.source {
            Source::Node(id) => self.nodes.iter().find(|n| n.id == id),
            _ => None,
        }
        .filter(|n| n.kind == NodeKind::Playlist)
        .map(|n| n.id);
        let playlists: Vec<(i64, String)> = self
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Playlist)
            .map(|n| (n.id, n.name.clone()))
            .collect();
        let reorderable =
            in_playlist.filter(|_| self.sort.is_empty() && self.search.trim().is_empty());
        let mut pending: Vec<Pending> = Vec::new();
        self.list_keys(ui, &mut pending);
        let mut inline = self.inline.take();
        let mut inline_shown = false;
        let mut inline_end = None;

        let ctx = ui.ctx().clone();
        let mut builder = TableBuilder::new(ui);
        if std::mem::take(&mut self.scroll_to_selected)
            && let Some(row) = self.selection.cursor()
        {
            builder = builder.scroll_to_row(row, Some(Align::Center));
        }
        let columns = self.columns.visible();
        let with_preview = columns.contains(&BrowserColumn::Preview);
        let with_covers = columns.contains(&BrowserColumn::Cover);
        let with_stems = columns.contains(&BrowserColumn::Stems);
        builder = builder.id_salt(("tracks", &columns));
        for (i, column) in columns.iter().enumerate() {
            builder = builder.column(if i + 1 == columns.len() {
                Column::remainder().at_least(80.0).clip(true)
            } else {
                column.width()
            });
        }
        builder
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click_and_drag())
            .cell_layout(Layout::left_to_right(Align::Center))
            .header(24.0, |mut row| {
                for &column in &columns {
                    row.col(|ui| header(ui, self, column));
                }
            })
            .body(|body| {
                body.rows(self.row_height, self.view.len(), |mut row| {
                    let n = row.index();
                    let id = self.view[n];
                    let Some(track) = self.row(id) else { return };
                    row.set_selected(self.selection.contains(n));
                    let marks = if with_preview {
                        self.cue_marks(id)
                    } else {
                        std::rc::Rc::from([])
                    };
                    let stems = if with_stems {
                        self.stems_path(id)
                    } else {
                        None
                    };
                    let cx = CellContext {
                        played: played.contains(&id),
                        keys,
                        preview_at: (previewed == Some(id)).then_some(preview_at).flatten(),
                        cues: &marks,
                        cover: if with_covers {
                            self.cover(&ctx, id)
                        } else {
                            None
                        },
                        stems: stems.as_deref(),
                    };
                    let mut wave = None;
                    let mut cells = Vec::with_capacity(columns.len());
                    for &column in &columns {
                        row.col(|ui| {
                            cells.push((column, ui.max_rect().x_range()));
                            if let Some(edit) =
                                inline.as_mut().filter(|e| e.id == id && e.column == column)
                            {
                                inline_shown = true;
                                if let Some(end) = super::inline::edit_box(ui, edit) {
                                    inline_end = Some(end);
                                }
                            } else if let Some(rect) = column.cell(ui, n, track, &cx, &t) {
                                wave = Some(rect);
                            }
                        });
                    }
                    let response = row.response();
                    let title = if track.artist.is_empty() {
                        track.title.clone()
                    } else {
                        format!("{} - {}", track.artist, track.title)
                    };
                    if response.drag_started() {
                        let rows: Vec<usize> = if self.selection.contains(n) {
                            self.selection.rows().collect()
                        } else {
                            vec![n]
                        };
                        response.dnd_set_drag_payload(TrackDrag {
                            id,
                            title,
                            tracks: rows.iter().map(|&r| self.view[r]).collect(),
                            entries: reorderable.map(|node| {
                                (node, rows.iter().map(|&r| self.view_source[r]).collect())
                            }),
                        });
                    }
                    if let Some(drag) = response.dnd_hover_payload::<TrackDrag>()
                        && drag
                            .entries
                            .as_ref()
                            .is_some_and(|(node, _)| Some(*node) == reorderable)
                    {
                        drop_line(&response, t.accent);
                    }
                    if let Some(drag) = response.dnd_release_payload::<TrackDrag>()
                        && let Some((node, entries)) = &drag.entries
                        && Some(*node) == reorderable
                    {
                        pending.push(Pending::MoveEntries(
                            *node,
                            entries.clone(),
                            self.view_source[n],
                        ));
                    }
                    let on_wave = wave.zip(response.hover_pos()).and_then(|(r, p)| {
                        crate::columns::cue_under(r, p, &marks)
                            .or_else(|| crate::columns::preview_fraction(r, p))
                    });
                    if on_wave.is_some() {
                        response.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if response.clicked() {
                        if let Some(at) = on_wave {
                            pending.push(Pending::PreviewFrom(id, at));
                        }
                        let m = response.ctx.input(|i| i.modifiers);
                        let only = self.selection.len() == 1 && self.selection.contains(n);
                        if only
                            && !m.shift
                            && !m.command
                            && !response.double_clicked()
                            && let Some(pos) = response.interact_pointer_pos()
                            && let Some(&(column, _)) =
                                cells.iter().find(|(_, x)| x.contains(pos.x))
                        {
                            pending.push(Pending::StartEdit(id, column));
                        }
                        pending.push(Pending::Click(n, m.shift, m.command));
                    }
                    if response.secondary_clicked() && !self.selection.contains(n) {
                        pending.push(Pending::Click(n, false, false));
                    }
                    let track_path = track.path.clone();
                    response.context_menu(|ui| {
                        let count = self.selection.len().max(1);
                        let stem_file =
                            crate::stems::is_stem_file(std::path::Path::new(&track_path))
                                || self.generated_stems(id).is_some();
                        for deck in 0..deck_count {
                            let label =
                                format!("Load to deck {}", backspn_control::deck_letter(deck));
                            if !stem_file {
                                if ui.button(label).clicked() {
                                    actions.push(BrowserAction::Load { deck, track: id });
                                    ui.close();
                                }
                                continue;
                            }
                            ui.menu_button(label, |ui| {
                                for (text, mode) in [
                                    ("Load as Track", backspn_control::LoadMode::Track),
                                    ("Load as Stem", backspn_control::LoadMode::Stems),
                                ] {
                                    if ui.button(text).clicked() {
                                        actions.push(BrowserAction::LoadAs {
                                            deck,
                                            track: id,
                                            mode,
                                        });
                                        ui.close();
                                    }
                                }
                            });
                        }
                        if ui.button(tr("Preview (headphones)")).clicked() {
                            pending.push(Pending::Preview(id));
                            ui.close();
                        }
                        if count == 1 {
                            if self.can_generate_stems(id)
                                && ui.button(tr("Generate Stems")).clicked()
                            {
                                pending.push(Pending::GenerateStems(id));
                                ui.close();
                            }
                            if self.has_own_stems(id)
                                && ui.button(tr("Delete Associated Stems")).clicked()
                            {
                                pending.push(Pending::DeleteStems(id));
                                ui.close();
                            }
                        }
                        ui.separator();
                        ui.menu_button(tr("Add to playlist"), |ui| {
                            for (node, name) in &playlists {
                                if ui.button(name).clicked() {
                                    pending.push(Pending::AddTo(*node));
                                    ui.close();
                                }
                            }
                        });
                        if let Some(node) = in_playlist {
                            if ui
                                .button(format!("Remove from playlist ({count})"))
                                .clicked()
                            {
                                pending.push(Pending::RemoveFrom(node));
                                ui.close();
                            }
                            if ui
                                .button(tr("Consolidate"))
                                .on_hover_text(tr("Make the order shown the playlist's order"))
                                .clicked()
                            {
                                pending.push(Pending::Consolidate(node));
                                ui.close();
                            }
                        }
                        if self.source == Source::Loops
                            && let Some(list) = self.loops
                            && ui.button(format!("Remove from Loops ({count})")).clicked()
                        {
                            pending.push(Pending::RemoveFrom(list));
                            ui.close();
                        }
                        if self.source == Source::Recordings
                            && ui.button(format!("Delete ({count})")).clicked()
                        {
                            pending.push(Pending::PurgeRecordings);
                            ui.close();
                        }
                        if let Source::History(session) = self.source
                            && ui
                                .button(format!("Delete from history ({count})"))
                                .clicked()
                        {
                            pending.push(Pending::RemoveFromHistory(session));
                            ui.close();
                        }
                        if ui.button(tr("Reset Played-State")).clicked() {
                            pending.push(Pending::ResetPlayed);
                            ui.close();
                        }
                        ui.menu_button(tr("Analyze"), |ui| {
                            if ui.button(tr("All")).clicked() {
                                pending.push(Pending::Analyse(false));
                                ui.close();
                            }
                            if ui.button(tr("Special…")).clicked() {
                                pending.push(Pending::Analyse(true));
                                ui.close();
                            }
                        });
                        if count == 1 {
                            if ui
                                .button(tr("Scan"))
                                .on_hover_text(tr("Read the file's tags again"))
                                .clicked()
                            {
                                pending.push(Pending::Rescan(false));
                                ui.close();
                            }
                        } else {
                            ui.menu_button(tr("Scan"), |ui| {
                                if ui
                                    .button(tr("Full Rescan"))
                                    .on_hover_text(tr("Read every selected file's tags again"))
                                    .clicked()
                                {
                                    pending.push(Pending::Rescan(false));
                                    ui.close();
                                }
                                if ui
                                    .button(tr("Quick Scan"))
                                    .on_hover_text(tr("Read again only the files modified since"))
                                    .clicked()
                                {
                                    pending.push(Pending::Rescan(true));
                                    ui.close();
                                }
                            });
                        }
                        if ui.button(tr("Lock analysis")).clicked() {
                            pending.push(Pending::Lock(true));
                            ui.close();
                        }
                        if ui.button(tr("Unlock analysis")).clicked() {
                            pending.push(Pending::Lock(false));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button(tr("Import Cover…")).clicked() {
                            pending.push(Pending::Cover(true));
                            ui.close();
                        }
                        if ui.button(tr("Delete Cover")).clicked() {
                            pending.push(Pending::Cover(false));
                            ui.close();
                        }
                        ui.separator();
                        if id >= 0
                            && ui
                                .button(tr("Relocate…"))
                                .on_hover_text(tr("Assign the file of this track"))
                                .clicked()
                        {
                            pending.push(Pending::RelocateOne(id));
                            ui.close();
                        }
                        if ui.button(tr("Show in Finder")).clicked() {
                            reveal(&track_path);
                            ui.close();
                        }
                        if ui
                            .button(format!("Delete from collection ({count})"))
                            .clicked()
                        {
                            pending.push(Pending::DeleteFromCollection);
                            ui.close();
                        }
                        if id >= 0 && ui.button(tr("Search in playlists…")).clicked() {
                            pending.push(Pending::SearchInPlaylists(id));
                            ui.close();
                        }
                        if ui.button(format!("Edit… ({count})")).clicked() {
                            pending.push(Pending::Edit);
                            ui.close();
                        }
                        ui.menu_button(tr("Color"), |ui| {
                            for (value, name) in
                                backspn_library::smart::COLOR_NAMES.iter().enumerate()
                            {
                                let value = value as u8;
                                let mut label = RichText::new(*name);
                                if let Some(c) = crate::columns::track_colour(value) {
                                    label = label.color(c);
                                }
                                if ui.button(label).clicked() {
                                    pending.push(Pending::Color(value));
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button(tr("Rating"), |ui| {
                            for stars_n in 0..=5u8 {
                                if ui.button(stars(stars_n * 51)).clicked() {
                                    pending.push(Pending::Rate(stars_n * 51));
                                    ui.close();
                                }
                            }
                        });
                    });
                });
            });

        match (inline, inline_end) {
            (Some(edit), Some(true)) => self.commit_edit(&edit),
            (Some(edit), None) if !inline_shown => self.commit_edit(&edit),
            (edit, None) => self.inline = edit,
            (_, Some(_)) => {}
        }
        for p in pending {
            self.apply(p);
        }
    }

    pub(super) fn columns_popup(&mut self, ctx: &egui::Context) {
        let Some((at, opened)) = self.columns_menu_at else {
            return;
        };
        let area = egui::Area::new(egui::Id::unique("browser-columns-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| self.columns_menu(ui));
            });
        // A two finger tap presses and releases in the frame that opens the
        // menu, and that press must not count as a click outside it.
        let clicked_outside = ctx.cumulative_frame_nr() != opened
            && ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| !area.response.rect.contains(p))
            });
        if clicked_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.columns_menu_at = None;
        }
    }

    pub(super) fn columns_menu(&mut self, ui: &mut Ui) {
        // Size of the unfiltered list, kept while searching so the menu does
        // not jump around.
        let size_id = egui::Id::unique("browser-columns-menu-size");
        let full: Option<egui::Vec2> = ui.data(|d| d.get_temp(size_id));
        ui.label(RichText::new(tr("Columns")).weak());
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.columns_query)
                .hint_text(tr("Search"))
                .desired_width(full.map_or(160.0, |s| s.x)),
        );
        if !field.has_focus() && !field.lost_focus() {
            field.request_focus();
        }
        let query = self.columns_query.trim().to_lowercase();
        let shown: Vec<BrowserColumn> = BrowserColumn::ALL
            .iter()
            .copied()
            .filter(|c| c.name().to_lowercase().contains(&query))
            .collect();
        if field.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Some(&first) = shown.first()
        {
            self.columns.set_shown(first, !self.columns.is_shown(first));
            field.request_focus();
        }
        let grid = ui.scope(|ui| {
            if let Some(size) = full.filter(|_| !query.is_empty()) {
                ui.set_min_size(size);
                ui.set_max_width(size.x);
            }
            if shown.is_empty() {
                ui.label(RichText::new(tr("No matches")).weak());
                return;
            }
            let half = BrowserColumn::ALL.len().div_ceil(2);
            ui.horizontal_top(|ui| {
                for part in shown.chunks(half) {
                    ui.vertical(|ui| {
                        for &column in part {
                            let mut shown = self.columns.is_shown(column);
                            if crate::widgets::check(ui, &mut shown, column.name()).changed() {
                                self.columns.set_shown(column, shown);
                            }
                        }
                    });
                }
            });
        });
        if query.is_empty() {
            let size = grid.response.rect.size();
            if full != Some(size) {
                ui.data_mut(|d| d.insert_temp(size_id, size));
                ui.ctx().request_repaint();
            }
        }
        ui.separator();
        if ui.button(tr("Reset columns")).clicked() {
            self.columns = ColumnLayout::default();
        }
    }

    pub(super) fn list_keys(&self, ui: &Ui, pending: &mut Vec<Pending>) {
        if ui.ctx().text_edit_focused() {
            return;
        }
        let (all, delete) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::A),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                    | i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace),
            )
        });
        if all {
            pending.push(Pending::SelectAll);
        }
        if delete && self.selection.len() > 0 {
            match self.source {
                Source::Node(node)
                    if self
                        .nodes
                        .iter()
                        .any(|n| n.id == node && n.kind == NodeKind::Playlist) =>
                {
                    pending.push(Pending::RemoveFrom(node));
                }
                Source::History(session) => pending.push(Pending::RemoveFromHistory(session)),
                Source::Recordings => pending.push(Pending::PurgeRecordings),
                Source::Loops => {
                    if let Some(list) = self.loops {
                        pending.push(Pending::RemoveFrom(list));
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn apply(&mut self, pending: Pending) {
        let result = match pending {
            Pending::Click(row, shift, command) => {
                self.selection.click(row, shift, command);
                return;
            }
            Pending::SelectAll => {
                self.selection.select_all(self.view.len());
                return;
            }
            Pending::StartEdit(id, column) => {
                self.start_edit(id, column);
                return;
            }
            Pending::ResetPlayed => {
                let tracks: Vec<i64> = self.selected_rows().into_iter().map(|(_, id)| id).collect();
                self.reset_played(&tracks);
                return;
            }
            Pending::AddTo(node) => {
                let tracks = self.resolve_selection();
                self.with_lib(|lib| lib.add_to_playlist(node, &tracks))
            }
            Pending::Color(_) | Pending::Rate(_) | Pending::Lock(_) => {
                return self.edit_selection(&pending);
            }
            Pending::RemoveFrom(node) => {
                let entries = self.selected_entries();
                self.selection.clear();
                self.with_lib(|lib| lib.remove_entries(node, &entries))
            }
            Pending::Consolidate(node) => {
                let shown = self.view_source.clone();
                self.with_lib(|lib| lib.consolidate_playlist(node, &shown))
            }
            Pending::MoveEntries(node, entries, to) => {
                self.selection.clear();
                self.with_lib(|lib| lib.move_playlist_entries(node, &entries, to))
            }
            Pending::Rescan(quick) => {
                let tracks = self.resolve_selection();
                self.ask_rescan(tracks, quick);
                return;
            }
            Pending::Analyse(special) => {
                let tracks = self.resolve_selection();
                if special {
                    self.analysis_dialog = Some((tracks, self.default_parts()));
                } else {
                    self.ask_analyse(tracks);
                }
                return;
            }
            Pending::Preview(id) => {
                if let Some(action) = self.preview(id, None) {
                    self.tree_actions.push(action);
                }
                return;
            }
            Pending::PreviewFrom(id, at) => {
                if let Some(action) = self.preview(id, Some(at)) {
                    self.tree_actions.push(action);
                }
                return;
            }
            Pending::SearchInPlaylists(id) => {
                match self.with_lib_value(|lib| lib.playlists_with(id)) {
                    Ok(lists) => self.found_in = Some((id, lists)),
                    Err(e) => crate::toast::error(e.to_string()),
                }
                return;
            }
            Pending::Edit => {
                let tracks = self.resolve_selection();
                if !tracks.is_empty() {
                    let mut edit = TrackEdit::new(tracks);
                    edit.set_overview(&self.rows);
                    self.load_edit(&mut edit);
                    self.editing = Some(edit);
                }
                return;
            }
            Pending::RelocateOne(id) => return self.relocate_one(id),
            Pending::GenerateStems(id) => return self.generate_stems(id),
            Pending::DeleteStems(id) => return self.delete_stems(id),
            Pending::DeleteFromCollection => return self.delete_selection(),
            Pending::PurgeRecordings => return self.purge_recordings(),
            Pending::Cover(import) => return self.change_cover(import),
            Pending::RemoveFromHistory(session) => {
                let positions: Vec<i64> = self
                    .selected_entries()
                    .into_iter()
                    .filter_map(|i| self.history_positions.get(i).map(|&(_, p)| p))
                    .collect();
                self.selection.clear();
                self.with_lib(|lib| lib.remove_history_entries(session, &positions))
            }
        };
        Self::report(result);
        self.recompute();
    }

    pub(super) fn edit_selection(&mut self, pending: &Pending) {
        let tracks = self.resolve_selection();
        let result = self.with_lib(|lib| match *pending {
            Pending::Rate(rating) => tracks.iter().try_for_each(|&id| lib.set_rating(id, rating)),
            Pending::Color(color) => lib.set_color(&tracks, color),
            Pending::Lock(locked) => lib.set_locked(&tracks, locked),
            _ => Ok(()),
        });
        for &id in &tracks {
            self.refresh_track(id);
        }
        if matches!(pending, Pending::Rate(_)) {
            self.tags_changed(&tracks);
        }
        Self::report(result);
    }

    pub(super) fn preview(&mut self, id: i64, from: Option<f32>) -> Option<BrowserAction> {
        let track = self.row(id)?;
        let path = PathBuf::from(track.path.split('#').next().unwrap_or(&track.path));
        let title = if track.artist.is_empty() {
            track.title.clone()
        } else {
            format!("{} - {}", track.artist, track.title)
        };
        Some(BrowserAction::Preview(
            self.preview.load_or_play(id, path, title, from),
        ))
    }

    pub(super) fn relocate_one(&mut self, id: i64) {
        let Some(file) = rfd::FileDialog::new()
            .add_filter("Audio", crate::AUDIO_EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        let path = file.to_string_lossy().into_owned();
        let result = self.with_lib(|lib| {
            if lib.track_by_path(&path)?.is_some() {
                return Err(backspn_library::Error::Invalid(
                    "That file is in the collection already.".into(),
                ));
            }
            lib.set_track_path(id, &path)
        });
        Self::report(result);
        self.refresh();
    }
}
