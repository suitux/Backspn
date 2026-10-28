//! The tools an AI can call, with the descriptions and input schemas it
//! reads. Every client (the MCP server, the in-app assistant) uses this one
//! list; a tool's name is the `type` of its [`Request`](crate::protocol::Request).

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// How a session goes, for the client's system prompt or server notes.
pub const INSTRUCTIONS: &str = "\
Backspn is a DJ app. You plan a set and the app plays it on its own, mixing \
track to track; you only step in when the listener asks for a change.

1. library_overview: see genres, tempo and years in the library, and the \
effects a transition can use.
2. plan_set: turn the listener's words into a brief (duration, tempo and \
energy curve, harmony, genres, mood, era, rating, freshness, tracks to \
include, transition style). Read the warnings; relax the brief or search \
with search_tracks if too few tracks fit.
3. Review the list and edit it if needed (order, swaps, a different \
transition into a given track: a named style or a recipe written beat by \
beat with faders, EQ, filter, effects, loops and fader cuts).
4. start_set with the track ids. The app plays the whole set.
5. Later: get_set_status to see where the set is; adjust_set to re-plan \
what is left from a new brief (\"more energy\", \"no vocals\"); \
replace_upcoming to set the next tracks yourself; stop_set to stop.

The manual tools (load_track, play, sync, set_mixer, crossfader, set_fx, \
set_pattern) act at once; using the deck and mixer ones on the set's decks \
pauses the set.

The pattern player is a drum machine in an effect unit: pick a kit from \
library_overview and write steps per sound over one bar (16, 32 or 64 \
cells of x and .). Use set_pattern to layer drums over the set (it keeps \
playing until changed), or a \"pattern\" event in a transition recipe for \
drums during a mix (it stops when the mix ends).

Beyond sets, you can run the whole app: search_library, get_track, \
edit_tracks and analyze_tracks for tracks; list_lists, get_list, \
create_list, edit_list, delete_list and show_list for playlists, \
smartlists and folders; list_history and get_history for past sessions; \
list_controls and control for any control a controller can map; \
get_settings and set_settings for Preferences. Deleting a list never \
deletes tracks or files.

Energy is estimated 1–10 from loudness, tempo and drive (or written by the \
user in the comment). Keys are in Camelot notation.";

fn brief_schema() -> Value {
    let number = |description: &str| json!({"type": "number", "description": description});
    let words = |description: &str| json!({"type": "array", "items": {"type": "string"}, "description": description});
    json!({
        "type": "object",
        "description": "What the set should sound like. Every field is optional.",
        "properties": {
            "duration_min": number("Length of the set in minutes (default 60)."),
            "track_count": {"type": "integer", "description": "Number of tracks, instead of a duration."},
            "play_minutes": number("Minutes each track plays before the next mixes in; leave out to play whole tracks."),
            "bpm_start": number("Tempo at the start."),
            "bpm_peak": number("Tempo at the peak."),
            "bpm_end": number("Tempo at the end."),
            "bpm_min": number("Lowest tempo allowed."),
            "bpm_max": number("Highest tempo allowed."),
            "max_tempo_shift_pct": number("Largest tempo change between two tracks, in percent (default 6)."),
            "keylock": {"type": "boolean", "description": "Keep the pitch when the tempo changes (default true)."},
            "energy_start": number("Energy 1–10 at the start."),
            "energy_peak": number("Energy 1–10 at the peak."),
            "energy_end": number("Energy 1–10 at the end."),
            "curve": {
                "type": "string",
                "enum": ["linear", "warmup", "build", "peak_time", "wave", "plateau", "cooldown"],
                "description": "Shape of the energy and tempo over the set."
            },
            "peak_at_pct": number("Where the peak falls, in percent of the set (peak_time curve)."),
            "harmonic": {
                "type": "string",
                "enum": ["strict", "relaxed", "off"],
                "description": "strict: same, relative or neighbouring key; relaxed: also a semitone away (default); off: ignore keys."
            },
            "start_key": {"type": "string", "description": "Key of the first track, e.g. \"8A\" or \"Am\"."},
            "genres_include": words("Genres to draw from (matched as words in the genre)."),
            "genres_exclude": words("Genres to leave out."),
            "mood": {"type": "string", "description": "Free words matched against genre, comments, title and album (\"dark\", \"vocal\", \"acid\")."},
            "artists_include": words("Only these artists."),
            "artists_exclude": words("Never these artists."),
            "labels_include": words("Only these labels."),
            "labels_exclude": words("Never these labels."),
            "release_year_from": {"type": "integer"},
            "release_year_to": {"type": "integer"},
            "source": {
                "type": "object",
                "description": "Draw only from a playlist or smartlist, by name: {\"playlist\": \"Friday\"}.",
                "properties": {"playlist": {"type": "string"}, "smartlist": {"type": "string"}}
            },
            "min_rating": {"type": "integer", "minimum": 0, "maximum": 5, "description": "Lowest star rating."},
            "familiarity": {
                "type": "string",
                "enum": ["mix", "fresh", "classics"],
                "description": "fresh prefers tracks played little; classics, tracks played a lot."
            },
            "skip_played_since_days": {"type": "integer", "description": "Leave out tracks played in the last N days."},
            "no_artist_repeat_within": {"type": "integer", "description": "Tracks before an artist can repeat (default 4)."},
            "must_play": {"type": "array", "items": {"type": "integer"}, "description": "Track ids that must be in the set."},
            "opener": {"type": "integer", "description": "Track id to open with."},
            "closer": {"type": "integer", "description": "Track id to close with."},
            "transition": transition_schema()
        }
    })
}

fn transition_schema() -> Value {
    json!({
        "type": "object",
        "description": "A named transition style.",
        "properties": {
            "style": {
                "type": "string",
                "enum": ["blend", "cut", "fade", "filter_sweep", "chop", "echo_out", "loop_out", "brake", "backspin"],
                "description": "blend: long mix with the bass swapped halfway; cut: swap on a beat; fade: even crossfade; filter_sweep: the old track filters away as the new one opens; chop: fader cuts on the beat; echo_out: the old track echoes away and the new one drops; loop_out: the old track loops while fading; brake / backspin: the old track stops with a brake or spin and the new one drops."
            },
            "bars": {"type": "integer", "description": "Length of blend, fade, filter_sweep, chop and loop_out, in bars (default 16)."}
        }
    })
}

fn recipe_schema() -> Value {
    let side = json!({"type": "string", "enum": ["in", "out"]});
    json!({
        "type": "object",
        "description": "A transition written beat by beat from the start of the mix. \"in\" is the incoming deck, \"out\" the outgoing one. The incoming track starts on beat 0 unless an event plays it later. At the end the outgoing deck stops and every control goes back to neutral.",
        "required": ["beats"],
        "properties": {
            "beats": {"type": "number", "description": "Length of the mix in beats (4 beats = 1 bar)."},
            "lanes": {
                "type": "array",
                "description": "Controls moving over time. Values: volume, EQ and FX 0–1 (EQ 0.5 flat, 0 kills the band); filter -1 low-pass … 0 off … 1 high-pass. Shape \"step\" jumps on each point (fader cuts), \"linear\" glides.",
                "items": {
                    "type": "object",
                    "required": ["control", "points"],
                    "properties": {
                        "control": {"type": "string", "enum": ["volume", "eq", "filter", "fx_wet", "fx_knob"]},
                        "side": side,
                        "band": {"type": "string", "enum": ["low", "mid", "high"]},
                        "unit": {"type": "integer", "description": "Effect unit 0–3."},
                        "knob": {"type": "integer", "description": "Effect knob 0–2."},
                        "shape": {"type": "string", "enum": ["linear", "step"]},
                        "points": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["beat", "value"],
                                "properties": {"beat": {"type": "number"}, "value": {"type": "number"}}
                            }
                        }
                    }
                }
            },
            "events": {
                "type": "array",
                "description": "One-off actions on a beat.",
                "items": {
                    "type": "object",
                    "required": ["beat", "do"],
                    "properties": {
                        "beat": {"type": "number"},
                        "do": {"type": "string", "enum": ["play", "stop", "fx_effect", "fx_assign", "fx_on", "loop", "loop_off", "beat_jump", "pattern"]},
                        "side": side,
                        "unit": {"type": "integer"},
                        "effect": {"type": "string", "description": "Effect name from library_overview."},
                        "on": {"type": "boolean"},
                        "beats": {"type": "number", "description": "Loop length or jump, in beats."},
                        "kit": {"type": "string", "description": "pattern: kit name."},
                        "pattern": {"type": "integer", "description": "pattern: stored pattern 1–8."},
                        "steps": pattern_steps_schema(),
                        "only_these": {"type": "boolean", "description": "pattern: silence the sounds not in steps."},
                        "volume": {"type": "number"},
                        "decay": {"type": "number"},
                        "pitch": {"type": "number"},
                        "duck": {"type": "boolean"},
                        "roll": {"type": "boolean"}
                    }
                }
            }
        }
    })
}

