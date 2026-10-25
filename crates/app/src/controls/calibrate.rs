use backspn_control::calibration::Step;
use eframe::egui::{RichText, Ui};

use super::Controls;
use crate::i18n::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    pub group: usize,
    pub position: Position,
}

impl Calibration {
    pub fn start(group: usize) -> Self {
        Self {
            group,
            position: Position::Left,
        }
    }

    pub fn ok(self) -> (Vec<Step>, Option<Self>) {
        let next = |position| Some(Self { position, ..self });
        match self.position {
            Position::Left => (vec![Step::Left], next(Position::Right)),
            Position::Right => (vec![Step::Right], next(Position::Center)),
            Position::Center => (vec![Step::Center, Step::Finish], None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
    NotConnected,
}

impl Controls {
    fn calibratable(&self) -> Option<(&'static str, Vec<&'static str>)> {
        self.devices
            .iter()
            .find(|d| d.connected() && !d.calibration_groups().is_empty())
            .map(|d| {
                let names = d.calibration_groups().iter().map(|g| g.name).collect();
                (d.name(), names)
            })
    }

    pub fn can_calibrate(&self) -> bool {
        self.calibratable().is_some()
    }

    fn send_calibration(&self, group: usize, steps: &[Step]) -> bool {
        let Some(device) = self
            .devices
            .iter()
            .find(|d| d.connected() && !d.calibration_groups().is_empty())
        else {
            return false;
        };
        steps.iter().all(|&s| device.calibrate(group, s))
    }

    pub fn cancel_calibration(&mut self) {
        if let Some(c) = self.calibration.take() {
            self.send_calibration(c.group, &[Step::Restore]);
            self.calibration_outcome = Some(Outcome::Cancelled);
        }
    }

    pub fn calibration_section(&mut self, ui: &mut Ui) {
        let Some((device, groups)) = self.calibratable() else {
            if self.calibration.take().is_some() {
                self.calibration_outcome = Some(Outcome::NotConnected);
            }
            ui.label(RichText::new(tr("Plug in the controller to calibrate its knobs.")).weak());
            return;
        };
        match self.calibration {
            None => {
                ui.label(tr(
                    "If a knob does not reach its ends or its centre, calibrate its group.",
                ));
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in groups.iter().enumerate() {
                        if ui
                            .button(format!("{} {}", tr("Calibrate"), tr(name)))
                            .clicked()
                        {
                            self.calibration = Some(Calibration::start(i));
                            self.calibration_outcome = None;
                        }
                    }
                });
            }
            Some(c) => {
                let group = groups.get(c.group).copied().unwrap_or_default();
                let place = match c.position {
                    Position::Left => tr("to the full left"),
                    Position::Right => tr("to the full right"),
                    Position::Center => tr("to the exact centre"),
                };
                ui.label(
                    RichText::new(format!(
                        "{} {} {} {} {}",
                        tr("Turn every one of the"),
                        tr(group),
                        tr("on the"),
                        device,
                        place
                    ))
                    .strong(),
                );
                ui.label(RichText::new(tr("then press OK.")).weak());
                ui.horizontal(|ui| {
                    if ui.button(tr("OK")).clicked() {
                        let (steps, next) = c.ok();
                        if self.send_calibration(c.group, &steps) {
                            self.calibration = next;
                            if next.is_none() {
                                self.calibration_outcome = Some(Outcome::Done);
                            }
                        } else {
                            self.calibration = None;
                            self.calibration_outcome = Some(Outcome::NotConnected);
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        self.cancel_calibration();
                    }
                });
            }
        }
        if let Some(outcome) = &self.calibration_outcome {
            let text = match outcome {
                Outcome::Done => tr("Calibrated. If a knob still reads wrong, calibrate it again."),
                Outcome::Cancelled => tr("Cancelled: the knobs keep their values before."),
                Outcome::NotConnected => tr("The controller is not connected: nothing was sent."),
            };
            ui.label(RichText::new(text).weak());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_right_then_centre_which_keeps_the_values() {
        let c = Calibration::start(1);
        let (steps, c) = c.ok();
        assert_eq!(steps, [Step::Left]);
        let c = c.unwrap();
        assert_eq!((c.group, c.position), (1, Position::Right));
        let (steps, c) = c.ok();
        assert_eq!(steps, [Step::Right]);
        let (steps, c) = c.unwrap().ok();
        assert_eq!(steps, [Step::Center, Step::Finish]);
        assert!(c.is_none(), "done after the centre");
    }
}
