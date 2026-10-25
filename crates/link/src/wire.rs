use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

use crate::timeline::{Beats, StartStop, Tempo, Timeline};

pub const MAX_MESSAGE: usize = 512;

const DISCOVERY_MAGIC: [u8; 8] = *b"_asdp_v\x01";
const LINK_MAGIC: [u8; 8] = *b"_link_v\x01";

pub const TTL: u8 = 5;

const fn key(s: [u8; 4]) -> u32 {
    u32::from_be_bytes(s)
}

pub const TIMELINE: u32 = key(*b"tmln");
pub const SESSION: u32 = key(*b"sess");
pub const START_STOP: u32 = key(*b"stst");
pub const MEASURE_V4: u32 = key(*b"mep4");
pub const MEASURE_V6: u32 = key(*b"mep6");
pub const HOST_TIME: u32 = key(*b"__ht");
pub const GHOST_TIME: u32 = key(*b"__gt");
pub const PREV_GHOST_TIME: u32 = key(*b"_pgt");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NodeId(pub [u8; 8]);

impl NodeId {
    pub fn random() -> Self {
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
            ^ u64::from(std::process::id()).rotate_left(32);
        let mut id = [0u8; 8];
        for byte in &mut id {
            seed ^= seed >> 12;
            seed ^= seed << 25;
            seed ^= seed >> 27;
            let r = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
            *byte = 33 + (r >> 56) as u8 % 94;
        }
        Self(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Alive,
    Response,
    ByeBye,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PeerState {
    pub timeline: Timeline,
    pub session: NodeId,
    pub start_stop: StartStop,
    pub measure: Option<SocketAddr>,
}

#[derive(Debug, Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn entry(&mut self, key: u32, value: &[u8]) {
        if value.is_empty() {
            return;
        }
        self.0.extend(key.to_be_bytes());
        self.0.extend((value.len() as u32).to_be_bytes());
        self.0.extend(value);
    }
}

fn timeline_bytes(t: &Timeline) -> Vec<u8> {
    let mut v = Vec::with_capacity(24);
    v.extend(t.tempo.micros_per_beat().to_be_bytes());
    v.extend(t.beat_origin.0.to_be_bytes());
    v.extend(t.time_origin.to_be_bytes());
    v
}

fn endpoint_bytes(addr: SocketAddr) -> (u32, Vec<u8>) {
    match addr {
        SocketAddr::V4(a) => {
            let mut v = a.ip().octets().to_vec();
            v.extend(a.port().to_be_bytes());
            (MEASURE_V4, v)
        }
        SocketAddr::V6(a) => {
            let mut v = a.ip().octets().to_vec();
            v.extend(a.port().to_be_bytes());
            (MEASURE_V6, v)
        }
    }
}

pub fn discovery(kind: Kind, ident: NodeId, state: Option<&PeerState>) -> Vec<u8> {
    let mut m = DISCOVERY_MAGIC.to_vec();
    m.push(match kind {
        Kind::Alive => 1,
        Kind::Response => 2,
        Kind::ByeBye => 3,
    });
    m.push(if kind == Kind::ByeBye { 0 } else { TTL });
    m.extend(0u16.to_be_bytes());
    m.extend(ident.0);
    if let (Some(s), false) = (state, kind == Kind::ByeBye) {
        let mut w = Writer::default();
        w.entry(TIMELINE, &timeline_bytes(&s.timeline));
        w.entry(SESSION, &s.session.0);
        let mut st = vec![u8::from(s.start_stop.playing)];
        st.extend(s.start_stop.beats.0.to_be_bytes());
        st.extend(s.start_stop.timestamp.to_be_bytes());
        w.entry(START_STOP, &st);
        if let Some(addr) = s.measure {
            let (k, v) = endpoint_bytes(addr);
            w.entry(k, &v);
        }
        m.extend(w.0);
    }
    m
}

#[derive(Debug, Clone, PartialEq)]
pub struct Discovery {
    pub kind: Kind,
    pub ttl: u8,
    pub ident: NodeId,
    pub state: PeerState,
}

fn entries(mut p: &[u8]) -> Option<Vec<(u32, &[u8])>> {
    let mut out = Vec::new();
    while !p.is_empty() {
        if p.len() < 8 {
            return None;
        }
        let k = u32::from_be_bytes(p[0..4].try_into().ok()?);
        let n = u32::from_be_bytes(p[4..8].try_into().ok()?) as usize;
        let value = p.get(8..8 + n)?;
        out.push((k, value));
        p = &p[8 + n..];
    }
    Some(out)
}

fn i64_at(v: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_be_bytes(v.get(at..at + 8)?.try_into().ok()?))
}

