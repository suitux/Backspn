#[derive(Debug, Clone, Copy)]
pub struct Group {
    pub name: &'static str,
    pub knobs: &'static [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Left,
    Right,
    Center,
    Finish,
    Restore,
}

impl Step {
    fn code(self) -> u8 {
        match self {
            Self::Left => 0x11,
            Self::Right => 0x12,
            Self::Center => 0x13,
            Self::Finish => 0x15,
            Self::Restore => 0x16,
        }
    }
}

pub const REPORT: u8 = 0xF4;
pub const REPORT_LEN: usize = 32;
const KNOB: u8 = 1;

pub fn report(step: Step, knob: u8) -> [u8; REPORT_LEN + 1] {
    let mut r = [0u8; REPORT_LEN + 1];
    r[0] = REPORT;
    r[1] = step.code();
    r[2] = knob;
    r[3] = KNOB;
    r
}

pub fn reports(step: Step, group: &Group) -> Vec<[u8; REPORT_LEN + 1]> {
    group.knobs.iter().map(|&k| report(step, k)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_report_per_knob_with_the_step_and_the_index() {
        let r = report(Step::Left, 15);
        assert_eq!(r.len(), 33);
        assert_eq!(&r[..4], &[0xF4, 0x11, 15, 1]);
        assert!(r[4..].iter().all(|&b| b == 0));
        let group = Group {
            name: "test",
            knobs: &[15, 16, 35],
        };
        let all = reports(Step::Restore, &group);
        assert_eq!(all.len(), 3);
        assert_eq!(all.iter().map(|r| r[2]).collect::<Vec<_>>(), [15, 16, 35]);
        assert!(all.iter().all(|r| r[1] == 0x16));
        assert_eq!(report(Step::Right, 0)[1], 0x12);
        assert_eq!(report(Step::Center, 0)[1], 0x13);
        assert_eq!(report(Step::Finish, 0)[1], 0x15);
    }
}
