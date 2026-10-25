use std::path::Path;

use backspn_library::{Node, NodeKind};
use eframe::egui::{self, RichText, Ui};

use crate::i18n::tr;
use crate::modal::Ask;
use crate::smartlist_editor::{self, SmartlistEditor};
use crate::theme::WindowExt;
use crate::{theme, widgets};

use super::dialogs::BrowserAsk;
use super::files::subfolders;
use super::{Browser, BrowserAction, NodeDrag, SetDrag, Source, TrackDrag};

const DECK_NAMES: [&str; 4] = ["A", "B", "C", "D"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TreeNode {
    Music,
    SampleSets,
    Recordings,
    Loops,
    History,
    Archive,
    DjSoftware,
}

impl TreeNode {
    pub const ALL: [Self; 7] = [
        Self::Music,
        Self::SampleSets,
        Self::Recordings,
        Self::Loops,
        Self::History,
        Self::Archive,
        Self::DjSoftware,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Music => "Music",
            Self::SampleSets => "All Sample Sets",
            Self::Recordings => "Audio Recordings",
            Self::Loops => "Loops",
            Self::History => "History",
            Self::Archive => "Archive",
            Self::DjSoftware => "DJ Software",
        }
    }

    pub fn of(source: &Source) -> Option<Self> {
        Some(match source {
            Source::External(super::Outside::Music, _) => Self::Music,
            Source::External(super::Outside::Program(_), _) => Self::DjSoftware,
            Source::Set(_) => Self::SampleSets,
            Source::Recordings => Self::Recordings,
            Source::Loops => Self::Loops,
            Source::History(_) => Self::History,
            _ => return None,
        })
    }
}

pub(super) fn set_label(set: &backspn_formats::nml::SampleSet) -> String {
    format!("{} ({})", set.title, set.entry_count())
}

impl Browser {
    pub(super) fn shown(&self, node: TreeNode) -> bool {
        !self.hidden_nodes.contains(&node)
    }

    pub fn set_hidden_nodes(&mut self, hidden: &[TreeNode]) {
        if self.hidden_nodes == hidden {
            return;
        }
        self.hidden_nodes = hidden.to_vec();
        let archived =
            matches!(self.source, Source::History(id) if Some(id) != self.current_history);
        if TreeNode::of(&self.source).is_some_and(|n| !self.shown(n))
            || (archived && !self.shown(TreeNode::Archive))
        {
            self.show(Source::Collection);
        }
    }

    fn fixed_nodes(&mut self, ui: &mut Ui) {
        if self.shown(TreeNode::Music) {
            self.music_tree(ui);
        }
        if self.shown(TreeNode::SampleSets) {
            self.sets_node(ui);
        }
        if self.shown(TreeNode::Recordings) {
            self.recordings_node(ui);
        }
        if self.shown(TreeNode::Loops) {
            self.loops_node(ui);
        }
        if self.shown(TreeNode::History) {
            self.history_tree(ui);
        }
    }

