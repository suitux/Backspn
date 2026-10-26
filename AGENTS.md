# Agent instructions

Backspn: DJ app in Rust (egui/eframe), macOS · Windows · Linux,
GPL-3.0-or-later. Workspace in `crates/*` (crate table in
`CONTRIBUTING.md`). Also: `insights/` (Cloudflare Worker, TypeScript) and
`site/` (website in Astro).

Crates missing from the `CONTRIBUTING.md` table: `sampler` (sample decks),
`encode` (writes `.bstem` in Opus), `separate` (stem separation with an
ONNX model, downloaded with permission and verified by SHA-256) and `link`
(network tempo sync).

## Real-time audio

- No allocations, locks or I/O in the audio callback (`crates/engine`).
  Commands go through `rtrb`, freed data goes back through the garbage
  channel, state uses atomics/triple buffer.
- DSP/analysis refactors: check bit for bit against the previous output.

## Way of working

- Do only what is asked, with the smallest change.
- **No commits or pushes.** The user makes them.
- `scripts/check.sh` (fmt, clippy pedantic `-D warnings`, tests) must pass
  when a change is done.
- Follow the practices in `CONTRIBUTING.md`: no duplication, rules in pure
  functions outside the UI with tests next to them, a single source of
  truth, tolerant persistence (read field by field, accept earlier
  versions), wiring tested with `App::offline()` in
  `crates/app/src/app/tests.rs`.
- Interface: visual rules in `docs/design.md`.
- Write everything in the repo (docs, agent files, UI text, identifiers) in
  English.

## Public code rules

- New dependencies must be GPL-3.0 compatible. Fonts OFL only; icons and
  assets our own.
- Do not remove `#[serde(alias = …)]` or old table names in SQLite
  migrations: they read settings and mappings saved by earlier betas. When
  renaming something persisted, add an alias.
- Controller profiles live in `crates/control/profiles/<manufacturer>/`.
- `unsafe_code = "deny"` across the workspace. Every `#[allow]` states its
  reason and is removed once no longer needed.
- UI text goes through `i18n::tr`; add the translation to
  `crates/app/lang/es.tsv` (English<TAB>Spanish).
- Usage statistics: only counts and flags in `insights::Report`. If the
  report changes, change `insights/src/schema.ts` too and refresh the
  fixture with `BACKSPN_BLESS=1 cargo test -p backspn-app server_fixture`.

## Names, trademarks and formats

- No names of other DJ applications, their manufacturers or their branded
  features in the UI, identifiers, docs or text. Use our own, generic
  names. Exceptions (nominative use): controller and mixer names for
  picking the device, and the import/interop modules that already contain
  them (`crates/formats/src/interop.rs`, `crates/library/src/outside/`,
  `crates/app/src/browser/external.rs`, `crates/control/profiles/`).
- **Write only our own formats**: other apps' files are read to import
  them, but everything Backspn saves or exports uses its own formats
  (`.backspn`, `.bstem`, `.bskit`, `set.json`…). Never write another
  application's format.
- Always describe the app's behaviour in our own words; never mention
  where a piece of logic comes from or how it was worked out.
- Do not copy code, tables, assets, skins, fonts, icons or samples from
  other applications.

## User data

- The user's libraries and other apps' files are read only; work on
  copies, never on the originals.
- Never commit real data (`*.nml`, `*.tsi`, collections, personal paths or
  emails). `tests/fixtures/` only holds synthetic files.
- Tests against real data are opt-in (`#[ignore]`, enabled by
  `BACKSPN_NML`, `BACKSPN_TRANSIENTS`, `BACKSPN_TSI`…), always on copies:
  `BACKSPN_NML=/tmp/copy.nml scripts/check.sh`.
- To try the app without touching real data: `BACKSPN_SETTINGS`,
  `BACKSPN_LIBRARY` and `BACKSPN_MAPPINGS` point to alternative files.

## Releases

`scripts/release.sh X.Y.Z[-suffix]` sets the workspace version, commits and
tags (only when the user asks). The `vX.Y.Z` tag must match `Cargo.toml`;
`.github/workflows/release.yml` publishes the installers.