fn pattern_steps_schema() -> Value {
    json!({
        "type": "object",
        "description": "Steps per sound name over one bar: 16, 32 or 64 cells of x (hit) and . (rest), e.g. {\"Kick\": \"x...x...x...x...\"}.",
        "additionalProperties": {"type": "string"}
    })
}

fn set_tracks_schema() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": {"type": "integer"},
                "play_minutes": {"type": "number", "description": "Minutes to play before the next mixes in."},
                "start_at_pct": {"type": "number", "minimum": 0, "maximum": 95, "description": "Where the track starts, in percent of its length (50 is the middle); leave out to start at its cue."},
                "transition": transition_schema(),
                "recipe": recipe_schema()
            }
        }
    })
}

fn deck() -> Value {
    json!({"type": "integer", "minimum": 0, "maximum": 3, "description": "Deck 0–3 (A–D)."})
}

fn object(properties: Value, required: &[&str]) -> Value {
    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), "object".into());
    schema.insert("properties".into(), properties);
    schema.insert("required".into(), json!(required));
    Value::Object(schema)
}

fn empty() -> Value {
    json!({"type": "object", "properties": {}})
}

pub fn tools() -> Vec<Tool> {
    let mut tools = set_tools();
    tools.extend(manual_tools());
    tools.extend(library_tools());
    tools.extend(list_tools());
    tools.extend(app_tools());
    tools
}

