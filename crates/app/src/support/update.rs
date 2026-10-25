use std::cmp::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

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

pub struct Check {
    answer: Option<Receiver<Option<Release>>>,
    pub release: Option<Release>,
}

impl Check {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let release = fetch_tags()
                .inspect_err(|e| log::info!("new-version check: {e}"))
                .ok()
                .and_then(|tags| newest(&tags, super::version()));
            tx.send(release).ok();
        });
        Self {
            answer: Some(rx),
            release: None,
        }
    }

    pub fn none() -> Self {
        Self {
            answer: None,
            release: None,
        }
    }

    pub fn poll(&mut self) {
        if let Some(rx) = &self.answer
            && let Ok(release) = rx.try_recv()
        {
            self.release = release;
            self.answer = None;
        }
    }
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
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .new_agent();
    let text = agent
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let tags: Vec<Tag> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
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