pub fn parse_discovery(m: &[u8]) -> Option<Discovery> {
    if m.len() < 20 || m[0..8] != DISCOVERY_MAGIC {
        return None;
    }
    let kind = match m[8] {
        1 => Kind::Alive,
        2 => Kind::Response,
        3 => Kind::ByeBye,
        _ => return None,
    };
    if u16::from_be_bytes([m[10], m[11]]) != 0 {
        return None;
    }
    let ident = NodeId(m[12..20].try_into().ok()?);
    let mut state = PeerState::default();
    for (k, v) in entries(&m[20..])? {
        match k {
            TIMELINE => {
                if v.len() != 24 {
                    return None;
                }
                let mpb = i64_at(v, 0)?;
                if mpb <= 0 {
                    return None;
                }
                state.timeline = Timeline {
                    tempo: Tempo::from_micros_per_beat(mpb),
                    beat_origin: Beats(i64_at(v, 8)?),
                    time_origin: i64_at(v, 16)?,
                };
            }
            SESSION => state.session = NodeId(v.try_into().ok()?),
            START_STOP => {
                if v.len() != 17 {
                    return None;
                }
                state.start_stop = StartStop {
                    playing: v[0] != 0,
                    beats: Beats(i64_at(v, 1)?),
                    timestamp: i64_at(v, 9)?,
                };
            }
            MEASURE_V4 => {
                if v.len() != 6 {
                    return None;
                }
                let ip = Ipv4Addr::new(v[0], v[1], v[2], v[3]);
                let port = u16::from_be_bytes([v[4], v[5]]);
                state.measure = Some(SocketAddr::V4(SocketAddrV4::new(ip, port)));
            }
            MEASURE_V6 => {
                if v.len() != 18 {
                    return None;
                }
                let ip = Ipv6Addr::from(<[u8; 16]>::try_from(&v[0..16]).ok()?);
                let port = u16::from_be_bytes([v[16], v[17]]);
                state.measure = Some(SocketAddr::V6(SocketAddrV6::new(ip, port, 0, 0)));
            }
            _ => {}
        }
    }
    Some(Discovery {
        kind,
        ttl: m[9],
        ident,
        state,
    })
}

pub fn ping(host_time: i64, prev_ghost: Option<i64>) -> Vec<u8> {
    let mut m = LINK_MAGIC.to_vec();
    m.push(1);
    let mut w = Writer::default();
    w.entry(HOST_TIME, &host_time.to_be_bytes());
    if let Some(g) = prev_ghost {
        w.entry(PREV_GHOST_TIME, &g.to_be_bytes());
    }
    m.extend(w.0);
    m
}

pub fn pong(ping: &[u8], session: NodeId, ghost_now: i64) -> Option<Vec<u8>> {
    if ping.len() < 9 || ping[0..8] != LINK_MAGIC || ping[8] != 1 || ping.len() - 9 > 32 {
        return None;
    }
    let mut m = LINK_MAGIC.to_vec();
    m.push(2);
    let mut w = Writer::default();
    w.entry(SESSION, &session.0);
    w.entry(GHOST_TIME, &ghost_now.to_be_bytes());
    m.extend(w.0);
    m.extend(&ping[9..]);
    Some(m)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pong {
    pub session: NodeId,
    pub ghost: i64,
    pub prev_ghost: i64,
    pub host: i64,
}

pub fn parse_pong(m: &[u8]) -> Option<Pong> {
    if m.len() < 9 || m[0..8] != LINK_MAGIC || m[8] != 2 {
        return None;
    }
    let mut p = Pong::default();
    for (k, v) in entries(&m[9..])? {
        let as_i64 = || i64::from_be_bytes(v.try_into().unwrap_or([0; 8]));
        match k {
            SESSION => p.session = NodeId(v.try_into().ok()?),
            GHOST_TIME => p.ghost = as_i64(),
            PREV_GHOST_TIME => p.prev_ghost = as_i64(),
            HOST_TIME => p.host = as_i64(),
            _ => {}
        }
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> PeerState {
        PeerState {
            timeline: Timeline {
                tempo: Tempo(120.0),
                beat_origin: Beats(3_000_000),
                time_origin: 123_456,
            },
            session: NodeId(*b"ABCDEFGH"),
            start_stop: StartStop {
                playing: true,
                beats: Beats(8_000_000),
                timestamp: 99,
            },
            measure: Some("192.168.1.5:50000".parse().unwrap()),
        }
    }

    #[test]
    fn an_alive_message_has_the_layout_and_size_of_the_spec() {
        let m = discovery(Kind::Alive, NodeId(*b"node0001"), Some(&state()));
        assert_eq!(m.len(), 107);
        assert_eq!(&m[0..8], b"_asdp_v\x01");
        assert_eq!((m[8], m[9]), (1, 5));
        assert_eq!(&m[20..24], b"tmln");
        assert_eq!(u32::from_be_bytes(m[24..28].try_into().unwrap()), 24);
        assert_eq!(i64::from_be_bytes(m[28..36].try_into().unwrap()), 500_000);
        let back = parse_discovery(&m).unwrap();
        assert_eq!(back.ident, NodeId(*b"node0001"));
        assert_eq!(back.state, state());
    }

    #[test]
    fn byebye_is_a_bare_header_and_garbage_is_dropped() {
        let m = discovery(Kind::ByeBye, NodeId(*b"node0001"), Some(&state()));
        assert_eq!((m.len(), m[9]), (20, 0));
        assert_eq!(parse_discovery(&m).unwrap().kind, Kind::ByeBye);
        assert_eq!(parse_discovery(b"_asdp_v\x01\x01"), None);
        let mut bad = discovery(Kind::Alive, NodeId(*b"node0001"), Some(&state()));
        bad.truncate(40);
        assert_eq!(parse_discovery(&bad), None, "an entry past the end");
    }

    #[test]
    fn ping_pong_echoes_the_ping_payload() {
        let p = ping(1000, Some(77));
        let reply = pong(&p, NodeId(*b"SESSION1"), 5000).unwrap();
        let back = parse_pong(&reply).unwrap();
        assert_eq!(
            back,
            Pong {
                session: NodeId(*b"SESSION1"),
                ghost: 5000,
                prev_ghost: 77,
                host: 1000
            }
        );
        assert_eq!(pong(&reply, NodeId::default(), 0), None, "not a ping");
    }

    #[test]
    fn node_ids_are_printable() {
        let id = NodeId::random();
        assert!(id.0.iter().all(|b| (33..=126).contains(b)));
    }
}
