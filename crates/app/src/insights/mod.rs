mod report;
mod usage;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub use report::{Audio, Config, Library, Os, Report, SCHEMA, slug};
pub use usage::{Tally, Usage};

pub const PAGE: &str = "https://backspn.com/insights";
const ENDPOINT: &str = "https://insights.backspn.com/collect";
const FIRST_SEND: Duration = Duration::from_secs(60);
const PERIOD: Duration = Duration::from_hours(24);
const SAVE_EVERY: Duration = Duration::from_mins(5);
const TIMEOUT: Duration = Duration::from_secs(10);

pub fn path() -> PathBuf {
    crate::prefs::settings_path().with_file_name("insights.json")
}

fn env_off(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off"
        )
    })
}

pub fn switched_off() -> bool {
    env_off(std::env::var("BACKSPN_INSIGHTS").ok().as_deref())
}

fn debug() -> bool {
    std::env::var_os("BACKSPN_INSIGHTS_DEBUG").is_some_and(|v| v == "1")
}

pub fn allowed() -> bool {
    !switched_off() && (!cfg!(debug_assertions) || debug())
}

fn endpoint() -> String {
    std::env::var("BACKSPN_INSIGHTS_ENDPOINT").unwrap_or_else(|_| ENDPOINT.to_owned())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    id: String,
    pending: Usage,
    last_sent: Option<u64>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            id: new_id(),
            pending: Usage::default(),
            last_sent: None,
        }
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl Store {
    fn load(path: &std::path::Path) -> Self {
        let mut store: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        if uuid::Uuid::parse_str(&store.id).is_err() {
            store.id = new_id();
        }
        store
    }

    fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    fn due(&self, now: u64) -> bool {
        self.last_sent
            .is_none_or(|last| now.saturating_sub(last) >= PERIOD.as_secs())
    }
}

struct Sending {
    sent: Usage,
    answer: Receiver<Result<(), String>>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Carry {
    open: f64,
    playing: f64,
    recording: f64,
}

fn whole(carry: &mut f64, add: f64) -> u64 {
    *carry += add.max(0.0);
    let seconds = carry.floor();
    *carry -= seconds;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let seconds = seconds as u64;
    seconds
}

pub struct Insights {
    store: Store,
    path: Option<PathBuf>,
    carry: Carry,
    last_deck: Option<usize>,
    controllers: BTreeSet<String>,
    on: BTreeSet<(&'static str, usize)>,
    saved_at: Instant,
    next_try: Instant,
    sending: Option<Sending>,
    pub last_error: Option<String>,
}

impl Insights {
    pub fn open() -> Self {
        let path = path();
        let mut insights = Self::new(Store::load(&path), Some(path));
        insights.next_try = Instant::now() + if debug() { Duration::ZERO } else { FIRST_SEND };
        insights
    }

    pub fn none() -> Self {
        Self::new(Store::default(), None)
    }

