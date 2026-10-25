# Contributing to Backspn

## Workspace

| Crate | What it does |
|---|---|
| `crates/formats` | Readers for `collection.nml`, the transient cache, `.tsi` mappings and `.stem.mp4`; own stem format (`.bstem`, Opus) |
| `crates/decode` | MP3, AAC/M4A, ALAC, FLAC, WAV, AIFF and Ogg Vorbis to stereo f32 |
| `crates/deck` | Transport (play/pause, seek, booth-player style CUE, CUP) with declick and cubic resampling |
| `crates/engine` | Audio output via cpal (CoreAudio, WASAPI or ALSA; 256 frames), lock-free commands, triple-buffered state, xrun and load counters |
| `crates/analysis` | Transients, BPM and beatgrid, key and EBU R128 loudness |
| `crates/stretch`, `crates/mixer`, `crates/sync` | Keylock (Signalsmith Stretch), mixer (EQ, filters, limiter) and master clock |
| `crates/fx`, `crates/dsp` | Beat-synced effects and DSP building blocks |
| `crates/library` | SQLite library: import, own export format (`.backspn`), search, playlists, smartlists |
| `crates/control` | MIDI and keyboard mappings, MIDI learn, native HID drivers behind a trait (`control::controller`) |
| `crates/app` | The egui application |
| `insights/` | Cloudflare Worker that collects the anonymous usage reports and serves the public statistics (TypeScript, own workflow `.github/workflows/insights.yml`) |

## Checks

CI (`.github/workflows/check.yml`) runs `scripts/check.sh` on macOS, Windows
and Linux (on Windows, with Git for Windows' bash). Run it before sending a
pull request:

```sh
scripts/check.sh                                                          # fmt + clippy -D warnings + tests
cargo run --release -p backspn-engine --example soak -- --seconds 3600    # xruns/latency
cargo run --release -p backspn-engine --example cpu                       # worst-case CPU
cargo run --release -p backspn-library --example nml -- import COPY.nml library.sqlite
```

Tests against real data are opt-in. Always use copies, never the originals:

```sh
cp "<your collection folder>/collection.nml" /tmp/collection.nml
BACKSPN_NML=/tmp/collection.nml scripts/check.sh
```

Real user data is never committed; `tests/fixtures/` only holds synthetic
files.

## Practices

- **No duplication**: anything repeated goes into a shared function or type in
  the crate that owns it (e.g. `Grid::beats_in`, `analysis::stft::Stft`,
  `library::paths::names_from_file_name`, `control::deck_letter`).
- **Encapsulation**: rules live in pure functions outside the UI
  (`app/src/track_store.rs`, `columns::ColumnLayout`); egui views only draw and
  send commands. Each piece of data has a single source of truth (the
  configuration lives in `Settings`, not in copies inside `App`).
- **Tests**: every new rule gets a test next to its code (a `tests` module at
  the end of the file or the crate's `tests.rs`). DSP/analysis refactors are
  checked bit for bit against the previous output.
  - Whatever sits behind a dialog goes in its own function, so it can be
    tested.
  - The wiring between pieces (a mapped action reaching the deck, a cue
    reaching the browser…) is tested with the whole app:
    `app/src/app/tests.rs`. `App::offline()` uses default settings, an
    in-memory library and an engine without a sound card (`Engine::offline`,
    the test renders). It does not write to disk or open controllers.
- **Tolerant persistence**: settings and mappings are read field by field; an
  unreadable value falls back to its default, is logged, and a `.bak` copy is
  saved before rewriting. Saved formats accept earlier versions.
- **Own formats only**: Backspn reads other applications' files to import
  them, but everything it writes uses its own formats.
- **Lints**: clippy `pedantic` with `-D warnings`; every `#[allow]` states its
  reason and is removed once it is no longer needed.
- **Interface**: follow the visual rules in [docs/design.md](docs/design.md).
- **Usage statistics**: only counts and flags go into `insights::Report`
  (`crates/app/src/insights/`), never names, paths or free text. Changing the
  report means changing `insights/src/schema.ts` too, then refreshing the shared
  fixture with `BACKSPN_BLESS=1 cargo test -p backspn-app server_fixture`; both
  the Rust and the Worker tests read it, so they fail while the two disagree.

## Releases

Only a release tag publishes installers (`.github/workflows/release.yml`):

- `vX.Y.Z` (`v0.1.0`, `v1.1.0`): release, marked as latest.
- `vX.Y.Z-<suffix>` (`v0.1.0-beta1`): pre-release, never latest.

`X.Y.Z` must match the workspace version (`Cargo.toml`); otherwise nothing is
published. The release goes out once the packages for all three systems are
ready.

`scripts/release.sh` sets the workspace version, commits it, tags and, after
asking, pushes `main` and the tag:

```sh
scripts/release.sh 0.1.0-beta1
```
