use backspn_control::{Action, SamplePart, Trigger};
use backspn_engine::loop_recorder::LoopCopy;
use backspn_engine::{
    Command, DECKS, DeckCommand, EngineState, SampleDeck, SampleDeckCommand, Track,
};

use crate::App;
use crate::sample_view::{self, DeckKind, LoopDrag, SampleEvent, SampleView};

impl App {
    pub(crate) fn deck_kind(&self, deck: usize) -> DeckKind {
        if self.decks[deck].sample_deck.is_some() {
            DeckKind::SampleDeck
        } else {
            DeckKind::Track
        }
    }

    pub(crate) fn set_deck_kind(&mut self, deck: usize, deck_kind: DeckKind) {
        if deck >= DECKS || self.deck_kind(deck) == deck_kind {
            return;
        }
        match deck_kind {
            DeckKind::SampleDeck => {
                self.outbox.push(Command::Deck(deck, DeckCommand::Pause));
                self.outbox.push(Command::Deck(deck, DeckCommand::Unload));
                let rate = self.output_rate();
                self.outbox.push(Command::SampleDeck(
                    deck,
                    SampleDeckCommand::Install(Box::new(SampleDeck::new(rate))),
                ));
                let mut view = Box::new(SampleView::new(deck));
                view.capture_source =
                    sample_view::default_capture_source(deck, self.capturable_sources());
                view.initial_commands(sample_view::behaviour(&self.settings), &mut self.outbox);
                view.library_sets = self.library_sets();
                self.decks[deck].sample_deck = Some(view);
                self.decks[deck].resend_stem_controls(&mut self.outbox);
            }
            DeckKind::Track => {
                self.autosave_set(deck);
                self.decks[deck].sample_deck = None;
                self.outbox
                    .push(Command::SampleDeck(deck, SampleDeckCommand::Remove));
            }
        }
        self.validate_capture_sources();
    }

    fn capturable_sources(&self) -> [bool; 5] {
        let mut c = [false; 5];
        for (i, deck) in self.decks.iter().enumerate().take(DECKS.min(4)) {
            c[i] = deck.sample_deck.is_none();
        }
        c[4] = true;
        c
    }

    fn validate_capture_sources(&mut self) {
        let capturable = self.capturable_sources();
        for deck in &mut self.decks {
            if let Some(view) = &mut deck.sample_deck
                && view.capture_source < 4
                && !capturable[view.capture_source]
            {
                view.capture_source =
                    sample_view::next_capture_source(view.capture_source, capturable);
            }
        }
    }

    fn library_sets(&self) -> Vec<(i64, String)> {
        self.browser
            .library_sets()
            .into_iter()
            .map(|s| (s.id, s.set.title))
            .collect()
    }

    pub(crate) fn poll_sample_deck(&mut self) {
        let sync = self.engine_state().sync;
        for deck in 0..DECKS {
            let Some(view) = &mut self.decks[deck].sample_deck else {
                continue;
            };
            view.can_lead = self.settings.sample_decks_master;
            view.is_master = sync.master == Some(deck);
            view.is_synced = sync.synced[deck];
            let failures = view.poll(&mut self.outbox);
            let events = view.take_events();
            if let Some(e) = failures.first() {
                log::warn!("cannot load sample: {e}");
                self.decks[deck].notify(crate::deck_informer::Notice::load_error(
                    "The track could not be loaded",
                ));
            }
            for event in &events {
                self.handle_sample_event(deck, event);
            }
        }
    }

    fn handle_sample_event(&mut self, deck: usize, event: &SampleEvent) {
        match event {
            SampleEvent::LoadCell { slot, cell, track } => {
                self.load_cell(deck, *slot, *cell, *track);
            }
            SampleEvent::LoadSelected { slot, cell } => {
                if let Some(track) = self.browser.selected() {
                    self.load_cell(deck, *slot, *cell, track);
                }
            }
            SampleEvent::SaveSet => self.save_sample_set(deck, false),
            SampleEvent::Capture { slot, cell } => self.capture(deck, *slot, *cell),
            SampleEvent::CaptureFrom {
                slot,
                cell,
                deck: source,
            } => self.capture_from(deck, *slot, *cell, *source),
            SampleEvent::CopyFromSlot { slot, source } => {
                self.copy_from_slot(deck, *slot, *source);
            }
            SampleEvent::FromLoop { slot, cell, from } => match from {
                LoopDrag::Recorder => self.capture_loop_recorder(deck, *slot, *cell),
                LoopDrag::Deck(source) => self.capture_from(deck, *slot, *cell, *source),
            },
            SampleEvent::RotateCaptureSource => {
                let capturable = self.capturable_sources();
                if let Some(view) = &mut self.decks[deck].sample_deck {
                    view.capture_source =
                        sample_view::next_capture_source(view.capture_source, capturable);
                }
            }
            SampleEvent::LoadSet(id) => self.load_sample_set(deck, *id),
        }
    }

