use std::net::{IpAddr, SocketAddr};

use crate::timeline::{
    Beats, GhostXForm, StartStop, Tempo, Timeline, client_from_session, session_from_client,
};
use crate::wire::{Discovery, Kind, NodeId, PeerState};

const SESSION_EPS: i64 = 500_000;
const REMEASURE: i64 = 30_000_000;
const PRUNE_PAD: i64 = 1_000_000;

#[derive(Debug, Clone, PartialEq)]
struct Peer {
    id: NodeId,
    gateway: IpAddr,
    state: PeerState,
    deadline: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Respond {
        gateway: IpAddr,
        to: SocketAddr,
    },
    Broadcast,
    Measure {
        session: NodeId,
        gateway: IpAddr,
        endpoint: SocketAddr,
    },
}

#[derive(Debug, Clone)]
pub struct Core {
    pub node: NodeId,
    session: NodeId,
    session_timeline: Timeline,
    xform: GhostXForm,
    others: Vec<(NodeId, Timeline)>,
    measuring: Vec<NodeId>,
    remeasure_at: Option<i64>,
    peers: Vec<Peer>,
    session_start_stop: StartStop,
    client: Timeline,
    client_start_stop: StartStop,
    pub start_stop_sync: bool,
}

impl Core {
    pub fn new(tempo: f64, now: i64) -> Self {
        let node = NodeId::random();
        let xform = GhostXForm {
            slope: 1.0,
            intercept: -now,
        };
        let session_timeline = Timeline {
            tempo: Tempo(tempo).clamped(),
            beat_origin: Beats(0),
            time_origin: 0,
        };
        let client = client_from_session(&Timeline::default(), &session_timeline, now, &xform);
        Self {
            node,
            session: node,
            session_timeline,
            xform,
            others: Vec::new(),
            measuring: Vec::new(),
            remeasure_at: None,
            peers: Vec::new(),
            session_start_stop: StartStop::default(),
            client,
            client_start_stop: StartStop::default(),
            start_stop_sync: false,
        }
    }

    pub fn reset(&mut self, now: i64) {
        let beats = self
            .session_timeline
            .to_beats(self.xform.host_to_ghost(now));
        self.node = NodeId::random();
        self.session = self.node;
        self.xform = GhostXForm {
            slope: 1.0,
            intercept: -now,
        };
        self.session_timeline = Timeline {
            tempo: self.session_timeline.tempo,
            beat_origin: beats,
            time_origin: 0,
        };
        self.session_start_stop = StartStop::default();
        self.others.clear();
        self.measuring.clear();
        self.remeasure_at = None;
        self.peers.clear();
        self.client = client_from_session(&self.client, &self.session_timeline, now, &self.xform);
    }

    pub fn state(&self, measure: Option<SocketAddr>) -> PeerState {
        PeerState {
            timeline: self.session_timeline,
            session: self.session,
            start_stop: self.session_start_stop,
            measure,
        }
    }

    pub fn xform(&self) -> GhostXForm {
        self.xform
    }

    pub fn session(&self) -> NodeId {
        self.session
    }

    pub fn num_peers(&self) -> usize {
        let mut ids: Vec<NodeId> = self
            .peers
            .iter()
            .filter(|p| p.state.session == self.session)
            .map(|p| p.id)
            .collect();
        ids.sort();
        ids.dedup();
        ids.len()
    }

    pub fn client(&self) -> (Timeline, StartStop) {
        (self.client, self.client_start_stop)
    }

    pub fn on_discovery(
        &mut self,
        gateway: IpAddr,
        src: SocketAddr,
        msg: &Discovery,
        now: i64,
    ) -> Vec<Effect> {
        if msg.ident == self.node {
            return Vec::new();
        }
        let before = self.num_peers();
        let mut out = Vec::new();
        match msg.kind {
            Kind::ByeBye => {
                self.peers
                    .retain(|p| !(p.id == msg.ident && p.gateway == gateway));
            }
            Kind::Alive | Kind::Response => {
                if msg.kind == Kind::Alive {
                    out.push(Effect::Respond { gateway, to: src });
                }
                let deadline = now + i64::from(msg.ttl) * 1_000_000;
                let new_timeline = !self.peers.iter().any(|p| {
                    p.state.session == msg.state.session && p.state.timeline == msg.state.timeline
                });
                let new_start_stop = !self.peers.iter().any(|p| {
                    p.state.session == msg.state.session
                        && p.state.start_stop == msg.state.start_stop
                });
                let peer = Peer {
                    id: msg.ident,
                    gateway,
                    state: msg.state,
                    deadline,
                };
                match self
                    .peers
                    .iter_mut()
                    .find(|p| p.id == msg.ident && p.gateway == gateway)
                {
                    Some(p) => *p = peer,
                    None => self.peers.push(peer),
                }
                if new_timeline {
                    out.extend(self.saw_timeline(msg.state.session, msg.state.timeline, now));
                }
                if new_start_stop {
                    out.extend(self.saw_start_stop(msg.state.session, msg.state.start_stop, now));
                }
            }
        }
        out.extend(self.peers_changed(before, now));
        out
    }

