use std::fmt::Write;

pub const MAX_URL: usize = 8000;

pub fn issue_url(title: &str, body: &str) -> String {
    format!(
        "https://github.com/{}/issues/new?title={}&body={}",
        super::REPO,
        encode(title),
        encode(body)
    )
}

pub fn issue_body(intro: &str, log: &str) -> String {
    let head = format!(
        "{intro}\n\n**Version:** {}\n**System:** {}\n\n**Log:**\n```\n",
        super::version(),
        super::system()
    );
    let tail = "\n```\n";
    let room = MAX_URL.saturating_sub(issue_url("", &format!("{head}{tail}")).len() + 200);
    format!("{head}{}{tail}", log_tail(log, room))
}

fn log_tail(log: &str, room: usize) -> &str {
    let mut size = 0;
    let mut from = log.len();
    for (i, c) in log.char_indices().rev() {
        let mut buf = [0; 4];
        size += encode(c.encode_utf8(&mut buf)).len();
        if size > room {
            break;
        }
        from = i;
    }
    if from == 0 {
        return log;
    }
    let start = log[from..].find('\n').map_or(log.len(), |n| from + n + 1);
    &log[start..]
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            write!(out, "%{b:02X}").ok();
        }
    }
    out
}

pub fn open(url: &str) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(windows)]
    let result = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = result {
        log::warn!("cannot open {url}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_encoded() {
        assert_eq!(encode("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(
            issue_url("Crash", "x\ny"),
            "https://github.com/suitux/Backspn/issues/new?title=Crash&body=x%0Ay"
        );
    }

    #[test]
    fn a_long_log_is_cut_from_the_top_to_fit() {
        let mut log = String::new();
        for i in 0..5000 {
            writeln!(log, "line {i}").unwrap();
        }
        let body = issue_body("Something broke.", &log);
        assert!(issue_url("Crash", &body).len() <= MAX_URL);
        assert!(body.contains("line 4999\n"));
        assert!(!body.contains("line 0\n"));
        assert!(body.starts_with("Something broke.\n\n**Version:** "));
        let short = issue_body("Hi.", "only line\n");
        assert!(short.contains("```\nonly line\n\n```"));
    }
}
