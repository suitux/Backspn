#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeylockRange {
    Normal,
    #[default]
    Scratch,
}

pub(crate) fn heard_pitch(range: KeylockRange, speed: f64, key_scale: f64) -> Option<f64> {
    let s = speed.abs();
    let divisor = match range {
        KeylockRange::Scratch if (0.7..=1.5).contains(&s) => s,
        KeylockRange::Scratch if (0.4..=0.7).contains(&s) => 1.4 - s,
        KeylockRange::Scratch if (1.5..=2.0).contains(&s) => 3.0 - s,
        KeylockRange::Normal if (2.0..=3.0).contains(&s) => 4.0 - s,
        KeylockRange::Normal if s < 2.0 => s,
        _ => return None,
    };
    Some((key_scale / divisor).clamp(0.1, 10.0) * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn scratch_range_holds_the_key_then_fades_out() {
        let r = KeylockRange::Scratch;
        assert!(near(heard_pitch(r, 1.2, 1.0), 1.0));
        assert!(near(heard_pitch(r, 0.7, 2.0), 2.0));
        assert!(near(heard_pitch(r, 0.55, 1.0), 0.55 / 0.85));
        assert!(near(heard_pitch(r, 0.4, 1.0), 0.4));
        assert!(near(heard_pitch(r, 2.0, 1.0), 2.0));
        assert_eq!(heard_pitch(r, 0.3, 1.0), None);
        assert_eq!(heard_pitch(r, 2.1, 1.0), None);
    }

    #[test]
    fn normal_range_reaches_twice_the_speed() {
        let r = KeylockRange::Normal;
        assert!(near(heard_pitch(r, 0.2, 1.0), 1.0));
        assert!(near(heard_pitch(r, 1.9, 1.0), 1.0));
        assert!(near(heard_pitch(r, 2.5, 1.0), 2.5 / 1.5));
        assert_eq!(heard_pitch(r, 3.1, 1.0), None);
    }
}