/// How to write a library search or a smartlist rule.
pub const QUERY_SYNTAX: &str = "\
Plain words search every text field. A rule is $FIELD OP VALUE, joined \
with & (and) and | (or). Fields: ALL, ARTIST, TITLE, GENRE, LABEL, \
COMMENT, COMMENT2, REMIXER, PRODUCER, MIX, RELEASE, CATALOG_NO, KEY, \
KEYTEXT, BPM, RATING (stars), PLAYCOUNT, TIME (m:ss), COLOR, BITRATE, \
IMPORTDATE, RELEASEDATE, LASTPLAYEDDATE, PLAYED, ANALYZED, \
ANALYSISLOCKED, FILENAME, FILEPATH, ID. Ops: == <> > >= < <=, % \
(contains), !% (does not contain), ~ (near: BPM within 2.5, KEY \
harmonic), STARTS_WITH, ENDS_WITH. Quote text with spaces. Dates: \
2025-03-08, DAYS_AGO(14), WEEKS_AGO(2), MONTHS_AGO(1), YEARS_AGO(1). \
Booleans: TRUE, FALSE. Example: $GENRE % \"Techno\" & $BPM ~ 128 | \
$RATING >= 4";

fn ids() -> Value {
    json!({"type": "array", "items": {"type": "integer"}})
}

