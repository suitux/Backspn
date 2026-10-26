use std::path::PathBuf;

use backspn_library::Node;

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

pub fn step(current: Option<usize>, step: i32, filled: &[bool]) -> Option<usize> {
    let is_filled = |i: usize| filled.get(i).copied().unwrap_or(false);
    match (current, step.signum()) {
        (None, _) => (0..SLOTS).find(|&i| is_filled(i)),
        (Some(c), 1) => (c + 1..SLOTS).find(|&i| is_filled(i)),
        (Some(c), -1) => (0..c).rev().find(|&i| is_filled(i)),
        (Some(_), _) => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Collection,
    History,
    Recordings,
    Loops,
    Node(i64),
    Archive(i64),
    Set(i64),
    Folder(PathBuf),
    External {
        library: String,
        at: Vec<usize>,
        name: String,
    },
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
    let inside = format!("{old}/t");
    for slot in slots.iter_mut() {
        if slot == old {
            new.clone_into(slot);
        } else if let Some(rest) = slot.strip_prefix(&inside) {
            *slot = format!("{new}/t{rest}");
        }
    }
}

pub fn encode(nodes: &[Node], target: &Target) -> Option<String> {
    Some(match target {
        Target::Collection => "$COLLECTION".to_owned(),
        Target::History => "$HISTORY".to_owned(),
        Target::Recordings => "$RECORDER".to_owned(),
        Target::Loops => "$LOOPS".to_owned(),
        Target::Node(id) => node_path(nodes, *id)?,
        Target::Archive(id) => format!("$ARCHIVE/{id}"),
        Target::Set(id) => format!("$SET/{id}"),
        Target::Folder(dir) => format!("$FOLDER/{}", dir.display()),
        Target::External { library, at, name } => {
            let at: Vec<String> = at.iter().map(usize::to_string).collect();
            format!("$EXTERNAL/{library}/{}/{name}", at.join("."))
        }
    })
}

pub fn resolve(nodes: &[Node], slot: &str) -> Option<Target> {
    match slot {
        "$COLLECTION" => return Some(Target::Collection),
        "$HISTORY" => return Some(Target::History),
        "$RECORDER" => return Some(Target::Recordings),
        "$LOOPS" => return Some(Target::Loops),
        _ => {}
    }
    if let Some(id) = slot.strip_prefix("$ARCHIVE/") {
        return id.parse().ok().map(Target::Archive);
    }
    if let Some(id) = slot.strip_prefix("$SET/") {
        return id.parse().ok().map(Target::Set);
    }
    if let Some(dir) = slot.strip_prefix("$FOLDER/") {
        return Some(Target::Folder(PathBuf::from(dir)));
    }
    if let Some(rest) = slot.strip_prefix("$EXTERNAL/") {
        let mut parts = rest.splitn(3, '/');
        let library = parts.next()?.to_owned();
        let at = parts.next()?;
        let at = if at.is_empty() {
            Vec::new()
        } else {
            at.split('.')
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?
        };
        let name = parts.next()?.to_owned();
        return Some(Target::External { library, at, name });
    }
    let rest = slot.strip_prefix("$PLAYLIST/t")?;
    let mut parent = None;
    for name in rest.split("/t") {
        let node = nodes
            .iter()
            .find(|n| n.parent == parent && n.name == name)?;
        parent = Some(node.id);
    }
    parent.map(Target::Node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use backspn_library::NodeKind;

    #[test]
    fn slots_follow_a_moved_node() {
        let mut slots = defaults();
        slots[3] = "$PLAYLIST/tGigs".to_owned();
        slots[4] = "$PLAYLIST/tGigs/tPeak".to_owned();
        slots[5] = "$PLAYLIST/tGigsOld".to_owned();
        retarget(&mut slots, "$PLAYLIST/tGigs", "$PLAYLIST/t2025/tGigs");
        assert_eq!(slots[3], "$PLAYLIST/t2025/tGigs");
        assert_eq!(
            slots[4], "$PLAYLIST/t2025/tGigs/tPeak",
            "children move along"
        );
        assert_eq!(slots[5], "$PLAYLIST/tGigsOld", "not a name prefix");
    }

    #[test]
    fn encoder_steps_one_favorite_and_stops_at_the_ends() {
        let all = [true; SLOTS];
        assert_eq!(step(None, 1, &all), Some(0));
        assert_eq!(step(Some(3), 1, &all), Some(4));
        assert_eq!(step(Some(3), 4, &all), Some(4), "one slot per click");
        assert_eq!(step(Some(3), -1, &all), Some(2));
        assert_eq!(step(Some(0), -1, &all), None);
        assert_eq!(step(Some(SLOTS - 1), 1, &all), None);
    }

    #[test]
    fn encoder_skips_empty_slots() {
        let mut filled = [false; SLOTS];
        filled[1] = true;
        filled[2] = true;
        filled[7] = true;
        assert_eq!(step(None, 1, &filled), Some(1), "first filled, not slot 0");
        assert_eq!(step(Some(2), 1, &filled), Some(7));
        assert_eq!(step(Some(7), -1, &filled), Some(2));
        assert_eq!(step(Some(1), -1, &filled), None);
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
        assert_eq!(resolve(&nodes, "$PLAYLIST/tGigs"), Some(Target::Node(2)));
        assert_eq!(resolve(&nodes, "$PLAYLIST/tMissing"), None);
        assert_eq!(resolve(&nodes, "$HISTORY"), Some(Target::History));
        assert_eq!(resolve(&nodes, "$RECORDER"), Some(Target::Recordings));
        assert_eq!(resolve(&nodes, ""), None);
        assert_eq!(defaults().len(), SLOTS);
        for target in [
            Target::Collection,
            Target::Loops,
            Target::Node(3),
            Target::Archive(7),
            Target::Set(4),
            Target::Folder(PathBuf::from("/Music/House")),
            Target::External {
                library: "serato".into(),
                at: vec![0, 3],
                name: "Crate / Peak".into(),
            },
            Target::External {
                library: "music".into(),
                at: Vec::new(),
                name: "Music".into(),
            },
        ] {
            let slot = encode(&nodes, &target).expect("encodes");
            assert_eq!(resolve(&nodes, &slot), Some(target), "{slot}");
        }
    }
}
