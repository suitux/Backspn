mod apply;
mod manifest;
mod target;

use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as Atomic};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use apply::Staged;
pub use target::Target;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    pre: Option<String>,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) if !pre.is_empty() => (core, Some(pre.to_owned())),
            Some(_) => return None,
            None => (text, None),
        };
        let mut parts = core.split('.').map(|n| {
            n.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| n.parse::<u64>().ok())
                .flatten()
        });
        let core = [parts.next()??, parts.next()??, parts.next()??];
        parts.next().is_none().then_some(Self { core, pre })
    }

    pub fn is_pre_release(&self) -> bool {
        self.pre.is_some()
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => split_number(a).cmp(&split_number(b)),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn split_number(suffix: &str) -> (&str, Option<u64>) {
    let digits = suffix.len() - suffix.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (text, number) = suffix.split_at(suffix.len() - digits);
    (text, number.parse().ok())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
}

impl Release {
    pub fn version(&self) -> &str {
        self.tag.strip_prefix('v').unwrap_or(&self.tag)
    }

    pub fn url(&self) -> String {
        format!(
            "https://github.com/{}/releases/tag/{}",
            super::REPO,
            self.tag
        )
    }

    fn asset_url(&self, name: &str) -> String {
        format!(
            "https://github.com/{}/releases/download/{}/{name}",
            super::REPO,
            self.tag
        )
    }
}

pub fn newest(tags: &[String], current: &str) -> Option<Release> {
    let current = Version::parse(current)?;
    tags.iter()
        .filter_map(|tag| Some((Version::parse(tag)?, tag)))
        .filter(|(v, _)| current.is_pre_release() || !v.is_pre_release())
        .max_by(|a, b| a.0.cmp(&b.0))
        .filter(|(v, _)| *v > current)
        .map(|(_, tag)| Release { tag: tag.clone() })
}

pub fn shows(release: &Release, dismissed: Option<&str>) -> bool {
    dismissed != Some(release.version())
}

static READY: Mutex<Option<Staged>> = Mutex::new(None);
static RESTART: AtomicBool = AtomicBool::new(false);

pub fn restart_after_quit(restart: bool) {
    RESTART.store(restart, Atomic::SeqCst);
}

pub fn finish() {
    let staged = READY.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(staged) = staged
        && let Err(e) = apply::install(&staged, RESTART.load(Atomic::SeqCst))
    {
        eprintln!("cannot install the update: {e}");
    }
}

fn downloads() -> PathBuf {
    super::folder("Updates")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    Idle,
    Downloading(f32),
    Ready,
}

struct Download {
    got: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    answer: Receiver<Result<Staged, String>>,
}

impl Download {
    fn start(release: Release, target: Target) -> Self {
        let got = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (g, t, c) = (got.clone(), total.clone(), cancel.clone());
        std::thread::spawn(move || {
            let result = fetch_update(
                &release,
                &target,
                &mut |got, total| {
                    g.store(got, Atomic::Relaxed);
                    t.store(total, Atomic::Relaxed);
                },
                &c,
            );
            tx.send(result).ok();
        });
        Self {
            got,
            total,
            cancel,
            answer: rx,
        }
    }

    fn fraction(&self) -> f32 {
        let total = self.total.load(Atomic::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.got.load(Atomic::Relaxed) as f64 / total as f64).min(1.0) as f32
    }
}

pub struct Check {
    answer: Option<Receiver<Option<Release>>>,
    pub release: Option<Release>,
    pub target: Target,
    download: Option<Download>,
    ready: bool,
}

impl Check {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = std::fs::remove_dir_all(downloads());
            let release = fetch_tags()
                .inspect_err(|e| log::info!("new-version check: {e}"))
                .ok()
                .and_then(|tags| newest(&tags, super::version()));
            tx.send(release).ok();
        });
        Self {
            answer: Some(rx),
            release: None,
            target: Target::current(),
            download: None,
            ready: false,
        }
    }

    pub fn none() -> Self {
        Self {
            answer: None,
            release: None,
            target: Target::Unsupported(""),
            download: None,
            ready: false,
        }
    }

    pub fn can_install(&self) -> bool {
        self.target.kind().is_some()
    }

    pub fn stage(&self) -> Stage {
        match &self.download {
            _ if self.ready => Stage::Ready,
            Some(d) => Stage::Downloading(d.fraction()),
            None => Stage::Idle,
        }
    }

    pub fn download(&mut self) {
        if let Some(release) = &self.release
            && self.can_install()
            && self.stage() == Stage::Idle
        {
            self.download = Some(Download::start(release.clone(), self.target.clone()));
        }
    }

    pub fn cancel(&self) {
        if let Some(d) = &self.download {
            d.cancel.store(true, Atomic::Relaxed);
        }
    }

    pub fn poll(&mut self) -> Option<String> {
        if let Some(rx) = &self.answer
            && let Ok(release) = rx.try_recv()
        {
            self.release = release;
            self.answer = None;
        }
        let d = self.download.as_ref()?;
        let result = d.answer.try_recv().ok()?;
        let cancelled = d.cancel.load(Atomic::Relaxed);
        self.download = None;
        match result {
            Ok(staged) => {
                *READY.lock().unwrap_or_else(PoisonError::into_inner) = Some(staged);
                self.ready = true;
                None
            }
            Err(_) if cancelled => None,
            Err(e) => {
                log::warn!("update: {e}");
                Some(e)
            }
        }
    }
}

