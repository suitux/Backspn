use backspn_engine::{CueKind, DeckState, HOTCUES, HotcuePoint};
use backspn_library::CuePoint;

use crate::track_store::HotcueSet;

const NEW_NAME: &str = "n.n.";
const NAVIGATE_FROM_S: f64 = 0.000_1;
const AT_PLAYHEAD_S: f64 = 1e-10;
const SAME_S: f64 = 1e-6;

pub fn hotcue_point(cue: &CuePoint) -> HotcuePoint {
    HotcuePoint {
        start_s: cue.start_s,
        kind: CueKind::from(cue.kind),
        loop_s: (cue.kind == CuePoint::LOOP && cue.len_s > 0.0).then_some(cue.len_s),
    }
}

pub fn active_cue(s: &DeckState) -> Option<HotcuePoint> {
    if !s.loaded {
        return None;
    }
    Some(match (s.loop_active, s.loop_start_s, s.loop_end_s) {
        (true, Some(start_s), Some(end_s)) if end_s > start_s => HotcuePoint {
            start_s,
            kind: CueKind::Loop,
            loop_s: Some(end_s - start_s),
        },
        _ => HotcuePoint {
            start_s: s.cue_s,
            kind: CueKind::Cue,
            loop_s: None,
        },
    })
}

pub fn cue_here(s: &DeckState) -> Option<HotcuePoint> {
    let start_s = match s.grid {
        Some(grid) if s.snap => grid.nearest_beat(s.position_s).max(0.0),
        _ => s.position_s,
    };
    s.loaded.then_some(HotcuePoint {
        start_s,
        kind: CueKind::Cue,
        loop_s: None,
    })
}

pub fn loop_length(s: &DeckState, start_s: f64, beats: f64) -> Option<f64> {
    let len = beats * s.grid?.period_s();
    (start_s + len <= s.duration_s).then_some(len)
}

