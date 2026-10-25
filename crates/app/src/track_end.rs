use backspn_engine::DeckState;

pub const DEFAULT_WARNING_S: f64 = 30.0;
const OVERLAY_TOGGLE_S: f64 = 0.7;

pub fn warning(state: &DeckState, warning_s: f64) -> bool {
    state.loaded && state.playing && state.duration_s - state.position_s < warning_s
}

pub fn overlay_on(since_s: f64) -> bool {
    let ticks = (since_s / OVERLAY_TOGGLE_S).floor();
    ticks >= 2.0 && ticks % 2.0 == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_in_the_last_seconds_while_playing() {
        let state = DeckState {
            loaded: true,
            playing: true,
            duration_s: 300.0,
            position_s: 275.0,
            ..DeckState::default()
        };
        assert!(warning(&state, 30.0));
        assert!(!warning(&state, 20.0));
        assert!(!warning(
            &DeckState {
                playing: false,
                ..state
            },
            30.0
        ));
    }

    #[test]
    fn overlay_blinks_from_the_second_tick() {
        assert!(!overlay_on(0.5) && !overlay_on(1.0));
        assert!(overlay_on(1.5) && !overlay_on(2.2) && overlay_on(2.9));
    }
}