/// Tools for library tracks: search, details, edits, analysis.
fn library_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "search_library",
            description: "Searches the library like the browser's search field: plain words or rules. Returns tracks with tempo, key and energy.",
            input_schema: object(
                json!({
                    "query": {"type": "string", "description": QUERY_SYNTAX},
                    "limit": {"type": "integer", "description": "Most tracks to return (default 50)."}
                }),
                &["query"],
            ),
        },
        Tool {
            name: "get_track",
            description: "Everything the library knows about a track: tags, tempo, key, grid, loudness, energy, plays, dates, file and cue points.",
            input_schema: object(json!({"id": {"type": "integer"}}), &["id"]),
        },
        Tool {
            name: "edit_tracks",
            description: "Changes tags, key, rating, play count or track number of tracks; the new tags are written to the files as the user's tag settings say.",
            input_schema: object(
                json!({
                    "ids": ids(),
                    "changes": {
                        "type": "object",
                        "properties": {
                            "title": {"type": "string"}, "artist": {"type": "string"},
                            "album": {"type": "string"}, "genre": {"type": "string"},
                            "label": {"type": "string"}, "comment": {"type": "string"},
                            "comment2": {"type": "string"}, "remixer": {"type": "string"},
                            "producer": {"type": "string"}, "mix": {"type": "string"},
                            "catalog_no": {"type": "string"}, "release_date": {"type": "string"},
                            "key": {"type": "string", "description": "\"8A\", \"Am\"…; empty clears it."},
                            "rating": {"type": "integer", "minimum": 0, "maximum": 5},
                            "play_count": {"type": "integer"},
                            "track_number": {"type": "integer"}
                        }
                    }
                }),
                &["ids", "changes"],
            ),
        },
        Tool {
            name: "analyze_tracks",
            description: "Queues tracks for analysis (tempo, beat grid, key, loudness).",
            input_schema: object(json!({"ids": ids()}), &["ids"]),
        },
    ]
}

/// Tools for the playlist tree and the play history.
fn list_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_lists",
            description: "The playlist tree: folders, playlists and smartlists with ids, parents, rules and track counts.",
            input_schema: empty(),
        },
        Tool {
            name: "get_list",
            description: "The tracks of a playlist or smartlist, in order.",
            input_schema: object(
                json!({"id": {"type": "integer"}, "limit": {"type": "integer"}}),
                &["id"],
            ),
        },
        Tool {
            name: "create_list",
            description: "Creates a playlist (optionally with tracks), a smartlist (with a rule) or a folder, at the top level or inside a folder.",
            input_schema: object(
                json!({
                    "kind": {"type": "string", "enum": ["playlist", "smartlist", "folder"]},
                    "name": {"type": "string"},
                    "parent": {"type": "integer", "description": "Folder id."},
                    "query": {"type": "string", "description": format!("Smartlist rule. {QUERY_SYNTAX}")},
                    "tracks": ids()
                }),
                &["kind", "name"],
            ),
        },
        Tool {
            name: "edit_list",
            description: "Renames a list, moves it into a folder or to the top, changes a smartlist's rule, or adds and removes tracks of a playlist.",
            input_schema: object(
                json!({
                    "id": {"type": "integer"},
                    "name": {"type": "string"},
                    "parent": {"type": "integer"},
                    "to_top": {"type": "boolean"},
                    "query": {"type": "string"},
                    "add": ids(),
                    "remove": ids()
                }),
                &["id"],
            ),
        },
        Tool {
            name: "delete_list",
            description: "Deletes a playlist, smartlist or folder (with what it holds). The tracks stay in the library and on disk.",
            input_schema: object(json!({"id": {"type": "integer"}}), &["id"]),
        },
        Tool {
            name: "show_list",
            description: "Shows a playlist or smartlist in the browser.",
            input_schema: object(json!({"id": {"type": "integer"}}), &["id"]),
        },
        Tool {
            name: "list_history",
            description: "Past sessions in the play history, with dates and track counts.",
            input_schema: empty(),
        },
        Tool {
            name: "get_history",
            description: "The tracks played in a history session, in order.",
            input_schema: object(json!({"id": {"type": "integer"}}), &["id"]),
        },
    ]
}

