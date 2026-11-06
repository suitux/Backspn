# Backspn website

Astro landing page, in English (`/`) and Spanish (`/es`).

Published at <https://backspn.com> on GitHub Pages: every push to `main`
that touches `site/` deploys it (`.github/workflows/pages.yml`).
backspn.app redirects there from Cloudflare.

```bash
cd site
pnpm install
pnpm dev      # http://localhost:4321
pnpm build    # static output in dist/
```

- Styles: **always Tailwind** (v4, Vite plugin). The app's tokens
  (deck colours, backgrounds, typefaces) are in `@theme` in
  `src/styles/site.css`; only what Tailwind cannot express goes there.
- Texts: `src/i18n/en.ts` and `src/i18n/es.ts` (same keys).
- While a media file is missing, its slot shows pieces of the app drawn in
  HTML (`src/components/AppWindow.astro`, `Ui.astro`); in `pnpm dev` the slot
  says which file it expects.

## Screenshots

`public/media/screens/` holds copies of the README screenshots in
`docs/images/`. When you retake one there, copy it here too (and update its
`size` in `src/media.ts` if the dimensions change).

| File | Where it shows |
|---|---|
| `stems.webp` | Stems |
| `sample-decks.webp` | Sample decks and drum machine |
| `s4-screens.webp` | Controllers |
| `drum-machine-*.webp`, `loop-recorder.webp`, `save-as-loop.webp` | A closer look (gallery) |

## Clips

`public/media/clips/` holds short silent screen recordings of the app that
loop where a still would otherwise go. Each `.mp4` (H.264, `+faststart`, no
audio) has its first frame as a `.webp` poster with the same name; what each
one shows is in `src/media.ts`, with its `size`.

| File | Where it shows |
|---|---|
| `hero.mp4` | Hero |
| `mixing.mp4` | Four decks, one clock |
| `effects.mp4` | Effects |
| `library.mp4` | Library |
| `drum-machine.mp4`, `themes.mp4` | A closer look (gallery) |

To record one: capture the screen with ffmpeg while the app plays, then crop
and encode it, for example
`ffmpeg -i raw.mov -vf "crop=W:H:X:Y,scale=1600:-2,format=yuv420p" -c:v libx264 -crf 24 -movflags +faststart -an clip.mp4`,
and take the poster with `ffmpeg -i clip.mp4 -frames:v 1 poster.jpg` and
`cwebp -q 80 poster.jpg -o clip.webp`.

## Videos

The videos section is hidden until at least one of these files is in
`public/media/` (16:9, each with a `.jpg` poster); what to record in each is in
`src/media.ts`.

| File | Where it shows |
|---|---|
| `videos/overview.mp4` | Main video |
| `videos/set.mp4` | Four-deck mix |
| `videos/setup.mp4` | First launch |
