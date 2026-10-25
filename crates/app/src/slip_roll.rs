use backspn_engine::DeckCommand;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlipRoll {
    saved_slip: Option<bool>,
}

impl SlipRoll {
    pub fn commands(
        &mut self,
        on: bool,
        beats: f64,
        (slip, loop_active, current): (bool, bool, f64),
    ) -> Vec<DeckCommand> {
        let mut out = Vec::new();
        if on {
            if self.saved_slip.is_none() {
                self.saved_slip = Some(slip);
                if !slip {
                    out.push(DeckCommand::SetSlip(true));
                }
            }
            if !loop_active {
                out.push(DeckCommand::AutoLoop { beats });
            } else if (beats - current).abs() > f64::EPSILON && current > 0.0 {
                out.push(DeckCommand::LoopScale(beats / current));
            }
        } else if let Some(saved) = self.saved_slip.take() {
            if loop_active {
                out.push(DeckCommand::LoopToggle);
            }
            if saved != slip {
                out.push(DeckCommand::SetSlip(saved));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[DeckCommand]) -> Vec<String> {
        v.iter().map(|c| format!("{c:?}")).collect()
    }

    #[test]
    fn a_roll_keeps_slip_and_puts_it_back() {
        let mut r = SlipRoll::default();
        let on = r.commands(true, 0.25, (false, false, 4.0));
        assert_eq!(names(&on), ["SetSlip(true)", "AutoLoop { beats: 0.25 }"]);
        let more = r.commands(true, 0.5, (true, true, 0.25));
        assert_eq!(names(&more), ["LoopScale(2.0)"]);
        let off = r.commands(false, 0.5, (true, true, 0.5));
        assert_eq!(names(&off), ["LoopToggle", "SetSlip(false)"]);
        assert_eq!(r, SlipRoll::default());
    }

    #[test]
    fn slip_already_on_stays_on() {
        let mut r = SlipRoll::default();
        assert_eq!(
            names(&r.commands(true, 1.0, (true, false, 4.0))),
            ["AutoLoop { beats: 1.0 }"]
        );
        assert_eq!(
            names(&r.commands(false, 1.0, (true, true, 1.0))),
            ["LoopToggle"]
        );
        assert!(r.commands(false, 1.0, (true, false, 1.0)).is_empty());
    }
}