/// Tools for every control and every setting of the app.
fn app_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_controls",
            description: "Every control a controller can map: decks, transport, cues, loops, beat jumps, tempo, key, sync, mixer, effects, pattern player, sample decks, stems, loop recorder, recording, clock and browser. Each comes with its label and the action to pass to control. Filter by words in the label.",
            input_schema: object(
                json!({"filter": {"type": "string", "description": "Words to find in the labels, e.g. \"deck b loop\"."}}),
                &[],
            ),
        },
        Tool {
            name: "control",
            description: "Moves a control as a controller would: buttons are tapped (or pressed/released), knobs and faders take a value 0–1, encoders a delta. Pass an action exactly as list_controls gives it; change its deck, channel or unit numbers as needed. Moving the set's decks or mixer pauses a playing set.",
            input_schema: object(
                json!({
                    "action": {"description": "An action from list_controls, e.g. {\"action\": \"LoopSize\", \"deck\": 0, \"beats\": 4.0}."},
                    "value": {"type": "number", "description": "Knob or fader position, 0–1."},
                    "delta": {"type": "number", "description": "Encoder steps."},
                    "press": {"type": "string", "enum": ["tap", "press", "release"]}
                }),
                &["action"],
            ),
        },
        Tool {
            name: "get_settings",
            description: "The app's settings (Preferences). Without fields, every setting with plain values shown and nested ones summarised; with fields, those settings whole.",
            input_schema: object(
                json!({"fields": {"type": "array", "items": {"type": "string"}}}),
                &[],
            ),
        },
        Tool {
            name: "set_settings",
            description: "Changes settings as Preferences would; names come from get_settings and nested objects can be changed field by field. Applied on the app's next frame; audio device changes restart the audio.",
            input_schema: object(json!({"changes": {"type": "object"}}), &["changes"]),
        },
    ]
}

/// Tools that plan and run a set.
fn set_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "library_overview",
            description: "What the library holds: track count, tempo and year range, the main genres, playlists, smartlists, the effects a transition can use and the pattern player's kits with their sounds. Call this first.",
            input_schema: empty(),
        },
        Tool {
            name: "search_tracks",
            description: "Tracks that pass a brief's filters (genres, tempo, years, rating, mood words…), with tempo, key and energy.",
            input_schema: object(
                json!({"filters": brief_schema(), "limit": {"type": "integer", "description": "Most tracks to return (default 50)."}}),
                &[],
            ),
        },
        Tool {
            name: "plan_set",
            description: "Plans a set from a brief: picks and orders tracks so tempo and energy follow the curve, keys mix and artists do not repeat. Returns the tracks with start times and the reason for each, plus warnings. Nothing plays yet.",
            input_schema: object(json!({"brief": brief_schema()}), &["brief"]),
        },
        Tool {
            name: "start_set",
            description: "Starts playing a set: the app loads, syncs and mixes every track on its own. Pass the ids from plan_set, edited as you like.",
            input_schema: object(
                json!({
                    "tracks": set_tracks_schema(),
                    "transition": transition_schema(),
                    "keylock": {"type": "boolean"}
                }),
                &["tracks"],
            ),
        },
        Tool {
            name: "get_set_status",
            description: "Where the set is: the track playing and the next ones, elapsed time, current energy and tempo against the plan, and whether it is mixing, paused or done.",
            input_schema: empty(),
        },
        Tool {
            name: "adjust_set",
            description: "Plans again what is left of the set, keeping the track playing. Pass only the brief fields that change; the rest stay as planned. Use it for \"more energy\", \"darker\", \"no more vocals\"…",
            input_schema: object(json!({"brief": brief_schema()}), &["brief"]),
        },
        Tool {
            name: "replace_upcoming",
            description: "Replaces the tracks after the one playing with these.",
            input_schema: object(json!({"tracks": set_tracks_schema()}), &["tracks"]),
        },
        Tool {
            name: "stop_set",
            description: "Stops the set; the music keeps playing, the app just stops mixing.",
            input_schema: empty(),
        },
        Tool {
            name: "resume_set",
            description: "Resumes a set that paused because someone took over a deck.",
            input_schema: empty(),
        },
    ]
}