    pub(crate) fn load_cell(&mut self, deck: usize, slot: usize, cell: usize, track: i64) {
        let Some(id) = self.browser.resolve(track) else {
            return;
        };
        let Some(row) = self.browser.library_track(id) else {
            return;
        };
        if row.sample_type.is_none() && crate::stems::is_stem_file(std::path::Path::new(&row.path))
        {
            self.decks[deck].notify(crate::deck_informer::Notice::load_error(
                "A stem file cannot go into a sample cell",
            ));
            return;
        }
        let auto_gain = row
            .lufs
            .filter(|_| self.settings.sample_auto_gain)
            .map(crate::auto_gain::auto_gain_db);
        let startup = backspn_library::paths::startup_volume();
        if let Some(view) = &mut self.decks[deck].sample_deck {
            view.load_cell(slot, cell, &row, auto_gain, &startup);
        }
    }

    fn autosave_set(&mut self, deck: usize) {
        if self.settings.sample_autosave_sets
            && self.decks[deck]
                .sample_deck
                .as_ref()
                .is_some_and(|r| r.modified && r.has_cells())
        {
            self.save_sample_set(deck, true);
        }
    }

    pub(crate) fn copy_from_slot(&mut self, deck: usize, slot: usize, source: usize) {
        let (from_deck, from_slot) = (source / 4, source % 4);
        if from_deck >= DECKS || slot >= 4 {
            return;
        }
        if (from_deck, from_slot) == (deck, slot) {
            crate::toast::warning("Cannot copy: a slot cannot be copied onto itself");
            return;
        }
        let Some((cell, track)) = self.decks[from_deck].sample_deck.as_ref().and_then(|r| {
            let active = r.set.slots[from_slot].active_cell as usize;
            let cell = r.cell(from_slot, active)?.clone();
            Some((
                cell,
                r.tracks[from_slot * backspn_engine::SAMPLE_CELLS + active]?,
            ))
        }) else {
            return;
        };
        let Some(target) = self.decks[deck]
            .sample_deck
            .as_ref()
            .map(|r| r.set.slots[slot].active_cell as usize)
        else {
            return;
        };
        self.load_cell(deck, slot, target, track);
        if let Some(view) = &mut self.decks[deck].sample_deck {
            view.copy_cell_attributes(slot, target, &cell, &mut self.outbox);
        }
    }

    pub(crate) fn load_sample_set(&mut self, deck: usize, id: i64) {
        if self.decks.get(deck).is_none_or(|d| d.sample_deck.is_none()) {
            return;
        }
        let Some(row) = self.browser.library_set(id) else {
            return;
        };
        self.autosave_set(deck);
        let tracks: Vec<Option<backspn_library::TrackRow>> = row
            .tracks
            .iter()
            .map(|t| t.and_then(|t| self.browser.library_track(t)))
            .collect();
        let behaviour = sample_view::behaviour(&self.settings);
        if let Some(view) = &mut self.decks[deck].sample_deck {
            view.load_set(&row, &tracks, &mut self.outbox, behaviour);
        }
    }

    pub(crate) fn save_sample_set(&mut self, deck: usize, quiet: bool) {
        let state = self.engine_state();
        let Some(sample_deck) = state.sample_deck[deck] else {
            return;
        };
        let Some(view) = &self.decks[deck].sample_deck else {
            return;
        };
        let set = view.set_to_save(&sample_deck);
        let same_title = self.browser.library_set_by_title(&set.title);
        let target = match (view.set_id, same_title) {
            (Some(id), _) => Some(id),
            (None, Some(other)) if !quiet => {
                if let Some(view) = &mut self.decks[deck].sample_deck {
                    view.confirm_overwrite = Some(other);
                }
                return;
            }
            (None, other) => other,
        };
        self.store_sample_set(deck, target);
    }

