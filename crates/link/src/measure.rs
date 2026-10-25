use std::net::SocketAddr;

use crate::timeline::GhostXForm;
use crate::wire::{self, NodeId, Pong};

pub const RETRY: i64 = 50_000;
const MAX_RETRIES: u32 = 5;
const SAMPLES: usize = 100;

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Ping(Vec<u8>),
    Done(Option<GhostXForm>),
    Wait,
}

#[derive(Debug, Clone)]
pub struct Measurement {
    pub session: NodeId,
    pub endpoint: SocketAddr,
    samples: Vec<f64>,
    retries: u32,
    pub deadline: i64,
}

impl Measurement {
    pub fn start(session: NodeId, endpoint: SocketAddr, now: i64) -> (Self, Vec<u8>) {
        let m = Self {
            session,
            endpoint,
            samples: Vec::new(),
            retries: 0,
            deadline: now + RETRY,
        };
        (m, wire::ping(now, None))
    }

    pub fn on_pong(&mut self, p: &Pong, now: i64) -> Step {
        if p.session != self.session {
            return Step::Done(None);
        }
        if p.ghost != 0 && p.host != 0 {
            self.samples
                .push(p.ghost as f64 - (now + p.host) as f64 / 2.0);
            if p.prev_ghost != 0 {
                self.samples
                    .push((p.ghost + p.prev_ghost) as f64 / 2.0 - p.host as f64);
            }
        }
        if self.samples.len() > SAMPLES {
            return Step::Done(Some(GhostXForm {
                slope: 1.0,
                intercept: median(&mut self.samples).round() as i64,
            }));
        }
        self.deadline = now + RETRY;
        Step::Ping(wire::ping(now, Some(p.ghost)))
    }

    pub fn on_timeout(&mut self, now: i64) -> Step {
        if self.retries >= MAX_RETRIES {
            return Step::Done(None);
        }
        self.retries += 1;
        self.deadline = now + RETRY;
        Step::Ping(wire::ping(now, None))
    }
}

pub fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    if n.is_multiple_of(2) {
        f64::midpoint(v[n / 2 - 1], v[n / 2])
    } else {
        v[n / 2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_steady_offset_is_measured_by_the_median() {
        let sid = NodeId(*b"SESSION1");
        let (mut m, _) = Measurement::start(sid, "10.0.0.2:4000".parse().unwrap(), 0);
        let mut host = 1000;
        let mut prev = 0;
        loop {
            let ghost = host + 100 + 7000;
            let pong = Pong {
                session: sid,
                ghost,
                prev_ghost: prev,
                host,
            };
            prev = ghost;
            host += 200;
            match m.on_pong(&pong, host) {
                Step::Ping(_) => {}
                Step::Done(Some(x)) => {
                    assert!((x.intercept - 7000).abs() <= 1, "{x:?}");
                    break;
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn five_timeouts_or_another_session_fail_it() {
        let sid = NodeId(*b"SESSION1");
        let (mut m, _) = Measurement::start(sid, "10.0.0.2:4000".parse().unwrap(), 0);
        for _ in 0..5 {
            assert!(matches!(m.on_timeout(0), Step::Ping(_)));
        }
        assert_eq!(m.on_timeout(0), Step::Done(None));
        let (mut m, _) = Measurement::start(sid, "10.0.0.2:4000".parse().unwrap(), 0);
        let other = Pong {
            session: NodeId(*b"OTHERSES"),
            ..Pong::default()
        };
        assert_eq!(m.on_pong(&other, 5), Step::Done(None));
        assert!((median(&mut [3.0, 1.0, 2.0, 10.0]) - 2.5).abs() < 1e-9);
    }
}
