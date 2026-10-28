//! AI control: answers the requests that arrive on the local socket and
//! drives the autopilot that plays a planned set.

use std::collections::HashSet;

use backspn_control::{Action, Trigger};
use backspn_engine::{Command, DECKS, DeckCommand, DeckState, EffectKind, EngineState, FxMode};
use backspn_library::set_plan::{self, SessionBrief, SetPlan, Source, Start, Transition};
use backspn_library::{NodeKind, TrackRow};
use backspn_remote::protocol::{self, MixerControl, Request, SetTrack, TrackInfo};
use backspn_remote::server::{Call, Server};
use backspn_remote::transition::{self, Band, PatternSpec};
use eframe::egui;
use serde_json::{Value, json};

use crate::App;
use crate::autopilot::{self, AutoTrack, Autopilot, DeckView, Pause, Status, Step, Target};

const SEARCH_LIMIT: usize = 50;
/// How often the autopilot looks at the decks while a set plays.
const TICK: std::time::Duration = std::time::Duration::from_millis(30);
/// Loop length that holds a track whose successor is late.
const HOLD_BEATS: f64 = 4.0;

#[derive(Debug, Default)]
pub(crate) struct Remote {
    server: Option<Server>,
    pub(crate) error: Option<String>,
    pilot: Option<Autopilot>,
    brief: SessionBrief,
    plan: Option<SetPlan>,
    /// When the set started, in app seconds.
    started_at: f64,
    /// Settings asked for, applied on the next preferences pass.
    asked_settings: Option<crate::prefs::Settings>,
}

impl Remote {
    /// The set playing: its position and whether it is paused.
    pub(crate) fn progress(&self) -> Option<(usize, usize, bool)> {
        self.pilot
            .as_ref()
            .map(|p| (p.cursor() + 1, p.tracks().len(), !p.running()))
    }

    pub(crate) fn stop(&mut self) {
        self.pilot = None;
    }

    pub(super) fn ask_settings(&mut self, settings: crate::prefs::Settings) {
        self.asked_settings = Some(settings);
    }

    pub(crate) fn take_settings(&mut self) -> Option<crate::prefs::Settings> {
        self.asked_settings.take()
    }

    /// A control moved by hand pauses the set if it drives one of its decks.
    pub(crate) fn hand_moved(&mut self, action: Action) {
        if let Some(pilot) = &mut self.pilot
            && autopilot::takes_over(action)
        {
            pilot.pause(Pause::Manual);
        }
    }
}

fn today() -> i64 {
    let now = backspn_library::clock::LocalTime::now();
    backspn_library::smart::days_from_civil(i64::from(now.year), now.month, now.day)
}

fn effect_name(kind: EffectKind) -> Option<String> {
    match serde_json::to_value(kind) {
        Ok(Value::String(name)) if kind != EffectKind::None => Some(name),
        _ => None,
    }
}

fn effect_kind(name: &str) -> Result<EffectKind, String> {
    serde_json::from_value(Value::String(name.trim().to_owned()))
        .map_err(|_| format!("unknown effect \"{name}\"; see library_overview for the list"))
}

/// Builds the autopilot's tracks, checking ids and transitions.
fn auto_tracks(tracks: &[SetTrack], known: &dyn Fn(i64) -> bool) -> Result<Vec<AutoTrack>, String> {
    let missing: Vec<i64> = tracks
        .iter()
        .map(|t| t.id)
        .filter(|id| !known(*id))
        .collect();
    if !missing.is_empty() {
        return Err(format!("tracks not in the library: {missing:?}"));
    }
    tracks
        .iter()
        .map(|t| {
            let recipe = t
                .recipe
                .clone()
                .or_else(|| t.transition.map(transition::preset));
            if let Some(recipe) = &recipe {
                let problems = recipe.check(backspn_engine::FX_UNITS);
                if !problems.is_empty() {
                    return Err(format!("transition into {}: {}", t.id, problems.join("; ")));
                }
                for action in recipe.events.iter().map(|e| &e.action) {
                    if let transition::Action::FxEffect { effect, .. } = action {
                        effect_kind(effect)?;
                    }
                }
            }
            Ok(AutoTrack {
                id: t.id,
                play_s: t.play_minutes.filter(|m| *m > 0.0).map(|m| m * 60.0),
                start: t.start_at_pct.filter(|p| *p > 0.0).map(|p| p / 100.0),
                transition: recipe,
            })
        })
        .collect()
}

