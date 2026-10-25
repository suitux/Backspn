use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const KEEP_ALIVE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PortMessages {
    pub port: String,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "one_or_more"
    )]
    pub connect: Vec<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dump: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keep_alive: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quit: Vec<u8>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_leds: bool,
}

fn one_or_more<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Vec<u8>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Messages {
        One(Vec<u8>),
        More(Vec<Vec<u8>>),
    }
    Ok(match Messages::deserialize(d)? {
        Messages::One(m) if m.is_empty() => Vec::new(),
        Messages::One(m) => vec![m],
        Messages::More(list) => list,
    })
}

impl PortMessages {
    pub fn on_connect(&self) -> Vec<&[u8]> {
        self.connect
            .iter()
            .chain([&self.dump])
            .filter(|m| !m.is_empty())
            .map(Vec::as_slice)
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct KeepAlive {
    last: Instant,
}

impl KeepAlive {
    pub fn new(now: Instant) -> Self {
        Self { last: now }
    }

    pub fn sent(&mut self, now: Instant) {
        self.last = now;
    }

    pub fn due(&mut self, now: Instant) -> bool {
        let due = now.saturating_duration_since(self.last) > KEEP_ALIVE;
        if due {
            self.last = now;
        }
        due
    }

    pub fn next(&self, now: Instant) -> Duration {
        (self.last + KEEP_ALIVE).saturating_duration_since(now) + Duration::from_millis(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_reads_one_message_or_a_list() {
        let read = |json| serde_json::from_str::<PortMessages>(json).unwrap().connect;
        assert_eq!(
            read(r#"{"port": "", "connect": [155, 8, 0]}"#),
            [vec![155, 8, 0]]
        );
        assert_eq!(
            read(r#"{"port": "", "connect": [[240, 247], [144, 8, 127]]}"#),
            [vec![240, 247], vec![144, 8, 127]]
        );
        assert_eq!(
            read(r#"{"port": "", "connect": []}"#),
            Vec::<Vec<u8>>::new()
        );
        assert_eq!(read(r#"{"port": ""}"#), Vec::<Vec<u8>>::new());
    }

    #[test]
    fn connect_then_dump_and_keep_alive_when_idle() {
        let m = PortMessages {
            port: "DDJ".into(),
            connect: vec![vec![0x9F, 0x70, 0x00]],
            dump: vec![0xF0, 0x03, 0x01, 0xF7],
            ..PortMessages::default()
        };
        assert_eq!(
            m.on_connect(),
            [&[0x9F, 0x70, 0x00][..], &[0xF0, 0x03, 0x01, 0xF7][..]]
        );
        assert_eq!(PortMessages::default().on_connect(), Vec::<&[u8]>::new());
        let more = PortMessages {
            connect: vec![vec![0xF0, 0x01, 0xF7], vec![0x90, 0x08, 0x7F]],
            ..m.clone()
        };
        assert_eq!(more.on_connect().len(), 3);
        assert_eq!(more.on_connect()[1], [0x90, 0x08, 0x7F]);
        let t = Instant::now();
        let at = |ms| t + Duration::from_millis(ms);
        let mut k = KeepAlive::new(t);
        assert!(!k.due(at(200)));
        k.sent(at(150));
        assert!(!k.due(at(300)));
        assert!(k.due(at(351)));
        assert!(!k.due(at(400)));
        assert_eq!(k.next(at(400)), Duration::from_millis(152));
    }
}