fn same_point(a: &HotcuePoint, b: &HotcuePoint) -> bool {
    a.kind == b.kind
        && (a.start_s - b.start_s).abs() < SAME_S
        && (a.loop_s.unwrap_or(0.0) - b.loop_s.unwrap_or(0.0)).abs() < SAME_S
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CueList {
    cues: Vec<(u32, CuePoint)>,
    next_id: u32,
    selected: Option<u32>,
}

impl CueList {
    pub fn new(cues: &[CuePoint]) -> Self {
        let mut list = Self::default();
        for cue in cues {
            if cue.kind != CuePoint::GRID || cue.hotcue.is_some() {
                list.insert(cue.clone());
            }
        }
        list
    }

    pub fn cues(&self) -> impl Iterator<Item = &CuePoint> {
        self.cues.iter().map(|(_, c)| c)
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected.and_then(|id| self.index_of(id))
    }

    pub fn hotcues(&self) -> HotcueSet {
        let mut set = [None; HOTCUES];
        for (_, cue) in &self.cues {
            if let Some(slot) = cue.hotcue.map(usize::from).filter(|&s| s < HOTCUES) {
                set[slot] = Some(hotcue_point(cue));
            }
        }
        set
    }

    pub fn store(&mut self, active: HotcuePoint) {
        let id = self.insert(new_cue(active));
        self.auto_map();
        self.selected = Some(id);
    }

    pub fn map_hotcue(&mut self, active: HotcuePoint, slot: usize) {
        let Ok(slot) = u8::try_from(slot) else { return };
        if usize::from(slot) >= HOTCUES {
            return;
        }
        for (_, cue) in &mut self.cues {
            if cue.hotcue == Some(slot) {
                cue.hotcue = None;
            }
        }
        let id = self.insert(CuePoint {
            hotcue: Some(slot),
            ..new_cue(active)
        });
        self.selected = Some(id);
    }

    pub fn select_hotcue(&mut self, slot: usize) {
        if let Some((id, _)) = self
            .cues
            .iter()
            .find(|(_, c)| c.hotcue.map(usize::from) == Some(slot))
        {
            self.selected = Some(*id);
        }
    }

    pub fn selected_cue(&self) -> Option<&CuePoint> {
        self.selected().map(|i| &self.cues[i].1)
    }

    pub fn select(&mut self, index: usize) -> Option<HotcuePoint> {
        let (id, cue) = self.cues.get(index)?;
        self.selected = Some(*id);
        Some(hotcue_point(cue))
    }

    pub fn rename_selected(&mut self, name: &str) -> bool {
        let Some(index) = self.selected() else {
            return false;
        };
        let cue = &mut self.cues[index].1;
        if cue.name == name {
            return false;
        }
        name.clone_into(&mut cue.name);
        true
    }

    pub fn delete_selected(&mut self) -> bool {
        let Some(index) = self.selected() else {
            return false;
        };
        self.cues.remove(index);
        self.selected = None;
        true
    }

    pub fn navigate(&mut self, forward: bool, position_s: f64) -> Option<HotcuePoint> {
        let selectable: Vec<usize> = (0..self.cues.len())
            .filter(|&i| self.cues[i].1.kind != CuePoint::GRID)
            .collect();
        let first = *selectable.first()?;
        let last = *selectable.last()?;
        let start = |i: usize| self.cues[i].1.start_s;
        let current = self
            .selected()
            .filter(|i| selectable.contains(i))
            .filter(|&i| (start(i) - position_s).abs() < AT_PLAYHEAD_S);
        let target = match current {
            Some(i) if forward => selectable.iter().copied().find(|&j| j > i).unwrap_or(i),
            Some(i) => selectable
                .iter()
                .copied()
                .rev()
                .find(|&j| j < i)
                .unwrap_or(i),
            None if forward => {
                let from = position_s + NAVIGATE_FROM_S;
                selectable
                    .iter()
                    .copied()
                    .find(|&j| start(j) > from)
                    .or_else(|| selectable.iter().copied().rev().find(|&j| start(j) < from))
                    .unwrap_or(last)
            }
            None => {
                let from = position_s - NAVIGATE_FROM_S;
                selectable
                    .iter()
                    .copied()
                    .rev()
                    .find(|&j| start(j) < from)
                    .unwrap_or(first)
            }
        };
        self.selected = Some(self.cues[target].0);
        Some(hotcue_point(&self.cues[target].1))
    }

    pub fn move_selected(&mut self, delta_s: f64) -> bool {
        let Some(index) = self.selected() else {
            return false;
        };
        let (id, mut cue) = self.cues.remove(index);
        cue.start_s = (cue.start_s + delta_s).max(0.0);
        self.place(id, cue);
        true
    }

    pub fn set_selected_type(&mut self, kind: CueKind, loop_s: Option<f64>) -> bool {
        let Some(index) = self.selected() else {
            return false;
        };
        let cue = &mut self.cues[index].1;
        let kind = i32::from(kind);
        if cue.kind == kind {
            return false;
        }
        cue.kind = kind;
        if let Some(len) = loop_s.filter(|_| kind == CuePoint::LOOP) {
            cue.len_s = len;
        }
        true
    }

    pub fn sync_hotcues(&mut self, deck: &HotcueSet) -> bool {
        let mine = self.hotcues();
        let mut changed = false;
        for (slot, (theirs, ours)) in deck.iter().zip(mine).enumerate() {
            let slot_u8 = slot as u8;
            let at = self
                .cues
                .iter()
                .position(|(_, c)| c.hotcue == Some(slot_u8));
            match (theirs, ours, at) {
                (Some(t), Some(o), _) if same_point(t, &o) => continue,
                (None, None, _) | (None, Some(_), None) => continue,
                (None, _, Some(index)) => {
                    let (id, _) = self.cues.remove(index);
                    if self.selected == Some(id) {
                        self.selected = None;
                    }
                }
                (Some(t), _, Some(index)) => {
                    let (id, mut cue) = self.cues.remove(index);
                    cue.kind = i32::from(t.kind);
                    cue.start_s = t.start_s;
                    cue.len_s = t.loop_s.unwrap_or(0.0);
                    self.place(id, cue);
                }
                (Some(t), _, None) => {
                    self.insert(CuePoint {
                        hotcue: Some(slot_u8),
                        ..new_cue(*t)
                    });
                }
            }
            changed = true;
        }
        changed
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.cues.iter().position(|(i, _)| *i == id)
    }

    fn insert(&mut self, cue: CuePoint) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.place(id, cue);
        id
    }

    fn place(&mut self, id: u32, cue: CuePoint) {
        let at = self.cues.partition_point(|(_, c)| c.start_s <= cue.start_s);
        self.cues.insert(at, (id, cue));
    }

    fn auto_map(&mut self) {
        for i in 0..self.cues.len() {
            if self.cues[i].1.hotcue.is_some() || self.cues[i].1.kind == CuePoint::GRID {
                continue;
            }
            let free =
                (0..HOTCUES as u8).find(|s| self.cues.iter().all(|(_, c)| c.hotcue != Some(*s)));
            let Some(free) = free else { return };
            self.cues[i].1.hotcue = Some(free);
        }
    }
}