    pub fn prune(&mut self, now: i64) -> Vec<Effect> {
        let before = self.num_peers();
        self.peers.retain(|p| p.deadline + PRUNE_PAD > now);
        self.peers_changed(before, now)
    }

    fn peers_changed(&mut self, before: usize, now: i64) -> Vec<Effect> {
        if before > 0
            && self.num_peers() == 0
            && self.peers.iter().all(|p| p.state.session != self.session)
        {
            self.reset(now);
            return vec![Effect::Broadcast];
        }
        Vec::new()
    }

    fn saw_timeline(&mut self, sid: NodeId, tl: Timeline, now: i64) -> Vec<Effect> {
        if sid == self.session {
            if tl.beat_origin > self.session_timeline.beat_origin {
                self.session_timeline = tl.clamped();
                self.update_client(now);
                return vec![Effect::Broadcast];
            }
            return Vec::new();
        }
        if let Some((_, stored)) = self.others.iter_mut().find(|(id, _)| *id == sid) {
            if tl.beat_origin > stored.beat_origin {
                *stored = tl;
            }
            return Vec::new();
        }
        self.others.push((sid, tl));
        self.measure(sid).into_iter().collect()
    }

    fn saw_start_stop(&mut self, sid: NodeId, st: StartStop, _now: i64) -> Vec<Effect> {
        if sid != self.session || st.timestamp <= self.session_start_stop.timestamp {
            return Vec::new();
        }
        self.session_start_stop = st;
        if self.start_stop_sync {
            self.update_client_start_stop();
        }
        vec![Effect::Broadcast]
    }

    fn measure(&mut self, sid: NodeId) -> Option<Effect> {
        if self.measuring.contains(&sid) {
            return None;
        }
        let peer = self
            .peers
            .iter()
            .filter(|p| p.state.session == sid && p.state.measure.is_some())
            .min_by_key(|p| (p.id != sid, p.id))?;
        self.measuring.push(sid);
        Some(Effect::Measure {
            session: sid,
            gateway: peer.gateway,
            endpoint: peer.state.measure?,
        })
    }

    pub fn on_measured(&mut self, sid: NodeId, xform: Option<GhostXForm>, now: i64) -> Vec<Effect> {
        self.measuring.retain(|s| *s != sid);
        let Some(x) = xform.filter(|x| !x.failed()) else {
            if sid == self.session {
                self.remeasure_at = Some(now + REMEASURE);
            } else {
                self.others.retain(|(id, _)| *id != sid);
                self.peers.retain(|p| p.state.session != sid);
            }
            return Vec::new();
        };
        if sid == self.session {
            self.xform = x;
            self.update_client(now);
            return vec![Effect::Broadcast];
        }
        let diff = x.host_to_ghost(now) - self.xform.host_to_ghost(now);
        let join = diff > SESSION_EPS || (diff.abs() < SESSION_EPS && sid < self.session);
        if !join {
            return Vec::new();
        }
        let tl = self
            .others
            .iter()
            .find(|(id, _)| *id == sid)
            .map_or(self.session_timeline, |(_, t)| *t);
        self.others.retain(|(id, _)| *id != sid);
        self.others.push((self.session, self.session_timeline));
        self.session = sid;
        self.session_timeline = tl.clamped();
        self.xform = x;
        self.session_start_stop = StartStop::default();
        self.remeasure_at = Some(now + REMEASURE);
        self.update_client(now);
        vec![Effect::Broadcast]
    }

    pub fn tick(&mut self, now: i64) -> Vec<Effect> {
        match self.remeasure_at {
            Some(at) if now >= at && self.session != self.node => {
                self.remeasure_at = None;
                self.measure(self.session).into_iter().collect()
            }
            _ => Vec::new(),
        }
    }

    fn update_client(&mut self, now: i64) {
        self.client = client_from_session(&self.client, &self.session_timeline, now, &self.xform);
        if self.start_stop_sync && self.session_start_stop != StartStop::default() {
            self.update_client_start_stop();
        }
    }

    fn update_client_start_stop(&mut self) {
        let s = self.session_start_stop;
        self.client_start_stop = StartStop {
            playing: s.playing,
            beats: s.beats,
            timestamp: self.xform.ghost_to_host(s.timestamp),
        };
    }

    pub fn commit_timeline(&mut self, client: Timeline, now: i64) -> Vec<Effect> {
        let client = client.clamped();
        if client == self.client {
            return Vec::new();
        }
        self.client = client;
        let new = session_from_client(&self.session_timeline, &client, now, &self.xform);
        if new == self.session_timeline {
            return Vec::new();
        }
        self.session_timeline = new;
        let sid = self.session;
        for p in self.peers.iter_mut().filter(|p| p.state.session == sid) {
            p.state.timeline = new;
        }
        self.client = client_from_session(&self.client, &self.session_timeline, now, &self.xform);
        vec![Effect::Broadcast]
    }

