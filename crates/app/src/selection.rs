use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    rows: BTreeSet<usize>,
    cursor: Option<usize>,
    anchor: Option<usize>,
}

impl Selection {
    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    pub fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.rows.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn contains(&self, row: usize) -> bool {
        self.rows.contains(&row)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn select(&mut self, row: usize) {
        self.rows = BTreeSet::from([row]);
        self.cursor = Some(row);
        self.anchor = Some(row);
    }

    pub fn toggle(&mut self, row: usize) {
        if !self.rows.remove(&row) {
            self.rows.insert(row);
        }
        self.cursor = Some(row);
        self.anchor = Some(row);
    }

    pub fn extend_to(&mut self, row: usize) {
        let anchor = self.anchor.unwrap_or(row);
        self.rows = (anchor.min(row)..=anchor.max(row)).collect();
        self.cursor = Some(row);
        self.anchor = Some(anchor);
    }

    pub fn select_all(&mut self, len: usize) {
        self.rows = (0..len).collect();
        if self.cursor.is_none_or(|c| c >= len) {
            self.cursor = len.checked_sub(1);
        }
    }

    pub fn set_cursor(&mut self, row: usize) {
        self.rows.insert(row);
        self.cursor = Some(row);
        self.anchor = Some(row);
    }

    pub fn click(&mut self, row: usize, shift: bool, command: bool) {
        if shift {
            self.extend_to(row);
        } else if command {
            self.toggle(row);
        } else {
            self.select(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(s: &Selection) -> Vec<usize> {
        s.rows().collect()
    }

    #[test]
    fn clicks_select_toggle_and_extend() {
        let mut s = Selection::default();
        s.click(2, false, false);
        assert_eq!(rows(&s), [2]);
        s.click(5, true, false);
        assert_eq!(rows(&s), [2, 3, 4, 5], "shift: range from the anchor");
        s.click(0, true, false);
        assert_eq!(rows(&s), [0, 1, 2], "the anchor stays");
        s.click(7, false, true);
        assert_eq!(rows(&s), [0, 1, 2, 7], "command adds");
        s.click(1, false, true);
        assert_eq!(rows(&s), [0, 2, 7], "and removes");
        assert_eq!(s.cursor(), Some(1));
        s.select_all(4);
        assert_eq!(rows(&s), [0, 1, 2, 3]);
        s.clear();
        assert_eq!((s.len(), s.cursor()), (0, None));
    }
}
