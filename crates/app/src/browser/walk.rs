//! How the tree encoder walks the browser tree: every row in the order the
//! tree shows it, the headers of the groups included, and what its push
//! opens or closes.

use std::path::{Path, PathBuf};

use backspn_library::external::ExternalNode;
use backspn_library::{Node, NodeKind};
use eframe::egui;

use super::external::node_at;
use super::files::subfolders;
use super::{Browser, Outside, Source, TreeNode};

/// A group of the tree that holds lists but is not a list itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Group {
    /// A folder of playlists.
    Playlists(i64),
    SampleSets,
    Archive,
    /// The History row while nothing has been played yet.
    History,
}

/// Something of the tree that opens and closes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Fold {
    Group(Group),
    /// A folder of the computer.
    Dir(PathBuf),
    /// A folder of another library.
    Outside(Outside, Vec<usize>),
}

impl Fold {
    /// Folders of playlists start open, the rest closed.
    fn opens_by_default(&self) -> bool {
        matches!(self, Self::Group(Group::Playlists(_)))
    }
}

/// A row the tree encoder stops at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Stop {
    Group(Group),
    Source(Source),
}

impl Browser {
    pub(super) fn is_open(&self, fold: &Fold) -> bool {
        self.folds
            .get(fold)
            .copied()
            .unwrap_or_else(|| fold.opens_by_default())
    }

    /// The state a fold must take this frame, when a controller changed it.
    pub(super) fn forced_open(&mut self, fold: &Fold) -> Option<bool> {
        (self.toggled.as_ref() == Some(fold)).then(|| {
            self.toggled = None;
            self.is_open(fold)
        })
    }

    /// Keeps the state the tree drew a fold with, clicks included.
    pub(super) fn remember_fold(&mut self, ctx: &egui::Context, id: egui::Id, fold: Fold) {
        if let Some(state) = egui::collapsing_header::CollapsingState::load(ctx, id) {
            self.folds.insert(fold, state.is_open());
        }
    }

    pub(super) fn tree_cursor_on(&self, group: Group) -> bool {
        self.tree_cursor == Some(group)
    }

    /// The rows the tree encoder steps through, from top to bottom.
    pub(super) fn stops(&self) -> Vec<Stop> {
        let mut out = vec![Stop::Source(Source::Collection)];
        if self.shown(TreeNode::Music) && self.music.is_some() {
            self.outside_stops(Outside::Music, &mut out);
        }
        if self.shown(TreeNode::SampleSets) {
            out.push(Stop::Group(Group::SampleSets));
            if self.is_open(&Fold::Group(Group::SampleSets)) {
                out.extend(self.sets.iter().map(|s| Stop::Source(Source::Set(s.id))));
            }
        }
        for (node, source) in [
            (TreeNode::Recordings, Source::Recordings),
            (TreeNode::Loops, Source::Loops),
        ] {
            if self.shown(node) {
                out.push(Stop::Source(source));
            }
        }
        if self.shown(TreeNode::History) {
            self.history_stops(&mut out);
        }
        self.playlist_stops(None, &mut out);
        for root in &self.folders {
            self.folder_stops(root, &mut out);
        }
        if self.shown(TreeNode::DjSoftware) {
            for i in 0..self.programs.len() {
                self.outside_stops(Outside::Program(i), &mut out);
            }
        }
        if self.library_xml.is_some() {
            self.outside_stops(Outside::Xml, &mut out);
        }
        out
    }

    fn history_stops(&self, out: &mut Vec<Stop>) {
        let current = self.current_history;
        out.push(current.map_or(Stop::Group(Group::History), |c| {
            Stop::Source(Source::History(c))
        }));
        let archived: Vec<i64> = self
            .histories
            .iter()
            .rev()
            .map(|h| h.id)
            .filter(|&id| Some(id) != current)
            .collect();
        if archived.is_empty() || !self.shown(TreeNode::Archive) {
            return;
        }
        out.push(Stop::Group(Group::Archive));
        if self.is_open(&Fold::Group(Group::Archive)) {
            out.extend(
                archived
                    .into_iter()
                    .map(|id| Stop::Source(Source::History(id))),
            );
        }
    }

