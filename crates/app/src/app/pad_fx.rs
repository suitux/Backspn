use backspn_control::{Action, Trigger};
use backspn_engine::pad_fx::{self, LAYERS, LayerOp, PadFxPads, PadFxType};
use backspn_engine::pad_host::{self, PadCommand};
use backspn_engine::{Command, DeckCommand, FxCommand};

use crate::App;

#[derive(Debug, Default)]
pub(crate) struct PadFxDeck {
    pads: PadFxPads,
    loaded: [Option<PadFxType>; LAYERS],
    slip: Option<SlipLoop>,
}

#[derive(Debug, Clone, Copy)]
struct SlipLoop {
    layer: usize,
    beats: f64,
    slip_before: bool,
}

impl PadFxDeck {
    pub fn lit(&self) -> [u16; 2] {
        std::array::from_fn(|bank| {
            (0..16u8)
                .filter(|&pad| self.pads.lit((bank as u8, pad)))
                .fold(0, |bits, pad| bits | 1 << pad)
        })
    }

    fn slip_loop(&mut self, op: LayerOp, slip: bool) -> Option<Vec<DeckCommand>> {
        match op {
            LayerOp::Start(layer, preset) if preset.kind == PadFxType::SlipLoop => {
                self.slip = Some(SlipLoop {
                    layer,
                    beats: preset.beat,
                    slip_before: slip,
                });
                Some(vec![
                    DeckCommand::SetSlip(true),
                    DeckCommand::AutoLoop { beats: preset.beat },
                ])
            }
            LayerOp::Update(layer, preset) => {
                let slip = self.slip.as_mut().filter(|s| s.layer == layer)?;
                slip.beats = preset.beat;
                Some(vec![DeckCommand::AutoLoop { beats: preset.beat }])
            }
            LayerOp::Stop(layer) => {
                let slip = self.slip.filter(|s| s.layer == layer)?;
                self.slip = None;
                Some(vec![
                    DeckCommand::AutoLoop { beats: slip.beats },
                    DeckCommand::SetSlip(slip.slip_before),
                ])
            }
            _ => None,
        }
    }

    fn command(&mut self, op: LayerOp, rate: u32) -> Option<PadCommand> {
        Some(match op {
            LayerOp::Start(layer, preset) => {
                let effect = if self.loaded[layer] == Some(preset.kind) {
                    None
                } else {
                    let effect = pad_host::effect(&preset, rate)?;
                    self.loaded[layer] = Some(preset.kind);
                    Some(effect)
                };
                PadCommand::Start {
                    layer,
                    effect,
                    params: pad_host::params(&preset),
                    quantized: preset.kind.quantized(),
                    tail: preset.kind.tail(),
                }
            }
            LayerOp::Update(layer, preset) => PadCommand::Update {
                layer,
                params: pad_host::params(&preset),
            },
            LayerOp::Stop(layer) => PadCommand::Stop { layer },
            LayerOp::StartRelease(preset) => PadCommand::StartRelease {
                kind: pad_host::release_kind(preset.kind)?,
                params: pad_host::params(&preset),
            },
            LayerOp::StopRelease => PadCommand::StopRelease,
        })
    }
}

impl App {
    pub(super) fn pad_fx_action(&mut self, action: Action, trigger: Trigger) {
        let pressed = trigger == Trigger::Press;
        match action {
            Action::PadFx { deck, bank, pad }
                if matches!(trigger, Trigger::Press | Trigger::Release) =>
            {
                self.pad_fx(deck, bank, pad, pressed);
            }
            Action::PadFxBeat { deck, direction } if pressed => {
                self.pad_fx_beat(deck, direction > 0.0);
            }
            _ => {}
        }
    }

    pub(super) fn pad_fx(&mut self, deck: usize, bank: usize, pad: usize, pressed: bool) {
        let Some(preset) = pad_fx::factory(bank, pad) else {
            return;
        };
        if preset.kind != PadFxType::SlipLoop && !pad_host::plays(preset.kind) {
            return;
        }
        let Some(d) = self.pad_fx.get_mut(deck) else {
            return;
        };
        let id = (bank as u8, pad as u8);
        let mut ops = Vec::new();
        if pressed {
            d.pads.press(id, preset, |op| ops.push(op));
        } else {
            d.pads.release(id, |op| ops.push(op));
        }
        self.send_pad_ops(deck, ops);
    }

    pub(super) fn pad_fx_beat(&mut self, deck: usize, longer: bool) {
        let Some(d) = self.pad_fx.get_mut(deck) else {
            return;
        };
        let mut ops = Vec::new();
        d.pads.shift_beat(longer, |op| ops.push(op));
        self.send_pad_ops(deck, ops);
    }

    fn send_pad_ops(&mut self, deck: usize, ops: Vec<LayerOp>) {
        let Some(rate) = self.engine.as_ref().ok().map(|e| e.info().sample_rate) else {
            return;
        };
        let slip = self.engine_state().decks[deck].slip;
        for op in ops {
            if let Some(commands) = self.pad_fx[deck].slip_loop(op, slip) {
                self.outbox
                    .extend(commands.into_iter().map(|c| Command::Deck(deck, c)));
            } else if let Some(c) = self.pad_fx[deck].command(op, rate) {
                self.outbox.push(Command::Fx(FxCommand::Pad(deck, c)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slip_loop_loops_with_slip_and_lets_go_back() {
        let mut d = PadFxDeck::default();
        let quarter = pad_fx::factory(1, 2).unwrap();
        let half = pad_fx::factory(1, 3).unwrap();
        let mut ops = Vec::new();
        d.pads.press((1, 2), quarter, |o| ops.push(o));
        d.pads.press((1, 3), half, |o| ops.push(o));
        let start = d.slip_loop(ops[0], false).unwrap();
        assert!(
            matches!(start[..], [DeckCommand::SetSlip(true), DeckCommand::AutoLoop { beats }] if (beats - 0.25).abs() < 1e-9)
        );
        let size = d.slip_loop(ops[1], false).unwrap();
        assert!(
            matches!(size[..], [DeckCommand::AutoLoop { beats }] if (beats - 0.5).abs() < 1e-9)
        );
        let mut rest = Vec::new();
        d.pads.release((1, 3), |o| rest.push(o));
        d.pads.release((1, 2), |o| rest.push(o));
        let stop = d.slip_loop(*rest.last().unwrap(), false).unwrap();
        assert!(matches!(
            stop[..],
            [DeckCommand::AutoLoop { .. }, DeckCommand::SetSlip(false)]
        ));
        let echo = pad_fx::factory(0, 5).unwrap();
        assert!(d.slip_loop(LayerOp::Start(1, echo), false).is_none());
    }

    #[test]
    fn a_held_pad_lights_and_reuses_its_loaded_layer() {
        let mut d = PadFxDeck::default();
        let echo = pad_fx::factory(0, 5).unwrap();
        let mut ops = Vec::new();
        d.pads.press((0, 5), echo, |o| ops.push(o));
        assert_eq!(d.lit(), [1 << 5, 0]);
        let first = d.command(ops[0], 48_000);
        assert!(matches!(
            first,
            Some(PadCommand::Start {
                layer: 0,
                effect: Some(_),
                quantized: true,
                ..
            })
        ));
        d.pads.release((0, 5), |_| {});
        ops.clear();
        d.pads.press((0, 5), echo, |o| ops.push(o));
        assert!(matches!(
            d.command(ops[0], 48_000),
            Some(PadCommand::Start {
                effect: None,
                quantized: true,
                tail: true,
                ..
            })
        ));
    }
}
