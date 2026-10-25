use backspn_engine::MAX_AUTO_GAIN_DB;

pub fn auto_gain_db(lufs: f64) -> f32 {
    ((-0.80 * lufs - 6.35) as f32).clamp(-MAX_AUTO_GAIN_DB, MAX_AUTO_GAIN_DB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loud_tracks_go_down_quiet_ones_up() {
        assert!((auto_gain_db(-6.25) - -1.35).abs() < 1e-4);
        assert!((auto_gain_db(-14.0) - 4.85).abs() < 1e-4);
        assert_eq!(auto_gain_db(-40.0), MAX_AUTO_GAIN_DB);
    }
}
