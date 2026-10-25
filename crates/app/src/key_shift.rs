use std::time::Duration;

use backspn_engine::MAX_KEY_ADJUST;

pub const FINE_STEP: f32 = 0.01;
pub const RESET_HOLD: Duration = Duration::from_millis(350);

pub fn semitone_step(current: f32, up: bool) -> f32 {
    let next = if up {
        (current + 1.05).floor()
    } else {
        (current - 1.05).ceil()
    };
    next.clamp(-MAX_KEY_ADJUST, MAX_KEY_ADJUST)
}

pub fn fine_step(current: f32, steps: f32) -> f32 {
    (current + steps * FINE_STEP).clamp(-MAX_KEY_ADJUST, MAX_KEY_ADJUST)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semitone_steps_go_to_the_next_whole_semitone() {
        assert_eq!(semitone_step(0.0, true), 1.0);
        assert_eq!(semitone_step(0.3, true), 1.0);
        assert_eq!(semitone_step(-0.98, true), 0.0);
        assert_eq!(semitone_step(0.3, false), 0.0);
        assert_eq!(semitone_step(0.0, false), -1.0);
        assert_eq!(semitone_step(2.0, false), 1.0);
        assert_eq!(semitone_step(12.0, true), 12.0);
    }

    #[test]
    fn fine_steps_are_cents() {
        assert!((fine_step(1.0, 3.0) - 1.03).abs() < 1e-6);
        assert_eq!(fine_step(-11.995, -2.0), -12.0);
    }
}
