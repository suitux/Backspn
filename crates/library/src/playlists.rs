use std::collections::HashSet;

use rusqlite::params;

use crate::{Error, Library, Node, NodeKind, Result};

pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && !name.starts_with('_') && !name.contains(['$', '\\'])
}

pub const RECORDINGS: &str = "_RECORDINGS";
pub const LOOPS: &str = "_LOOPS";

pub fn is_hidden(name: &str) -> bool {
    name.starts_with('_')
}

pub fn visible(nodes: &[Node]) -> Vec<Node> {
    let mut hidden = HashSet::new();
    nodes
        .iter()
        .filter(|n| {
            let out = is_hidden(&n.name) || n.parent.is_some_and(|p| hidden.contains(&p));
            if out {
                hidden.insert(n.id);
            }
            !out
        })
        .cloned()
        .collect()
}

const INVALID_NAME: &str = "A name must not be empty, start with '_' or contain '$' or '\\'.";

pub fn unique_name(base: &str, taken: &[&str]) -> String {
    let free = |n: &str| !taken.contains(&n);
    if free(base) {
        return base.to_owned();
    }
    (2..=taken.len() + 2)
        .map(|i| format!("{base} ({i})"))
        .find(|n| free(n))
        .unwrap_or_else(|| base.to_owned())
}

impl Library {
    pub fn recordings_list(&mut self) -> Result<i64> {
        let found = self
            .nodes()?
            .into_iter()
            .find(|n| n.parent.is_none() && n.kind == NodeKind::Playlist && n.name == RECORDINGS);
        match found {
            Some(n) => Ok(n.id),
            None => self.create_node(None, NodeKind::Playlist, RECORDINGS, None),
        }
    }

    pub fn loops_list(&mut self) -> Result<i64> {
        let found = self
            .nodes()?
            .into_iter()
            .find(|n| n.parent.is_none() && n.kind == NodeKind::Playlist && n.name == LOOPS);
        match found {
            Some(n) => Ok(n.id),
            None => self.create_node(None, NodeKind::Playlist, LOOPS, None),
        }
    }

    fn sibling_names(&self, parent: Option<i64>) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM nodes WHERE parent IS ?1")?;
        let rows = stmt.query_map([parent], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn unique_node_name(&self, parent: Option<i64>, base: &str) -> Result<String> {
        let names = self.sibling_names(parent)?;
        Ok(unique_name(
            base,
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
        ))
    }

    pub fn create_playlist_named(&mut self, parent: Option<i64>, name: &str) -> Result<i64> {
        if !valid_name(name) {
            return Err(Error::Invalid(INVALID_NAME.into()));
        }
        let name = self.unique_node_name(parent, name.trim())?;
        self.create_node(parent, NodeKind::Playlist, &name, None)
    }

    pub fn rename_node_checked(&mut self, id: i64, name: &str) -> Result<()> {
        if !valid_name(name) {
            return Err(Error::Invalid(INVALID_NAME.into()));
        }
        let parent: Option<i64> =
            self.conn
                .query_row("SELECT parent FROM nodes WHERE id = ?1", [id], |r| r.get(0))?;
        let taken: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE parent IS ?1 AND name = ?2 AND id <> ?3",
            params![parent, name, id],
            |r| r.get(0),
        )?;
        if taken > 0 {
            return Err(Error::Invalid(format!(
                "There is already a playlist or folder named \"{name}\" here."
            )));
        }
        self.rename_node(id, name)
    }

