use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub seconds: u32,
}

impl LocalTime {
    pub fn now() -> Self {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self::from_unix(secs)
    }

    #[allow(unsafe_code)]
    pub fn from_unix(secs: u64) -> Self {
        let t = libc::time_t::try_from(secs).unwrap_or(0);
        let tm = unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            #[cfg(windows)]
            libc::localtime_s(&raw mut tm, &raw const t);
            #[cfg(not(windows))]
            libc::localtime_r(&raw const t, &raw mut tm);
            tm
        };
        Self {
            year: tm.tm_year + 1900,
            month: u32::try_from(tm.tm_mon + 1).unwrap_or(1),
            day: u32::try_from(tm.tm_mday).unwrap_or(1),
            seconds: u32::try_from(tm.tm_hour * 3600 + tm.tm_min * 60 + tm.tm_sec).unwrap_or(0),
        }
    }

    pub fn date_text(self) -> String {
        format!("{}/{}/{}", self.year, self.month, self.day)
    }

    pub fn packed_date(self) -> u32 {
        (u32::try_from(self.year).unwrap_or(0) << 16) | (self.month << 8) | self.day
    }

    pub fn history_name(self) -> String {
        format!(
            "history_{}y{:02}m{:02}d_{:02}h{:02}m{:02}s",
            self.year,
            self.month,
            self.day,
            self.seconds / 3600,
            self.seconds / 60 % 60,
            self.seconds % 60
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_names_right() {
        let t = LocalTime {
            year: 2025,
            month: 3,
            day: 8,
            seconds: 21 * 3600 + 34 * 60 + 7,
        };
        assert_eq!(t.packed_date(), 132_711_176);
        assert_eq!(t.date_text(), "2025/3/8");
        assert_eq!(t.history_name(), "history_2025y03m08d_21h34m07s");
    }

    #[test]
    fn now_is_a_plausible_local_time() {
        let t = LocalTime::now();
        assert!(t.year >= 2024 && (1..=12).contains(&t.month) && t.seconds < 86_400);
    }
}