fn status_name(status: Status) -> Value {
    match status {
        Status::Starting => json!({"state": "starting"}),
        Status::Playing => json!({"state": "playing"}),
        Status::Mixing { progress } => json!({"state": "mixing", "progress": progress}),
        Status::Holding => json!({"state": "waiting for the next track, looping"}),
        Status::Paused(reason) => json!({"state": "paused", "reason": match reason {
            Pause::DeckChanged => "a deck of the set was loaded or stopped by hand",
            Pause::Manual => "a control of the set was moved by hand",
            Pause::Stopped => "the playing track stopped",
        }}),
        Status::Finished => json!({"state": "finished"}),
    }
}

impl App {
    fn now_s(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// Starts or stops listening as the setting asks, answers waiting
    /// requests and moves the set on.
    pub(crate) fn drive_remote(&mut self, ctx: &egui::Context, state: &EngineState) {
        self.listen(ctx);
        let calls: Vec<Call> = self
            .remote
            .server
            .as_ref()
            .map(|s| std::iter::from_fn(|| s.next_call()).collect())
            .unwrap_or_default();
        for call in calls {
            let result = self.answer(call.request.clone(), state);
            call.answer(result);
        }
        self.step_pilot(state);
        if self.remote.pilot.as_ref().is_some_and(Autopilot::running) {
            ctx.request_repaint_after(TICK);
        }
    }

    fn listen(&mut self, ctx: &egui::Context) {
        let remote = &mut self.remote;
        match (self.settings.remote_control, remote.server.is_some()) {
            (true, false) if remote.error.is_none() => {
                let ctx = ctx.clone();
                match Server::start(&crate::dirs::data(), move || ctx.request_repaint()) {
                    Ok(server) => remote.server = Some(server),
                    Err(e) => remote.error = Some(e.to_string()),
                }
            }
            (false, true) => {
                remote.server = None;
                remote.pilot = None;
            }
            (false, false) => remote.error = None,
            _ => {}
        }
    }

    fn deck_views(&self, state: &EngineState) -> [DeckView; DECKS] {
        std::array::from_fn(|d| {
            let s = &state.decks[d];
            DeckView {
                track: self.decks[d].track_id(),
                loaded: s.loaded,
                playing: s.playing,
                position_s: s.position_s,
                duration_s: s.duration_s,
                beat_s: self.beat_s(d, s),
                tempo: s.tempo,
                synced: state.sync.synced[d],
                keylock: s.keylock,
                master: state.sync.master == Some(d),
            }
        })
    }

    /// Track seconds per beat on a deck: from its grid, or else from the
    /// tempo the library knows for the loaded track.
    fn beat_s(&self, deck: usize, s: &DeckState) -> Option<f64> {
        s.grid
            .map(|g| g.period_at(s.position_s))
            .filter(|p| p.is_finite() && *p > 0.0)
            .or_else(|| {
                let bpm = self.library_bpm(deck)?;
                Some(60.0 / bpm)
            })
    }

    /// The tempo a deck plays at.
    fn deck_bpm(&self, deck: usize, s: &DeckState) -> Option<f64> {
        self.beat_s(deck, s).map(|b| 60.0 / b * s.tempo)
    }

    fn library_bpm(&self, deck: usize) -> Option<f64> {
        let id = self.decks[deck].track_id()?;
        self.browser
            .row(id)?
            .bpm
            .filter(|b| b.is_finite() && *b > 0.0)
    }

    fn step_pilot(&mut self, state: &EngineState) {
        let views = self.deck_views(state);
        let now = self.now_s();
        let Some(pilot) = &mut self.remote.pilot else {
            return;
        };
        let steps = pilot.step(&views, now);
        for step in steps {
            self.apply_step(step, state);
        }
        self.flush();
    }

    fn press(&mut self, action: Action, state: &EngineState) {
        self.execute(action, Trigger::Press, state);
    }

    fn set(&mut self, action: Action, value: f32, state: &EngineState) {
        self.execute(action, Trigger::Set(value), state);
    }

    fn apply_step(&mut self, step: Step, state: &EngineState) {
        let playing = |d: usize| state.decks[d].playing;
        match step {
            Step::Load { deck, track } => self.load_from_library(deck, track),
            Step::Seek { deck, seconds } => self
                .outbox
                .push(Command::Deck(deck, DeckCommand::Seek { seconds })),
            Step::Play { deck } if !playing(deck) => self.press(Action::PlayPause { deck }, state),
            Step::Stop { deck } if playing(deck) => self.press(Action::PlayPause { deck }, state),
            Step::Sync { deck, on } if state.sync.synced[deck] != on => {
                self.press(Action::Sync { deck }, state);
            }
            Step::Master { deck } if state.sync.master != Some(deck) => {
                self.press(Action::Master { deck }, state);
            }
            Step::Keylock { deck, on } if state.decks[deck].keylock != on => {
                self.press(Action::Keylock { deck }, state);
            }
            Step::Tempo { deck, ratio } => {
                let fader = self.decks[deck].tempo_fader_for(ratio);
                self.set(Action::Tempo { deck }, fader, state);
            }
            Step::Hold { deck } => self.press(
                Action::LoopSize {
                    deck,
                    beats: HOLD_BEATS,
                },
                state,
            ),
            Step::Set { target, value } => self.set_target(target, value, state),
            Step::FxEffect { unit, effect } => {
                if let Ok(kind) = effect_kind(&effect) {
                    self.single_effect(unit, kind);
                }
            }
            Step::FxAssign { unit, deck, on } => self.set(
                Action::FxAssign {
                    channel: deck,
                    unit,
                },
                f32::from(u8::from(on)),
                state,
            ),
            Step::FxOn { unit, on } => {
                self.set(Action::FxOn { unit }, f32::from(u8::from(on)), state);
            }
            Step::Loop { deck, beats } => self.press(Action::LoopSize { deck, beats }, state),
            Step::LoopOff { deck } if state.decks[deck].loop_active => {
                self.press(Action::LoopActive { deck }, state);
            }
            Step::Pattern { pattern, deck } => {
                // Checked when the set started.
                let decks = deck.map(|d| [d]);
                let _ =
                    self.set_pattern(&pattern, decks.as_ref().map(<[usize; 1]>::as_slice), state);
            }
            Step::BeatJump { deck, beats } => {
                let action = Action::BeatJump { deck, beats };
                self.execute(action, Trigger::Press, state);
                self.execute(action, Trigger::Release, state);
            }
            _ => {}
        }
    }

    fn set_target(&mut self, target: Target, value: f32, state: &EngineState) {
        let band = |b: Band| match b {
            Band::Low => 0,
            Band::Mid => 1,
            Band::High => 2,
        };
        let (action, value) = match target {
            Target::Volume(channel) => (Action::Volume { channel }, value),
            Target::Eq(channel, b) => (
                Action::Eq {
                    channel,
                    band: band(b),
                },
                value,
            ),
            Target::Filter(channel) => (Action::Filter { channel }, f32::midpoint(value, 1.0)),
            Target::FxWet(unit) => (Action::FxDryWet { unit }, value),
            Target::FxKnob(unit, knob) => (Action::FxKnob { unit, knob }, value),
        };
        self.set(action, value.clamp(0.0, 1.0), state);
    }

    /// Puts one effect alone in a unit.
    fn single_effect(&mut self, unit: usize, kind: EffectKind) {
        self.fx.set_mode(unit, FxMode::Single, &mut self.outbox);
        let mut effects = self.fx.effects();
        effects[unit][0] = kind;
        self.fx.set_effects(&effects, &mut self.outbox);
    }

    /// Routes an effect unit to `decks` and sets its amount and switch.
    fn fx_unit(
        &mut self,
        unit: usize,
        decks: Option<&[usize]>,
        wet: Option<f32>,
        on: Option<bool>,
        state: &EngineState,
    ) {
        if let Some(decks) = decks {
            for d in 0..self.settings.deck_count.min(DECKS) {
                let assign = f32::from(u8::from(decks.contains(&d)));
                self.set(Action::FxAssign { channel: d, unit }, assign, state);
            }
        }
        if let Some(wet) = wet {
            self.set(Action::FxDryWet { unit }, wet.clamp(0.0, 1.0), state);
        }
        if let Some(on) = on {
            self.set(Action::FxOn { unit }, f32::from(u8::from(on)), state);
        }
    }

    /// Sets up a drum pattern unit as asked, playing through `decks`.
    fn set_pattern(
        &mut self,
        spec: &PatternSpec,
        decks: Option<&[usize]>,
        state: &EngineState,
    ) -> Result<(), String> {
        let problems = spec.problems(backspn_engine::FX_UNITS);
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        let u = spec.unit;
        let mut out = std::mem::take(&mut self.outbox);
        let result = (|| {
            self.fx.drum_unit(u, spec.kit.as_deref(), &mut out)?;
            if let Some(n) = spec.pattern {
                self.fx.drum_pattern(u, n - 1, &mut out);
            }
            for (sound, steps) in &spec.steps {
                self.fx.drum_steps(u, sound, steps, &mut out)?;
            }
            if spec.only_these {
                let keep: Vec<String> = spec.steps.keys().cloned().collect();
                self.fx.drum_only(u, &keep, &mut out);
            }
            if let Some(v) = spec.decay {
                self.fx.drum_knob(u, 0, v, &mut out);
            }
            if let Some(v) = spec.pitch {
                self.fx.drum_knob(u, 1, v, &mut out);
            }
            if let Some(on) = spec.duck {
                self.fx.drum_duck(u, on, &mut out);
            }
            if let Some(on) = spec.roll {
                self.fx.drum_roll(u, on, &mut out);
            }
            Ok::<(), String>(())
        })();
        out.append(&mut self.outbox);
        self.outbox = out;
        result?;
        self.fx_unit(u, decks, spec.volume, spec.on, state);
        Ok(())
    }

    fn rows_from(&self, source: Option<&Source>) -> Result<Vec<TrackRow>, String> {
        let rows = self.browser.rows();
        let Some(source) = source else {
            return Ok(rows.to_vec());
        };
        let (kind, name) = match source {
            Source::Playlist(name) => (NodeKind::Playlist, name),
            Source::Smartlist(name) => (NodeKind::Smartlist, name),
        };
        let ids: HashSet<i64> = self
            .browser
            .named_list_tracks(kind, name)
            .ok_or_else(|| format!("no list named \"{name}\""))?
            .into_iter()
            .collect();
        Ok(rows
            .iter()
            .filter(|t| ids.contains(&t.id))
            .cloned()
            .collect())
    }

    pub(super) fn answer(
        &mut self,
        request: Request,
        state: &EngineState,
    ) -> Result<Value, String> {
        match request {
            Request::LibraryOverview => {
                let mut overview = protocol::overview(self.browser.rows());
                overview.playlists = self.browser.list_names(NodeKind::Playlist);
                overview.smartlists = self.browser.list_names(NodeKind::Smartlist);
                overview.effects = EffectKind::ALL
                    .into_iter()
                    .filter_map(effect_name)
                    .collect();
                overview.kits = self
                    .fx
                    .kit_list()
                    .into_iter()
                    .map(|(name, sounds)| protocol::KitInfo { name, sounds })
                    .collect();
                serde_json::to_value(&overview).map_err(|e| e.to_string())
            }
            Request::SearchTracks { filters, limit } => {
                let rows = self.rows_from(filters.source.as_ref())?;
                let found = set_plan::matching(&filters, &rows, today());
                let tracks: Vec<TrackInfo> = found
                    .iter()
                    .take(limit.unwrap_or(SEARCH_LIMIT))
                    .map(|t| TrackInfo::from(*t))
                    .collect();
                Ok(json!({"total": found.len(), "tracks": tracks}))
            }
            Request::PlanSet { brief } => {
                let rows = self.rows_from(brief.source.as_ref())?;
                let plan = set_plan::plan(
                    &brief,
                    &rows,
                    self.browser.played(),
                    today(),
                    Start::default(),
                );
                let answer = plan_json(&plan);
                self.remote.brief = brief;
                self.remote.plan = Some(plan);
                Ok(answer)
            }
            Request::StartSet {
                tracks,
                transition,
                keylock,
            } => self.start_set(&tracks, transition, keylock),
            Request::GetSetStatus => Ok(self.set_status(state)),
            Request::AdjustSet { brief } => self.adjust_set(&brief, state),
            Request::ReplaceUpcoming { tracks } => {
                let browser = &self.browser;
                let tracks = auto_tracks(&tracks, &|id| id > 0 && browser.row(id).is_some())?;
                let pilot = self.remote.pilot.as_mut().ok_or("no set is playing")?;
                pilot.replace_upcoming(tracks);
                Ok(self.set_status(state))
            }
            Request::StopSet => {
                self.remote.pilot = None;
                Ok(json!({"stopped": true}))
            }
            Request::ResumeSet => {
                let pilot = self.remote.pilot.as_mut().ok_or("no set is playing")?;
                pilot.resume();
                Ok(self.set_status(state))
            }
            Request::GetState => Ok(self.state_json(state)),
            Request::LoadTrack { .. }
            | Request::Play { .. }
            | Request::Sync { .. }
            | Request::SetMixer { .. }
            | Request::Crossfader { .. }
            | Request::SetPattern { .. }
            | Request::SetFx { .. } => self.manual(request, state),
            other => self.answer_app(other, state),
        }
    }

    fn start_set(
        &mut self,
        tracks: &[SetTrack],
        transition: Option<Transition>,
        keylock: Option<bool>,
    ) -> Result<Value, String> {
        if tracks.is_empty() {
            return Err("the set has no tracks".to_owned());
        }
        let browser = &self.browser;
        let auto = auto_tracks(tracks, &|id| id > 0 && browser.row(id).is_some())?;
        let default = transition::preset(transition.unwrap_or(self.remote.brief.transition));
        let keylock = keylock.or(self.remote.brief.keylock).unwrap_or(true);
        self.remote.pilot = Some(Autopilot::new(auto, default, keylock));
        self.remote.started_at = self.now_s();
        Ok(json!({"started": true, "tracks": tracks.len()}))
    }

    fn adjust_set(&mut self, patch: &Value, state: &EngineState) -> Result<Value, String> {
        let brief = protocol::patch_brief(&self.remote.brief, patch)?;
        let pilot = self.remote.pilot.as_ref().ok_or("no set is playing")?;
        let cursor = pilot.cursor();
        let set: Vec<i64> = pilot.tracks().iter().map(|t| t.id).collect();
        let current = set.get(cursor).copied();
        let mut played: HashSet<i64> = self.browser.played().clone();
        played.extend(&set[..=cursor.min(set.len().saturating_sub(1))]);
        let wanted = brief.duration_min.unwrap_or(60.0);
        let elapsed = (self.now_s() - self.remote.started_at) / 60.0;
        let rows = self.rows_from(brief.source.as_ref())?;
        let plan = set_plan::plan(
            &brief,
            &rows,
            &played,
            today(),
            Start {
                after: current,
                from_pct: (elapsed / wanted).clamp(0.0, 0.95),
            },
        );
        let upcoming: Vec<SetTrack> = plan
            .tracks
            .iter()
            .map(|t| SetTrack {
                id: t.id,
                play_minutes: brief.play_minutes,
                start_at_pct: None,
                transition: None,
                recipe: None,
            })
            .collect();
        let browser = &self.browser;
        let auto = auto_tracks(&upcoming, &|id| id > 0 && browser.row(id).is_some())?;
        if let Some(pilot) = &mut self.remote.pilot {
            pilot.replace_upcoming(auto);
        }
        let answer = json!({"plan": plan_json(&plan), "status": self.set_status(state)});
        self.remote.brief = brief;
        self.remote.plan = Some(plan);
        Ok(answer)
    }

    fn set_status(&self, state: &EngineState) -> Value {
        let Some(pilot) = &self.remote.pilot else {
            return json!({"state": "no set"});
        };
        let now = self.now_s();
        let cursor = pilot.cursor();
        let info = |id: i64| self.browser.row(id).map(TrackInfo::from);
        let tracks: Vec<Value> = pilot
            .tracks()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let target = self
                    .remote
                    .plan
                    .as_ref()
                    .and_then(|p| p.tracks.iter().find(|p| p.id == t.id));
                json!({
                    "index": i,
                    "track": info(t.id),
                    "played": i < cursor,
                    "playing": i == cursor,
                    "target_energy": target.and_then(|p| p.target_energy),
                    "target_bpm": target.and_then(|p| p.target_bpm),
                })
            })
            .collect();
        let playing_deck = (0..DECKS).find(|&d| {
            state.decks[d].playing
                && self.decks[d].track_id() == pilot.tracks().get(cursor).map(|t| t.id)
        });
        let mut status = status_name(pilot.status(now));
        status["elapsed_min"] =
            json!(((now - self.remote.started_at) / 60.0 * 10.0).round() / 10.0);
        status["position"] = json!(format!("{}/{}", cursor + 1, pilot.tracks().len()));
        if let Some(d) = playing_deck {
            let s = &state.decks[d];
            status["deck"] = json!(d);
            status["bpm_now"] = json!(self.deck_bpm(d, s).map(|b| (b * 10.0).round() / 10.0));
            status["track_left_s"] =
                json!(((s.duration_s - s.position_s) / s.tempo.max(0.01)).round());
        }
        status["tracks"] = Value::Array(tracks);
        status
    }

    fn state_json(&self, state: &EngineState) -> Value {
        let decks: Vec<Value> = (0..self.settings.deck_count.min(DECKS))
            .map(|d| {
                let s = &state.decks[d];
                let channel = &state.mixer.channels[d];
                json!({
                    "deck": d,
                    "track": self.decks[d].track_id().and_then(|id| self.browser.row(id)).map(TrackInfo::from),
                    "playing": s.playing,
                    "position_s": (s.position_s * 10.0).round() / 10.0,
                    "duration_s": s.duration_s.round(),
                    "bpm": self.deck_bpm(d, s).map(|b| (b * 10.0).round() / 10.0),
                    "grid": s.grid.is_some(),
                    "tempo": s.tempo,
                    "synced": state.sync.synced[d],
                    "master": state.sync.master == Some(d),
                    "keylock": s.keylock,
                    "loop": s.loop_active,
                    "volume": channel.fader,
                    "eq": {"low": channel.eq[0], "mid": channel.eq[1], "high": channel.eq[2]},
                    "filter": channel.filter,
                })
            })
            .collect();
        json!({
            "decks": decks,
            "crossfader": state.mixer.crossfader,
            "set": self.remote.pilot.as_ref().map(|p| status_name(p.status(self.now_s()))),
        })
    }

    fn manual(&mut self, request: Request, state: &EngineState) -> Result<Value, String> {
        let deck_ok = |d: usize| {
            (d < self.settings.deck_count.min(DECKS))
                .then_some(d)
                .ok_or(format!("deck {d} does not exist"))
        };
        let act = |app: &mut Self, action: Action, trigger: Trigger| {
            app.remote.hand_moved(action);
            app.execute(action, trigger, state);
        };
        match request {
            Request::LoadTrack { deck, id } => {
                let deck = deck_ok(deck)?;
                if self.browser.row(id).is_none() {
                    return Err(format!("track {id} is not in the library"));
                }
                self.remote.hand_moved(Action::LoadSelected { deck });
                self.load_from_library(deck, id);
            }
            Request::Play { deck, on } => {
                let deck = deck_ok(deck)?;
                if state.decks[deck].playing != on {
                    act(self, Action::PlayPause { deck }, Trigger::Press);
                }
            }
            Request::Sync { deck, on } => {
                let deck = deck_ok(deck)?;
                if state.sync.synced[deck] != on {
                    act(self, Action::Sync { deck }, Trigger::Press);
                }
            }
            Request::SetMixer {
                deck,
                control,
                value,
            } => {
                let channel = deck_ok(deck)?;
                let (action, value) = match control {
                    MixerControl::Volume => (Action::Volume { channel }, value),
                    MixerControl::Gain => (Action::Gain { channel }, value),
                    MixerControl::EqLow => (Action::Eq { channel, band: 0 }, value),
                    MixerControl::EqMid => (Action::Eq { channel, band: 1 }, value),
                    MixerControl::EqHigh => (Action::Eq { channel, band: 2 }, value),
                    MixerControl::Filter => (Action::Filter { channel }, f32::midpoint(value, 1.0)),
                };
                act(self, action, Trigger::Set(value.clamp(0.0, 1.0)));
            }
            Request::Crossfader { value } => {
                act(
                    self,
                    Action::Crossfader,
                    Trigger::Set(value.clamp(0.0, 1.0)),
                );
            }
            Request::SetPattern { pattern, decks } => {
                if let Some(decks) = &decks {
                    for &d in decks {
                        deck_ok(d)?;
                    }
                }
                self.set_pattern(&pattern, decks.as_deref(), state)?;
            }
            Request::SetFx {
                unit,
                effect,
                on,
                wet,
                decks,
            } => {
                if unit >= backspn_engine::FX_UNITS {
                    return Err(format!("effect unit {unit} does not exist"));
                }
                if let Some(effect) = effect {
                    self.single_effect(unit, effect_kind(&effect)?);
                }
                self.fx_unit(unit, decks.as_deref(), wet, on, state);
            }
            _ => return Err("not a manual request".to_owned()),
        }
        self.flush();
        Ok(json!({"done": true}))
    }
}

