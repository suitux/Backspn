use std::sync::OnceLock;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the timecode and track state notices come with DVS"
    )
)]
pub enum Level {
    Error = 0,
    Warning = 1,
    Status = 2,
}

const LEVELS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    DeckLocked,
    DeckType,
    Unplayable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub short: String,
    pub long: String,
    pub level: Level,
    pub kept: bool,
}

impl Notice {
    pub fn auto(level: Level, short: &str, long: &str) -> Self {
        Self {
            short: short.to_owned(),
            long: long.to_owned(),
            level,
            kept: false,
        }
    }

    pub fn deck_locked() -> Self {
        Self::kept(
            Level::Warning,
            "Deck is playing",
            "Stop the deck before loading another track",
        )
    }

    pub fn deck_type() -> Self {
        Self::kept(
            Level::Warning,
            "A live input deck cannot load tracks",
            "Change its deck type first",
        )
    }

    pub fn unplayable() -> Self {
        Self::kept(
            Level::Warning,
            "This file cannot be played",
            "It may be missing, damaged or in a format the app cannot read.",
        )
    }

    pub fn live_input_locked() -> Self {
        Self::auto(
            Level::Warning,
            "Deck is playing",
            "Stop the deck before making it a live input",
        )
    }

    pub fn copy_error(long: &str) -> Self {
        Self::auto(Level::Warning, "Could not copy", long)
    }

    pub fn load_error(long: &str) -> Self {
        Self::auto(Level::Warning, "Could not load", long)
    }

