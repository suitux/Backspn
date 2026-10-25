# Backspn website

Astro landing page, in English (`/`) and Spanish (`/es/`).

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
- While a recording is missing, its slot shows pieces of the app drawn in
  HTML (`src/components/AppWindow.astro`, `Ui.astro`); in `pnpm dev` the slot
  says which file it expects.

## Recordings

Drop the file into `public/media/` with the name from the table and
rebuild; the list (with what to record in each) is in `src/media.ts`.
Feature clips can be `.gif`, `.webp` or `.mp4` (the `.mp4` plays looped and
muted); if you change the extension, change it in `src/media.ts` too.

| File | Where it shows |
|---|---|
| `videos/hero.mp4` (+ `hero.jpg`) | Hero, looped and muted |
| `features/mixing.gif` | Four decks, one clock |
| `features/stems.gif` | Stems |
| `features/effects.gif` | Effects |
| `features/samples.gif` | Sample decks and drum machine |
| `features/library.gif` | Library |
| `features/controllers.gif` | Controllers |
| `videos/overview.mp4` (+ `.jpg`) | Main video |
| `videos/set.mp4` (+ `.jpg`) | Four-deck mix |
| `videos/setup.mp4` (+ `.jpg`) | First launch |

16:10 aspect ratio for clips and 16:9 for videos.