/// Tools that act on the decks and mixer at once.
fn manual_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "get_state",
            description: "The decks and mixer right now: tracks loaded, playing, position, tempo, sync, faders and EQ.",
            input_schema: empty(),
        },
        Tool {
            name: "load_track",
            description: "Loads a library track on a deck.",
            input_schema: object(
                json!({"deck": deck(), "id": {"type": "integer"}}),
                &["deck", "id"],
            ),
        },
        Tool {
            name: "play",
            description: "Starts or stops a deck.",
            input_schema: object(
                json!({"deck": deck(), "on": {"type": "boolean"}}),
                &["deck", "on"],
            ),
        },
        Tool {
            name: "sync",
            description: "Turns tempo sync on or off for a deck.",
            input_schema: object(
                json!({"deck": deck(), "on": {"type": "boolean"}}),
                &["deck", "on"],
            ),
        },
        Tool {
            name: "set_mixer",
            description: "Sets a mixer control of a deck's channel. Volume, gain and EQ 0–1 (EQ 0.5 flat); filter -1 low-pass … 0 off … 1 high-pass.",
            input_schema: object(
                json!({
                    "deck": deck(),
                    "control": {"type": "string", "enum": ["volume", "gain", "eq_low", "eq_mid", "eq_high", "filter"]},
                    "value": {"type": "number"}
                }),
                &["deck", "control", "value"],
            ),
        },
        Tool {
            name: "crossfader",
            description: "Moves the crossfader, 0 left … 1 right.",
            input_schema: object(json!({"value": {"type": "number"}}), &["value"]),
        },
        Tool {
            name: "set_pattern",
            description: "Sets up the pattern player (a drum machine in an effect unit): kit, stored pattern or steps per sound, volume, decay, pitch, duck, roll, on or off, and the decks it plays through. It keeps playing until changed.",
            input_schema: object(
                json!({
                    "unit": {"type": "integer", "minimum": 0, "maximum": 3},
                    "kit": {"type": "string", "description": "Kit name from library_overview."},
                    "pattern": {"type": "integer", "minimum": 1, "maximum": 8, "description": "One of the kit's stored patterns."},
                    "steps": pattern_steps_schema(),
                    "only_these": {"type": "boolean", "description": "Silence the sounds not in steps."},
                    "on": {"type": "boolean"},
                    "volume": {"type": "number", "description": "0–1."},
                    "decay": {"type": "number", "description": "0–1."},
                    "pitch": {"type": "number", "description": "0–1, 0.5 is the sound's own pitch."},
                    "duck": {"type": "boolean", "description": "Duck the track under the hits."},
                    "roll": {"type": "boolean", "description": "Repeat the current step while on."},
                    "decks": {"type": "array", "items": {"type": "integer"}}
                }),
                &["unit"],
            ),
        },
        Tool {
            name: "set_fx",
            description: "Sets an effect unit: the effect, on or off, the dry/wet amount and the decks it applies to.",
            input_schema: object(
                json!({
                    "unit": {"type": "integer", "minimum": 0, "maximum": 3},
                    "effect": {"type": "string", "description": "Effect name from library_overview."},
                    "on": {"type": "boolean"},
                    "wet": {"type": "number"},
                    "decks": {"type": "array", "items": {"type": "integer"}}
                }),
                &["unit"],
            ),
        },
    ]
}