    pub fn commit_start_stop(&mut self, playing: bool, time: i64, now: i64) -> Vec<Effect> {
        self.client_start_stop = StartStop {
            playing,
            beats: self.client.to_beats(time),
            timestamp: now,
        };
        if !self.start_stop_sync {
            return Vec::new();
        }
        let ghost = self.xform.host_to_ghost(now);
        if ghost <= self.session_start_stop.timestamp {
            return Vec::new();
        }
        self.session_start_stop = StartStop {
            playing,
            beats: self
                .session_timeline
                .to_beats(self.xform.host_to_ghost(time)),
            timestamp: ghost,
        };
        vec![Effect::Broadcast]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alive(ident: NodeId, session: NodeId, tl: Timeline) -> Discovery {
        Discovery {
            kind: Kind::Alive,
            ttl: 5,
            ident,
            state: PeerState {
                timeline: tl,
                session,
                start_stop: StartStop::default(),
                measure: Some("10.0.0.2:4000".parse().unwrap()),
            },
        }
    }

    fn gw() -> IpAddr {
        "10.0.0.1".parse().unwrap()
    }

    fn src() -> SocketAddr {
        "10.0.0.2:5000".parse().unwrap()
    }

    #[test]
    fn a_new_session_is_measured_and_the_older_one_joined() {
        let mut a = Core::new(120.0, 1_000_000);
        let other = NodeId(*b"!!!!!!!!");
        let tl = Timeline {
            tempo: Tempo(128.0),
            beat_origin: Beats(5_000_000),
            time_origin: 0,
        };
        let fx = a.on_discovery(gw(), src(), &alive(other, other, tl), 2_000_000);
        assert!(fx.contains(&Effect::Respond {
            gateway: gw(),
            to: src()
        }));
        assert!(
            fx.iter()
                .any(|e| matches!(e, Effect::Measure { session, .. } if *session == other))
        );
        assert_eq!(a.num_peers(), 0, "not in our session yet");
        let x = GhostXForm {
            slope: 1.0,
            intercept: a.xform().intercept + 10_000_000,
        };
        let fx = a.on_measured(other, Some(x), 2_100_000);
        assert_eq!(fx, [Effect::Broadcast]);
        assert_eq!(a.session(), other);
        assert_eq!(a.client().0.tempo, Tempo(128.0));
        assert_eq!(a.num_peers(), 1);
    }

    #[test]
    fn a_later_beat_origin_wins_and_the_last_peer_leaving_resets() {
        let mut a = Core::new(120.0, 0);
        let me = a.session();
        let peer = NodeId(*b"zzzzzzzz");
        let tl = |bpm, origin| Timeline {
            tempo: Tempo(bpm),
            beat_origin: Beats(origin),
            time_origin: 0,
        };
        a.on_discovery(gw(), src(), &alive(peer, me, tl(130.0, 1)), 10);
        assert_eq!(a.client().0.tempo, Tempo(130.0));
        a.on_discovery(gw(), src(), &alive(peer, me, tl(90.0, 0)), 20);
        assert_eq!(a.client().0.tempo, Tempo(130.0), "older origin ignored");
        assert_eq!(a.num_peers(), 1);
        let fx = a.prune(20_000_000);
        assert_eq!(fx, [Effect::Broadcast]);
        assert_ne!(a.session(), me, "a new session");
        assert_eq!(a.client().0.tempo, Tempo(130.0), "the tempo stays");
    }

    #[test]
    fn a_local_tempo_change_goes_out() {
        let mut a = Core::new(120.0, 0);
        let mut tl = a.client().0;
        tl.tempo = Tempo(124.0);
        assert_eq!(a.commit_timeline(tl, 1_000), [Effect::Broadcast]);
        assert_eq!(a.state(None).timeline.tempo, Tempo(124.0));
        assert_eq!(a.commit_timeline(a.client().0, 2_000), []);
    }

    #[test]
    fn start_stop_follows_only_newer_states_of_our_session() {
        let mut a = Core::new(120.0, 0);
        a.start_stop_sync = true;
        let me = a.session();
        let mut msg = alive(NodeId(*b"zzzzzzzz"), me, a.state(None).timeline);
        msg.state.start_stop = StartStop {
            playing: true,
            beats: Beats(0),
            timestamp: 500,
        };
        a.on_discovery(gw(), src(), &msg, 10);
        assert!(a.client().1.playing);
        msg.state.start_stop = StartStop {
            playing: false,
            beats: Beats(0),
            timestamp: 400,
        };
        a.on_discovery(gw(), src(), &msg, 20);
        assert!(a.client().1.playing, "older state ignored");
    }
}
