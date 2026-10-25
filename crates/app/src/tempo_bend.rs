use std::time::Duration;

use crate::repeat;

const FULL_BEND: f64 = 1.0 / 64.0;
const PROGRESSIVE_STEP: f64 = 0.05;

pub fn amount(sensitivity: f64, progressive: bool, held: Duration) -> f64 {
    let full = sensitivity / 100.0 * FULL_BEND;
    if progressive {
        full * PROGRESSIVE_STEP * f64::from(1 + repeat::repeats_due(held))
    } else {
        full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bend_from_sensitivity_and_progression() {
        let ms = Duration::from_millis;
        assert!((amount(100.0, false, ms(0)) - 0.015_625).abs() < 1e-12);
        assert!((amount(50.0, false, ms(5_000)) - 0.007_812_5).abs() < 1e-12);
        let step = 0.015_625 * 0.05;
        assert!((amount(100.0, true, ms(0)) - step).abs() < 1e-12);
        assert!((amount(100.0, true, ms(450)) - 2.0 * step).abs() < 1e-12);
        assert!((amount(100.0, true, ms(2_350)) - 21.0 * step).abs() < 1e-12);
    }
}
