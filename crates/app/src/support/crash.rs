use std::fs;
use std::path::PathBuf;

use super::log::Previous;

fn report_path() -> PathBuf {
    super::folder("Crashlogs").join("last-crash.txt")
}

pub fn install() {
    let path = report_path();
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = panic_text(
            &info.to_string(),
            &std::backtrace::Backtrace::force_capture().to_string(),
        );
        log::error!("{text}");
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).ok();
        }
        fs::write(&path, &text).ok();
        default(info);
    }));
}

fn panic_text(info: &str, backtrace: &str) -> String {
    format!(
        "Backspn {} on {} crashed:\n{info}\n\nBacktrace:\n{backtrace}",
        super::version(),
        super::system()
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub panic: Option<String>,
    pub log: String,
}

pub fn take_pending(previous: &Previous) -> Option<Pending> {
    let path = report_path();
    let panic = fs::read_to_string(&path).ok();
    if panic.is_some() {
        fs::remove_file(&path).ok();
    }
    pending(panic, previous)
}

fn pending(panic: Option<String>, previous: &Previous) -> Option<Pending> {
    (panic.is_some() || !previous.closed).then(|| Pending {
        panic,
        log: previous.text.clone(),
    })
}

impl Pending {
    pub fn title(&self) -> String {
        let what = self
            .panic
            .as_deref()
            .and_then(|p| p.lines().nth(2))
            .unwrap_or("closed unexpectedly");
        format!("Crash: {what}")
    }

    pub fn body(&self) -> String {
        let intro = match &self.panic {
            Some(p) => format!(
                "Backspn crashed.\n\n**What I was doing:**\n\n\n**Error:**\n```\n{}\n```",
                first_lines(p, 12)
            ),
            None => "Backspn closed unexpectedly (no error was recorded).\n\n\
                     **What I was doing:**\n"
                .to_owned(),
        };
        super::github::issue_body(&intro, &self.log)
    }
}

fn first_lines(text: &str, n: usize) -> String {
    text.lines().take(n).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(closed: bool) -> Previous {
        Previous {
            text: "x - >>> LOG OPEN, a\nx - boom\n".into(),
            closed,
        }
    }

    #[test]
    fn a_panic_or_an_open_session_is_reported() {
        assert_eq!(pending(None, &session(true)), None);
        let open = pending(None, &session(false)).unwrap();
        assert_eq!(open.title(), "Crash: closed unexpectedly");
        assert!(open.body().contains("no error was recorded"));
        assert!(open.body().contains("x - boom"));
        let text = panic_text("panicked at src/a.rs:1:2:\nindex out of bounds", "0: main");
        let crashed = pending(Some(text), &session(true)).unwrap();
        assert_eq!(crashed.title(), "Crash: index out of bounds");
        assert!(crashed.body().contains("index out of bounds"));
        assert!(crashed.body().contains("x - boom"));
    }
}
