use backspn_control::{MoveMode, MoveSize};
use backspn_engine::{DeckCommand, LoopEdge};

pub fn command(mode: MoveMode, size: MoveSize, steps: i32, loop_beats: f64) -> Option<DeckCommand> {
    if steps == 0 {
        return None;
    }
    let edge = match mode {
        MoveMode::BeatJump => {
            let size = if size == MoveSize::LOOP {
                MoveSize::of_loop(loop_beats)
            } else {
                size
            };
            return Some(DeckCommand::BeatJump {
                beats: size.beats()? * f64::from(steps),
                stay_in_loop: size.is_fine(),
            });
        }
        MoveMode::Loop => LoopEdge::Both,
        MoveMode::LoopIn => LoopEdge::In,
        MoveMode::LoopOut => LoopEdge::Out,
    };
    Some(DeckCommand::MoveLoop {
        edge,
        beats: size.beats(),
        steps,
    })
}

pub fn is_fine_jump(beats: f64) -> bool {
    MoveSize::FINE
        .beats()
        .is_some_and(|fine| beats.abs() <= fine + 1e-12)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jump(command: Option<DeckCommand>) -> Option<(f64, bool)> {
        match command? {
            DeckCommand::BeatJump {
                beats,
                stay_in_loop,
            } => Some((beats, stay_in_loop)),
            _ => None,
        }
    }

    #[test]
    fn beatjump_uses_the_size_or_the_loop_size() {
        let four = MoveSize::of_loop(4.0);
        assert_eq!(
            jump(command(MoveMode::BeatJump, four, -2, 1.0)),
            Some((-8.0, false))
        );
        assert_eq!(
            jump(command(MoveMode::BeatJump, MoveSize::LOOP, 1, 0.5)),
            Some((0.5, false))
        );
        assert_eq!(
            jump(command(MoveMode::BeatJump, MoveSize::FINE, 1, 4.0)),
            Some((1.0 / 32.0, true))
        );
        assert!(command(MoveMode::BeatJump, four, 0, 4.0).is_none());
    }

    #[test]
    fn loop_modes_move_the_ends() {
        assert!(matches!(
            command(MoveMode::LoopOut, MoveSize::LOOP, -1, 4.0),
            Some(DeckCommand::MoveLoop {
                edge: LoopEdge::Out,
                beats: None,
                steps: -1
            })
        ));
        assert!(matches!(
            command(MoveMode::Loop, MoveSize::FINE, 3, 4.0),
            Some(DeckCommand::MoveLoop {
                edge: LoopEdge::Both,
                beats: Some(_),
                steps: 3
            })
        ));
    }

    #[test]
    fn fine_jumps() {
        assert!(is_fine_jump(-1.0 / 32.0) && is_fine_jump(1.0 / 320.0));
        assert!(!is_fine_jump(1.0 / 16.0));
    }
}
