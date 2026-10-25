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
| `two-decks.webp` | Hero |
| `layout-parallel.webp` | Four decks, one clock |
| `stems.webp` | Stems |
| `four-decks-light.webp` | Effects |
| `sample-decks.webp` | Sample decks and drum machine |
| `layout-essential.webp` | Library |
| `s4-screens.webp` | Controllers |
| `drum-machine*.webp`, `loop-recorder.webp`, `save-as-loop.webp`, `themes.webp` | A closer look (gallery) |

## Videos

The videos section is hidden until at least one of these files is in
`public/media/` (16:9, each with a `.jpg` poster); what to record in each is in
`src/media.ts`.

| File | Where it shows |
|---|---|
| `videos/overview.mp4` | Main video |
| `videos/set.mp4` | Four-deck mix |
| `videos/setup.mp4` | First launch |
