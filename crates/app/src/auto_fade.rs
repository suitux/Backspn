pub const DEFAULT_TIME_S: f64 = 4.0;
const INSTANT_S: f64 = 0.05;

pub fn step(position: f32, target: f32, time_s: f64, dt_s: f64) -> (f32, bool) {
    if time_s <= INSTANT_S {
        return (target, true);
    }
    let delta = (dt_s / time_s) as f32;
    let (next, arrived) = if position < target {
        (position + delta, position + delta >= target)
    } else {
        (position - delta, position - delta <= target)
    };
    if arrived {
        (target, true)
    } else {
        (next, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crosses_the_whole_travel_in_the_time() {
        assert_eq!(step(0.0, 1.0, 4.0, 1.0), (0.25, false));
        assert_eq!(step(0.9, 1.0, 4.0, 1.0), (1.0, true));
        assert_eq!(step(0.5, 0.0, 2.0, 0.5), (0.25, false));
        assert_eq!(step(0.3, 1.0, 0.0, 0.01), (1.0, true));
    }
}