fn plan_json(plan: &SetPlan) -> Value {
    json!({
        "tracks": plan.tracks,
        "warnings": plan.warnings.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "candidates": plan.candidates,
        "total_min": (plan.total_min * 10.0).round() / 10.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_track(id: i64) -> SetTrack {
        SetTrack {
            id,
            play_minutes: None,
            start_at_pct: None,
            transition: None,
            recipe: None,
        }
    }

    #[test]
    fn set_tracks_need_known_ids_and_playable_transitions() {
        let known = |id: i64| id < 10;
        assert!(
            auto_tracks(&[set_track(1), set_track(12)], &known)
                .unwrap_err()
                .contains("12")
        );
        let mut styled = set_track(2);
        styled.transition = Some(Transition {
            style: backspn_library::set_plan::TransitionStyle::EchoOut,
            bars: 8,
        });
        styled.play_minutes = Some(3.0);
        let tracks = auto_tracks(&[set_track(1), styled], &known).unwrap();
        assert_eq!(tracks[0].transition, None);
        assert_eq!(tracks[1].play_s, Some(180.0));
        assert!(tracks[1].transition.as_ref().unwrap().beats > 0.0);
        let mut bad = set_track(3);
        bad.recipe = Some(
            serde_json::from_value(json!({"beats": 8, "events": [
                {"beat": 0, "do": "fx_effect", "unit": 0, "effect": "Kazoo"}]}))
            .unwrap(),
        );
        assert!(auto_tracks(&[bad], &known).unwrap_err().contains("Kazoo"));
    }

    #[test]
    fn effects_go_by_their_names() {
        assert_eq!(effect_kind("Echo"), Ok(EffectKind::Echo));
        assert_eq!(
            effect_name(EffectKind::VinylBrake).as_deref(),
            Some("VinylBrake")
        );
        assert_eq!(effect_name(EffectKind::None), None);
        for preset in ["Echo", "VinylBrake", "BackSpin"] {
            assert!(effect_kind(preset).is_ok(), "{preset}");
        }
    }
}