    pub(crate) fn store_sample_set(&mut self, deck: usize, target: Option<i64>) {
        let state = self.engine_state();
        let Some(sample_deck) = state.sample_deck[deck] else {
            return;
        };
        let Some(view) = &mut self.decks[deck].sample_deck else {
            return;
        };
        let set = view.set_to_save(&sample_deck);
        let tracks = view.tracks.clone();
        match self.browser.save_library_set(target, &set, &tracks) {
            Some(id) => {
                let sets = self.library_sets();
                if let Some(view) = &mut self.decks[deck].sample_deck {
                    view.set_id = Some(id);
                    view.set.title = set.title;
                    view.set.bpm = set.bpm;
                    view.modified = false;
                    view.confirm_overwrite = None;
                    view.library_sets = sets;
                }
            }
            None => crate::toast::error("The sample set could not be saved"),
        }
        let sets = self.library_sets();
        for d in 0..DECKS {
            if d != deck
                && let Some(view) = &mut self.decks[d].sample_deck
            {
                view.library_sets.clone_from(&sets);
            }
        }
    }

    pub(crate) fn capture(&mut self, deck: usize, slot: usize, cell: usize) {
        let Some(source) = self.decks[deck]
            .sample_deck
            .as_ref()
            .map(|r| r.capture_source)
        else {
            return;
        };
        self.capture_from(deck, slot, cell, source);
    }

    pub(crate) fn capture_from(&mut self, deck: usize, slot: usize, cell: usize, source: usize) {
        if self.decks[deck].sample_deck.is_none() {
            return;
        }
        if source >= DECKS {
            self.capture_loop_recorder(deck, slot, cell);
            return;
        }
        if self.decks[source].sample_deck.is_some() {
            self.decks[deck].notify(crate::deck_informer::Notice::copy_error(
                "A sample deck cannot be captured from",
            ));
            return;
        }
        let state = self.engine_state();
        let Some((audio, name)) = self.decks[source].audio() else {
            self.decks[deck].notify(crate::deck_informer::Notice::copy_error(
                "Load a track into the source deck to capture from it",
            ));
            return;
        };
        let loop_beats = self.decks[source].loop_beats();
        let Some((start, end)) = crate::sample_capture::range(&state.decks[source], loop_beats)
        else {
            self.decks[deck].notify(crate::deck_informer::Notice::copy_error(
                "The source deck has no track to capture",
            ));
            return;
        };
        if end - start > crate::sample_capture::MAX_LOOP_S {
            self.decks[deck].notify(crate::deck_informer::Notice::copy_error(
                "The loop is too long for a cell (48 seconds at most)",
            ));
            return;
        }
        let bpm = crate::sample_capture::bpm(&state.decks[source], start);
        let track = crate::sample_capture::cut(&audio, start, end);
        let name = crate::sample_capture::name(&name, backspn_library::clock::LocalTime::now());
        self.store_capture(deck, slot, cell, track, name, bpm);
    }

    pub(crate) fn capture_loop_recorder(&mut self, deck: usize, slot: usize, cell: usize) {
        let lr = self.engine_state().loop_recorder;
        if !lr.has_loop() {
            self.decks[deck].notify(crate::deck_informer::Notice::copy_error(
                "The loop recorder has nothing to copy yet",
            ));
            return;
        }
        self.request_loop_copy(
            lr.frames,
            super::loop_rec::LoopCopyFor::Cell(deck, slot, cell),
        );
    }

    pub(super) fn finish_cell_capture(
        &mut self,
        (deck, slot, cell): (usize, usize, usize),
        copy: &LoopCopy,
    ) {
        if copy.frames.is_empty() {
            self.decks[deck].notify(crate::deck_informer::Notice::copy_error(
                "The loop recorder's loop could not be copied to the cell",
            ));
            return;
        }
        let (track, bpm, name) = super::loop_rec::copied_loop(copy);
        self.store_capture(deck, slot, cell, track, name, bpm);
    }

    fn store_capture(
        &mut self,
        deck: usize,
        slot: usize,
        cell: usize,
        track: Track,
        name: String,
        bpm: f64,
    ) {
        let path = match crate::sample_capture::write_wav(&self.settings.samples_dir, &name, &track)
        {
            Ok(path) => path,
            Err(e) => {
                crate::toast::error(format!("Cannot write the sample: {e}"));
                return;
            }
        };
        let duration = track.duration_s();
        let Some(id) = self.browser.add_sample(&path, bpm, duration, None) else {
            return;
        };
        let Some(row) = self.browser.library_track(id) else {
            return;
        };
        let captured = crate::sample_capture::Captured {
            track: std::sync::Arc::new(track),
            name,
            bpm,
        };
        let startup = backspn_library::paths::startup_volume();
        if let Some(view) = &mut self.decks[deck].sample_deck {
            view.load_captured(slot, cell, &captured, &row, &startup, &mut self.outbox);
        }
    }

