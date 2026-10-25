use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};

use crate::core::{Core, Effect};
use crate::host_micros;
use crate::measure::{Measurement, Step};
use crate::wire::{self, Kind};

pub const GROUP: Ipv4Addr = Ipv4Addr::new(224, 76, 78, 75);
pub const GROUP_V6: Ipv6Addr = Ipv6Addr::new(0xff12, 0, 0, 0, 0, 0, 0, 0x8080);
pub const PORT: u16 = 20808;
const PERIOD: i64 = 250_000;
const MIN_GAP: i64 = 50_000;
const RESCAN: i64 = 5_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Addr {
    ip: IpAddr,
    scope: u32,
}

impl Addr {
    fn group(self) -> SocketAddr {
        match self.ip {
            IpAddr::V4(_) => SocketAddr::V4(SocketAddrV4::new(GROUP, PORT)),
            IpAddr::V6(_) => SocketAddr::V6(SocketAddrV6::new(GROUP_V6, PORT, 0, self.scope)),
        }
    }

    fn reach(self, to: SocketAddr) -> SocketAddr {
        match to {
            SocketAddr::V6(a) => {
                SocketAddr::V6(SocketAddrV6::new(*a.ip(), a.port(), 0, self.scope))
            }
            v4 @ SocketAddr::V4(_) => v4,
        }
    }
}

#[derive(Debug)]
struct Gateway {
    addr: Addr,
    unicast: UdpSocket,
    multicast: UdpSocket,
    measure: UdpSocket,
    last_send: i64,
    next_send: i64,
    measurements: Vec<Measurement>,
}

fn nonblocking(s: Socket) -> std::io::Result<UdpSocket> {
    let s: UdpSocket = s.into();
    s.set_nonblocking(true)?;
    Ok(s)
}

impl Gateway {
    fn open(addr: Addr) -> std::io::Result<Self> {
        let (unicast, multicast, measure) = match addr.ip {
            IpAddr::V4(a) => open_v4(a)?,
            IpAddr::V6(a) => open_v6(a, addr.scope)?,
        };
        Ok(Self {
            addr,
            unicast,
            multicast,
            measure,
            last_send: i64::MIN / 2,
            next_send: 0,
            measurements: Vec::new(),
        })
    }

    fn measure_endpoint(&self) -> Option<SocketAddr> {
        self.measure.local_addr().ok()
    }

    fn send_state(&mut self, core: &Core, kind: Kind, to: SocketAddr, now: i64) {
        let m = wire::discovery(kind, core.node, Some(&core.state(self.measure_endpoint())));
        if m.len() < wire::MAX_MESSAGE {
            let _ = self.unicast.send_to(&m, self.addr.reach(to));
        }
        self.last_send = now;
    }

    fn byebye(&self, core: &Core) {
        let m = wire::discovery(Kind::ByeBye, core.node, None);
        let _ = self.unicast.send_to(&m, self.addr.group());
    }
}

type Sockets = (UdpSocket, UdpSocket, UdpSocket);

fn open_v4(addr: Ipv4Addr) -> std::io::Result<Sockets> {
    let loopback = addr.is_loopback();
    let unicast = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    unicast.set_reuse_address(true)?;
    unicast.set_multicast_loop_v4(loopback)?;
    unicast.set_multicast_if_v4(&addr)?;
    unicast.bind(&SocketAddr::V4(SocketAddrV4::new(addr, 0)).into())?;
    let multicast = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    multicast.set_reuse_address(true)?;
    #[cfg(unix)]
    multicast.set_reuse_port(true)?;
    multicast.set_broadcast(!loopback)?;
    multicast.set_multicast_loop_v4(loopback)?;
    multicast.set_multicast_if_v4(&addr)?;
    multicast.bind(&SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, PORT)).into())?;
    multicast.join_multicast_v4(&GROUP, &addr)?;
    let measure = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    measure.bind(&SocketAddr::V4(SocketAddrV4::new(addr, 0)).into())?;
    Ok((
        nonblocking(unicast)?,
        nonblocking(multicast)?,
        nonblocking(measure)?,
    ))
}