    fn new(store: Store, path: Option<PathBuf>) -> Self {
        Self {
            store,
            path,
            carry: Carry::default(),
            last_deck: None,
            controllers: BTreeSet::new(),
            on: BTreeSet::new(),
            saved_at: Instant::now(),
            next_try: Instant::now(),
            sending: None,
            last_error: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.store.id
    }

    pub fn usage(&mut self) -> &mut Usage {
        &mut self.store.pending
    }

    pub fn pending(&self) -> &Usage {
        &self.store.pending
    }

    pub fn last_sent(&self) -> Option<SystemTime> {
        self.store
            .last_sent
            .map(|s| UNIX_EPOCH + Duration::from_secs(s))
    }

    pub fn is_sending(&self) -> bool {
        self.sending.is_some()
    }

    pub fn elapsed(&mut self, dt: f64, playing: bool, recording: bool) {
        let open = whole(&mut self.carry.open, dt);
        let playing = whole(&mut self.carry.playing, if playing { dt } else { 0.0 });
        let recording = whole(&mut self.carry.recording, if recording { dt } else { 0.0 });
        let u = &mut self.store.pending;
        u.seconds_open += open;
        u.seconds_playing += playing;
        u.seconds_recording += recording;
    }

    pub fn played(&mut self, deck: usize) {
        let u = &mut self.store.pending;
        u.tracks_played += 1;
        if self.last_deck.is_some_and(|last| last != deck) {
            u.mixes += 1;
        }
        self.last_deck = Some(deck);
    }

    pub fn rising(&mut self, what: &'static str, index: usize, on: bool) -> bool {
        if on {
            self.on.insert((what, index))
        } else {
            self.on.remove(&(what, index));
            false
        }
    }

    pub fn saw_controllers(&mut self, names: impl IntoIterator<Item = String>) {
        self.controllers.extend(
            names
                .into_iter()
                .map(|n| slug(&n))
                .filter(|n| !n.is_empty()),
        );
    }

    pub fn controllers(&self) -> Vec<String> {
        self.controllers.iter().cloned().collect()
    }

    pub fn forget(&mut self) {
        if !self.store.pending.is_empty() {
            self.store.pending = Usage::default();
            self.save();
        }
    }

    pub fn save(&mut self) {
        self.saved_at = Instant::now();
        if let Some(path) = &self.path
            && let Err(e) = self.store.save(path)
        {
            log::warn!("insights: cannot save {}: {e}", path.display());
        }
    }

    pub fn tick(&mut self, now: Instant) {
        self.poll();
        if self.path.is_some() && now.duration_since(self.saved_at) >= SAVE_EVERY {
            self.save();
        }
    }

    pub fn due(&self, now: Instant) -> bool {
        self.path.is_some()
            && self.sending.is_none()
            && now >= self.next_try
            && (self.store.due(now_secs()) || debug())
    }

    pub fn send(&mut self, report: &Report) {
        self.next_try = Instant::now() + PERIOD;
        let Ok(body) = serde_json::to_string(report) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let url = endpoint();
        std::thread::spawn(move || {
            tx.send(post(&url, &body)).ok();
        });
        self.sending = Some(Sending {
            sent: report.usage.clone(),
            answer: rx,
        });
    }

    fn poll(&mut self) {
        let Some(sending) = &self.sending else {
            return;
        };
        let Ok(answer) = sending.answer.try_recv() else {
            return;
        };
        let sent = self.sending.take().map(|s| s.sent).unwrap_or_default();
        match answer {
            Ok(()) => {
                self.store.pending.subtract(&sent);
                self.store.last_sent = Some(now_secs());
                self.last_error = None;
                self.save();
            }
            Err(e) => {
                log::info!("insights: {e}");
                self.last_error = Some(e);
            }
        }
    }
}

fn post(url: &str, body: &str) -> Result<(), String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .new_agent();
    agent
        .post(url)
        .header("Content-Type", "application/json")
        .send(body)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_switches_insights_off() {
        for off in ["0", "false", "OFF", " off "] {
            assert!(env_off(Some(off)), "{off}");
        }
        for on in ["1", "true", "", "yes"] {
            assert!(!env_off(Some(on)), "{on}");
        }
        assert!(!env_off(None));
    }

    #[test]
    fn a_mix_is_a_track_played_after_one_on_another_deck() {
        let mut i = Insights::none();
        i.played(0);
        i.played(0);
        assert_eq!(i.pending().mixes, 0);
        i.played(1);
        i.played(0);
        assert_eq!((i.pending().tracks_played, i.pending().mixes), (4, 2));
    }

    #[test]
    fn time_is_counted_in_whole_seconds_without_losing_the_rest() {
        let mut i = Insights::none();
        for _ in 0..10 {
            i.elapsed(0.25, true, false);
        }
        assert_eq!(i.pending().seconds_open, 2);
        assert_eq!(i.pending().seconds_playing, 2);
        assert_eq!(i.pending().seconds_recording, 0);
        for _ in 0..2 {
            i.elapsed(0.25, false, false);
        }
        assert_eq!(i.pending().seconds_open, 3);
    }

    #[test]
    fn the_store_survives_a_restart_and_a_bad_id_is_replaced() {
        let dir = std::env::temp_dir().join(format!("backspn-insights-{}", std::process::id()));
        let path = dir.join("insights.json");
        let mut store = Store::default();
        store.pending.mixes = 3;
        store.save(&path).unwrap();
        assert_eq!(Store::load(&path), store);
        std::fs::write(&path, r#"{"id":"me@example.com","pending":{"mixes":1}}"#).unwrap();
        let loaded = Store::load(&path);
        assert!(uuid::Uuid::parse_str(&loaded.id).is_ok());
        assert_eq!(loaded.pending.mixes, 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_report_is_due_once_a_day() {
        let mut store = Store::default();
        assert!(store.due(1_000_000));
        store.last_sent = Some(1_000_000);
        assert!(!store.due(1_000_000 + 3600));
        assert!(store.due(1_000_000 + PERIOD.as_secs()));
    }

    #[test]
    fn only_switching_on_counts() {
        let mut i = Insights::none();
        let seen: Vec<bool> = [false, true, true, false, true]
            .into_iter()
            .map(|on| i.rising("sync", 0, on))
            .collect();
        assert_eq!(seen, [false, true, false, false, true]);
        assert!(i.rising("sync", 1, true));
    }

    #[test]
    fn controllers_are_kept_as_ids() {
        let mut i = Insights::none();
        i.saw_controllers(["Kontrol S4 MK3".to_owned(), String::new()]);
        assert_eq!(i.controllers(), ["kontrol-s4-mk3"]);
    }
}
