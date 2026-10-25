#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Focus {
    pub deck: usize,
    last: usize,
}

impl Focus {
    pub fn select(&mut self, deck: usize) {
        if deck != self.deck {
            self.last = self.deck;
            self.deck = deck;
        }
    }

    pub fn toggle(&mut self) {
        let to = if self.last == self.deck {
            usize::from(self.deck == 0)
        } else {
            self.last
        };
        self.select(to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_goes_back_to_the_deck_before() {
        let mut f = Focus::default();
        f.toggle();
        assert_eq!(f.deck, 1, "none before: A to B");
        f.select(3);
        f.toggle();
        assert_eq!(f.deck, 1);
        f.toggle();
        assert_eq!(f.deck, 3);
        f.select(3);
        f.toggle();
        assert_eq!(f.deck, 1);
    }
}
