//! The library as AI control uses it: search, tracks, lists and history,
//! through the same paths as the browser so its view stays current.

use backspn_library::{
    CuePoint, HistoryEntry, HistorySession, Library, Node, NodeKind, Query, TrackRow,
};

use super::{Browser, Source};
use crate::track_edit::Edit;

/// A list of the playlist tree with its size.
#[derive(Debug, Clone)]
pub struct ListInfo {
    pub node: Node,
    pub tracks: usize,
}

impl Browser {
    fn smart_context(&self) -> backspn_library::smart::Context<'_> {
        backspn_library::smart::Context {
            played: Some(&self.played),
            ..backspn_library::smart::Context::default()
        }
    }

    /// Library tracks that match a search, as the search field reads it.
    pub fn search_rows(&self, text: &str) -> Result<Vec<&TrackRow>, String> {
        let cx = self.smart_context();
        match Query::parse_search(text).map_err(|e| e.message)? {
            None => Ok(self.rows.iter().collect()),
            Some(query) => Ok(self
                .rows
                .iter()
                .filter(|t| query.matches_in(t, &cx))
                .collect()),
        }
    }

    pub fn track_cues(&self, id: i64) -> Vec<CuePoint> {
        self.lib
            .as_ref()
            .ok()
            .and_then(|lib| lib.cues(id).ok())
            .unwrap_or_default()
    }

    /// Writes tag and rating changes as the track editor does.
    pub fn edit_tracks(&mut self, ids: &[i64], edits: &[Edit]) {
        self.write_changes(ids, edits);
    }

    pub fn analyze_tracks(&mut self, ids: &[i64]) {
        let parts = self.default_parts();
        self.analyse(ids, parts);
    }

    fn library_ref(&self) -> Result<&Library, String> {
        self.lib.as_ref().map_err(Clone::clone)
    }

    fn node(&self, id: i64) -> Result<&Node, String> {
        self.nodes
            .iter()
            .find(|n| n.id == id && !backspn_library::playlists::is_hidden(&n.name))
            .ok_or_else(|| format!("no list with id {id}"))
    }

    fn folder(&self, id: i64) -> Result<(), String> {
        match self.node(id)?.kind {
            NodeKind::Folder => Ok(()),
            _ => Err(format!("list {id} is not a folder")),
        }
    }

    /// Visible folders, playlists and smartlists with their sizes.
    pub fn lists(&self) -> Vec<ListInfo> {
        let lib = self.library_ref().ok();
        let cx = self.smart_context();
        backspn_library::playlists::visible(&self.nodes)
            .into_iter()
            .map(|node| {
                let tracks = match node.kind {
                    NodeKind::Folder => 0,
                    _ => lib
                        .and_then(|lib| lib.node_tracks_in(node.id, &cx).ok())
                        .map_or(0, |t| t.len()),
                };
                ListInfo { node, tracks }
            })
            .collect()
    }

    pub fn list_contents(&self, id: i64) -> Result<Vec<i64>, String> {
        self.node(id)?;
        let cx = self.smart_context();
        self.library_ref()?
            .node_tracks_in(id, &cx)
            .map_err(|e| e.to_string())
    }

    fn change(
        &mut self,
        f: impl FnOnce(&mut Library) -> backspn_library::Result<i64>,
    ) -> Result<i64, String> {
        let result = self.with_lib_value(f).map_err(|e| e.to_string());
        self.refresh();
        result
    }

    pub fn create_list(
        &mut self,
        kind: NodeKind,
        name: &str,
        parent: Option<i64>,
        query: Option<&str>,
        tracks: &[i64],
    ) -> Result<i64, String> {
        if let Some(parent) = parent {
            self.folder(parent)?;
        }
        let query = match (kind, query) {
            (NodeKind::Smartlist, Some(q)) => {
                Query::parse(q).map_err(|e| format!("rule: {}", e.message))?;
                Some(q)
            }
            (NodeKind::Smartlist, None) => return Err("a smartlist needs a rule".to_owned()),
            (_, _) => None,
        };
        if !backspn_library::playlists::valid_name(name) {
            return Err("a name must not be empty, start with '_' or contain '$' or '\\'".into());
        }
        self.change(|lib| {
            let name = lib.unique_node_name(parent, name.trim())?;
            let id = lib.create_node(parent, kind, &name, query)?;
            if kind == NodeKind::Playlist && !tracks.is_empty() {
                lib.add_to_playlist(id, tracks)?;
            }
            Ok(id)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn edit_list(
        &mut self,
        id: i64,
        name: Option<&str>,
        parent: Option<i64>,
        to_top: bool,
        query: Option<&str>,
        add: &[i64],
        remove: &[i64],
    ) -> Result<(), String> {
        let kind = self.node(id)?.kind;
        if let Some(parent) = parent {
            self.folder(parent)?;
        }
        if query.is_some() && kind != NodeKind::Smartlist {
            return Err(format!("list {id} is not a smartlist"));
        }
        if (!add.is_empty() || !remove.is_empty()) && kind != NodeKind::Playlist {
            return Err(format!("list {id} is not a playlist"));
        }
        let positions: Vec<usize> = if remove.is_empty() {
            Vec::new()
        } else {
            self.list_contents(id)?
                .iter()
                .enumerate()
                .filter(|(_, t)| remove.contains(t))
                .map(|(i, _)| i)
                .collect()
        };
        self.change(|lib| {
            if let Some(name) = name {
                lib.rename_node_checked(id, name.trim())?;
            }
            if let Some(query) = query {
                lib.set_node_query(id, query)?;
            }
            if parent.is_some() || to_top {
                lib.move_node(id, parent)?;
            }
            if !positions.is_empty() {
                lib.remove_entries(id, &positions)?;
            }
            if !add.is_empty() {
                lib.add_to_playlist(id, add)?;
            }
            Ok(id)
        })
        .map(drop)
    }

    pub fn delete_list(&mut self, id: i64) -> Result<(), String> {
        self.node(id)?;
        self.change(|lib| lib.delete_node(id).map(|()| id))
            .map(drop)
    }

    pub fn show_list(&mut self, id: i64) -> Result<(), String> {
        self.node(id)?;
        self.show(Source::Node(id));
        Ok(())
    }

    pub fn history_sessions(&self) -> Result<Vec<(HistorySession, usize)>, String> {
        let lib = self.library_ref()?;
        let sessions = lib.history_sessions().map_err(|e| e.to_string())?;
        Ok(sessions
            .into_iter()
            .map(|s| {
                let n = lib.history_entries(s.id).map_or(0, |e| e.len());
                (s, n)
            })
            .collect())
    }

    pub fn history_entries(&self, id: i64) -> Result<Vec<HistoryEntry>, String> {
        self.library_ref()?
            .history_entries(id)
            .map_err(|e| e.to_string())
    }
}