    pub fn duplicate_list(&mut self, id: i64) -> Result<i64> {
        let (parent, name, kind, query): (Option<i64>, String, String, Option<String>) =
            self.conn.query_row(
                "SELECT parent, name, kind, query FROM nodes WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        let kind = NodeKind::parse(&kind);
        if kind == NodeKind::Folder {
            return Err(Error::Invalid("Folders cannot be duplicated.".into()));
        }
        let name = self.unique_node_name(parent, &name)?;
        let copy = self.create_node(parent, kind, &name, query.as_deref())?;
        self.conn.execute(
            "INSERT INTO playlist_entries (node_id, position, track_id, key) \
             SELECT ?2, position, track_id, key FROM playlist_entries WHERE node_id = ?1",
            params![id, copy],
        )?;
        Ok(copy)
    }

    pub fn smartlist_to_playlist(&mut self, id: i64) -> Result<i64> {
        let (parent, name): (Option<i64>, String) =
            self.conn
                .query_row("SELECT parent, name FROM nodes WHERE id = ?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
        let tracks = self.node_tracks(id)?;
        let name = self.unique_node_name(parent, &name)?;
        let playlist = self.create_node(parent, NodeKind::Playlist, &name, None)?;
        self.add_to_playlist(playlist, &tracks)?;
        Ok(playlist)
    }

    fn entry_rows(&self, node: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT rowid FROM playlist_entries WHERE node_id = ?1 AND track_id IS NOT NULL \
             ORDER BY position",
        )?;
        let rows = stmt.query_map([node], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn reorder(&mut self, node: i64, order: &[usize]) -> Result<()> {
        let rows = self.entry_rows(node)?;
        let tx = self.conn.transaction()?;
        for (position, &i) in order.iter().enumerate() {
            tx.execute(
                "UPDATE playlist_entries SET position = ?2 WHERE rowid = ?1",
                params![rows[i], position as i64],
            )?;
        }
        tx.execute(
            "UPDATE playlist_entries SET position = position + ?2 WHERE node_id = ?1 AND track_id IS NULL",
            params![node, order.len() as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn consolidate_playlist(&mut self, node: i64, shown: &[usize]) -> Result<()> {
        let count = self.entry_rows(node)?.len();
        let mut order: Vec<usize> = shown.iter().copied().filter(|&i| i < count).collect();
        let listed: std::collections::HashSet<usize> = order.iter().copied().collect();
        order.extend((0..count).filter(|i| !listed.contains(i)));
        self.reorder(node, &order)
    }

    pub fn move_playlist_entries(&mut self, node: i64, indices: &[usize], to: usize) -> Result<()> {
        let count = self.entry_rows(node)?.len();
        let moving: std::collections::BTreeSet<usize> =
            indices.iter().copied().filter(|&i| i < count).collect();
        let before = (0..to.min(count)).filter(|i| !moving.contains(i));
        let after = (to.min(count)..count).filter(|i| !moving.contains(i));
        let order: Vec<usize> = before.chain(moving.iter().copied()).chain(after).collect();
        self.reorder(node, &order)
    }

    pub fn remove_entries(&mut self, node: i64, indices: &[usize]) -> Result<()> {
        let rows = self.entry_rows(node)?;
        let tx = self.conn.transaction()?;
        for &i in indices {
            if let Some(row) = rows.get(i) {
                tx.execute("DELETE FROM playlist_entries WHERE rowid = ?1", [row])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_duplicate_entries(&mut self, node: i64) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT rowid, track_id FROM playlist_entries \
             WHERE node_id = ?1 AND track_id IS NOT NULL ORDER BY position",
        )?;
        let entries: Vec<(i64, i64)> = stmt
            .query_map([node], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        let mut seen = std::collections::HashSet::new();
        let repeated: Vec<i64> = entries
            .into_iter()
            .filter(|&(_, track)| !seen.insert(track))
            .map(|(row, _)| row)
            .collect();
        let tx = self.conn.transaction()?;
        for row in &repeated {
            tx.execute("DELETE FROM playlist_entries WHERE rowid = ?1", [row])?;
        }
        tx.commit()?;
        Ok(repeated.len())
    }

    pub fn playlists_with(&self, track: i64) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT node_id FROM playlist_entries WHERE track_id = ?1")?;
        let ids: Vec<i64> = stmt
            .query_map([track], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(self
            .nodes()?
            .into_iter()
            .map(|n| n.id)
            .filter(|id| ids.contains(id))
            .collect())
    }

    pub fn move_node(&mut self, id: i64, parent: Option<i64>) -> Result<()> {
        let now: Option<i64> =
            self.conn
                .query_row("SELECT parent FROM nodes WHERE id = ?1", [id], |r| r.get(0))?;
        if now == parent {
            return Ok(());
        }
        let mut up = parent;
        while let Some(p) = up {
            if p == id {
                return Err(Error::Invalid("A folder cannot go inside itself.".into()));
            }
            up = self
                .conn
                .query_row("SELECT parent FROM nodes WHERE id = ?1", [p], |r| r.get(0))?;
        }
        let name: String =
            self.conn
                .query_row("SELECT name FROM nodes WHERE id = ?1", [id], |r| r.get(0))?;
        let name = self.unique_node_name(parent, &name)?;
        let position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM nodes WHERE parent IS ?1",
            [parent],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "UPDATE nodes SET parent = ?2, position = ?3, name = ?4 WHERE id = ?1",
            params![id, parent, position, name],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn library_with_playlist(n: usize) -> (Library, i64, Vec<i64>) {
        let mut lib = Library::open_in_memory().unwrap();
        let tracks: Vec<i64> = (0..n)
            .map(|i| lib.add_file(Path::new(&format!("/m/{i}.mp3"))).unwrap())
            .collect();
        let list = lib
            .create_node(None, NodeKind::Playlist, "Set", None)
            .unwrap();
        lib.add_to_playlist(list, &tracks).unwrap();
        (lib, list, tracks)
    }

    #[test]
    fn names_follow_the_rules() {
        assert!(valid_name("Friday"));
        for bad in ["", "  ", "_hidden", "a$b", "a\\b"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        let taken = ["Set", "Set (2)"];
        assert_eq!(unique_name("Set", &taken), "Set (3)");
        assert_eq!(unique_name("New", &taken), "New");
    }

    #[test]
    fn duplicate_and_save_as_playlist_copy_the_tracks() {
        let (mut lib, list, tracks) = library_with_playlist(3);
        let copy = lib.duplicate_list(list).unwrap();
        assert_eq!(lib.node_tracks(copy).unwrap(), tracks);
        let name = |lib: &Library, id| {
            lib.nodes()
                .unwrap()
                .into_iter()
                .find(|n| n.id == id)
                .unwrap()
                .name
        };
        assert_eq!(name(&lib, copy), "Set (2)");
        let smart = lib
            .create_node(None, NodeKind::Smartlist, "All", Some("$TITLE % \"\""))
            .unwrap();
        let saved = lib.smartlist_to_playlist(smart).unwrap();
        let twin = lib.duplicate_list(smart).unwrap();
        assert_eq!(
            lib.node_tracks(twin).unwrap(),
            lib.node_tracks(smart).unwrap()
        );
        assert_eq!(name(&lib, saved), "All (2)");
        assert_eq!(
            lib.node_tracks(saved).unwrap(),
            lib.node_tracks(smart).unwrap()
        );
        assert!(lib.rename_node_checked(copy, "Set").is_err(), "taken");
        assert!(lib.rename_node_checked(copy, "_x").is_err(), "invalid");
        lib.rename_node_checked(copy, "Saturday").unwrap();
    }

    #[test]
    fn entries_move_reorder_and_go() {
        let (mut lib, list, t) = library_with_playlist(5);
        lib.move_playlist_entries(list, &[3, 4], 1).unwrap();
        assert_eq!(
            lib.node_tracks(list).unwrap(),
            [t[0], t[3], t[4], t[1], t[2]]
        );
        lib.move_playlist_entries(list, &[0], 5).unwrap();
        assert_eq!(
            lib.node_tracks(list).unwrap(),
            [t[3], t[4], t[1], t[2], t[0]]
        );
        lib.consolidate_playlist(list, &[4, 2, 3]).unwrap();
        assert_eq!(
            lib.node_tracks(list).unwrap(),
            [t[0], t[1], t[2], t[3], t[4]]
        );
        lib.remove_entries(list, &[1, 3]).unwrap();
        assert_eq!(lib.node_tracks(list).unwrap(), [t[0], t[2], t[4]]);
    }

    #[test]
    fn repeated_entries_go_keeping_the_first() {
        let (mut lib, list, t) = library_with_playlist(3);
        lib.add_to_playlist(list, &[t[2], t[0], t[2]]).unwrap();
        assert_eq!(lib.remove_duplicate_entries(list).unwrap(), 3);
        assert_eq!(lib.node_tracks(list).unwrap(), [t[0], t[1], t[2]]);
        assert_eq!(lib.remove_duplicate_entries(list).unwrap(), 0);
    }

    #[test]
    fn finds_the_playlists_of_a_track() {
        let (mut lib, list, t) = library_with_playlist(2);
        let other = lib
            .create_node(None, NodeKind::Playlist, "Other", None)
            .unwrap();
        lib.add_to_playlist(other, &[t[1]]).unwrap();
        assert_eq!(lib.playlists_with(t[0]).unwrap(), [list]);
        assert_eq!(lib.playlists_with(t[1]).unwrap(), [list, other]);
    }

    #[test]
    fn nodes_move_between_folders() {
        let (mut lib, list, _) = library_with_playlist(1);
        let folder = lib
            .create_node(None, NodeKind::Folder, "Gigs", None)
            .unwrap();
        let inner = lib
            .create_node(Some(folder), NodeKind::Folder, "2025", None)
            .unwrap();
        lib.move_node(list, Some(inner)).unwrap();
        lib.move_node(list, Some(inner)).unwrap();
        assert_eq!(
            lib.nodes()
                .unwrap()
                .iter()
                .find(|n| n.id == list)
                .unwrap()
                .parent,
            Some(inner)
        );
        let name = |lib: &Library| {
            lib.nodes()
                .unwrap()
                .into_iter()
                .find(|n| n.id == list)
                .unwrap()
                .name
        };
        assert_eq!(name(&lib), "Set", "moving where it is changes nothing");
        assert!(
            lib.move_node(folder, Some(inner)).is_err(),
            "not into itself"
        );
    }

    #[test]
    fn the_recordings_list_is_made_once_and_hidden() {
        let (mut lib, list, _) = library_with_playlist(1);
        let rec = lib.recordings_list().unwrap();
        assert_eq!(lib.recordings_list().unwrap(), rec, "found again");
        let folder = lib
            .create_node(None, NodeKind::Folder, "_Own", None)
            .unwrap();
        let inner = lib
            .create_node(Some(folder), NodeKind::Playlist, "Inner", None)
            .unwrap();
        let shown: Vec<i64> = visible(&lib.nodes().unwrap())
            .iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(
            shown,
            [list],
            "hidden nodes and what is under them stay out"
        );
        assert!(!shown.contains(&inner));
    }

    #[test]
    fn a_named_playlist_is_checked_then_made_unique() {
        let (mut lib, _, _) = library_with_playlist(0);
        assert!(lib.create_playlist_named(None, "_x").is_err());
        let id = lib.create_playlist_named(None, "Set").unwrap();
        let node = lib
            .nodes()
            .unwrap()
            .into_iter()
            .find(|n| n.id == id)
            .unwrap();
        assert_eq!(node.name, "Set (2)");
    }
}