/// Builds the request for a tool call from its name and arguments.
pub fn request(name: &str, arguments: Value) -> Result<crate::protocol::Request, String> {
    let mut object = match arguments {
        Value::Object(map) => map,
        Value::Null => serde_json::Map::new(),
        other => return Err(format!("arguments must be an object, not {other}")),
    };
    object.insert("type".into(), Value::String(name.into()));
    serde_json::from_value(Value::Object(object)).map_err(|e| format!("{name}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Request;

    #[test]
    fn every_tool_is_a_request() {
        let tools = tools();
        let examples = [
            ("library_overview", json!({})),
            (
                "search_tracks",
                json!({"filters": {"genres_include": ["house"]}, "limit": 5}),
            ),
            (
                "plan_set",
                json!({"brief": {"duration_min": 45, "curve": "peak_time"}}),
            ),
            (
                "start_set",
                json!({"tracks": [{"id": 1}, {"id": 2, "transition": {"style": "chop"}},
                       {"id": 3, "recipe": {"beats": 16, "lanes": [], "events": []}}]}),
            ),
            ("get_set_status", json!({})),
            ("adjust_set", json!({"brief": {"energy_end": 9}})),
            ("replace_upcoming", json!({"tracks": [{"id": 7}]})),
            ("stop_set", json!({})),
            ("resume_set", json!({})),
            ("get_state", json!({})),
            ("load_track", json!({"deck": 0, "id": 4})),
            ("play", json!({"deck": 0, "on": true})),
            ("sync", json!({"deck": 1, "on": true})),
            (
                "set_mixer",
                json!({"deck": 1, "control": "filter", "value": -0.5}),
            ),
            ("crossfader", json!({"value": 0.5})),
            (
                "set_pattern",
                json!({"unit": 1, "kit": "808", "steps": {"Kick": "x...x...x...x..."}, "on": true}),
            ),
            ("search_library", json!({"query": "$GENRE % techno"})),
            ("get_track", json!({"id": 3})),
            (
                "edit_tracks",
                json!({"ids": [3], "changes": {"rating": 4, "comment": "peak"}}),
            ),
            ("analyze_tracks", json!({"ids": [3]})),
            ("list_lists", json!({})),
            ("get_list", json!({"id": 2})),
            (
                "create_list",
                json!({"kind": "smartlist", "name": "Fast", "query": "$BPM > 135"}),
            ),
            ("edit_list", json!({"id": 2, "add": [3], "to_top": true})),
            ("delete_list", json!({"id": 2})),
            ("show_list", json!({"id": 2})),
            ("list_history", json!({})),
            ("get_history", json!({"id": 1})),
            ("list_controls", json!({"filter": "loop"})),
            (
                "control",
                json!({"action": {"action": "PlayPause", "deck": 0}}),
            ),
            ("get_settings", json!({})),
            ("set_settings", json!({"changes": {"deck_count": 4}})),
            (
                "set_fx",
                json!({"unit": 0, "effect": "Echo", "on": true, "decks": [0]}),
            ),
        ];
        assert_eq!(tools.len(), examples.len());
        for (name, arguments) in examples {
            let tool = tools.iter().find(|t| t.name == name).expect(name);
            assert!(tool.description.len() > 20, "{name}");
            assert_eq!(tool.input_schema["type"], "object", "{name}");
            request(name, arguments).unwrap();
        }
    }

    #[test]
    fn calls_without_arguments_and_bad_arguments() {
        assert_eq!(
            request("get_set_status", Value::Null),
            Ok(Request::GetSetStatus)
        );
        assert!(request("play", json!({"deck": 0})).is_err());
        assert!(request("nope", json!({})).is_err());
        assert!(request("play", json!([1])).is_err());
    }
}
