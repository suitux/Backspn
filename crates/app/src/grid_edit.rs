use backspn_engine::Grid;

const MAX_DOUBLED_BPM: f64 = 300.0;
const MIN_HALVED_BPM: f64 = 40.0;
const NUDGE_BPM: f64 = 0.01;
const MOVE_S: f64 = 0.010;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridEdit {
    SetBpm(f64),
    Double,
    Half,
    Nudge(i32),
    MoveMarker(i32),
    SetMarker,
    CopyPhase(f64),
}

pub fn apply(grid: Grid, edit: GridEdit, position_s: f64) -> Option<Grid> {
    let bpm = |bpm: f64| (bpm > 0.0 && bpm.is_finite()).then_some(Grid { bpm, ..grid });
    match edit {
        GridEdit::SetBpm(b) => bpm(b),
        GridEdit::Double => bpm(grid.bpm * 2.0).filter(|g| g.bpm <= MAX_DOUBLED_BPM),
        GridEdit::Half => bpm(grid.bpm * 0.5).filter(|g| g.bpm >= MIN_HALVED_BPM),
        GridEdit::Nudge(steps) => bpm(grid.bpm + f64::from(steps) * NUDGE_BPM),
        GridEdit::MoveMarker(steps) => Some(Grid {
            anchor_s: grid.anchor_s + f64::from(steps) * MOVE_S,
            ..grid
        }),
        GridEdit::SetMarker => Some(Grid {
            anchor_s: position_s,
            ..grid
        }),
        GridEdit::CopyPhase(master_beat) => copy_phase(grid, master_beat, position_s),
    }
}

fn copy_phase(grid: Grid, master_beat: f64, position_s: f64) -> Option<Grid> {
    if grid.bpm < MIN_HALVED_BPM {
        return None;
    }
    let beat = grid.period_s();
    let offset = (position_s - master_beat * beat) % beat;
    Some(Grid {
        anchor_s: (grid.anchor_s / beat).floor() * beat + offset,
        ..grid
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: Grid = Grid {
        bpm: 128.0,
        anchor_s: 0.5,
        ..Grid::FIXED
    };

    fn bpm(edit: GridEdit) -> Option<f64> {
        apply(GRID, edit, 3.0).map(|g| g.bpm)
    }

    #[test]
    fn tempo_edits_keep_the_marker() {
        assert_eq!(bpm(GridEdit::Double), Some(256.0));
        assert_eq!(bpm(GridEdit::Half), Some(64.0));
        assert!((bpm(GridEdit::Nudge(-3)).unwrap() - 127.97).abs() < 1e-9);
        assert_eq!(bpm(GridEdit::SetBpm(124.5)), Some(124.5));
        assert_eq!(bpm(GridEdit::SetBpm(0.0)), None);
        assert_eq!(
            apply(GRID, GridEdit::Double, 3.0).map(|g| g.anchor_s),
            Some(0.5)
        );
    }

    #[test]
    fn double_and_half_stay_in_range() {
        let fast = Grid { bpm: 151.0, ..GRID };
        assert_eq!(apply(fast, GridEdit::Double, 0.0), None);
        let slow = Grid { bpm: 79.0, ..GRID };
        assert_eq!(apply(slow, GridEdit::Half, 0.0), None);
        let edge = Grid { bpm: 150.0, ..GRID };
        assert_eq!(
            apply(edge, GridEdit::Double, 0.0).map(|g| g.bpm),
            Some(300.0)
        );
    }

    #[test]
    fn copy_phase_puts_the_playhead_at_the_masters_phase() {
        let grid = Grid {
            bpm: 120.0,
            anchor_s: 10.3,
            ..Grid::FIXED
        };
        let copied = apply(grid, GridEdit::CopyPhase(57.25), 21.0).unwrap();
        assert!((copied.anchor_s - 9.875).abs() < 1e-9);
        let beat_at_playhead = copied.beat_at(21.0);
        assert!((beat_at_playhead - beat_at_playhead.floor() - 0.25).abs() < 1e-9);
        let slow = Grid { bpm: 39.0, ..grid };
        assert_eq!(apply(slow, GridEdit::CopyPhase(0.5), 21.0), None);
    }

    #[test]
    fn the_marker_moves_by_10_ms_or_to_the_playhead() {
        let moved = apply(GRID, GridEdit::MoveMarker(-2), 3.0).unwrap();
        assert!((moved.anchor_s - 0.48).abs() < 1e-9);
        assert_eq!(
            apply(GRID, GridEdit::SetMarker, 3.0).map(|g| g.anchor_s),
            Some(3.0)
        );
    }
}
