# ASIO on Windows: status

Work in progress on the `ASIO` branch. Remove this file once ASIO ships.

## Done

- `backspn_engine::Driver` (`System`, `Asio`): `Config::driver`, saved in
  `Settings::driver`. `Driver::available()` only offers ASIO on Windows; a
  saved `Asio` on another platform plays through the system driver.
- `cpal` builds with the `asio` feature on Windows only
  (`crates/engine/Cargo.toml`). The ASIO SDK is downloaded by `asio-sys`
  during the build; it needs LLVM/Clang (`LIBCLANG_PATH`), noted in
  `CONTRIBUTING.md`. The SDK is offered under GPLv3 since October 2025, so it
  is compatible with our licence.
- Output and input accept `F32`, `I32`, `I24` and `I16` samples
  (`playable` in `crates/engine/src/lib.rs`). The renderer still works in
  f32: the output callback renders into a scratch buffer allocated before the
  stream starts and converts it to the device's format; the input converts
  to f32 before the ring buffer. The f32 path gives the same samples as
  before.
- If the driver refuses the requested buffer size (ASIO drivers only take
  certain sizes), the stream is built again with the driver's own size.
- `Engine::reopen` drops the input stream before opening the new device:
  ASIO allows a single driver loaded at a time.
- Preferences › Audio shows a "Driver" row (WASAPI / ASIO) on Windows.
  Changing it starts on the new driver's default device and keeps the old
  device's settings for when it comes back (`mixer_cards::switch_driver`,
  tested).

## To do

1. **Build on Windows.** The `#[cfg(windows)]` code (`host()` in
   `crates/engine/src/lib.rs`) has never been compiled. Check that the
   Windows job of `check.yml` and the Windows build of `release.yml` pass; if
   bindgen does not find libclang, set `LIBCLANG_PATH` in both workflows
   (`C:\Program Files\LLVM\bin`).
2. **Try it with real hardware.** Test with a USB interface that has its own
   ASIO driver, and with a generic ASIO driver for built-in sound cards:
   - the device list, start, stop and switching between WASAPI and ASIO;
   - integer formats (most drivers give `I32`): sound is clean, with no
     clipping or noise;
   - buffer sizes: what the slider offers against what the driver accepts
     (the slider moves in steps of 64; the driver may only take powers of
     two);
   - the latency readout and xruns at 64 and 128 frames;
   - the input (recording from inputs, external mixer) on the same driver;
   - unplugging and plugging the device back while playing.
3. **Device list while playing.** Listing ASIO devices loads each driver;
   with a stream running, probably only the active driver can be loaded.
   Check what Preferences and the background card detection
   (`App::offer_audio`, every 2 s) see, and whether the polling disturbs
   the running stream. If it does, skip the polling while the driver is
   ASIO.
4. **Driver control panel.** ASIO drivers set their buffer and clock in
   their own panel. Consider a button that opens it (cpal does not expose
   it today) or a note under the buffer slider when the driver is ASIO.
5. **Card names.** Mixer card offers (`mixer_cards::CARDS`) match the
   system device names; ASIO driver names are different, so offers won't
   fire under ASIO. Decide whether to match the ASIO names too.
6. **Website and docs.** Once tested, mention ASIO in `site/src/i18n/en.ts`
   and `es.ts` (the "Audio" row and the "Which systems does it run on?"
   answer) and in the README.
