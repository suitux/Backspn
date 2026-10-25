use backspn_library::{Node, NodeKind};

pub const SLOTS: usize = 12;

pub fn defaults() -> Vec<String> {
    let mut slots = vec![
        "$PLAYLIST/tPreparation".to_owned(),
        "$HISTORY".to_owned(),
        "$COLLECTION".to_owned(),
    ];
    slots.resize(SLOTS, String::new());
    slots
}

pub fn step(current: Option<usize>, step: i32) -> usize {
    match (current, step.signum()) {
        (None, _) => 0,
        (Some(c), 1) => (c + 1).min(SLOTS - 1),
        (Some(c), -1) => c.saturating_sub(1),
        (Some(c), _) => c,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Collection,
    History,
    Recordings,
    Node(i64),
}

pub fn node_path(nodes: &[Node], id: i64) -> Option<String> {
    let mut names = Vec::new();
    let mut at = Some(id);
    while let Some(i) = at {
        let node = nodes.iter().find(|n| n.id == i)?;
        names.push(node.name.as_str());
        at = node.parent;
    }
    names.reverse();
    Some(format!("$PLAYLIST/t{}", names.join("/t")))
}

pub fn retarget(slots: &mut [String], old: &str, new: &str) {
    for slot in slots.iter_mut().filter(|s| *s == old) {
        new.clone_into(slot);
    }
}

pub fn resolve(nodes: &[Node], slot: &str) -> Option<Target> {
    match slot {
        "$COLLECTION" => return Some(Target::Collection),
        "$HISTORY" => return Some(Target::History),
        "$RECORDER" => return Some(Target::Recordings),
        _ => {}
    }
    let rest = slot.strip_prefix("$PLAYLIST/t")?;
    let mut parent = None;
    let mut found = None;
    for name in rest.split("/t") {
        let node = nodes
            .iter()
            .find(|n| n.parent == parent && n.name == name)?;
        parent = Some(node.id);
        found = Some(node);
    }
    found
        .filter(|n| n.kind != NodeKind::Folder)
        .map(|n| Target::Node(n.id))
}

pub fn label(nodes: &[Node], slot: &str) -> String {
    match resolve(nodes, slot) {
        Some(Target::Collection) => "Collection".to_owned(),
        Some(Target::History) => "History".to_owned(),
        Some(Target::Recordings) => "Audio Recordings".to_owned(),
        Some(Target::Node(id)) => nodes
            .iter()
            .find(|n| n.id == id)
            .map_or_else(String::new, |n| n.name.clone()),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_follow_a_moved_node() {
        let mut slots = defaults();
        slots[3] = "$PLAYLIST/tGigs".to_owned();
        slots[4] = "$PLAYLIST/tGigs/tPeak".to_owned();
        retarget(&mut slots, "$PLAYLIST/tGigs", "$PLAYLIST/t2025/tGigs");
        assert_eq!(slots[3], "$PLAYLIST/t2025/tGigs");
        assert_eq!(slots[4], "$PLAYLIST/tGigs/tPeak", "only the exact path");
    }

    #[test]
    fn encoder_steps_one_favorite_and_stops_at_the_ends() {
        assert_eq!(step(None, 1), 0);
        assert_eq!(step(Some(3), 1), 4);
        assert_eq!(step(Some(3), 4), 4, "one slot per click");
        assert_eq!(step(Some(3), -1), 2);
        assert_eq!(step(Some(0), -1), 0);
        assert_eq!(step(Some(SLOTS - 1), 1), SLOTS - 1);
    }

    fn node(id: i64, parent: Option<i64>, kind: NodeKind, name: &str) -> Node {
        Node {
            id,
            parent,
            position: 0,
            kind,
            name: name.into(),
            uuid: None,
            query: None,
        }
    }

    #[test]
    fn slots_round_trip_through_their_text_form() {
        let nodes = [
            node(1, None, NodeKind::Playlist, "Preparation"),
            node(2, None, NodeKind::Folder, "Gigs"),
            node(3, Some(2), NodeKind::Smartlist, "Peak"),
        ];
        assert_eq!(
            node_path(&nodes, 3).as_deref(),
            Some("$PLAYLIST/tGigs/tPeak")
        );
        assert_eq!(
            resolve(&nodes, "$PLAYLIST/tGigs/tPeak"),
            Some(Target::Node(3))
        );
        assert_eq!(
            resolve(&nodes, "$PLAYLIST/tPreparation"),
            Some(Target::Node(1))
        );
        assert_eq!(
            resolve(&nodes, "$PLAYLIST/tGigs"),
            None,
            "folders are not lists"
        );
        assert_eq!(resolve(&nodes, "$HISTORY"), Some(Target::History));
        assert_eq!(resolve(&nodes, "$RECORDER"), Some(Target::Recordings));
        assert_eq!(resolve(&nodes, ""), None);
        assert_eq!(label(&nodes, "$COLLECTION"), "Collection");
        assert_eq!(defaults().len(), SLOTS);
    }
}