    fn playlist_stops(&self, parent: Option<i64>, out: &mut Vec<Stop>) {
        let mut children: Vec<&Node> = self.nodes.iter().filter(|n| n.parent == parent).collect();
        children.sort_by_key(|n| n.position);
        for node in children {
            if node.kind == NodeKind::Folder {
                let group = Group::Playlists(node.id);
                out.push(Stop::Group(group));
                if self.is_open(&Fold::Group(group)) {
                    self.playlist_stops(Some(node.id), out);
                }
            } else {
                out.push(Stop::Source(Source::Node(node.id)));
            }
        }
    }

    fn folder_stops(&self, dir: &Path, out: &mut Vec<Stop>) {
        out.push(Stop::Source(Source::Folder(dir.to_path_buf())));
        if self.is_open(&Fold::Dir(dir.to_path_buf())) {
            for child in self.subdirs.get(dir).into_iter().flatten() {
                self.folder_stops(child, out);
            }
        }
    }

    fn outside_stops(&self, kind: Outside, out: &mut Vec<Stop>) {
        fn walk(
            b: &Browser,
            kind: Outside,
            nodes: &[ExternalNode],
            at: &[usize],
            out: &mut Vec<Stop>,
        ) {
            for (i, node) in nodes.iter().enumerate() {
                let mut path = at.to_vec();
                path.push(i);
                out.push(Stop::Source(Source::External(kind, path.clone())));
                if let ExternalNode::Folder { children, .. } = node
                    && b.is_open(&Fold::Outside(kind, path.clone()))
                {
                    walk(b, kind, children, &path, out);
                }
            }
        }
        out.push(Stop::Source(Source::External(kind, Vec::new())));
        if let Some(lib) = self.outside_library(kind) {
            walk(self, kind, &lib.playlists, &[], out);
        }
    }

    pub fn move_source(&mut self, delta: i32) {
        let stops = self.stops();
        let here = match self.tree_cursor {
            Some(group) => Stop::Group(group),
            None => Stop::Source(self.source.clone()),
        };
        let at = stops.iter().position(|s| *s == here).unwrap_or(0) as i64;
        let next = (at + i64::from(delta)).clamp(0, stops.len() as i64 - 1) as usize;
        match stops[next].clone() {
            Stop::Group(group) => self.tree_cursor = Some(group),
            Stop::Source(Source::External(kind, path)) => {
                self.selection.clear();
                self.open_outside(kind, path);
            }
            Stop::Source(source) => {
                self.selection.clear();
                self.show(source);
            }
        }
    }

    /// Opens or closes the row the tree encoder is on, when it holds more.
    pub fn toggle_tree_open(&mut self) {
        let fold = match (self.tree_cursor, &self.source) {
            (Some(Group::History), _) => return,
            (Some(group), _) => Fold::Group(group),
            (None, Source::Folder(dir)) => {
                let dir = dir.clone();
                let children = self
                    .subdirs
                    .entry(dir.clone())
                    .or_insert_with(|| subfolders(&dir));
                if children.is_empty() {
                    return;
                }
                Fold::Dir(dir)
            }
            (None, Source::External(kind, at)) => {
                let folder = self
                    .outside_library(*kind)
                    .and_then(|lib| node_at(lib, at))
                    .is_some_and(|n| matches!(n, ExternalNode::Folder { .. }));
                if !folder {
                    return;
                }
                Fold::Outside(*kind, at.clone())
            }
            _ => return,
        };
        let open = !self.is_open(&fold);
        self.folds.insert(fold.clone(), open);
        self.toggled = Some(fold);
    }
}