fn new_cue(active: HotcuePoint) -> CuePoint {
    let looping = active.kind == CueKind::Loop;
    CuePoint {
        kind: if looping {
            CuePoint::LOOP
        } else {
            CuePoint::CUE
        },
        name: NEW_NAME.into(),
        start_s: active.start_s,
        len_s: if looping {
            active.loop_s.unwrap_or(0.0)
        } else {
            0.0
        },
        hotcue: None,
        display_order: 0,
        repeats: -1,
        color: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(kind: i32, start_s: f64, hotcue: Option<u8>) -> CuePoint {
        CuePoint {
            kind,
            name: "x".into(),
            start_s,
            len_s: 0.0,
            hotcue,
            display_order: 0,
            repeats: -1,
            color: None,
        }
    }

    fn at(start_s: f64) -> HotcuePoint {
        HotcuePoint {
            start_s,
            kind: CueKind::Cue,
            loop_s: None,
        }
    }

    fn starts(list: &CueList) -> Vec<f64> {
        list.cues().map(|c| c.start_s).collect()
    }

    fn slots(list: &CueList) -> Vec<Option<u8>> {
        list.cues().map(|c| c.hotcue).collect()
    }

    #[test]
    fn store_keeps_the_active_loop_or_the_cue_point() {
        let mut s = DeckState {
            loaded: true,
            cue_s: 3.0,
            position_s: 5.1,
            duration_s: 10.0,
            loop_start_s: Some(4.0),
            loop_end_s: Some(6.0),
            ..DeckState::default()
        };
        assert_eq!(active_cue(&s), Some(at(3.0)));
        s.loop_active = true;
        assert_eq!(
            active_cue(&s),
            Some(HotcuePoint {
                start_s: 4.0,
                kind: CueKind::Loop,
                loop_s: Some(2.0)
            })
        );
        assert_eq!(cue_here(&s), Some(at(5.1)));
        s.grid = Some(backspn_engine::Grid {
            bpm: 120.0,
            anchor_s: 0.0,
            ..backspn_engine::Grid::FIXED
        });
        assert_eq!(cue_here(&s), Some(at(5.1)), "without snap");
        s.snap = true;
        assert_eq!(cue_here(&s), Some(at(5.0)));
        assert_eq!(loop_length(&s, 1.0, 4.0), Some(2.0));
        assert_eq!(loop_length(&s, 9.0, 4.0), None, "past the end");
        s.grid = None;
        assert_eq!(loop_length(&s, 1.0, 4.0), None, "no grid");
        assert_eq!(active_cue(&DeckState::default()), None);
    }

    #[test]
    fn unmapped_grid_markers_are_left_out_and_cues_are_sorted() {
        let list = CueList::new(&[
            cue(CuePoint::CUE, 5.0, Some(1)),
            cue(CuePoint::GRID, 0.1, None),
            cue(CuePoint::LOAD, 2.0, None),
            cue(CuePoint::GRID, 9.0, Some(0)),
        ]);
        assert_eq!(starts(&list), [2.0, 5.0, 9.0]);
        let hotcues = list.hotcues();
        assert_eq!(hotcues[0].map(|h| h.kind), Some(CueKind::Grid));
        assert_eq!(hotcues[1].map(|h| h.start_s), Some(5.0));
    }

    #[test]
    fn store_names_types_and_maps_every_unmapped_cue() {
        let mut list = CueList::new(&[
            cue(CuePoint::CUE, 1.0, Some(0)),
            cue(CuePoint::LOAD, 2.0, None),
        ]);
        list.store(HotcuePoint {
            start_s: 3.0,
            kind: CueKind::Loop,
            loop_s: Some(2.0),
        });
        let new: Vec<_> = list.cues().collect();
        assert_eq!(
            (new[2].kind, new[2].len_s, new[2].name.as_str()),
            (CuePoint::LOOP, 2.0, "n.n.")
        );
        assert_eq!(slots(&list), [Some(0), Some(1), Some(2)]);
        assert_eq!(list.selected(), Some(2));
        list.store(HotcuePoint {
            kind: CueKind::FadeIn,
            ..at(0.5)
        });
        assert_eq!(
            list.cues().next().map(|c| (c.kind, c.hotcue)),
            Some((CuePoint::CUE, Some(3)))
        );
    }

    #[test]
    fn store_leaves_cues_unmapped_when_all_hotcues_are_taken() {
        let full: Vec<_> = (0..8)
            .map(|s| cue(CuePoint::CUE, f64::from(s), Some(s)))
            .collect();
        let mut list = CueList::new(&full);
        list.store(at(20.0));
        assert_eq!(list.cues().last().map(|c| c.hotcue), Some(None));
    }

    #[test]
    fn map_hotcue_takes_the_slot_from_another_cue() {
        let mut list = CueList::new(&[cue(CuePoint::CUE, 1.0, Some(2))]);
        list.map_hotcue(at(4.0), 2);
        assert_eq!(slots(&list), [None, Some(2)]);
        assert_eq!(list.hotcues()[2].map(|h| h.start_s), Some(4.0));
        list.map_hotcue(at(5.0), 8);
        assert_eq!(starts(&list), [1.0, 4.0], "slot out of range");
    }

    #[test]
    fn picking_and_renaming() {
        let mut list = CueList::new(&[
            cue(CuePoint::CUE, 1.0, None),
            cue(CuePoint::LOOP, 2.0, None),
        ]);
        assert!(!list.rename_selected("a"));
        assert_eq!(list.select(5), None);
        assert_eq!(list.select(1).map(|p| p.kind), Some(CueKind::Loop));
        assert!(list.rename_selected("Drop"));
        assert!(!list.rename_selected("Drop"), "unchanged");
        assert_eq!(list.selected_cue().map(|c| c.name.as_str()), Some("Drop"));
    }

    #[test]
    fn delete_removes_the_selected_cue() {
        let mut list = CueList::new(&[
            cue(CuePoint::CUE, 1.0, Some(0)),
            cue(CuePoint::CUE, 2.0, None),
        ]);
        assert!(!list.delete_selected(), "nothing selected");
        list.select_hotcue(0);
        assert!(list.delete_selected());
        assert_eq!(starts(&list), [2.0]);
        assert_eq!(list.selected(), None);
    }

    #[test]
    fn navigation_from_the_playhead_skips_grid_markers() {
        let mut list = CueList::new(&[
            cue(CuePoint::CUE, 1.0, None),
            cue(CuePoint::GRID, 2.0, Some(0)),
            cue(CuePoint::FADE_OUT, 3.0, None),
        ]);
        assert_eq!(list.navigate(true, 0.0).map(|p| p.start_s), Some(1.0));
        assert_eq!(list.navigate(true, 1.5).map(|p| p.start_s), Some(3.0));
        assert_eq!(list.navigate(true, 5.0).map(|p| p.start_s), Some(3.0));
        assert_eq!(list.navigate(false, 2.9).map(|p| p.start_s), Some(1.0));
        assert_eq!(list.navigate(false, 0.5).map(|p| p.start_s), Some(1.0));
        assert_eq!(CueList::default().navigate(true, 0.0), None);
    }

    #[test]
    fn navigation_steps_through_cues_at_the_same_place() {
        let mut list = CueList::new(&[
            cue(CuePoint::CUE, 1.0, None),
            cue(CuePoint::LOAD, 1.0, None),
            cue(CuePoint::CUE, 2.0, None),
        ]);
        list.navigate(true, 0.0);
        assert_eq!(list.selected(), Some(0));
        list.navigate(true, 1.0);
        assert_eq!(list.selected(), Some(1));
        list.navigate(false, 1.0);
        assert_eq!(list.selected(), Some(0));
        list.navigate(false, 1.0);
        assert_eq!(list.selected(), Some(0), "first one: stays");
    }

    #[test]
    fn moving_keeps_the_selection_and_the_order() {
        let mut list = CueList::new(&[
            cue(CuePoint::CUE, 1.0, Some(0)),
            cue(CuePoint::CUE, 2.0, None),
        ]);
        assert!(!list.move_selected(1.0));
        list.select_hotcue(0);
        assert!(list.move_selected(1.5));
        assert_eq!(starts(&list), [2.0, 2.5]);
        assert_eq!(list.selected(), Some(1));
        list.move_selected(-10.0);
        assert_eq!(starts(&list), [0.0, 2.0]);
    }

    #[test]
    fn type_change_gives_a_new_loop_its_length() {
        let mut list = CueList::new(&[cue(CuePoint::CUE, 1.0, Some(0))]);
        list.select_hotcue(0);
        assert!(list.set_selected_type(CueKind::Loop, Some(2.0)));
        assert_eq!(list.hotcues()[0].and_then(|h| h.loop_s), Some(2.0));
        assert!(
            !list.set_selected_type(CueKind::Loop, Some(4.0)),
            "unchanged"
        );
        assert!(list.set_selected_type(CueKind::Load, Some(4.0)));
        assert_eq!(
            list.hotcues()[0].map(|h| (h.kind, h.loop_s)),
            Some((CueKind::Load, None))
        );
    }

    #[test]
    fn hotcues_changed_on_the_deck_update_the_list() {
        let mut list = CueList::new(&[
            cue(CuePoint::LOAD, 1.0, Some(0)),
            cue(CuePoint::CUE, 2.0, Some(1)),
            cue(CuePoint::CUE, 3.0, None),
        ]);
        let mut deck = list.hotcues();
        assert!(!list.sync_hotcues(&deck));
        deck[0] = deck[0].map(|h| HotcuePoint {
            start_s: h.start_s + 1e-9,
            ..h
        });
        assert!(!list.sync_hotcues(&deck), "rounding is no change");
        list.select_hotcue(1);
        deck[1] = None;
        deck[4] = Some(at(0.5));
        assert!(list.sync_hotcues(&deck));
        assert_eq!(starts(&list), [0.5, 1.0, 3.0]);
        assert_eq!(slots(&list), [Some(4), Some(0), None]);
        assert_eq!(list.selected(), None, "the deleted cue was selected");
        assert_eq!(list.cues().next().map(|c| c.name.as_str()), Some("n.n."));
    }
}
