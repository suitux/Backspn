pub const MIN_BPM: f64 = 20.0;
pub const MAX_BPM: f64 = 999.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Beats(pub i64);

impl Beats {
    pub fn from_f64(b: f64) -> Self {
        Self((b * 1e6).round() as i64)
    }

    pub fn floating(self) -> f64 {
        self.0 as f64 / 1e6
    }

    fn rem(self, q: Self) -> Self {
        if q.0 == 0 {
            Self(0)
        } else {
            Self(self.0 % q.0)
        }
    }
}

impl std::ops::Add for Beats {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self(self.0 + o.0)
    }
}

impl std::ops::Sub for Beats {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self(self.0 - o.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Tempo(pub f64);

impl Tempo {
    pub fn micros_per_beat(self) -> i64 {
        (60e6 / self.0).round() as i64
    }

    pub fn from_micros_per_beat(mpb: i64) -> Self {
        Self(60e6 / mpb as f64)
    }

    pub fn micros_to_beats(self, micros: i64) -> Beats {
        Beats::from_f64(micros as f64 / self.micros_per_beat() as f64)
    }

    pub fn beats_to_micros(self, b: Beats) -> i64 {
        (b.floating() * self.micros_per_beat() as f64).round() as i64
    }

    #[must_use]
    pub fn clamped(self) -> Self {
        Self(self.0.clamp(MIN_BPM, MAX_BPM))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Timeline {
    pub tempo: Tempo,
    pub beat_origin: Beats,
    pub time_origin: i64,
}

impl Timeline {
    pub fn to_beats(&self, time: i64) -> Beats {
        self.beat_origin + self.tempo.micros_to_beats(time - self.time_origin)
    }

    pub fn from_beats(&self, beats: Beats) -> i64 {
        self.time_origin + self.tempo.beats_to_micros(beats - self.beat_origin)
    }

    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            tempo: self.tempo.clamped(),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GhostXForm {
    pub slope: f64,
    pub intercept: i64,
}

impl GhostXForm {
    pub fn host_to_ghost(&self, host: i64) -> i64 {
        (self.slope * host as f64).round() as i64 + self.intercept
    }

    pub fn ghost_to_host(&self, ghost: i64) -> i64 {
        ((ghost - self.intercept) as f64 / self.slope).round() as i64
    }

    pub fn failed(&self) -> bool {
        self.slope == 0.0 && self.intercept == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartStop {
    pub playing: bool,
    pub beats: Beats,
    pub timestamp: i64,
}

pub fn phase(b: Beats, q: Beats) -> Beats {
    if q.0 == 0 {
        return Beats(0);
    }
    let bins = (b.0.abs() + q.0) / q.0;
    Beats(b.0 + bins * q.0).rem(q)
}

pub fn next_phase_match(x: Beats, target: Beats, q: Beats) -> Beats {
    let d = Beats(phase(target, q).0 - phase(x, q).0 + q.0).rem(q);
    x + d
}

pub fn closest_phase_match(x: Beats, target: Beats, q: Beats) -> Beats {
    next_phase_match(x - Beats::from_f64(0.5 * q.floating()), target, q)
}

pub fn to_phase_encoded_beats(tl: &Timeline, time: i64, q: Beats) -> Beats {
    let b = tl.to_beats(time);
    closest_phase_match(b, b - tl.beat_origin, q)
}

pub fn from_phase_encoded_beats(tl: &Timeline, beat: Beats, q: Beats) -> i64 {
    let from_origin = beat - tl.beat_origin;
    let origin_offset = from_origin - phase(from_origin, q);
    let inv = closest_phase_match(q - phase(from_origin, q), q - phase(beat, q), q);
    tl.from_beats(tl.beat_origin + origin_offset + q - inv)
}

pub fn client_from_session(
    cur: &Timeline,
    session: &Timeline,
    now: i64,
    x: &GhostXForm,
) -> Timeline {
    let tmp = Timeline {
        tempo: session.tempo,
        beat_origin: cur.to_beats(now),
        time_origin: now,
    };
    let host_beat_zero = x.ghost_to_host(session.from_beats(Beats(0)));
    Timeline {
        tempo: session.tempo,
        beat_origin: tmp.to_beats(host_beat_zero),
        time_origin: host_beat_zero,
    }
}

pub fn session_from_client(cur: &Timeline, client: &Timeline, at: i64, x: &GhostXForm) -> Timeline {
    let ghost_beat0 = x.host_to_ghost(client.time_origin);
    if cur.to_beats(ghost_beat0) == Beats(0) && client.tempo == cur.tempo {
        return *cur;
    }
    let tmp = Timeline {
        tempo: client.tempo,
        beat_origin: Beats(0),
        time_origin: ghost_beat0,
    };
    let origin = cur
        .to_beats(x.host_to_ghost(at))
        .max(cur.beat_origin + Beats(1));
    Timeline {
        tempo: client.tempo,
        beat_origin: origin,
        time_origin: tmp.from_beats(origin),
    }
}

pub fn shift_client(tl: &Timeline, shift: Beats) -> Timeline {
    let d = tl.from_beats(shift) - tl.from_beats(Beats(0));
    Timeline {
        time_origin: tl.time_origin - d,
        ..*tl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: f64) -> Beats {
        Beats::from_f64(x)
    }

    #[test]
    fn tempo_goes_through_whole_micros_per_beat() {
        assert_eq!(Tempo(120.0).micros_per_beat(), 500_000);
        assert_eq!(Tempo(999.0).micros_per_beat(), 60_060);
        assert!((Tempo::from_micros_per_beat(60_060).0 - 999.000_999).abs() < 1e-3);
        assert_eq!(Tempo(120.0).micros_to_beats(1_000_000), b(2.0));
        assert_eq!(Tempo(120.0).beats_to_micros(b(1.5)), 750_000);
        assert_eq!(Tempo(5.0).clamped(), Tempo(MIN_BPM));
    }

    #[test]
    fn timeline_maps_both_ways() {
        let tl = Timeline {
            tempo: Tempo(120.0),
            beat_origin: b(4.0),
            time_origin: 1_000_000,
        };
        assert_eq!(tl.to_beats(2_000_000), b(6.0));
        assert_eq!(tl.from_beats(b(6.0)), 2_000_000);
    }

    #[test]
    fn phase_is_always_within_the_quantum() {
        let q = b(4.0);
        assert_eq!(phase(b(5.0), q), b(1.0));
        assert_eq!(phase(b(-1.0), q), b(3.0));
        assert_eq!(phase(b(3.0), Beats(0)), Beats(0));
        assert_eq!(next_phase_match(b(5.0), b(0.0), q), b(8.0));
        assert_eq!(closest_phase_match(b(5.0), b(0.0), q), b(4.0));
        assert_eq!(closest_phase_match(b(7.0), b(0.0), q), b(8.0));
    }

    #[test]
    fn phase_encoded_beats_round_trip() {
        let tl = Timeline {
            tempo: Tempo(128.0),
            beat_origin: b(1.25),
            time_origin: 3_000_000,
        };
        let q = b(4.0);
        for t in [3_000_000, 5_123_456, 9_999_999] {
            let beat = to_phase_encoded_beats(&tl, t, q);
            let back = from_phase_encoded_beats(&tl, beat, q);
            assert!((back - t).abs() <= 2, "{t} → {back}");
        }
    }

    #[test]
    fn a_local_change_wins_by_a_later_beat_origin() {
        let x = GhostXForm {
            slope: 1.0,
            intercept: -1000,
        };
        let session = Timeline {
            tempo: Tempo(120.0),
            beat_origin: b(0.0),
            time_origin: 0,
        };
        let client = client_from_session(&Timeline::default(), &session, 5000, &x);
        assert_eq!(session_from_client(&session, &client, 5000, &x), session);
        let faster = Timeline {
            tempo: Tempo(130.0),
            ..client
        };
        let new = session_from_client(&session, &faster, 5000, &x);
        assert_eq!(new.tempo, Tempo(130.0));
        assert!(new.beat_origin > session.beat_origin);
    }

    #[test]
    fn ghost_transform_and_its_failure_mark() {
        let x = GhostXForm {
            slope: 1.0,
            intercept: 250,
        };
        assert_eq!(x.host_to_ghost(1000), 1250);
        assert_eq!(x.ghost_to_host(1250), 1000);
        assert!(GhostXForm::default().failed());
        assert!(!x.failed());
    }
}