    fn kept(level: Level, short: &str, long: &str) -> Self {
        Self {
            kept: true,
            ..Self::auto(level, short, long)
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    notice: Notice,
    key: Option<Key>,
    until: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Inactive,
    Active { start: f64, pulses: u32 },
    Idle { since: f64 },
}

const PULSE_S: f64 = 1.0;
const TURNS: u32 = 3;
const IDLE_S: f64 = 10.0;
pub const LOADING_WARNING_S: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Show<'a> {
    Track,
    Notice { notice: &'a Notice, bright: bool },
}

#[derive(Debug, Clone)]
pub struct DeckInformer {
    queues: [Vec<Entry>; LEVELS],
    current: Option<usize>,
    shown: Option<usize>,
    phase: Phase,
}

impl Default for DeckInformer {
    fn default() -> Self {
        Self {
            queues: std::array::from_fn(|_| Vec::new()),
            current: None,
            shown: None,
            phase: Phase::Inactive,
        }
    }
}

pub fn toast_text(deck: usize, notice: &Notice) -> String {
    let letter = char::from(b'A' + u8::try_from(deck % 26).unwrap_or(0));
    format!(
        "{} ({letter}): {}",
        crate::i18n::tr(&notice.short),
        crate::i18n::tr(&notice.long)
    )
}

pub fn now() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

impl DeckInformer {
    pub fn post(&mut self, notice: Notice, key: Option<Key>, until: Option<f64>, now: f64) {
        self.update(now);
        if let Some(key) = key {
            self.remove(key);
        }
        let level = notice.level as usize;
        let kept = notice.kept;
        self.queues[level].push(Entry { notice, key, until });
        self.current = self.queues.iter().position(|q| !q.is_empty());
        if self.current == Some(level) {
            self.enter_active(now);
        } else if !kept {
            self.queues[level].pop();
        }
    }

    pub fn remove(&mut self, key: Key) {
        for queue in &mut self.queues {
            queue.retain(|e| e.key != Some(key));
        }
    }

    #[cfg(test)]
    fn has(&self, key: Key) -> bool {
        self.queues.iter().flatten().any(|e| e.key == Some(key))
    }

    fn all_empty(&self) -> bool {
        self.queues.iter().all(Vec::is_empty)
    }

    fn current_queue(&self) -> Option<&Vec<Entry>> {
        self.current.and_then(|c| self.queues.get(c))
    }

    fn enter_active(&mut self, t: f64) {
        self.phase = if self.all_empty() {
            Phase::Inactive
        } else {
            Phase::Active {
                start: t,
                pulses: 0,
            }
        };
    }

    fn enter_idle(&mut self, t: f64) {
        if self.all_empty() {
            self.phase = Phase::Inactive;
            return;
        }
        for queue in &mut self.queues {
            queue.retain(|e| e.notice.kept);
        }
        self.phase = Phase::Idle { since: t };
    }

    fn next_notice(&mut self) {
        let len = self.current_queue().map_or(0, Vec::len);
        self.shown = match self.shown {
            Some(i) if i + 1 < len => Some(i + 1),
            _ if len > 0 => Some(0),
            _ => None,
        };
    }

    pub fn update(&mut self, now: f64) {
        for queue in &mut self.queues {
            queue.retain(|e| e.until.is_none_or(|u| now < u));
        }
        loop {
            match self.phase {
                Phase::Inactive => return,
                Phase::Active { start, pulses } => {
                    let turn = start + 2.0 * PULSE_S * f64::from(pulses);
                    if now < turn {
                        return;
                    }
                    if pulses >= TURNS || self.current_queue().is_none_or(Vec::is_empty) {
                        self.enter_idle(turn);
                        continue;
                    }
                    self.next_notice();
                    self.phase = Phase::Active {
                        start,
                        pulses: pulses + 1,
                    };
                }
                Phase::Idle { since } => {
                    if now < since + IDLE_S {
                        return;
                    }
                    self.enter_active(since + IDLE_S);
                }
            }
        }
    }

    pub fn show(&self, now: f64) -> Show<'_> {
        let Phase::Active { start, pulses } = self.phase else {
            return Show::Track;
        };
        let notice = self
            .current_queue()
            .zip(self.shown)
            .and_then(|(q, i)| q.get(i));
        match notice {
            Some(entry) if pulses > 0 => {
                let turn = start + 2.0 * PULSE_S * f64::from(pulses - 1);
                Show::Notice {
                    notice: &entry.notice,
                    bright: now - turn < PULSE_S,
                }
            }
            _ => Show::Track,
        }
    }

    pub fn next_change(&self, now: f64) -> Option<f64> {
        let at = match self.phase {
            Phase::Inactive => return None,
            Phase::Active { start, pulses } => {
                let turn = start + 2.0 * PULSE_S * f64::from(pulses.saturating_sub(1));
                if now < turn + PULSE_S {
                    turn + PULSE_S
                } else {
                    turn + 2.0 * PULSE_S
                }
            }
            Phase::Idle { since } => since + IDLE_S,
        };
        let until = self
            .queues
            .iter()
            .flatten()
            .filter_map(|e| e.until)
            .fold(at, f64::min);
        Some((until - now).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short(informer: &DeckInformer, now: f64) -> Option<(&str, bool)> {
        match informer.show(now) {
            Show::Notice { notice, bright } => Some((notice.short.as_str(), bright)),
            Show::Track => None,
        }
    }

    fn at(informer: &mut DeckInformer, now: f64) -> Option<(String, bool)> {
        informer.update(now);
        short(informer, now).map(|(s, b)| (s.to_owned(), b))
    }

    #[test]
    fn a_toast_names_the_deck() {
        assert_eq!(
            toast_text(1, &Notice::deck_locked()),
            "Deck is playing (B): Stop the deck before loading another track"
        );
    }

    #[test]
    fn three_turns_then_the_track_and_auto_notices_go() {
        let mut i = DeckInformer::default();
        i.post(
            Notice::copy_error("Load a track into the deck first"),
            None,
            None,
            0.0,
        );
        assert_eq!(at(&mut i, 0.0), Some(("Could not copy".into(), true)));
        assert_eq!(at(&mut i, 1.5), Some(("Could not copy".into(), false)));
        assert_eq!(at(&mut i, 5.9), Some(("Could not copy".into(), false)));
        assert_eq!(at(&mut i, 6.0), None, "the track again after 6 s");
        assert!(i.all_empty(), "an auto notice goes at the pause");
        assert_eq!(at(&mut i, 20.0), None);
    }

    #[test]
    fn a_kept_notice_comes_back_after_the_pause() {
        let mut i = DeckInformer::default();
        i.post(Notice::deck_locked(), Some(Key::DeckLocked), None, 0.0);
        assert_eq!(at(&mut i, 7.0), None);
        assert_eq!(at(&mut i, 16.5), Some(("Deck is playing".into(), true)));
        i.remove(Key::DeckLocked);
        assert_eq!(at(&mut i, 18.5), None, "removed: back to the track");
    }

    #[test]
    fn loading_warnings_go_after_their_time() {
        let mut i = DeckInformer::default();
        let until = Some(LOADING_WARNING_S);
        i.post(Notice::deck_locked(), Some(Key::DeckLocked), until, 0.0);
        assert!(at(&mut i, 2.9).is_some());
        assert!(at(&mut i, 3.0).is_none() && !i.has(Key::DeckLocked));
        i.post(
            Notice::deck_locked(),
            Some(Key::DeckLocked),
            Some(13.0),
            10.0,
        );
        i.post(
            Notice::deck_locked(),
            Some(Key::DeckLocked),
            Some(14.0),
            11.0,
        );
        assert_eq!(i.queues[Level::Warning as usize].len(), 1);
    }

    #[test]
    fn the_more_serious_go_first_and_the_less_wait_or_drop() {
        assert!(Level::Error < Level::Warning && Level::Warning < Level::Status);
        let mut i = DeckInformer::default();
        let error = Notice::auto(Level::Error, "Bad", "");
        i.post(error, None, None, 0.0);
        i.post(Notice::live_input_locked(), None, None, 0.5);
        assert_eq!(i.queues[Level::Warning as usize].len(), 0);
        i.post(Notice::unplayable(), Some(Key::Unplayable), None, 0.5);
        assert_eq!(at(&mut i, 1.0), Some(("Bad".into(), false)));
    }

    #[test]
    fn notices_of_one_level_take_turns_every_two_seconds() {
        let mut i = DeckInformer::default();
        i.post(Notice::deck_locked(), Some(Key::DeckLocked), None, 0.0);
        i.post(Notice::unplayable(), Some(Key::Unplayable), None, 0.0);
        let first = at(&mut i, 0.1).unwrap().0;
        let second = at(&mut i, 2.1).unwrap().0;
        assert_ne!(first, second);
        assert_eq!(at(&mut i, 4.1).unwrap().0, first);
    }
}