    pub(crate) fn sample_action(
        &mut self,
        action: Action,
        trigger: Trigger,
        state: &EngineState,
    ) -> bool {
        match action {
            Action::DeckType { deck } if deck < DECKS => {
                let current = usize::from(self.deck_kind(deck) == DeckKind::SampleDeck);
                let next = match trigger {
                    Trigger::Set(v) => (v.clamp(0.0, 1.0) * 3.0).round() as usize,
                    Trigger::Delta(d) if d != 0.0 => 1 - current,
                    Trigger::Press => 1 - current,
                    _ => return true,
                };
                match next {
                    0 => self.set_deck_kind(deck, DeckKind::Track),
                    1 => self.set_deck_kind(deck, DeckKind::SampleDeck),
                    _ => {}
                }
                true
            }
            Action::SampleSlot { deck, slot, part } if deck < DECKS => {
                let Some(sample_deck) = state.sample_deck[deck] else {
                    return true;
                };
                let view = &mut self.decks[deck];
                if let Some(r) = &mut view.sample_deck {
                    let mut controls = view.stem_controls;
                    r.slot_action(
                        slot,
                        part,
                        trigger,
                        &sample_deck,
                        &mut controls[slot.min(3)],
                        &mut self.outbox,
                    );
                    view.stem_controls = controls;
                }
                true
            }
            Action::SampleDeck {
                deck,
                part: SamplePart::LoadSet,
            } if deck < DECKS => {
                if trigger == Trigger::Press
                    && let Some(set) = self.browser.shown_set()
                {
                    self.set_deck_kind(deck, DeckKind::SampleDeck);
                    self.load_sample_set(deck, set);
                }
                true
            }
            Action::SampleDeck { deck, part } if deck < DECKS => {
                let Some(sample_deck) = state.sample_deck[deck] else {
                    return true;
                };
                let view = &mut self.decks[deck];
                if let Some(r) = &mut view.sample_deck {
                    let mut controls = view.stem_controls;
                    r.deck_action(part, trigger, &sample_deck, &mut controls, &mut self.outbox);
                    view.stem_controls = controls;
                }
                if part == SamplePart::CaptureSource {
                    self.validate_capture_sources();
                }
                true
            }
            Action::SampleCellStopLoad { deck, slot, cell } if deck < DECKS => {
                if trigger == Trigger::Press
                    && let Some(r) = &mut self.decks[deck].sample_deck
                {
                    let cell = r.page * 4 + cell % 4;
                    r.stop_or_load(slot, cell, &mut self.outbox);
                }
                true
            }
            Action::SampleCell { deck, slot, cell } if deck < DECKS => {
                let pressed = match trigger {
                    Trigger::Press => true,
                    Trigger::Release => false,
                    _ => return true,
                };
                if let Some(r) = &mut self.decks[deck].sample_deck {
                    let cell = r.page * 4 + cell % 4;
                    r.cell_action(slot, cell, pressed, &mut self.outbox);
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn send_sample_settings(&mut self) {
        let behaviour = sample_view::behaviour(&self.settings);
        for deck in 0..DECKS {
            if self.decks[deck].sample_deck.is_some() {
                self.outbox.push(Command::SampleDeck(
                    deck,
                    SampleDeckCommand::Deck(backspn_engine::SampleCommand::SetBehaviour(behaviour)),
                ));
            }
        }
    }

    pub(crate) fn sample_save_dialogs(&mut self, ctx: &eframe::egui::Context) {
        for deck in 0..DECKS {
            let Some(other) = self.decks[deck]
                .sample_deck
                .as_ref()
                .and_then(|r| r.confirm_overwrite)
            else {
                continue;
            };
            let mut answer: Option<Option<Option<i64>>> = None;
            eframe::egui::Window::new("Sample Set Name")
                .collapsible(false)
                .resizable(false)
                .anchor(eframe::egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Your collection already has a sample set with this name.");
                    ui.label("Overwrite it, save this one under the name below, or cancel.");
                    if let Some(view) = &mut self.decks[deck].sample_deck {
                        ui.text_edit_singleline(&mut view.set.title);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Overwrite").clicked() {
                            answer = Some(Some(Some(other)));
                        }
                        if ui.button("Save as new").clicked() {
                            answer = Some(Some(None));
                        }
                        if ui.button("Cancel").clicked() {
                            answer = Some(None);
                        }
                    });
                });
            match answer {
                Some(Some(target)) => self.store_sample_set(deck, target),
                Some(None) => {
                    if let Some(view) = &mut self.decks[deck].sample_deck {
                        view.confirm_overwrite = None;
                    }
                }
                None => {}
            }
        }
    }
}