fn open_v6(addr: Ipv6Addr, scope: u32) -> std::io::Result<Sockets> {
    let at = |port| SocketAddr::V6(SocketAddrV6::new(addr, port, 0, scope));
    let unicast = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    unicast.set_only_v6(true)?;
    unicast.set_reuse_address(true)?;
    unicast.set_multicast_loop_v6(false)?;
    unicast.set_multicast_if_v6(scope)?;
    unicast.bind(&at(0).into())?;
    let multicast = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    multicast.set_only_v6(true)?;
    multicast.set_reuse_address(true)?;
    #[cfg(unix)]
    multicast.set_reuse_port(true)?;
    multicast.set_multicast_loop_v6(false)?;
    multicast.set_multicast_if_v6(scope)?;
    multicast.bind(&SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, PORT, 0, 0)).into())?;
    multicast.join_multicast_v6(&GROUP_V6, scope)?;
    let measure = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    measure.set_only_v6(true)?;
    measure.bind(&at(0).into())?;
    Ok((
        nonblocking(unicast)?,
        nonblocking(multicast)?,
        nonblocking(measure)?,
    ))
}

#[derive(Debug, Clone)]
struct Found {
    name: String,
    ip: IpAddr,
    index: Option<u32>,
}

fn gateways_of(found: &[Found]) -> Vec<Addr> {
    let v4_names: Vec<&str> = found
        .iter()
        .filter(|f| f.ip.is_ipv4())
        .map(|f| f.name.as_str())
        .collect();
    let mut v: Vec<Addr> = found
        .iter()
        .filter_map(|f| match f.ip {
            IpAddr::V4(_) => Some(Addr { ip: f.ip, scope: 0 }),
            IpAddr::V6(a) => (v4_names.contains(&f.name.as_str())
                && !a.is_loopback()
                && a.is_unicast_link_local())
            .then(|| Addr {
                ip: f.ip,
                scope: f.index.unwrap_or(0),
            }),
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

fn interfaces() -> Vec<Addr> {
    let found: Vec<Found> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .map(|i| Found {
            ip: i.ip(),
            index: i.index,
            name: i.name,
        })
        .collect();
    gateways_of(&found)
}

#[derive(Debug)]
pub struct Shared {
    pub core: Core,
    pub pending: Vec<Effect>,
}

fn rescan(gateways: &mut HashMap<Addr, Gateway>) {
    let wanted = interfaces();
    gateways.retain(|a, _| wanted.contains(a));
    for a in wanted {
        if let std::collections::hash_map::Entry::Vacant(e) = gateways.entry(a)
            && let Ok(g) = Gateway::open(a)
        {
            e.insert(g);
        }
    }
}

fn receive(g: &mut Gateway, core: &mut Core, effects: &mut Vec<Effect>, now: i64) {
    let mut buf = [0u8; wire::MAX_MESSAGE];
    for sock in [&g.multicast, &g.unicast] {
        while let Ok((n, src)) = sock.recv_from(&mut buf) {
            if let Some(msg) = wire::parse_discovery(&buf[..n]) {
                effects.extend(core.on_discovery(g.addr.ip, src, &msg, now));
            }
        }
    }
    loop {
        match g.measure.recv_from(&mut buf) {
            Ok((n, src)) => {
                let ghost = core.xform().host_to_ghost(now);
                if let Some(reply) = wire::pong(&buf[..n], core.session(), ghost) {
                    let _ = g.measure.send_to(&reply, src);
                } else if let Some(p) = wire::parse_pong(&buf[..n]) {
                    for m in &mut g.measurements {
                        let step = m.on_pong(&p, now);
                        finish(core, m, step, &g.measure, effects, now);
                    }
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
    for m in &mut g.measurements {
        if now >= m.deadline {
            let step = m.on_timeout(now);
            finish(core, m, step, &g.measure, effects, now);
        }
    }
    g.measurements.retain(|m| m.deadline != i64::MAX);
}

fn apply(
    gateways: &mut HashMap<Addr, Gateway>,
    core: &mut Core,
    effects: Vec<Effect>,
    now: i64,
) -> bool {
    let mut broadcast = false;
    for e in effects {
        match e {
            Effect::Broadcast => broadcast = true,
            Effect::Respond { gateway, to } => {
                if let Some(g) = gateways.values_mut().find(|g| g.addr.ip == gateway) {
                    g.send_state(core, Kind::Response, to, now);
                }
            }
            Effect::Measure {
                session,
                gateway,
                endpoint,
            } => {
                if let Some(g) = gateways.values_mut().find(|g| g.addr.ip == gateway) {
                    let endpoint = g.addr.reach(endpoint);
                    let (m, ping) = Measurement::start(session, endpoint, now);
                    let _ = g.measure.send_to(&ping, endpoint);
                    g.measurements.push(m);
                } else {
                    let more = core.on_measured(session, None, now);
                    broadcast |= more.contains(&Effect::Broadcast);
                }
            }
        }
    }
    broadcast
}

pub fn run(shared: &Arc<Mutex<Shared>>, running: &Arc<AtomicBool>) {
    let mut gateways: HashMap<Addr, Gateway> = HashMap::new();
    let mut next_scan = 0;
    while running.load(Ordering::Relaxed) {
        let now = host_micros();
        if now >= next_scan {
            next_scan = now + RESCAN;
            rescan(&mut gateways);
        }
        let Ok(mut s) = shared.lock() else {
            return;
        };
        let mut effects = std::mem::take(&mut s.pending);
        effects.extend(s.core.prune(now));
        effects.extend(s.core.tick(now));
        for g in gateways.values_mut() {
            receive(g, &mut s.core, &mut effects, now);
        }
        let broadcast = apply(&mut gateways, &mut s.core, effects, now);
        for g in gateways.values_mut() {
            if broadcast {
                let gap = MIN_GAP - (now - g.last_send);
                g.next_send = if gap > 0 { now + gap } else { now };
            }
            if now >= g.next_send {
                g.send_state(&s.core, Kind::Alive, g.addr.group(), now);
                g.next_send = now + PERIOD;
            }
        }
        drop(s);
        std::thread::sleep(Duration::from_millis(2));
    }
    if let Ok(s) = shared.lock() {
        for g in gateways.values() {
            g.byebye(&s.core);
        }
    }
}

fn finish(
    core: &mut Core,
    m: &mut Measurement,
    step: Step,
    sock: &UdpSocket,
    effects: &mut Vec<Effect>,
    now: i64,
) {
    match step {
        Step::Ping(p) => {
            let _ = sock.send_to(&p, m.endpoint);
        }
        Step::Done(x) => {
            m.deadline = i64::MAX;
            effects.extend(core.on_measured(m.session, x, now));
        }
        Step::Wait => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(name: &str, ip: &str, index: u32) -> Found {
        Found {
            name: name.into(),
            ip: ip.parse().unwrap(),
            index: Some(index),
        }
    }

    #[test]
    fn ipv6_gateways_are_link_local_on_interfaces_with_ipv4() {
        let gateways = gateways_of(&[
            found("en0", "192.168.1.5", 4),
            found("en0", "fe80::1c2b:3a4f:fe5d:6e7f", 4),
            found("en0", "2001:db8::5", 4),
            found("lo0", "127.0.0.1", 1),
            found("lo0", "::1", 1),
            found("utun3", "fe80::aaaa", 9),
        ]);
        let ips: Vec<(String, u32)> = gateways
            .iter()
            .map(|a| (a.ip.to_string(), a.scope))
            .collect();
        assert_eq!(
            ips,
            [
                ("127.0.0.1".to_owned(), 0),
                ("192.168.1.5".to_owned(), 0),
                ("fe80::1c2b:3a4f:fe5d:6e7f".to_owned(), 4),
            ]
        );
    }

    #[test]
    fn an_ipv6_gateway_sends_to_the_group_and_peers_on_its_interface() {
        let a = Addr {
            ip: "fe80::1".parse().unwrap(),
            scope: 7,
        };
        assert_eq!(a.group().to_string(), "[ff12::8080%7]:20808");
        let peer: SocketAddr = "[fe80::2]:5000".parse().unwrap();
        assert_eq!(a.reach(peer).to_string(), "[fe80::2%7]:5000");
        let v4: SocketAddr = "10.0.0.2:5000".parse().unwrap();
        assert_eq!(a.reach(v4), v4);
    }
}
