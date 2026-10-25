pub const LEVELS_DB: [i32; 8] = [-36, -42, -48, -54, -60, -66, -72, -78];
pub const DEFAULT_LEVEL_DB: i32 = -60;

const STEPS_PER_S: u32 = 150;

pub fn position(samples: &[f32], sample_rate: u32, level_db: i32) -> Option<f64> {
    let threshold = 10f32.powf(level_db as f32 / 20.0);
    let (frames, _) = samples.as_chunks::<2>();
    let hit = frames
        .iter()
        .position(|[l, r]| l.abs().max(r.abs()) >= threshold)?;
    let step = (sample_rate / STEPS_PER_S).max(1) as usize;
    Some((hit - hit % step) as f64 / f64::from(sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn track(silent: usize, level: f32) -> Vec<f32> {
        let mut s = vec![0.0; 2 * silent];
        s.extend([0.0, level, 0.5, 0.5]);
        s
    }

    #[test]
    fn the_cue_lands_where_the_sound_starts_on_a_step() {
        let at = position(&track(1000, 0.01), RATE, -60).unwrap();
        assert!((at - 882.0 / f64::from(RATE)).abs() < 1e-9);
        assert_eq!(position(&track(0, 0.01), RATE, -60), Some(0.0));
    }

    #[test]
    fn quieter_than_the_level_is_silence() {
        let at = position(&track(300, 0.000_5), RATE, -60).unwrap();
        assert!((at - 294.0 / f64::from(RATE)).abs() < 1e-9);
        assert_eq!(position(&vec![0.0; 2000], RATE, -60), None);
    }
}
