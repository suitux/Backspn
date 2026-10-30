use std::any::Any;
use std::fmt::Display;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use super::log::Previous;
use crate::i18n::tr;

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

/// Runs the app and, if it ends on an error, says so in a system dialog: the
/// window may never have opened, so egui cannot be the one to show it.
pub fn tell_fatal<E: Display>(run: impl FnOnce() -> Result<(), E>) -> Result<(), E> {
    match panic::catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => {
            show_fatal(&e.to_string());
            Err(e)
        }
        Err(payload) => {
            show_fatal(panic_message(payload.as_ref()));
            panic::resume_unwind(payload)
        }
    }
}

fn show_fatal(error: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Backspn")
        .set_description(fatal_text(error, &super::log::path()))
        .show();
}

fn fatal_text(error: &str, log: &Path) -> String {
    format!(
        "{}\n\n{error}\n\n{} {}",
        tr("Backspn has to close because of an error:"),
        tr("Log file:"),
        log.display()
    )
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown error")
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
    fn the_fatal_message_names_the_error_and_the_log() {
        let text = fatal_text("no OpenGL context", Path::new("/data/Backspn.log"));
        assert!(text.contains("no OpenGL context"));
        assert!(text.contains("/data/Backspn.log"));
    }

    #[test]
    fn a_panic_message_is_read_from_its_payload() {
        let text: Box<dyn Any + Send> = Box::new("boom");
        assert_eq!(panic_message(text.as_ref()), "boom");
        let owned: Box<dyn Any + Send> = Box::new(String::from("bang"));
        assert_eq!(panic_message(owned.as_ref()), "bang");
        let other: Box<dyn Any + Send> = Box::new(1);
        assert_eq!(panic_message(other.as_ref()), "unknown error");
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
