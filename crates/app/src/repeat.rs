use std::time::Duration;

const TICK: Duration = Duration::from_millis(100);
const DELAY_TICKS: u32 = 3;

pub fn repeats_due(held: Duration) -> u32 {
    let ticks = u32::try_from(held.as_millis() / TICK.as_millis()).unwrap_or(u32::MAX);
    ticks.saturating_sub(DELAY_TICKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_controls_repeat_after_three_ticks() {
        let ms = |m| repeats_due(Duration::from_millis(m));
        assert_eq!((ms(0), ms(399), ms(400), ms(650)), (0, 0, 1, 3));
    }
}
