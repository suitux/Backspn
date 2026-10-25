pub const MAX_MARKERS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Segment {
    pub t_s: f64,
    pub beat: f64,
    pub len_s: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segments {
    items: [Segment; MAX_MARKERS],
    len: usize,
}

impl Segments {
    pub const NONE: Self = Self {
        items: [Segment {
            t_s: 0.0,
            beat: 0.0,
            len_s: 0.0,
        }; MAX_MARKERS],
        len: 0,
    };

    fn as_slice(&self) -> &[Segment] {
        &self.items[..self.len]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    pub bpm: f64,
    pub anchor_s: f64,
    pub segments: Segments,
}

impl Grid {
    pub const FIXED: Self = Self {
        bpm: 120.0,
        anchor_s: 0.0,
        segments: Segments::NONE,
    };

    pub fn new(bpm: f64, anchor_s: f64) -> Self {
        Self {
            bpm,
            anchor_s,
            ..Self::FIXED
        }
    }

    #[must_use]
    pub fn with_markers(self, markers: &[f64]) -> Self {
        let mut times: Vec<f64> = markers
            .iter()
            .copied()
            .filter(|t| t.is_finite() && *t > self.anchor_s + 1e-6)
            .collect();
        times.sort_by(f64::total_cmp);
        let mut segments = Segments::NONE;
        let (mut t, mut beat, mut beat_len) = (self.anchor_s, 0.0, self.period_s());
        for next in times {
            if segments.len + 1 >= MAX_MARKERS {
                break;
            }
            let stretch = next - t;
            let beats = (stretch / beat_len).round();
            if beats < 1.0 {
                continue;
            }
            beat_len = stretch / beats;
            segments.items[segments.len] = Segment {
                t_s: t,
                beat,
                len_s: beat_len,
            };
            segments.len += 1;
            t = next;
            beat += beats;
        }
        if segments.len > 0 {
            segments.items[segments.len] = Segment {
                t_s: t,
                beat,
                len_s: beat_len,
            };
            segments.len += 1;
        }
        Self { segments, ..self }
    }

    pub fn segments(&self) -> &[Segment] {
        self.segments.as_slice()
    }

    pub fn period_s(&self) -> f64 {
        60.0 / self.bpm
    }

    pub fn period_at(&self, t_s: f64) -> f64 {
        self.segment_at_time(t_s)
            .map_or(self.period_s(), |s| s.len_s)
    }

    fn segment_at_time(&self, t_s: f64) -> Option<&Segment> {
        let segs = self.segments();
        segs.iter().rev().find(|s| s.t_s <= t_s).or(segs.first())
    }

    pub fn beat_at(&self, t_s: f64) -> f64 {
        match self.segment_at_time(t_s) {
            Some(s) => s.beat + (t_s - s.t_s) / s.len_s,
            None => (t_s - self.anchor_s) / self.period_s(),
        }
    }

    pub fn time_of(&self, beat: f64) -> f64 {
        let segs = self.segments();
        match segs.iter().rev().find(|s| s.beat <= beat).or(segs.first()) {
            Some(s) => s.t_s + (beat - s.beat) * s.len_s,
            None => self.anchor_s + beat * self.period_s(),
        }
    }

    pub fn beats_in(&self, from_s: f64, to_s: f64) -> impl Iterator<Item = (f64, bool)> + '_ {
        let first = self.beat_at(from_s.max(0.0)).ceil();
        (0..)
            .map(move |n| first + f64::from(n))
            .map(|beat| (self.time_of(beat), beat.rem_euclid(4.0) == 0.0))
            .take_while(move |&(t, _)| t < to_s)
    }

    pub fn nearest_beat(&self, t_s: f64) -> f64 {
        self.time_of(self.beat_at(t_s).round())
    }

    pub fn offset_from_beat(&self, t_s: f64) -> f64 {
        t_s - self.nearest_beat(t_s)
    }
}

pub fn wrap_phase(beats: f64) -> f64 {
    (beats + 0.5).rem_euclid(1.0) - 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_arithmetic() {
        let g = Grid::new(120.0, 0.1);
        assert!((g.beat_at(1.1) - 2.0).abs() < 1e-12);
        assert!((g.time_of(-1.0) + 0.4).abs() < 1e-12);
        assert!((g.nearest_beat(1.32) - 1.1).abs() < 1e-12);
        assert!((g.offset_from_beat(1.32) - 0.22).abs() < 1e-12);
    }

    #[test]
    fn beats_in_a_window_mark_the_bars() {
        let grid = Grid::new(120.0, 0.25);
        let beats: Vec<(f64, bool)> = grid.beats_in(-1.0, 2.3).collect();
        assert_eq!(
            beats,
            [
                (0.25, true),
                (0.75, false),
                (1.25, false),
                (1.75, false),
                (2.25, true)
            ]
        );
        assert_eq!(grid.beats_in(5.0, 5.0).count(), 0);
    }

    #[test]
    fn flexgrid_stretches_have_whole_beats() {
        let g = Grid::new(120.0, 0.0).with_markers(&[4.1, 6.2]);
        assert_eq!(g.segments().len(), 3);
        assert!((g.beat_at(4.1) - 8.0).abs() < 1e-12);
        assert!((g.beat_at(6.2) - 12.0).abs() < 1e-12);
        assert!((g.time_of(13.0) - 6.725).abs() < 1e-12);
        assert!(
            (g.time_of(-2.0) + 1.025).abs() < 1e-12,
            "first stretch before"
        );
        assert!((g.period_at(5.0) - 0.525).abs() < 1e-12);
        assert_eq!(
            Grid::new(120.0, 0.0).with_markers(&[]),
            Grid::new(120.0, 0.0)
        );
    }

    #[test]
    fn wraps_phase() {
        assert!((wrap_phase(0.75) + 0.25).abs() < 1e-12);
        assert!((wrap_phase(-0.75) - 0.25).abs() < 1e-12);
        assert!((wrap_phase(3.1) - 0.1).abs() < 1e-9);
    }
}
