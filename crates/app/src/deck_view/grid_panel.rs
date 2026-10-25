use backspn_control::Action;
use backspn_engine::{Command, CueKind, DeckState};
use eframe::egui::{Button, DragValue, Ui, vec2};

use crate::grid_edit::GridEdit;
use crate::i18n::tr;
use crate::theme;
use crate::widgets;

use super::{DeckEvent, DeckView, PanelTab};

impl DeckView {
    pub(super) fn advanced_panel(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        tab: PanelTab,
        out: &mut Vec<Command>,
    ) {
        let t = theme::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (label, this) in [
                ("MOVE", PanelTab::Move),
                ("CUE", PanelTab::Cue),
                ("GRID", PanelTab::Grid),
            ] {
                if widgets::toggle(ui, label, tab == this, t.accent).clicked() {
                    self.panel = Some(this);
                }
            }
        });
        match tab {
            PanelTab::Move => self.move_panel(ui, state, out),
            PanelTab::Cue => self.cue_panel(ui, state),
            PanelTab::Grid => self.grid_panel(ui, state),
        }
    }

    fn grid_panel(&mut self, ui: &mut Ui, state: &DeckState) {
        let t = theme::get(ui.ctx());
        let deck = self.index;
        let locked = self.locked();
        let editable = state.loaded && !locked && state.grid.is_some();
        let mut events = Vec::new();
        let grid = |edit| DeckEvent::Grid { deck, edit };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let h = 24.0;
            let lock = ui
                .add_enabled_ui(state.loaded && self.track_id().is_some(), |ui| {
                    widgets::toggle(ui, "LOCK", locked, t.danger)
                })
                .inner;
            if lock
                .on_hover_text(tr("Lock the track's beatgrid and tempo"))
                .clicked()
            {
                events.push(DeckEvent::Lock {
                    deck,
                    locked: !locked,
                });
            }
            let mut bpm = state.grid.map_or(0.0, |g| g.bpm);
            let field = ui.add_enabled(
                editable,
                DragValue::new(&mut bpm)
                    .speed(0.01)
                    .fixed_decimals(3)
                    .range(1.0..=999.0),
            );
            if field.changed() {
                events.push(grid(GridEdit::SetBpm(bpm)));
            }
            for (label, edit, hint) in [
                ("−", GridEdit::Nudge(-1), "Tempo −0.01 BPM"),
                ("+", GridEdit::Nudge(1), "Tempo +0.01 BPM"),
                ("÷2", GridEdit::Half, "Halve the tempo"),
                ("×2", GridEdit::Double, "Double the tempo"),
            ] {
                if ui
                    .add_enabled(editable, Button::new(label).min_size(vec2(28.0, h)))
                    .on_hover_text(hint)
                    .clicked()
                {
                    events.push(grid(edit));
                }
            }
            if ui
                .add_enabled(
                    state.loaded && !locked,
                    Button::new("TAP").min_size(vec2(40.0, h)),
                )
                .on_hover_text(tr(
                    "Tap the beats: every fourth tap sets the tempo and grid",
                ))
                .clicked()
            {
                events.push(DeckEvent::Action(Action::BeatTap { deck }));
            }
            self.grid_tools(ui, state, editable, &mut events);
        });
        if self.confirm_autogrid {
            ui.horizontal(|ui| {
                ui.label(tr("Reset the beatgrid of this track?"));
                if ui.button(tr("Reset")).clicked() {
                    events.push(DeckEvent::Reanalyse { deck, grid: true });
                    self.confirm_autogrid = false;
                }
                if ui.button(tr("Cancel")).clicked() {
                    self.confirm_autogrid = false;
                }
            });
        }
        self.events.extend(events);
    }

    fn grid_tools(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        editable: bool,
        events: &mut Vec<DeckEvent>,
    ) {
        let deck = self.index;
        let h = 24.0;
        let locked = self.locked();
        let grid = |edit| DeckEvent::Grid { deck, edit };
        let analysable = state.loaded && !locked && self.track_id().is_some();
        if ui
            .add_enabled(analysable, Button::new("RESET").min_size(vec2(48.0, h)))
            .on_hover_text(tr("Detect the tempo again; the grid marker stays"))
            .clicked()
        {
            events.push(DeckEvent::Reanalyse { deck, grid: false });
        }
        if ui
            .add_enabled(analysable, Button::new("AUTOGRID").min_size(vec2(64.0, h)))
            .on_hover_text(tr("Detect the tempo and beatgrid again"))
            .clicked()
        {
            self.confirm_autogrid = true;
        }
        if ui
            .add_enabled_ui(state.loaded, |ui| {
                widgets::toggle(ui, "TICK", self.tick, theme::get(ui.ctx()).accent)
            })
            .inner
            .on_hover_text(tr("A click on each beat of the grid, in the headphones"))
            .clicked()
        {
            events.push(DeckEvent::Action(Action::DeckTick { deck }));
        }
        ui.separator();
        for (label, edit, hint) in [
            ("◀", GridEdit::MoveMarker(-1), "Grid marker 10 ms earlier"),
            ("▶", GridEdit::MoveMarker(1), "Grid marker 10 ms later"),
            ("SET", GridEdit::SetMarker, "Grid marker at the playhead"),
        ] {
            if ui
                .add_enabled(editable, Button::new(label).min_size(vec2(28.0, h)))
                .on_hover_text(hint)
                .clicked()
            {
                events.push(grid(edit));
            }
        }
        if ui
            .add_enabled(editable, Button::new("+M").min_size(vec2(28.0, h)))
            .on_hover_text(tr(
                "Add a grid marker here: the tempo may change from marker to marker",
            ))
            .clicked()
        {
            events.push(DeckEvent::Action(Action::CueSetStore { deck }));
            events.push(DeckEvent::Action(Action::CueType {
                deck,
                kind: i32::from(CueKind::Grid),
            }));
        }
    }
}
