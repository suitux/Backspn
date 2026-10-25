use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const MAX_KEYS: usize = 100;
const MAX_KEY_LEN: usize = 32;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tally(BTreeMap<String, u64>);

impl Tally {
    pub fn add(&mut self, key: &str) {
        self.add_n(key, 1);
    }

    pub fn add_n(&mut self, key: &str, n: u64) {
        if key.is_empty() || key.len() > MAX_KEY_LEN || n == 0 {
            return;
        }
        if let Some(count) = self.0.get_mut(key) {
            *count = count.saturating_add(n);
        } else if self.0.len() < MAX_KEYS {
            self.0.insert(key.to_owned(), n);
        }
    }

    #[cfg(test)]
    pub fn get(&self, key: &str) -> u64 {
        self.0.get(key).copied().unwrap_or(0)
    }

    pub fn merge(&mut self, other: &Self) {
        for (key, n) in &other.0 {
            self.add_n(key, *n);
        }
    }

    fn subtract(&mut self, sent: &Self) {
        for (key, n) in &sent.0 {
            if let Some(count) = self.0.get_mut(key) {
                *count = count.saturating_sub(*n);
            }
        }
        self.0.retain(|_, n| *n > 0);
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

macro_rules! usage {
    ($($field:ident),* $(,)?) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(default, rename_all = "camelCase")]
        pub struct Usage {
            $(pub $field: u64,)*
            pub inputs: Tally,
            pub fx: Tally,
            pub imports: Tally,
        }

        impl Usage {
            pub fn add(&mut self, other: &Self) {
                $(self.$field = self.$field.saturating_add(other.$field);)*
                self.inputs.merge(&other.inputs);
                self.fx.merge(&other.fx);
                self.imports.merge(&other.imports);
            }

            pub fn subtract(&mut self, sent: &Self) {
                $(self.$field = self.$field.saturating_sub(sent.$field);)*
                self.inputs.subtract(&sent.inputs);
                self.fx.subtract(&sent.fx);
                self.imports.subtract(&sent.imports);
            }

            pub fn is_empty(&self) -> bool {
                $(self.$field == 0 &&)* self.inputs.is_empty()
                    && self.fx.is_empty()
                    && self.imports.is_empty()
            }
        }
    };
}

usage!(
    sessions,
    seconds_open,
    seconds_playing,
    tracks_loaded,
    tracks_played,
    mixes,
    recordings,
    seconds_recording,
    stems_separated,
    syncs,
    hotcues,
    loops,
    beat_jumps,
    samples,
    auto_fades,
    link_sessions,
    crashes,
    unclean_exits,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_sent_comes_off_and_the_rest_stays() {
        let mut pending = Usage {
            mixes: 5,
            ..Usage::default()
        };
        pending.fx.add_n("filter", 3);
        let sent = pending.clone();
        pending.mixes += 2;
        pending.fx.add("delay");
        pending.subtract(&sent);
        assert_eq!(pending.mixes, 2);
        assert_eq!(pending.fx.get("filter"), 0);
        assert_eq!(pending.fx.get("delay"), 1);
        pending.subtract(&pending.clone());
        assert!(pending.is_empty());
    }

    #[test]
    fn tallies_stay_small() {
        let mut t = Tally::default();
        for i in 0..MAX_KEYS + 10 {
            t.add(&format!("k{i}"));
        }
        t.add(&"x".repeat(MAX_KEY_LEN + 1));
        assert_eq!(t.0.len(), MAX_KEYS);
        t.add("k0");
        assert_eq!(t.get("k0"), 2);
    }

    #[test]
    fn fields_are_camel_case() {
        let json = serde_json::to_string(&Usage::default()).unwrap();
        assert!(json.contains("\"secondsPlaying\":0"), "{json}");
        assert!(json.contains("\"uncleanExits\":0"), "{json}");
    }
}
