# In-app updater

The app downloads a new release and installs it when it restarts or quits,
so users no longer need to fetch it from GitHub. This page lists what is
done on the `updater` branch and what is left before it ships.

## How it works

1. `support::update::Check` reads the tags at startup (as before) and shows
   "Backspn X is available" in the top bar.
2. If this copy can update itself (`update::Target`), clicking the notice asks
   **Download** / **Release Notes** (the release's page) / **Later**. Nothing
   is downloaded until the user picks Download. Copies that cannot update
   themselves keep the link to the release's page.
3. The download reads `backspn-manifest.json` and its `.minisig` from the
   release, checks the signature against
   `crates/app/src/support/update/backspn.pub` and that the manifest is for
   the tag's version, then downloads the package for this system with its
   size and SHA-256 checked (`backspn_separate::fetch`).
4. The package is prepared next to the installed copy (`apply::stage`) and
   the notice turns into **Restart to Update**. The installed copy is not
   touched until then; quitting also installs it, without starting again.
5. After the window closes, `update::finish` (in `main`) swaps the new copy
   in and starts a helper that waits for the process to end:

| Target | How this copy is found | Install |
|---|---|---|
| macOS bundle | `…/Backspn.app/Contents/MacOS/Backspn`, not translocated nor under `/Volumes` | `.app.tar.gz` extracted next to the bundle, checked with `codesign --verify`, swapped by renaming; the helper removes the old bundle and runs `open` |
| Windows | `uninstall.exe` next to the `.exe` (NSIS install) | the installer runs with `/P /NS` (progress bar only), plus `/R` to start again |
| AppImage | `$APPIMAGE` | the new file is written next to it and renamed over it |
| `.deb` | `/usr/bin` and `pkexec` present | the helper runs `pkexec apt-get install -y <file>` |

The release workflow publishes `Backspn_X_apple_universal.app.tar.gz` next to
the `.dmg`, writes the manifest with `packaging/manifest.sh`, signs it with
minisign and checks the signature with the key in the repository before
publishing.

## Done on this branch

- `crates/app/src/support/update/` (was `update.rs`):
  - `mod.rs`: version check (unchanged), download state (`Stage`), the
    `READY`/`RESTART` hand-off to `finish`.
  - `manifest.rs`: reads and verifies the signed manifest, picks the package.
  - `target.rs`: tells how this copy was installed.
  - `apply.rs`: prepares the package and installs it after quitting.
  - `fixtures/`: a synthetic manifest signed with a test key (public key
    only) for the tests.
  - `backspn.pub`: placeholder for the release key.
- `crates/separate/src/model.rs`: the checked download is now
  `fetch(url, path, sha256, size, …)`, shared by the stem model and the
  updater. Its errors no longer name the model ("download failed", "the
  download does not match its checksum").
- UI: `app/report.rs` (notice, question, progress, Restart to Update),
  `main.rs` (`update_ask`, `update_prompt`, `update::finish`), `app/quit.rs`
  (a cancelled quit does not restart). 12 new texts in `lang/es.tsv`.
- Release: `packaging/manifest.sh` and the `.app.tar.gz`, manifest and
  signing steps in `.github/workflows/release.yml`; key setup in
  `CONTRIBUTING.md`.
- New dependency: `minisign-verify` (MIT).

Checked so far:

- `scripts/check.sh` passes. Tests cover the signature (valid, edited,
  another version, no key), package choice, unsafe names, target detection
  per system, the helper commands and the AppImage swap; a wiring test with
  `App::offline()` checks that a copy that cannot install only links.
- On macOS, by hand: a fake ad-hoc signed `.app` packed as in the workflow
  was extracted, passed `codesign --verify` and swapped in, with the old
  bundle kept aside; the wait-for-exit helper was run against a real process.
- `packaging/manifest.sh` on dummy packages gives valid JSON.

Not tried yet: a real update on Windows and Linux, the workflow's new steps,
and a whole update against a published release.

## Left to do

### Before the first release with the updater

- [ ] Make the release signing key (see Releases in `CONTRIBUTING.md`):
      `minisign -G -f -W -p crates/app/src/support/update/backspn.pub -s backspn-update.key`.
      Until then `backspn.pub` holds no key and the app only links to the
      release.
- [ ] Put the secret key's contents in the repository secret
      `BACKSPN_UPDATE_KEY`; keep the file outside the repository.
- [ ] Add `BACKSPN_UPDATE_URL` for local testing: when set, read the tags
      from `<url>/tags.json` and the release files from `<url>/<tag>/<name>`
      instead of GitHub (`update::fetch_tags`, `Release::asset_url`).
- [ ] Test on each system with a local server (below), then with a first
      pre-release published by the workflow.

### Known gaps

- Copies already installed (v0.0.21 and earlier) have no updater; their users
  update once by hand.
- The helper starts the new copy with `open` (macOS) or `exec` (Linux)
  without the environment of the old one, so `BACKSPN_SETTINGS` and the like
  are lost after a restart.
- Error messages from the download (`fetch_update`) are in English only; the
  toast prefix is translated.
- A `.deb` without `pkexec`, a Windows copy without the installer and a
  bundle in a folder the user cannot write to fall back to the link.

## Testing locally (macOS)

Needs `BACKSPN_UPDATE_URL` (above) and `brew install minisign`.

1. A test key (restore `backspn.pub` afterwards):

   ```sh
   minisign -G -f -W -p crates/app/src/support/update/backspn.pub -s /tmp/test-update.key
   ```

2. The "new release", served locally:

   ```sh
   cd crates/app && BACKSPN_VERSION=0.0.22 cargo packager --release --formats app && cd ../..
   codesign --force --deep --sign - target/packages/Backspn.app
   mkdir -p /tmp/rel/v0.0.22
   COPYFILE_DISABLE=1 tar -czf /tmp/rel/v0.0.22/Backspn_0.0.22_apple_universal.app.tar.gz -C target/packages Backspn.app
   packaging/manifest.sh 0.0.22 /tmp/rel/v0.0.22
   minisign -S -s /tmp/test-update.key -m /tmp/rel/v0.0.22/backspn-manifest.json -t "backspn-manifest 0.0.22"
   echo '[{"name":"v0.0.22"}]' > /tmp/rel/tags.json
   cd /tmp/rel && python3 -m http.server 8000
   ```

3. The "installed copy", in `~/Applications` so the real one in
   `/Applications` stays untouched, with test data:

   ```sh
   cd crates/app && BACKSPN_VERSION=0.0.21 cargo packager --release --formats app && cd ../..
   codesign --force --deep --sign - target/packages/Backspn.app
   mkdir -p ~/Applications && rm -rf ~/Applications/Backspn.app && cp -R target/packages/Backspn.app ~/Applications/
   BACKSPN_UPDATE_URL=http://localhost:8000 \
   BACKSPN_SETTINGS=/tmp/bs/settings.json BACKSPN_LIBRARY=/tmp/bs/library.sqlite BACKSPN_MAPPINGS=/tmp/bs/mappings.json \
     ~/Applications/Backspn.app/Contents/MacOS/Backspn
   ```

   The restarted copy does not get these variables (see Known gaps): quit it
   at once, or set them first with `launchctl setenv` and remove them with
   `launchctl unsetenv` afterwards.

4. Check:
   - the notice, the Download question, the progress and **Restart to Update**;
   - after the restart the version is 0.0.22 and neither
     `~/Applications/.Backspn.app.old` nor `.Backspn-update` is left;
   - one changed byte in the `.tar.gz` gives a checksum error and leaves the
     installed copy as it was;
   - an edited manifest that was not signed again is refused.

Windows and Linux: the same steps in a virtual machine, with
`--formats nsis` (install 0.0.21 with its installer, so `uninstall.exe` is
there) or `--formats appimage` (run the `.AppImage`).
