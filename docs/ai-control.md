# AI control

An AI assistant can plan a set from a description ("45 minutes of tech
house, 122 to 128 BPM, energy 4 → 8 → 6, harmonic mixing, nothing played
this week") and Backspn plays it on its own: it loads the next track on the
free deck, syncs it and mixes it in with the chosen transition. The
assistant only steps in when the listener asks for a change.

```
AI client ──stdio / MCP──► backspn-mcp ──TCP 127.0.0.1 (JSON lines + token)──► Backspn
```

## Turning it on

1. Preferences → AI Control → **Allow AI control**. It is off by default.
   While it is on, Backspn listens on a free local port and writes the port
   and a random token to `remote.json` in its data folder, readable only by
   the user. The file is removed when Backspn closes or the option is
   turned off.
2. Build the MCP server: `cargo build --release -p backspn-mcp`.
3. Add it to the assistant:
   - Claude Code: `claude mcp add backspn -- /path/to/target/release/backspn-mcp`
   - Claude Desktop, in `claude_desktop_config.json`:
     ```json
     { "mcpServers": { "backspn": { "command": "/path/to/target/release/backspn-mcp" } } }
     ```
   - Any other MCP client: run `backspn-mcp` as a stdio server.

The client's own subscription or key pays for the AI; Backspn needs none.

## Privacy

Track titles, artists, genres, tags and comments reach the assistant
through the tools it calls. Nothing else leaves the app, and the socket
only listens on 127.0.0.1.

## Tools

| Tool | What it does |
|---|---|
| `library_overview` | Track count, tempo and year range, main genres, playlists, smartlists, effect names and pattern kits with their sounds |
| `search_tracks` | Tracks that pass a brief's filters |
| `plan_set` | A set for a brief: tracks in order, start times, reasons and warnings |
| `start_set` | Plays a set; each track can bring its own transition |
| `get_set_status` | Track playing, next tracks, elapsed time, tempo, energy targets, state |
| `adjust_set` | Plans what is left again from changed brief fields |
| `replace_upcoming` | Sets the tracks after the one playing |
| `stop_set`, `resume_set` | Stops the set (the music keeps playing) or resumes it after a pause |
| `set_pattern` | Sets up the pattern player; it keeps playing until changed |
| `get_state`, `load_track`, `play`, `sync`, `set_mixer`, `crossfader`, `set_fx` | Manual control of decks, mixer and effects |

Beyond sets, the whole app:

| Tool | What it does |
|---|---|
| `search_library` | Searches like the browser's search field (plain words or `$FIELD OP VALUE` rules) |
| `get_track`, `edit_tracks`, `analyze_tracks` | A track's details and cue points; tag, key, rating and play count edits (written to files as the tag settings say); analysis |
| `list_lists`, `get_list`, `create_list`, `edit_list`, `delete_list`, `show_list` | The playlist tree: playlists, smartlists (with rules) and folders; adding and removing tracks, renaming, moving; deleting never touches tracks or files |
| `list_history`, `get_history` | Past sessions and what was played |
| `list_controls`, `control` | Every command a controller can map, found by label and moved as a button tap, a fader position or an encoder turn |
| `get_settings`, `set_settings` | Preferences: read, and change field by field; unknown names are refused; audio device changes restart the audio |

The MCP server also offers the prompt `dj_session` (duration, genres,
tempo, energy, mood, notes), which clients show as a form.

The catalog (names, descriptions, input schemas) lives in
`crates/remote/src/catalog.rs`; every client uses it.

## The brief

Every field is optional: duration or track count, minutes per track,
tempo at start, peak and end, tempo range, largest tempo change between
tracks, keylock, energy (1–10) at start, peak and end, the curve
(`linear`, `warmup`, `build`, `peak_time`, `wave`, `plateau`, `cooldown`)
and where the peak falls, harmony (`strict`, `relaxed`, `off`) and start
key, genres, mood words, artists, labels, years, playlist or smartlist,
minimum rating, freshness, days since last played, artist spacing, tracks
that must play, opener, closer and the default transition.

The planner (`crates/library/src/set_plan.rs`) filters the library,
builds target curves for tempo and energy, and walks them with a small
beam search scoring closeness to the curves, key compatibility, tempo
jumps, repeated artists, freshness, mood words and rating. Energy comes
from `library::energy::estimate`.

## Transitions

A transition is a recipe laid out in beats from the start of the mix
(`crates/remote/src/transition.rs`):

- **Lanes** move a control over time: volume, EQ band, filter, effect
  dry/wet or knob, on the incoming (`in`) or outgoing (`out`) deck. Points
  glide (`linear`) or jump on their beat (`step`, for fader cuts).
- **Events** fire once on a beat: play, stop, pick an effect, assign an
  effect unit to a deck, switch it on or off, loop, end a loop, beat jump.

The incoming track starts on beat 0 unless an event plays it later. At the
end the outgoing deck stops and every control used goes back to neutral.

Named styles build ready-made recipes: `blend` (bass swapped halfway),
`cut`, `fade`, `filter_sweep`, `chop` (fader cuts on the beat), `echo_out`,
`loop_out`, `brake` and `backspin`. An assistant can also write its own
recipe for any track.

## The pattern player

An effect unit in pattern mode is a drum machine with a kit of sounds and
eight stored patterns. A pattern spec (`PatternSpec` in
`crates/remote/src/transition.rs`) picks the kit by name, a stored pattern
(1–8) or steps per sound name over one bar (16, 32 or 64 cells of `x` and
`.`, fitted to the pattern's resolution), and sets volume, decay, pitch,
duck, roll and on/off. `only_these` silences the sounds not written.

- `set_pattern` sets it up at once and chooses the decks it plays through;
  it keeps playing over the set until changed.
- A `pattern` event in a recipe brings drums into a mix through the `in` or
  `out` deck; the unit switches off when the mix ends.

## The autopilot

`crates/app/src/autopilot.rs` is a pure state machine stepped every frame
with a view of the decks. It plays on decks A and B: it loads the next
track muted on the free deck, sets keylock, syncs it to the playing deck,
starts the recipe at the mix point (the minutes to play, or the end of the
track minus the recipe) and, after the mix, eases the new track's tempo
back to its own over 30 seconds. If the next track is not ready near the
end, the playing one loops.

Everything runs on the interface thread through the same actions as a
controller (`App::execute`); the audio thread is not touched.

Moving a control of the set's decks by hand (play, cue, sync, tempo,
volume, EQ, filter, crossfader, loading) pauses the set; `resume_set` picks
it up again from the decks as they are.

## Protocol

One JSON object per line. The client sends
`{"token": "…", "request": {"type": "<tool name>", …arguments}}` and gets
`{"ok": true, "result": …}` or `{"ok": false, "error": "…"}`. Requests
wait up to 20 seconds for the app to answer.