    pub(super) fn tree(&mut self, ui: &mut Ui) {
        let collection =
            ui.selectable_label(self.source == Source::Collection, tr("◉  Collection"));
        if collection.clicked() {
            self.show(Source::Collection);
        }
        collection.context_menu(|ui| self.node_menu(ui, None));
        self.fixed_nodes(ui);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let faint = theme::get(ui.ctx()).text_faint;
            let root = widgets::hud_label(ui, "PLAYLISTS", 10.0, faint);
            if let Some(drag) = root.dnd_release_payload::<NodeDrag>() {
                self.move_node(drag.0, None);
            }
            theme::accent_hover(ui, |ui| {
                let plus = egui::Button::new("+").frame(false);
                egui::containers::menu::MenuButton::from_button(plus)
                    .ui(ui, |ui| {
                        if ui.button(tr("New playlist")).clicked() {
                            self.new_playlist(None, NodeKind::Playlist, "New Playlist");
                            ui.close();
                        }
                        if ui.button(tr("New smartlist")).clicked() {
                            self.new_smartlist(None);
                            ui.close();
                        }
                    })
                    .0
                    .on_hover_text(tr("New playlist or smartlist"));
            });
        });
        self.subtree(ui, None);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            widgets::caption(ui, "FOLDERS");
            let add = theme::accent_hover(ui, |ui| {
                ui.add(egui::Button::new("+").frame(false))
                    .on_hover_text(tr("Add a folder of your computer"))
            });
            if add.clicked()
                && let Some(dirs) = rfd::FileDialog::new().pick_folders()
            {
                for dir in dirs {
                    if let Some(action) = self.add_folder(dir) {
                        self.tree_actions.push(action);
                    }
                }
            }
        });
        if self.folders.is_empty() {
            ui.label(
                RichText::new(tr("Add a folder to browse its files."))
                    .weak()
                    .size(11.0),
            );
        }
        let mut remove = None;
        for root in self.folders.clone() {
            let name = root.file_name().map_or_else(
                || root.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let response = self.folder_node(ui, &root, &name);
            response.context_menu(|ui| {
                self.folder_menu(ui, &root);
                ui.menu_button(tr("Scan"), |ui| {
                    for (label, quick) in [("Full Rescan", false), ("Quick Scan", true)] {
                        if ui.button(tr(label)).clicked() {
                            self.subdirs.clear();
                            self.recompute();
                            let tracks = self.folder_tracks(&root);
                            self.ask_rescan(tracks, quick);
                            ui.close();
                        }
                    }
                });
                if ui.button(tr("Remove from list")).clicked() {
                    remove = Some(root.clone());
                    ui.close();
                }
            });
            response.on_hover_text(root.display().to_string());
        }
        if let Some(root) = remove {
            self.folders.retain(|f| *f != root);
            if matches!(&self.source, Source::Folder(f) if f.starts_with(&root)) {
                self.show(Source::Collection);
            }
            self.tree_actions
                .push(BrowserAction::FoldersChanged(self.folders.clone()));
        }
        if self.shown(TreeNode::DjSoftware) {
            self.programs_tree(ui);
        }
        self.library_xml_tree(ui);
    }

    fn sets_node(&mut self, ui: &mut Ui) {
        let rows: Vec<(i64, String)> = self
            .sets
            .iter()
            .map(|s| (s.id, set_label(&s.set)))
            .collect();
        let header = egui::CollapsingHeader::new(tr("All Sample Sets"))
            .id_salt("sample-sets")
            .default_open(false);
        let header = crate::theme::collapsing(ui, header, |ui| {
            if rows.is_empty() {
                ui.label(RichText::new(tr("No Sample Sets in the collection")).weak());
            }
            for (id, label) in rows {
                let response = ui
                    .selectable_label(self.source == Source::Set(id), &label)
                    .interact(egui::Sense::drag());
                response.dnd_set_drag_payload(SetDrag(id));
                if response.clicked() {
                    self.open_set(id);
                }
                response.context_menu(|ui| self.set_menu(ui, id));
            }
        });
        header.header_response.context_menu(|ui| {
            if ui.button(tr("Import Sample Set…")).clicked() {
                self.tree_actions.push(BrowserAction::ImportSet);
                ui.close();
            }
        });
    }

    fn set_menu(&mut self, ui: &mut Ui, id: i64) {
        for deck in 0..self.sample_decks.len() {
            if ui
                .add_enabled(
                    self.sample_decks[deck],
                    egui::Button::new(format!("{} {}", tr("Load into Deck"), DECK_NAMES[deck])),
                )
                .clicked()
            {
                self.tree_actions
                    .push(BrowserAction::LoadSet { deck, set: id });
                ui.close();
            }
        }
        if ui.button(tr("Export Sample Set…")).clicked() {
            self.tree_actions.push(BrowserAction::ExportSet(id));
            ui.close();
        }
        if ui.button(tr("Delete")).clicked() {
            ui.close();
            self.ask = Some(Ask::yes_no(
                "Delete",
                "Delete the selected sample sets?",
                BrowserAsk::DeleteSets(vec![id]),
            ));
        }
    }

    fn recordings_node(&mut self, ui: &mut Ui) {
        let response =
            ui.selectable_label(self.source == Source::Recordings, tr("⏺ Audio Recordings"));
        if response.clicked() {
            self.show(Source::Recordings);
        }
        response.context_menu(|ui| {
            if ui.button(tr("Analyze")).clicked() {
                let tracks = self.recorded_tracks();
                self.ask_analyse(tracks);
                ui.close();
            }
        });
    }

    fn loops_node(&mut self, ui: &mut Ui) {
        let response = ui.selectable_label(self.source == Source::Loops, tr("🔁 Loops"));
        if response.clicked() {
            self.show(Source::Loops);
        }
        if let Some(from) = response.dnd_release_payload::<crate::sample_view::LoopDrag>() {
            self.tree_actions.push(BrowserAction::SaveLoop(*from));
        }
        if response
            .dnd_hover_payload::<crate::sample_view::LoopDrag>()
            .is_some()
        {
            let accent = theme::get(ui.ctx()).accent;
            ui.painter().rect_stroke(
                response.rect,
                2.0,
                egui::Stroke::new(2.0, accent),
                egui::StrokeKind::Inside,
            );
        }
        response.context_menu(|ui| {
            if ui.button(tr("Analyze")).clicked() {
                let tracks = self.loop_tracks();
                self.ask_analyse(tracks);
                ui.close();
            }
        });
    }

    pub(crate) fn loop_tracks(&self) -> Vec<i64> {
        self.loops
            .and_then(|list| self.lib.as_ref().ok()?.node_tracks(list).ok())
            .unwrap_or_default()
    }

    pub(crate) fn recorded_tracks(&self) -> Vec<i64> {
        self.recordings
            .and_then(|list| self.lib.as_ref().ok()?.node_tracks(list).ok())
            .unwrap_or_default()
    }

    pub(super) fn history_tree(&mut self, ui: &mut Ui) {
        let current = self.current_history;
        let label = ui.selectable_label(
            current.is_some_and(|c| self.source == Source::History(c)),
            "🕘 History",
        );
        if label.clicked() {
            match current {
                Some(c) => self.show(Source::History(c)),
                None => self.message = Some("Nothing played yet in this session.".to_owned()),
            }
        }
        label.context_menu(|ui| {
            if let Some(c) = current
                && ui.button(tr("Export history…")).clicked()
            {
                ui.close();
                let name = self
                    .histories
                    .iter()
                    .find(|h| h.id == c)
                    .map_or_else(|| "history".to_owned(), |h| h.name.clone());
                self.export_history(c, &name);
            }
            if ui
                .button(tr("Import history files…"))
                .on_hover_text(tr(
                    "Backspn history files, or NML ones of another DJ application (only read)",
                ))
                .clicked()
            {
                ui.close();
                self.import_histories();
            }
        });
        let archived: Vec<backspn_library::HistorySession> = self
            .histories
            .iter()
            .rev()
            .filter(|h| Some(h.id) != current)
            .cloned()
            .collect();
        if archived.is_empty() || !self.shown(TreeNode::Archive) {
            return;
        }
        let header = egui::CollapsingHeader::new(tr("Archive"))
            .id_salt("history-archive")
            .default_open(false);
        crate::theme::collapsing(ui, header, |ui| {
            for h in archived {
                let response = ui.selectable_label(
                    self.source == Source::History(h.id),
                    format!("🕘 {}", h.name),
                );
                if response.clicked() {
                    self.show(Source::History(h.id));
                }
                response.context_menu(|ui| {
                    if ui.button(tr("Export history…")).clicked() {
                        ui.close();
                        self.export_history(h.id, &h.name);
                    }
                    if ui.button(tr("Delete archive")).clicked() {
                        let result = self
                            .library()
                            .map_or(Ok(()), |lib| lib.delete_history_session(h.id));
                        Self::report(result);
                        self.refresh();
                        ui.close();
                    }
                });
            }
        });
    }

    pub(super) fn folder_node(&mut self, ui: &mut Ui, dir: &Path, name: &str) -> egui::Response {
        let children = self
            .subdirs
            .entry(dir.to_path_buf())
            .or_insert_with(|| subfolders(dir))
            .clone();
        let selected = matches!(&self.source, Source::Folder(f) if f == dir);
        let label = RichText::new(format!("📁  {name}"));
        let response = if children.is_empty() {
            ui.selectable_label(selected, label)
        } else {
            let id = ui.make_persistent_id(("folder", dir));
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                false,
            );
            let mut response = None;
            state
                .show_header(ui, |ui| {
                    response = Some(ui.selectable_label(selected, label));
                })
                .body(|ui| {
                    for child in &children {
                        let name = child
                            .file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        self.folder_node(ui, child, &name);
                    }
                });
            response.expect("header shown")
        };
        if response.clicked() {
            self.show(Source::Folder(dir.to_path_buf()));
        }
        if !self.folders.iter().any(|f| f == dir) {
            response.context_menu(|ui| self.folder_menu(ui, dir));
        }
        response
    }

    fn folder_menu(&mut self, ui: &mut Ui, dir: &Path) {
        if ui.button(tr("Import Folder as Playlist")).clicked() {
            let name = dir
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            self.folder_playlist = Some((dir.to_path_buf(), name, None));
            ui.close();
        }
    }

    pub(super) fn subtree(&mut self, ui: &mut Ui, parent: Option<i64>) {
        let children: Vec<Node> = self
            .nodes
            .iter()
            .filter(|n| n.parent == parent)
            .cloned()
            .collect();
        for node in children {
            match node.kind {
                NodeKind::Folder => {
                    let header = egui::CollapsingHeader::new(node.name.clone())
                        .id_salt(("node", node.id))
                        .default_open(true);
                    let header =
                        crate::theme::collapsing(ui, header, |ui| self.subtree(ui, Some(node.id)));
                    let response = header.header_response.interact(egui::Sense::drag());
                    response.dnd_set_drag_payload(NodeDrag(node.id));
                    if let Some(drag) = response.dnd_release_payload::<NodeDrag>() {
                        self.move_node(drag.0, Some(node.id));
                    }
                    response.context_menu(|ui| self.node_menu(ui, Some(&node)));
                }
                NodeKind::Playlist | NodeKind::Smartlist => {
                    let icon = if node.kind == NodeKind::Playlist {
                        "♫"
                    } else {
                        "⚙"
                    };
                    let response = ui.selectable_label(
                        self.source == Source::Node(node.id),
                        format!("{icon} {}", node.name),
                    );
                    let response = response.interact(egui::Sense::drag());
                    response.dnd_set_drag_payload(NodeDrag(node.id));
                    if response.clicked() {
                        self.show(Source::Node(node.id));
                    }
                    if node.kind == NodeKind::Playlist
                        && response.dnd_hover_payload::<TrackDrag>().is_some()
                    {
                        let t = theme::get(ui.ctx());
                        ui.painter().rect_stroke(
                            response.rect,
                            0.0,
                            egui::Stroke::new(1.0, t.invert),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if node.kind == NodeKind::Playlist
                        && let Some(drag) = response.dnd_release_payload::<TrackDrag>()
                    {
                        let tracks: Vec<i64> = drag
                            .tracks
                            .iter()
                            .filter_map(|&id| self.resolve(id))
                            .collect();
                        let result = self.with_lib(|lib| lib.add_to_playlist(node.id, &tracks));
                        Self::report(result);
                        if self.source == Source::Node(node.id) {
                            self.recompute();
                        }
                    }
                    response.context_menu(|ui| self.node_menu(ui, Some(&node)));
                }
            }
        }
    }

    pub(super) fn move_node(&mut self, id: i64, parent: Option<i64>) {
        if id == parent.unwrap_or(-1) {
            return;
        }
        let old = crate::favorites::node_path(&self.nodes, id);
        let result = self.with_lib(|lib| lib.move_node(id, parent));
        Self::report(result);
        self.refresh();
        if let (Some(old), Some(new)) = (old, crate::favorites::node_path(&self.nodes, id)) {
            crate::favorites::retarget(&mut self.favorites, &old, &new);
        }
    }

    pub(super) fn list_tracks(&self, node: Option<&Node>) -> Vec<i64> {
        match node {
            None => self.rows.iter().map(|r| r.id).collect(),
            Some(n) => self
                .lib
                .as_ref()
                .map(|lib| lib.node_tracks(n.id).unwrap_or_default())
                .unwrap_or_default(),
        }
    }

    pub(super) fn list_menu(&mut self, ui: &mut Ui, node: Option<&Node>) {
        let folder = node.is_none_or(|n| n.kind == NodeKind::Folder);
        let parent = node.map(|n| n.id);
        if ui.button(tr("Analyze")).clicked() {
            let tracks = self.list_tracks(node);
            self.ask_analyse(tracks);
            ui.close();
        }
        if folder
            && ui
                .button(tr("Import playlist…"))
                .on_hover_text(tr("Backspn, NML or M3U"))
                .clicked()
        {
            self.import_list(parent);
            ui.close();
        }
        if (!folder || node.is_none())
            && ui
                .button(tr("Export playlist…"))
                .on_hover_text(tr("Backspn or M3U"))
                .clicked()
        {
            self.export_list(node);
            ui.close();
        }
        if ui
            .button(tr("Print list…"))
            .on_hover_text(tr("The list shown, as a web page"))
            .clicked()
        {
            self.print_list();
            ui.close();
        }
        if node.is_none() && ui.button(tr("Clear collection…")).clicked() {
            ui.close();
            if self.loaded.is_empty() {
                self.ask = Some(Ask::yes_no(
                    "Clear collection",
                    "Do you really want to clear your entire collection? The files stay on disk.",
                    BrowserAsk::ClearCollection,
                ));
            } else {
                crate::toast::warning("Unload the decks before clearing the collection.");
            }
        }
        if node.is_none() && ui.button(tr("Check consistency…")).clicked() {
            self.check_consistency();
            ui.close();
        }
        if ui.button(tr("Reset Played-State")).clicked() {
            let tracks = self.list_tracks(node);
            self.reset_played(&tracks);
            ui.close();
        }
    }

    pub(super) fn node_menu(&mut self, ui: &mut Ui, node: Option<&Node>) {
        let folder = node.is_none_or(|n| n.kind == NodeKind::Folder);
        let parent = node.map(|n| n.id);
        let mut result: Option<backspn_library::Result<i64>> = None;
        if folder {
            for (label, kind, base) in [
                ("New playlist", NodeKind::Playlist, "New Playlist"),
                ("New folder", NodeKind::Folder, "New Folder"),
            ] {
                if ui.button(tr(label)).clicked() {
                    self.new_playlist(parent, kind, base);
                    ui.close();
                }
            }
            if ui.button(tr("New smartlist")).clicked() {
                self.new_smartlist(parent);
                ui.close();
            }
            ui.separator();
        }
        self.list_menu(ui, node);
        if let Some(node) = node {
            ui.separator();
            if ui.button(tr("Rename…")).clicked() {
                self.renaming = Some((node.id, node.name.clone(), None));
                ui.close();
            }
            if node.kind == NodeKind::Playlist && ui.button(tr("Duplicate playlist")).clicked() {
                result = Some(self.with_lib_value(|lib| lib.duplicate_list(node.id)));
                ui.close();
            }
            if node.kind == NodeKind::Playlist
                && ui.button(tr("Remove Duplicates from Playlist")).clicked()
            {
                result =
                    Some(self.with_lib_value(|lib| {
                        lib.remove_duplicate_entries(node.id).map(|_| node.id)
                    }));
                ui.close();
            }
            if node.kind == NodeKind::Smartlist {
                if ui.button(tr("Edit smartlist…")).clicked() {
                    self.smart_editor = Some(SmartlistEditor::edit(
                        node.id,
                        node.name.clone(),
                        node.query.as_deref().unwrap_or_default(),
                    ));
                    ui.close();
                }
                if ui.button(tr("Duplicate smartlist")).clicked() {
                    result = Some(self.with_lib_value(|lib| lib.duplicate_list(node.id)));
                    ui.close();
                }
                if ui.button(tr("Save as playlist")).clicked() {
                    result = Some(self.with_lib_value(|lib| lib.smartlist_to_playlist(node.id)));
                    ui.close();
                }
            }
            if ui.button(tr("Delete")).clicked() {
                ui.close();
                let what = match node.kind {
                    NodeKind::Folder => "folder and everything in it",
                    NodeKind::Playlist => "playlist",
                    NodeKind::Smartlist => "smartlist",
                };
                self.ask = Some(Ask::yes_no(
                    "Delete",
                    format!("Do you really want to delete the selected {what}?"),
                    BrowserAsk::DeleteNode(node.id),
                ));
            }
        }
        if let Some(result) = result {
            Self::report(result.map(drop));
            self.refresh();
        }
    }

    fn new_playlist(&mut self, parent: Option<i64>, kind: NodeKind, base: &str) {
        let result = self.with_lib_value(|lib| {
            let name = lib.unique_node_name(parent, base)?;
            lib.create_node(parent, kind, &name, None)
        });
        Self::report(result.map(drop));
        self.refresh();
    }

    fn new_smartlist(&mut self, parent: Option<i64>) {
        let name = self
            .lib
            .as_ref()
            .ok()
            .and_then(|lib| lib.unique_node_name(parent, "New Smartlist").ok())
            .unwrap_or_default();
        self.smart_editor = Some(SmartlistEditor::create(parent, name));
    }

    pub(super) fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some((id, mut name, error)) = self.renaming.take() else {
            return;
        };
        match name_window(
            ctx,
            "Rename",
            "Please enter the new name.",
            &mut name,
            error.as_deref(),
        ) {
            NameAnswer::Open => self.renaming = Some((id, name, error)),
            NameAnswer::Cancel => {}
            NameAnswer::Ok => match self.with_lib(|lib| lib.rename_node_checked(id, &name)) {
                Ok(()) => self.refresh(),
                Err(e) => self.renaming = Some((id, name, Some(e.to_string()))),
            },
        }
    }

    pub(super) fn folder_playlist_dialog(&mut self, ctx: &egui::Context) {
        let Some((dir, mut name, error)) = self.folder_playlist.take() else {
            return;
        };
        match name_window(
            ctx,
            "Import Folder as Playlist",
            "Name of the new playlist:",
            &mut name,
            error.as_deref(),
        ) {
            NameAnswer::Open => self.folder_playlist = Some((dir, name, error)),
            NameAnswer::Cancel => {}
            NameAnswer::Ok => {
                if let Err(e) = self.import_folder_as_playlist(&dir, &name) {
                    self.folder_playlist = Some((dir, name, Some(e.to_string())));
                }
            }
        }
    }

    pub(super) fn import_folder_as_playlist(
        &mut self,
        dir: &Path,
        name: &str,
    ) -> backspn_library::Result<()> {
        let files = super::files::audio_files(dir);
        let lib = self
            .lib
            .as_mut()
            .map_err(|e| backspn_library::Error::Invalid(e.clone()))?;
        let list = lib.create_playlist_named(None, name)?;
        let tracks: Vec<i64> = files.iter().filter_map(|f| lib.add_file(f).ok()).collect();
        lib.add_to_playlist(list, &tracks)?;
        self.refresh();
        self.show(Source::Node(list));
        self.analyse_imported(&tracks);
        Ok(())
    }

    pub(super) fn smartlist_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.smart_editor.take() else {
            return;
        };
        let mut open = true;
        let cx = backspn_library::smart::Context {
            played: Some(&self.played),
            ..backspn_library::smart::Context::default()
        };
        let rows = &self.rows;
        let outcome = editor.ui(ctx, &mut open, |query| {
            backspn_library::Query::parse(query)
                .map_or(0, |q| rows.iter().filter(|t| q.matches_in(t, &cx)).count())
        });
        match outcome {
            Some(smartlist_editor::Outcome::Save {
                node,
                parent,
                name,
                query,
            }) => {
                let result = self.with_lib_value(|lib| {
                    let Some(id) = node else {
                        let name = lib.unique_node_name(parent, &name)?;
                        return lib.create_node(parent, NodeKind::Smartlist, &name, Some(&query));
                    };
                    lib.set_node_query(id, &query)?;
                    if lib.nodes()?.iter().any(|n| n.id == id && n.name != name) {
                        lib.rename_node_checked(id, &name)?;
                    }
                    Ok(id)
                });
                match result {
                    Ok(id) => {
                        self.refresh();
                        self.show(Source::Node(id));
                    }
                    Err(e) => {
                        crate::toast::error(e.to_string());
                        self.smart_editor = Some(editor);
                    }
                }
            }
            None if open => self.smart_editor = Some(editor),
            None => {}
        }
    }
}

enum NameAnswer {
    Open,
    Ok,
    Cancel,
}

fn name_window(
    ctx: &egui::Context,
    title: &str,
    prompt: &str,
    name: &mut String,
    error: Option<&str>,
) -> NameAnswer {
    let mut open = true;
    let mut answer = NameAnswer::Open;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show_closable(ctx, &mut open, |ui| {
            ui.label(prompt);
            let edit = ui.text_edit_singleline(name);
            edit.request_focus();
            if let Some(e) = error {
                ui.colored_label(theme::get(ctx).danger, e);
            }
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() || enter {
                    answer = NameAnswer::Ok;
                }
                if ui.button(tr("Cancel")).clicked() {
                    answer = NameAnswer::Cancel;
                }
            });
        });
    if open { answer } else { NameAnswer::Cancel }
}
