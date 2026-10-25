# Interface design

Visual rules for the app (`crates/app`). Inspiration: the pricing page of an
open-source generative AI studio (ArtCraft), whose real CSS variables
(`:root` and `[data-theme=dark]`) were extracted in October 2026.

## The four rules

1. **A grid of lines, not of surfaces.** Cards (decks, mixer, FX units,
   browser) have the same colour as the window background
   (`base` = `surface`). What separates them is a shared 1 px line (`border`)
   (`layout::seam`, `theme::card`). No shadows and no gradients.
2. **Nothing rounded.** `border-radius: 0` on buttons, toggles, pads,
   chips, windows, menus, waveform wells and sliders. Knobs and their
   arcs stay circles (they are controls, not blocks).
3. **Three type registers.**
   - Running text: Inter Regular, 13.5 px (`Body`), 18 px (`Heading`).
   - Labels (*hud label*): Geist Mono Medium, 10–11 px, UPPERCASE,
     0.15 em tracking (`widgets::hud_label`, `widgets::caption`,
     section headers in Preferences, knob names).
   - Blocks (toggles, pads, transport, egui buttons): Geist Mono,
     11–13 px, UPPERCASE, 0.12 em tracking and 8 px side padding
     (`widgets::toggle`, `widgets::pad`, `widgets::transport`). A layout
     that reserves room for a block measures it (`widgets::toggle_width`),
     never with a constant. egui's `Button` style is mono 11.5
     (no automatic uppercase: egui does not apply it).
   - Margins: the FX, deck and waveform cards run edge to edge of the
     window; only the browser content leaves 10 px on each side.
   - Numeric readouts (BPM, times, keys): `Monospace` 12.5 /
     `Small` 10.5, in Geist Mono.
4. **The pointer inverts.** A neutral control under the mouse swaps
   background and text (`invert` / `invert_fg`: white on black in dark,
   black on off-white in light); when pressed, the inverse drops to 80 %. A
   coloured control (deck, PLAY, SYNC on) does not invert: its colour
   drops to 90 % (80 % pressed) and the text stays the same. What is not a
   block (browser row, checkbox box, slider handle, Drum Machine step cell
   or FX grid cell) does not invert either: a 10 % ink tint
   (`control_hover`); off cells carry that tint at rest so they show on the
   background and rise halfway to `text_faint` under the mouse
   (`widgets::cell_fill`). The outline has the same width in every state
   and nothing expands: the control does not change size on hover (test
   `hovering_a_button_does_not_resize_it`).

## Tokens

CSS variables from that page → `theme::Theme` field. The web's colours with
alpha (`line`, `muted`, `faint`) are stored already composited over the
background (`theme::mix`), because in the app every background is `base`.

| Variable | Dark | Light | Field |
|---|---|---|---|
| `--bg` | `#121316` | `#f2f1ee` | `base`, `surface` |
| `--bg-raised` | `#101014` | `#faf9f7` | `control` (buttons at rest) |
| `--bg-sunken` | `#060607` | `#e9e8e4` | `well` (waveforms, inputs) |
| `--ink` | `#f2f1ee` | `#101014` | `text` |
| `--muted` | ink 60 % | ink 62 % | `text_dim` |
| `--faint` | ink 38 % | ink 42 % | `text_faint` |
| `--line` | ink 15 % | ink 16 % | `border`, `beat` |
| `--line-strong` | ink 40 % | ink 40 % | `border_strong` (windows, popups, fader cap) |
| `--accent` | `#2d81ff` | `#2d81ff` | `accent`, `sync` |
| `--accent-ink` | `#74aaff` | `#1659c4` | `accent_ink` (accent as text, links) |
| `--invert-bg` / `--invert-fg` | `#fff` / `#0b0b0c` | `#101014` / `#f2f1ee` | `invert`, `invert_fg` |
| `--danger` | `#ff6259` | `#b3261e` | `danger` |
| `--plan` basic / pro / max / enterprise | `#00a873` `#9d4cff` `#d97700` `#3568c9` | same | `decks` D / C / B / A (fills) and `play` / `cue` |
| `--plan-ink` | `#2fd39c` `#c08fff` `#ffb347` `#7ea3ee` | `#007a54` `#7a2fe0` `#b35f00` `#2a55a8` | `decks_ink` (deck as text on the background: deck letter) |

Own (not on the page): `master` yellow `#e0b020` /
`#9a6b00`; `control_hover` (touched knob or fader) ink 10 % / 8 %; default
`wave` = blue A, green D, ink; `playhead` white / ink.

Chips (key, loudness) copy `plan-check`: the colour at 16 % over the
background behind text in that colour, no outline, and they keep upper and
lower case (musical keys need it).

Old theme names (`Midnight`, `Graphite` → Dark; `Daylight` →
Light) still load from `settings.json` (`serde(alias)`).

## Fonts

`crates/app/fonts/`: Inter Regular (text) and Geist Mono Medium (labels,
blocks, figures), both under the SIL Open Font License 1.1 (`OFL-*.txt` next to
each). They are registered ahead of egui's (`theme::fonts`), which stay as a
fallback for icons and emoji. The S4 screens (`screen.rs`) keep egui's
fonts: they are hardware, not this interface.

## Not adopted (for now)

- Archivo (large headlines) and Instrument Serif (decorative italic): the
  app has no headlines.
- Section numbering (`01/Pricing`), side digit ruler, corner ticks,
  3 % scanlines: web ornaments.
- Active block sliding between segments (Yearly/Monthly toggle):
  egui does not animate; the selected one is marked with the `accent` tint.
