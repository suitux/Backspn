use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use backspn_library::clock::LocalTime;

const OPEN: &str = ">>> LOG OPEN, ";
const CLOSE: &str = "<<< LOG CLOSE, ";
const LIMIT: usize = 2 << 20;
const KEEP: usize = 1 << 20;

static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn path() -> PathBuf {
    super::folder("Logs").join("Backspn.log")
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Previous {
    pub text: String,
    pub closed: bool,
}

pub fn init() -> Previous {
    let console =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).build();
    let level = console.filter();
    let previous = open(&path()).unwrap_or_else(|e| {
        eprintln!("cannot open the log file {}: {e}", path().display());
        Previous {
            closed: true,
            ..Previous::default()
        }
    });
    if log::set_boxed_logger(Box::new(Tee { console })).is_ok() {
        log::set_max_level(level);
    }
    previous
}

fn open(path: &Path) -> std::io::Result<Previous> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let old = fs::read_to_string(path).unwrap_or_default();
    let previous = Previous {
        text: last_session(&old).to_owned(),
        closed: closed_cleanly(&old),
    };
    if let Some(kept) = trimmed(&old, LIMIT, KEEP) {
        fs::write(path, kept)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let now = LocalTime::now();
    let exe = std::env::current_exe().map(|p| p.display().to_string());
    let lines = [
        String::new(),
        format!("{OPEN}{}", ctime(now)),
        format!("Executable path: {}", exe.unwrap_or_default()),
        format!("Starting Backspn ({}).", super::version()),
    ];
    for line in lines {
        writeln!(file, "{}", stamped(now, &line))?;
    }
    *FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(file);
    Ok(previous)
}

pub fn close() {
    let mut file = FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(f) = file.as_mut() {
        let now = LocalTime::now();
        writeln!(f, "{}", stamped(now, &format!("{CLOSE}{}", ctime(now)))).ok();
        f.flush().ok();
    }
    *file = None;
}

pub fn this_session() -> String {
    last_session(&fs::read_to_string(path()).unwrap_or_default()).to_owned()
}

struct Tee {
    console: env_logger::Logger,
}

impl log::Log for Tee {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.console.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if !self.console.matches(record) {
            return;
        }
        self.console.log(record);
        let text = format!("{}{}", level_prefix(record.level()), record.args());
        let mut file = FILE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(f) = file.as_mut() {
            writeln!(f, "{}", stamped(LocalTime::now(), &text)).ok();
        }
    }

    fn flush(&self) {
        self.console.flush();
        let mut file = FILE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(f) = file.as_mut() {
            f.flush().ok();
        }
    }
}

fn level_prefix(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "ERROR: ",
        log::Level::Warn => "WARNING: ",
        _ => "",
    }
}

fn stamped(t: LocalTime, text: &str) -> String {
    let s = t.seconds;
    format!(
        "{}-{:02}-{:02} {:02}:{:02}:{:02} - {text}",
        t.year,
        t.month,
        t.day,
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

fn ctime(t: LocalTime) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let s = t.seconds;
    format!(
        "{} {} {:>2} {:02}:{:02}:{:02} {}",
        DAYS[weekday(t.year, t.month, t.day)],
        MONTHS[(t.month.clamp(1, 12) - 1) as usize],
        t.day,
        s / 3600,
        s / 60 % 60,
        s % 60,
        t.year
    )
}

fn weekday(year: i32, month: u32, day: u32) -> usize {
    const OFFSET: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let month = month.clamp(1, 12);
    let y = if month < 3 { year - 1 } else { year };
    let d = y + y / 4 - y / 100
        + y / 400
        + OFFSET[(month - 1) as usize]
        + i32::try_from(day).unwrap_or(1);
    usize::try_from(d.rem_euclid(7)).unwrap_or(0)
}

pub(crate) fn last_session(log: &str) -> &str {
    log.rfind(OPEN).map_or(log, |at| {
        let line = log[..at].rfind('\n').map_or(0, |n| n + 1);
        &log[line..]
    })
}

pub(crate) fn closed_cleanly(log: &str) -> bool {
    !log.contains(OPEN) || last_session(log).contains(CLOSE)
}

fn trimmed(log: &str, limit: usize, keep: usize) -> Option<&str> {
    if log.len() <= limit {
        return None;
    }
    let mut from = log.len() - keep;
    while !log.is_char_boundary(from) {
        from += 1;
    }
    let start = log[from..].find('\n').map_or(log.len(), |n| from + n + 1);
    Some(&log[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i32, month: u32, day: u32, h: u32, m: u32, s: u32) -> LocalTime {
        LocalTime {
            year,
            month,
            day,
            seconds: h * 3600 + m * 60 + s,
        }
    }

    #[test]
    fn lines_and_dates_look_right() {
        let t = at(2026, 8, 16, 13, 18, 4);
        assert_eq!(
            stamped(t, "Applying preferences."),
            "2026-08-16 13:18:04 - Applying preferences."
        );
        assert_eq!(ctime(t), "Sun Aug 16 13:18:04 2026");
        assert_eq!(
            ctime(at(2026, 10, 3, 17, 37, 33)),
            "Sat Oct  3 17:37:33 2026"
        );
        assert_eq!(ctime(at(2024, 2, 29, 0, 0, 0)), "Thu Feb 29 00:00:00 2024");
        assert_eq!(level_prefix(log::Level::Warn), "WARNING: ");
        assert_eq!(level_prefix(log::Level::Info), "");
    }

    #[test]
    fn a_session_without_close_did_not_end_well() {
        let ok = "x - >>> LOG OPEN, a\nx - hi\nx - <<< LOG CLOSE, b\n";
        assert!(closed_cleanly(ok));
        let crashed = format!("{ok}\nx - >>> LOG OPEN, c\nx - boom\n");
        assert!(!closed_cleanly(&crashed));
        assert_eq!(last_session(&crashed), "x - >>> LOG OPEN, c\nx - boom\n");
        assert!(closed_cleanly(""));
        assert!(closed_cleanly("no marks at all\n"));
    }

    #[test]
    fn a_long_log_keeps_its_last_lines() {
        let log = "aaaa\nbbbb\ncccc\n";
        assert_eq!(trimmed(log, 100, 5), None);
        assert_eq!(trimmed(log, 10, 7), Some("cccc\n"));
        assert_eq!(trimmed("ñññ\nzz\n", 3, 6), Some("zz\n"));
    }

    #[test]
    fn opening_appends_and_reports_the_last_session() {
        let dir = std::env::temp_dir().join(format!("backspn-log-{}", std::process::id()));
        let file = dir.join("Logs/Backspn.log");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "x - >>> LOG OPEN, a\nx - boom\n").unwrap();
        let previous = open(&file).unwrap();
        assert!(!previous.closed);
        assert_eq!(previous.text, "x - >>> LOG OPEN, a\nx - boom\n");
        close();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("x - >>> LOG OPEN, a\nx - boom\n"));
        assert!(closed_cleanly(&text));
        assert!(last_session(&text).contains("Starting Backspn ("));
        fs::remove_dir_all(&dir).ok();
    }
}
