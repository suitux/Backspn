use backspn_engine::{DECKS, FX_UNITS};

use crate::App;
use crate::layouts::{DeckSetup, ScreenLayout};
use crate::status::DeckType;

impl App {
    pub(crate) fn deck_setup(&self) -> DeckSetup {
        DeckSetup {
            deck_types: std::array::from_fn(|d| {
                DeckType::of(self.deck_kind(d), self.decks[d].live)
            }),
            sample_sets: std::array::from_fn(|d| {
                self.decks[d].sample_deck.as_ref().and_then(|v| v.set_id)
            }),
            effects: self.fx.effects(),
        }
    }

    pub(crate) fn active_layout(&self) -> Option<usize> {
        crate::layouts::active(
            &self.settings.layouts,
            &ScreenLayout::of(&self.settings),
            &self.deck_setup(),
        )
    }

    pub(crate) fn apply_layout(&mut self, index: usize) {
        let Some(saved) = self.settings.layouts.get(index).cloned() else {
            return;
        };
        saved.layout.apply_to(&mut self.settings);
        for (deck, &size) in self.decks.iter_mut().zip(&self.settings.deck_sizes) {
            deck.set_size(size);
        }
        let mut out = Vec::new();
        for u in 0..FX_UNITS {
            self.fx.set_kit(u, &saved.layout.fx_kits[u], &mut out);
            self.fx.set_mode(u, saved.layout.fx_modes[u], &mut out);
            self.fx.set_routing(u, saved.layout.fx_routing[u], &mut out);
        }
        self.fx.set_effects(&saved.setup.effects, &mut out);
        self.outbox.extend(out);
        let state = self.engine_state();
        for d in 0..DECKS {
            let wanted = saved.setup.deck_types[d];
            if DeckType::of(self.deck_kind(d), self.decks[d].live) != wanted {
                self.set_deck_type(d, wanted, &state);
            }
            let loaded = self.decks[d].sample_deck.as_ref().and_then(|v| v.set_id);
            if let Some(id) =
                crate::layouts::set_to_load(wanted, saved.setup.sample_sets[d], loaded)
            {
                self.load_sample_set(d, id);
            }
        }
        self.settings.status = self.current_status(&state);
    }
}