fn fetch_update(
    release: &Release,
    target: &Target,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<Staged, String> {
    let kind = target.kind().ok_or("this copy cannot update itself")?;
    let manifest = manifest::read(
        &get(&release.asset_url(manifest::FILE))?,
        &get(&release.asset_url(manifest::SIGNATURE))?,
        release.version(),
    )?;
    let asset = manifest
        .asset(kind, manifest::arch(kind))
        .ok_or("the release has no package for this system")?;
    let file = downloads().join(release.version()).join(&asset.name);
    backspn_separate::fetch(
        &release.asset_url(&asset.name),
        &file,
        &asset.sha256,
        asset.size,
        progress,
        cancel,
    )
    .map_err(|e| e.to_string())?;
    apply::stage(target, &file)
}

fn get(url: &str) -> Result<String, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .new_agent();
    agent
        .get(url)
        .header("Accept", "application/vnd.github.v3+json")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

fn fetch_tags() -> Result<Vec<String>, String> {
    #[derive(serde::Deserialize)]
    struct Tag {
        name: String,
    }
    let url = format!(
        "https://api.github.com/repos/{}/tags?per_page=100",
        super::REPO
    );
    let tags: Vec<Tag> = serde_json::from_str(&get(&url)?).map_err(|e| e.to_string())?;
    Ok(tags.into_iter().map(|t| t.name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|&t| t.to_owned()).collect()
    }

    #[test]
    fn versions_parse_like_the_tags() {
        assert_eq!(Version::parse("v1.2.3").unwrap().core, [1, 2, 3]);
        assert!(Version::parse("0.1.0-beta2").unwrap().is_pre_release());
        for bad in ["v1.2", "v1.2.3.4", "latest", "v1.2.x", "v1.2.3-", "v+1.2.3"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn pre_releases_come_before_their_release() {
        let v = |t| Version::parse(t).unwrap();
        assert!(v("0.1.0-beta2") < v("0.1.0-beta10"));
        assert!(v("0.1.0-beta10") < v("0.1.0"));
        assert!(v("0.1.0") < v("0.1.1-beta1"));
        assert!(v("0.1.0-alpha3") < v("0.1.0-beta1"));
    }

    #[test]
    fn betas_hear_of_betas_and_releases_only_of_releases() {
        let all = tags(&["v0.1.0-beta2", "v0.1.0-beta3", "v0.0.9", "nightly"]);
        assert_eq!(newest(&all, "0.1.0-beta2").unwrap().tag, "v0.1.0-beta3");
        assert_eq!(newest(&all, "0.1.0-beta3"), None);
        assert_eq!(newest(&all, "0.0.8").unwrap().tag, "v0.0.9");
        assert_eq!(newest(&all, "0.0.9"), None);
        let with_release = tags(&["v0.1.0-beta3", "v0.1.0"]);
        assert_eq!(newest(&with_release, "0.1.0-beta2").unwrap().tag, "v0.1.0");
    }

    #[test]
    fn a_dismissed_version_stays_quiet_until_the_next() {
        let r = Release {
            tag: "v0.1.0-beta3".into(),
        };
        assert_eq!(r.version(), "0.1.0-beta3");
        assert_eq!(
            r.url(),
            "https://github.com/suitux/Backspn/releases/tag/v0.1.0-beta3"
        );
        assert!(shows(&r, None));
        assert!(!shows(&r, Some("0.1.0-beta3")));
        assert!(shows(&r, Some("0.1.0-beta2")));
    }
}
